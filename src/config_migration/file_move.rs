//! `FileMoveMigration` — relocate a single file from one or more legacy
//! locations to a new canonical location.
//!
//! Classification rules:
//!
//! | legacies present | target present | status           | behavior                                   |
//! |------------------|----------------|------------------|--------------------------------------------|
//! | none             | no             | `NothingToDo`    | —                                          |
//! | none             | yes            | `AlreadyApplied` | —                                          |
//! | ≥1               | yes            | `Conflict`       | user resolves manually                     |
//! | ≥1               | no             | `Pending`        | copy first-present to target; delete rest  |
//!
//! **Priority semantics.** `legacy_paths` is an **ordered priority list**.
//! It encodes "which of these the app would have read". The first entry
//! that exists is the authoritative source of truth; every other present
//! entry is by definition stale — the app has been ignoring it at read
//! time, so deleting it on migration matches existing behavior. Callers
//! must construct this list to reflect actual read-time priority.
//!
//! If a lower-priority legacy happens to have different bytes than the
//! source, we still discard it (that's the whole point of "stale"), but we
//! emit a `WARN` log so there's a paper trail if someone later asks what
//! happened to a hand-edited file.

use std::path::PathBuf;

use log::{info, warn};

use super::{
    Migration, MigrationError, Status, atomic_write, create_dir_private, read_bytes,
    remove_if_exists,
};

pub struct FileMoveMigration {
    id: String,
    target: PathBuf,
    legacy_paths: Vec<PathBuf>,
}

impl FileMoveMigration {
    /// `legacy_paths` is ordered by priority — the first path that exists
    /// becomes the migration source. Other present entries are stale and
    /// deleted together with the source on success.
    pub fn new(id: impl Into<String>, target: PathBuf, legacy_paths: Vec<PathBuf>) -> Self {
        Self {
            id: id.into(),
            target,
            legacy_paths,
        }
    }

    fn present_legacies(&self) -> Vec<PathBuf> {
        self.legacy_paths
            .iter()
            .filter(|p| p.exists())
            .cloned()
            .collect()
    }
}

impl Migration for FileMoveMigration {
    fn id(&self) -> &str {
        &self.id
    }

    fn inspect(&self) -> Status {
        let target_exists = self.target.exists();
        let present_empty = self.present_legacies().is_empty();
        match (target_exists, present_empty) {
            (true, true) => Status::AlreadyApplied,
            (true, false) => Status::Conflict,
            (false, true) => Status::NothingToDo,
            (false, false) => Status::Pending,
        }
    }

    fn execute(&self) -> Result<(), MigrationError> {
        let present = self.present_legacies();
        let Some((source, cleanup)) = present.split_first() else {
            // Runner should not call execute() on a non-Pending migration, but
            // be defensive: nothing to do is a success.
            return Ok(());
        };

        if let Some(parent) = self.target.parent() {
            info!(
                "migration '{}': ensuring parent directory {}",
                self.id,
                parent.display()
            );
            create_dir_private(&self.id, parent)?;
        }

        let bytes = read_bytes(&self.id, source)?;
        info!(
            "migration '{}': copying {} -> {} ({} bytes)",
            self.id,
            source.display(),
            self.target.display(),
            bytes.len()
        );
        atomic_write(&self.id, &self.target, &bytes)?;

        // Verify byte-for-byte.
        let readback = read_bytes(&self.id, &self.target)?;
        if readback != bytes {
            // Attempt to roll back the partially-written target so we don't
            // leave a corrupt file alongside the still-intact source.
            let _ = std::fs::remove_file(&self.target);
            return Err(MigrationError::VerifyMismatch {
                migration_id: self.id.clone(),
                path: self.target.clone(),
                expected_len: bytes.len() as u64,
                got_len: readback.len() as u64,
            });
        }
        info!(
            "migration '{}': verified {} bytes match",
            self.id,
            bytes.len()
        );

        remove_if_exists(&self.id, source)?;
        info!(
            "migration '{}': deleted source {}",
            self.id,
            source.display()
        );
        for stale in cleanup {
            // `stale` is known-stale by definition — the app would have
            // read `source` (higher priority) and ignored this one. But if
            // its bytes differ from the source, leave a paper trail: a
            // user may have hand-edited it expecting it to be active.
            let differs = std::fs::read(stale)
                .map(|content| content != bytes)
                .unwrap_or(true);
            if differs {
                warn!(
                    "migration '{}': discarding stale legacy {} ({} bytes) — content differs \
                     from migrated source {}; the app was already ignoring this file at read \
                     time, but if you recently edited it those changes are not carried over",
                    self.id,
                    stale.display(),
                    std::fs::metadata(stale).map(|m| m.len()).unwrap_or(0),
                    source.display(),
                );
            }
            remove_if_exists(&self.id, stale)?;
            info!(
                "migration '{}': deleted stale legacy {}",
                self.id,
                stale.display()
            );
        }

        Ok(())
    }

    fn prompt_line(&self) -> String {
        let present = self.present_legacies();
        let source = present
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let extras = present.len().saturating_sub(1);
        let extra_note = match extras {
            0 => String::new(),
            1 => " (plus 1 stale legacy file to delete)".to_string(),
            n => format!(" (plus {n} stale legacy files to delete)"),
        };
        format!(
            "move config from {} to {}{}",
            source,
            self.target.display(),
            extra_note
        )
    }

    fn conflict_message(&self) -> String {
        let legacies = self.present_legacies();

        let mut out = String::new();
        out.push_str("  New:\n");
        out.push_str(&format!("    {}\n", self.target.display()));
        if let Some(info) = file_info(&self.target) {
            out.push_str(&format!("    {info}\n"));
        }

        out.push_str("\n  Old:\n");
        for p in &legacies {
            out.push_str(&format!("    {}\n", p.display()));
            if let Some(info) = file_info(p) {
                out.push_str(&format!("    {info}\n"));
            }
        }

        out.push_str("\n  Keep the NEW file:\n");
        for p in &legacies {
            out.push_str(&format!("    rm {}\n", shell_quote(p)));
        }
        out.push_str("\n  Keep the OLD file (migration re-runs on next launch):\n");
        out.push_str(&format!("    rm {}\n", shell_quote(&self.target)));

        out.push_str(
            "\n  If an older Bookokrat is still running, it will keep re-creating the old file.",
        );
        out
    }
}

fn file_info(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let size = meta.len();
    let modified = meta.modified().ok().and_then(|t| {
        chrono::DateTime::<chrono::Local>::from(t)
            .to_rfc3339()
            .into()
    });
    match modified {
        Some(ts) => Some(format!("{size} bytes, modified {ts}")),
        None => Some(format!("{size} bytes")),
    }
}

fn shell_quote(path: &std::path::Path) -> String {
    let s = path.display().to_string();
    if s.chars().all(|c| c.is_alphanumeric() || "/._-".contains(c)) {
        s
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

// --------------------------------------------------------------------------
//                                tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Test scaffolding: three paths in a tempdir, helpers to populate them.
    struct Scenario {
        _tmp: tempfile::TempDir,
        a: PathBuf,
        b: PathBuf,
        c: PathBuf,
        migration: FileMoveMigration,
    }

    impl Scenario {
        fn new() -> Self {
            let tmp = tempfile::TempDir::new().unwrap();
            let a = tmp.path().join("legacy_dotfile/.bookokrat_settings.yaml");
            let b = tmp.path().join("Application Support/bookokrat/config.yaml");
            let c = tmp.path().join(".config/bookokrat/config.yaml");
            fs::create_dir_all(a.parent().unwrap()).unwrap();
            fs::create_dir_all(b.parent().unwrap()).unwrap();
            // NOTE: we intentionally do NOT pre-create the parent of `c`; the
            // migration must create it itself.
            let migration = FileMoveMigration::new("config", c.clone(), vec![b.clone(), a.clone()]);
            Self {
                _tmp: tmp,
                a,
                b,
                c,
                migration,
            }
        }

        fn write_a(&self, s: &[u8]) {
            fs::write(&self.a, s).unwrap();
        }
        fn write_b(&self, s: &[u8]) {
            fs::write(&self.b, s).unwrap();
        }
        fn write_c(&self, s: &[u8]) {
            fs::create_dir_all(self.c.parent().unwrap()).unwrap();
            fs::write(&self.c, s).unwrap();
        }
    }

    // ---- 8-row inspection table ----

    #[test]
    fn row1_none_is_nothing_to_do() {
        let s = Scenario::new();
        assert_eq!(s.migration.inspect(), Status::NothingToDo);
    }

    #[test]
    fn row2_only_target_is_already_applied() {
        let s = Scenario::new();
        s.write_c(b"target");
        assert_eq!(s.migration.inspect(), Status::AlreadyApplied);
    }

    #[test]
    fn row3_only_a_is_pending() {
        let s = Scenario::new();
        s.write_a(b"from-a");
        assert_eq!(s.migration.inspect(), Status::Pending);
    }

    #[test]
    fn row4_only_b_is_pending() {
        let s = Scenario::new();
        s.write_b(b"from-b");
        assert_eq!(s.migration.inspect(), Status::Pending);
    }

    #[test]
    fn row5_both_legacies_identical_migrates_first_priority_and_deletes_all() {
        let s = Scenario::new();
        s.write_a(b"same-content");
        s.write_b(b"same-content"); // B is first in legacy_paths, so it's the source
        assert_eq!(s.migration.inspect(), Status::Pending);
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), b"same-content");
        assert!(!s.a.exists(), "A should have been deleted");
        assert!(!s.b.exists(), "B should have been deleted");
    }

    #[test]
    fn row5_both_legacies_divergent_trusts_priority_and_discards_lower() {
        // Priority order says B is the authoritative source (what the app
        // was actually reading). A is known-stale — the app was ignoring
        // it. Migration trusts that contract.
        let s = Scenario::new();
        s.write_a(b"stale-dotfile-content");
        s.write_b(b"authoritative-content");
        assert_eq!(s.migration.inspect(), Status::Pending);
        s.migration.execute().unwrap();
        assert_eq!(
            fs::read(&s.c).unwrap(),
            b"authoritative-content",
            "B (first priority) must be the source"
        );
        assert!(!s.a.exists());
        assert!(!s.b.exists());
    }

    #[test]
    fn row6_target_plus_a_is_conflict() {
        let s = Scenario::new();
        s.write_a(b"from-a");
        s.write_c(b"target");
        assert_eq!(s.migration.inspect(), Status::Conflict);
    }

    #[test]
    fn row7_target_plus_b_is_conflict() {
        let s = Scenario::new();
        s.write_b(b"from-b");
        s.write_c(b"target");
        assert_eq!(s.migration.inspect(), Status::Conflict);
    }

    #[test]
    fn row8_all_three_present_is_conflict() {
        let s = Scenario::new();
        s.write_a(b"from-a");
        s.write_b(b"from-b");
        s.write_c(b"target");
        assert_eq!(s.migration.inspect(), Status::Conflict);
    }

    // ---- execute() happy paths ----

    #[test]
    fn execute_copies_only_a_when_only_a_present() {
        let s = Scenario::new();
        s.write_a(b"from-a");
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), b"from-a");
        assert!(!s.a.exists());
    }

    #[test]
    fn execute_copies_only_b_when_only_b_present() {
        let s = Scenario::new();
        s.write_b(b"from-b");
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), b"from-b");
        assert!(!s.b.exists());
    }

    #[test]
    fn execute_creates_parent_dir_if_missing() {
        let s = Scenario::new();
        s.write_a(b"x");
        assert!(!s.c.parent().unwrap().exists());
        s.migration.execute().unwrap();
        assert!(s.c.parent().unwrap().exists());
        assert_eq!(fs::read(&s.c).unwrap(), b"x");
    }

    #[test]
    fn execute_deletes_all_legacies_on_success() {
        let s = Scenario::new();
        s.write_a(b"a-content");
        s.write_b(b"b-content");
        s.migration.execute().unwrap();
        assert!(!s.a.exists());
        assert!(!s.b.exists());
    }

    #[test]
    fn execute_then_inspect_is_already_applied() {
        let s = Scenario::new();
        s.write_b(b"x");
        s.migration.execute().unwrap();
        assert_eq!(s.migration.inspect(), Status::AlreadyApplied);
    }

    #[test]
    fn execute_unicode_content_roundtrip() {
        let s = Scenario::new();
        let content = "πœ∑´®†¥¨ˆøπ\n— 中文 — עברית — русский\n".as_bytes();
        s.write_b(content);
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), content);
    }

    #[test]
    fn execute_empty_file_roundtrip() {
        let s = Scenario::new();
        s.write_b(b"");
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), b"");
        assert!(!s.b.exists());
    }

    #[test]
    fn execute_large_file_roundtrip() {
        let s = Scenario::new();
        let mut content = Vec::with_capacity(1_000_000);
        for i in 0..1_000_000u32 {
            content.push((i & 0xff) as u8);
        }
        s.write_b(&content);
        s.migration.execute().unwrap();
        assert_eq!(fs::read(&s.c).unwrap(), content);
    }

    #[test]
    fn execute_noop_when_nothing_to_do() {
        // Defensive: execute() on a NothingToDo migration must not error.
        let s = Scenario::new();
        assert_eq!(s.migration.inspect(), Status::NothingToDo);
        s.migration.execute().unwrap();
        assert!(!s.c.exists());
    }

    // ---- execute() error paths ----

    #[test]
    fn execute_source_read_error_surfaces_with_context() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Legacy path points at a directory, not a file — read() will fail.
        let src = tmp.path().join("dir_not_file");
        fs::create_dir(&src).unwrap();
        let dst = tmp.path().join("out/dst.yaml");
        let m = FileMoveMigration::new("t", dst, vec![src.clone()]);
        assert_eq!(m.inspect(), Status::Pending);
        match m.execute() {
            Err(MigrationError::Io { step, path, .. }) => {
                assert_eq!(step, "read");
                assert_eq!(path, src);
            }
            other => panic!("expected Io(read) error, got {other:?}"),
        }
    }

    // ---- prompt_line / conflict_message ----

    #[test]
    fn prompt_line_mentions_source_and_target() {
        let s = Scenario::new();
        s.write_b(b"x");
        let line = s.migration.prompt_line();
        assert!(line.contains(&s.b.display().to_string()));
        assert!(line.contains(&s.c.display().to_string()));
    }

    #[test]
    fn prompt_line_mentions_extra_stale_legacies() {
        let s = Scenario::new();
        // Identical content — otherwise this would be a Conflict, not Pending.
        s.write_a(b"shared");
        s.write_b(b"shared");
        assert_eq!(s.migration.inspect(), Status::Pending);
        let line = s.migration.prompt_line();
        assert!(line.contains("1 stale legacy"));
    }

    #[test]
    fn conflict_message_includes_file_sizes_and_rm_commands() {
        let s = Scenario::new();
        s.write_a(b"aa");
        s.write_b(b"bbbb");
        s.write_c(b"cccccc");
        let msg = s.migration.conflict_message();
        // Size of each present file appears somewhere in the message.
        assert!(msg.contains("2 bytes"), "A size missing: {msg}");
        assert!(msg.contains("4 bytes"), "B size missing: {msg}");
        assert!(msg.contains("6 bytes"), "target size missing: {msg}");
        // Concrete rm commands for both resolution paths.
        assert!(msg.contains("Keep the NEW"), "Keep NEW missing: {msg}");
        assert!(msg.contains("Keep the OLD"), "Keep OLD missing: {msg}");
        assert!(msg.contains("rm "), "rm command missing: {msg}");
    }

    #[test]
    fn conflict_message_shell_quotes_paths_with_spaces() {
        let s = Scenario::new();
        s.write_b(b"x");
        s.write_c(b"y");
        let msg = s.migration.conflict_message();
        // B path contains a space ("Application Support"), must be quoted.
        let b_str = s.b.display().to_string();
        assert!(b_str.contains(' '));
        assert!(
            msg.contains(&format!("\"{b_str}\"")),
            "path with space should be shell-quoted: {msg}"
        );
    }

    #[test]
    fn conflict_message_lists_target_and_all_present_legacies() {
        let s = Scenario::new();
        s.write_a(b"a");
        s.write_b(b"b");
        s.write_c(b"c");
        let msg = s.migration.conflict_message();
        assert!(msg.contains(&s.a.display().to_string()));
        assert!(msg.contains(&s.b.display().to_string()));
        assert!(msg.contains(&s.c.display().to_string()));
    }

    // ---- requires_confirmation defaults to true ----

    #[test]
    fn requires_confirmation_is_true_by_default() {
        let s = Scenario::new();
        assert!(s.migration.requires_confirmation());
    }

    // ---- degenerate input: empty legacy list ----

    #[test]
    fn empty_legacy_paths_is_nothing_to_do_when_target_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let m = FileMoveMigration::new("t", tmp.path().join("c"), vec![]);
        assert_eq!(m.inspect(), Status::NothingToDo);
    }

    #[test]
    fn empty_legacy_paths_is_already_applied_when_target_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let c = tmp.path().join("c");
        fs::write(&c, b"x").unwrap();
        let m = FileMoveMigration::new("t", c, vec![]);
        assert_eq!(m.inspect(), Status::AlreadyApplied);
    }
}
