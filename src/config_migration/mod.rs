//! Filesystem migration runner for config files (and, in the future, other
//! small on-disk artifacts).
//!
//! Migrations are opaque units of work that each decide (a) whether they
//! apply to the current filesystem state, and (b) how to apply themselves.
//! The runner composes them, refuses the whole plan if any are in conflict,
//! optionally asks the caller for confirmation, then executes in registration
//! order.
//!
//! The module has no dependency on application state. Callers pass in
//! already-resolved paths so tests can drive it with `tempfile::TempDir`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use log::{error, info, warn};

pub mod file_move;

pub use file_move::FileMoveMigration;

/// A single migration step. Implementations are expected to be pure with
/// respect to their own inputs: all paths and configuration are captured at
/// construction time.
pub trait Migration: Send + Sync {
    /// Stable identifier for logs and user-facing messages.
    fn id(&self) -> &str;

    /// Inspect the filesystem and classify what should happen. Must not mutate.
    fn inspect(&self) -> Status;

    /// Apply the migration. Must be atomic on success (no partial state).
    fn execute(&self) -> Result<(), MigrationError>;

    /// One line shown in the confirmation prompt, describing what will happen
    /// if the user approves. Called only when `inspect() == Pending`.
    fn prompt_line(&self) -> String;

    /// Multi-line explanation shown when `inspect() == Conflict`, telling the
    /// user how to resolve it manually.
    fn conflict_message(&self) -> String;

    /// Whether this migration requires user confirmation before running.
    /// Defaults to `true` (safer). Silent migrations (e.g. forward-compatible
    /// schema bumps) can override to `false`.
    fn requires_confirmation(&self) -> bool {
        true
    }
}

/// Result of inspecting a single migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing to migrate and nothing to clean up.
    NothingToDo,
    /// Migration already applied (target present, no legacy left).
    AlreadyApplied,
    /// Ready to run: source available, target path clear.
    Pending,
    /// Both target and legacy state exist; requires manual user resolution.
    Conflict,
}

/// Aggregate outcome of `inspect()` across all registered migrations.
#[derive(Debug)]
pub enum Report {
    /// Every migration reported `NothingToDo` or `AlreadyApplied`.
    NothingToDo,
    /// At least one migration is `Pending` and none are blocked. The Vec
    /// contains the ids in registration order.
    Proceed { pending: Vec<String> },
    /// At least one migration is blocked. Whole plan refuses. The Vec
    /// contains the blocked ids in registration order.
    Blocked { conflicts: Vec<String> },
}

/// Final outcome of `run()`.
#[derive(Debug)]
pub enum Outcome {
    /// No migrations were needed.
    NothingToDo,
    /// All pending migrations were applied successfully.
    Completed { applied: Vec<String> },
    /// User declined the confirmation prompt. Nothing was changed.
    Declined,
    /// Plan refused because one or more migrations are in conflict.
    Blocked { conflicts: Vec<String> },
    /// An execution step failed. The Vec lists migrations that ran
    /// successfully before the failure.
    Failed {
        applied: Vec<String>,
        failed_id: String,
        error: MigrationError,
    },
}

/// Errors raised by `Migration::execute`. Each variant carries enough context
/// to write a useful log line without further guessing.
#[derive(Debug)]
pub enum MigrationError {
    /// Failed to create the parent directory of the target.
    ParentCreate {
        migration_id: String,
        path: PathBuf,
        source: io::Error,
    },
    /// Generic I/O failure during a named step (read/write/rename/delete).
    Io {
        migration_id: String,
        step: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// Target was written but read-back verification disagreed.
    VerifyMismatch {
        migration_id: String,
        path: PathBuf,
        expected_len: u64,
        got_len: u64,
    },
    /// Filesystem state drifted between the initial `inspect()` (which
    /// reported `Pending`) and the moment `execute()` was about to run. A
    /// concurrent process likely touched the target or legacy paths; we
    /// refuse rather than proceed blind or silently skip.
    StateChanged {
        migration_id: String,
        observed_before: Status,
        observed_now: Status,
    },
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::ParentCreate {
                migration_id,
                path,
                source,
            } => write!(
                f,
                "[{migration_id}] failed to create parent directory {}: {source}",
                path.display()
            ),
            MigrationError::Io {
                migration_id,
                step,
                path,
                source,
            } => write!(
                f,
                "[{migration_id}] {step} failed on {}: {source}",
                path.display()
            ),
            MigrationError::VerifyMismatch {
                migration_id,
                path,
                expected_len,
                got_len,
            } => write!(
                f,
                "[{migration_id}] verify mismatch on {}: expected {expected_len} bytes, got {got_len}",
                path.display()
            ),
            MigrationError::StateChanged {
                migration_id,
                observed_before,
                observed_now,
            } => write!(
                f,
                "[{migration_id}] filesystem state changed between inspect and execute \
                 (was {observed_before:?}, now {observed_now:?}); refusing to proceed — \
                 close any other running Bookokrat instances and retry",
            ),
        }
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MigrationError::ParentCreate { source, .. } => Some(source),
            MigrationError::Io { source, .. } => Some(source),
            MigrationError::VerifyMismatch { .. } => None,
            MigrationError::StateChanged { .. } => None,
        }
    }
}

/// Inspect every migration and produce an aggregate report. Logs the
/// observation for each one at INFO (or WARN for conflicts).
pub fn inspect(migrations: &[Box<dyn Migration>]) -> Report {
    let mut pending: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut nothing = 0usize;
    let mut already = 0usize;

    for m in migrations {
        let status = m.inspect();
        match status {
            Status::NothingToDo => {
                info!("migration '{}': status=NothingToDo", m.id());
                nothing += 1;
            }
            Status::AlreadyApplied => {
                info!("migration '{}': status=AlreadyApplied", m.id());
                already += 1;
            }
            Status::Pending => {
                info!(
                    "migration '{}': status=Pending — {}",
                    m.id(),
                    m.prompt_line()
                );
                pending.push(m.id().to_string());
            }
            Status::Conflict => {
                warn!(
                    "migration '{}': status=Conflict\n{}",
                    m.id(),
                    m.conflict_message()
                );
                conflicts.push(m.id().to_string());
            }
        }
    }

    info!(
        "inspected {} migrations: {} pending, {} already-applied, {} conflicts, {} nothing-to-do",
        migrations.len(),
        pending.len(),
        already,
        conflicts.len(),
        nothing
    );

    if !conflicts.is_empty() {
        Report::Blocked { conflicts }
    } else if pending.is_empty() {
        Report::NothingToDo
    } else {
        Report::Proceed { pending }
    }
}

/// Format the confirmation prompt body shown to the user. One line per
/// confirmation-required pending migration, preceded by a header.
pub fn format_prompt(migrations: &[Box<dyn Migration>]) -> String {
    let mut out = String::new();
    out.push_str("Bookokrat needs to apply the following migration(s):\n\n");
    for m in migrations {
        if matches!(m.inspect(), Status::Pending) && m.requires_confirmation() {
            out.push_str(&format!("  - [{}] {}\n", m.id(), m.prompt_line()));
        }
    }
    out.push_str(
        "\nNOTE: close any other running Bookokrat instances first — \
         otherwise they may re-create the old files after migration.\n",
    );
    out.push_str("\nProceed?");
    out
}

/// Format the error shown to the user (pre-TUI stderr, and logged) when the
/// plan is blocked by conflicts.
pub fn format_conflict_error(migrations: &[Box<dyn Migration>]) -> String {
    let blockers: Vec<&Box<dyn Migration>> = migrations
        .iter()
        .filter(|m| matches!(m.inspect(), Status::Conflict))
        .collect();

    let mut out = String::new();
    let count = blockers.len();
    out.push_str(&format!(
        "Bookokrat: config migration blocked — {count} conflict(s), both old and new locations exist.\n\n"
    ));

    for (i, m) in blockers.iter().enumerate() {
        out.push_str(&format!(
            "── conflict {i}/{count}: [{id}] ──\n",
            i = i + 1,
            count = count,
            id = m.id(),
        ));
        out.push_str(&m.conflict_message());
        out.push_str("\n\n");
    }

    out.push_str("Resolve the conflict(s) above and relaunch.");
    out
}

/// Execute every `Pending` migration in registration order.
///
/// `confirm` is called once, with the pre-formatted prompt text, when at
/// least one pending migration has `requires_confirmation() == true`. If it
/// returns `false`, nothing is executed (including silent migrations, to
/// preserve ordering invariants if a future silent migration depends on a
/// user-visible one running first).
pub fn run<F>(migrations: &[Box<dyn Migration>], confirm: F) -> Outcome
where
    F: FnOnce(&str) -> bool,
{
    match inspect(migrations) {
        Report::NothingToDo => {
            info!("migration summary: nothing to do");
            Outcome::NothingToDo
        }
        Report::Blocked { conflicts } => {
            error!(
                "migration aborted: {} conflict(s) require manual resolution: {:?}",
                conflicts.len(),
                conflicts
            );
            Outcome::Blocked { conflicts }
        }
        Report::Proceed { pending } => {
            let needs_confirm = migrations
                .iter()
                .any(|m| matches!(m.inspect(), Status::Pending) && m.requires_confirmation());

            if needs_confirm {
                let prompt = format_prompt(migrations);
                info!(
                    "prompting user: {} migration(s) require confirmation: {:?}",
                    pending.len(),
                    pending
                );
                let approved = confirm(&prompt);
                if !approved {
                    info!("user response: declined — exiting without changes");
                    return Outcome::Declined;
                }
                info!("user response: approved");
            } else {
                info!(
                    "no confirmation required; running {} silent migration(s)",
                    pending.len()
                );
            }

            let mut applied: Vec<String> = Vec::new();
            for m in migrations {
                // Only run migrations that we committed to during the initial
                // inspect(). If one has drifted since then (a concurrent
                // process touched the filesystem), don't silently skip —
                // stop and surface it, so startup can't succeed on a
                // partial apply.
                if !pending.iter().any(|id| id == m.id()) {
                    continue;
                }
                let status_now = m.inspect();
                if !matches!(status_now, Status::Pending) {
                    let err = MigrationError::StateChanged {
                        migration_id: m.id().to_string(),
                        observed_before: Status::Pending,
                        observed_now: status_now,
                    };
                    error!("migration '{}': {err}", m.id());
                    return Outcome::Failed {
                        applied,
                        failed_id: m.id().to_string(),
                        error: err,
                    };
                }
                info!("migration '{}': executing", m.id());
                match m.execute() {
                    Ok(()) => {
                        info!("migration '{}': completed successfully", m.id());
                        applied.push(m.id().to_string());
                    }
                    Err(e) => {
                        error!("migration '{}': failed — {e}", m.id());
                        return Outcome::Failed {
                            applied,
                            failed_id: m.id().to_string(),
                            error: e,
                        };
                    }
                }
            }

            info!(
                "migration summary: {} applied, 0 failed; proceeding to app startup",
                applied.len()
            );
            Outcome::Completed { applied }
        }
    }
}

// ---------- shared filesystem helpers (used by Migration impls) ----------

/// Atomically write `bytes` to `path`: write to `<path>.tmp`, fsync the file,
/// rename into place. Leaves the filesystem either untouched or with the new
/// content in place, even on crash. `migration_id` is used only for error
/// context.
pub fn atomic_write(migration_id: &str, path: &Path, bytes: &[u8]) -> Result<(), MigrationError> {
    use std::io::Write;

    let tmp_path = tmp_sibling(path);
    let mut file = fs::File::create(&tmp_path).map_err(|source| MigrationError::Io {
        migration_id: migration_id.to_string(),
        step: "create_tmp",
        path: tmp_path.clone(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| MigrationError::Io {
        migration_id: migration_id.to_string(),
        step: "write_tmp",
        path: tmp_path.clone(),
        source,
    })?;
    file.sync_all().map_err(|source| MigrationError::Io {
        migration_id: migration_id.to_string(),
        step: "fsync_tmp",
        path: tmp_path.clone(),
        source,
    })?;
    drop(file);
    fs::rename(&tmp_path, path).map_err(|source| MigrationError::Io {
        migration_id: migration_id.to_string(),
        step: "rename_tmp_to_target",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Read a file's bytes with path-carrying error context.
pub fn read_bytes(migration_id: &str, path: &Path) -> Result<Vec<u8>, MigrationError> {
    fs::read(path).map_err(|source| MigrationError::Io {
        migration_id: migration_id.to_string(),
        step: "read",
        path: path.to_path_buf(),
        source,
    })
}

/// Delete a file, treating "already absent" as success. Other errors surface
/// with path context. `migration_id` is used only for error context.
pub fn remove_if_exists(migration_id: &str, path: &Path) -> Result<(), MigrationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MigrationError::Io {
            migration_id: migration_id.to_string(),
            step: "remove",
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Create a directory (and parents) with mode 0700 on Unix, best-effort on
/// other platforms.
pub fn create_dir_private(migration_id: &str, dir: &Path) -> Result<(), MigrationError> {
    fs::create_dir_all(dir).map_err(|source| MigrationError::ParentCreate {
        migration_id: migration_id.to_string(),
        path: dir.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(dir) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(dir, perms);
        }
    }
    Ok(())
}

fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("migration"));
    name.push(".tmp");
    path.parent()
        .map(|p| p.join(&name))
        .unwrap_or_else(|| PathBuf::from(name))
}

// --------------------------------------------------------------------------
//                                tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Minimal Migration used to drive the runner without touching the
    /// filesystem. `status` controls what inspect() returns; `on_execute`
    /// controls whether execute() succeeds, and records the call order on
    /// the shared `log`.
    struct FakeMigration {
        id: String,
        /// Interior-mutable so tests can simulate filesystem drift between
        /// calls to `inspect()` — the first call returns `status`, then
        /// `status` is replaced with `status_drift_to` (if set).
        status: Mutex<Status>,
        status_drift_to: Mutex<Option<Status>>,
        requires_confirmation: bool,
        execute_result: Mutex<Result<(), MigrationError>>,
        execution_log: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeMigration {
        fn new(id: &str, status: Status, log: std::sync::Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                id: id.to_string(),
                status: Mutex::new(status),
                status_drift_to: Mutex::new(None),
                requires_confirmation: true,
                execute_result: Mutex::new(Ok(())),
                execution_log: log,
            }
        }

        fn silent(mut self) -> Self {
            self.requires_confirmation = false;
            self
        }

        fn with_execute_error(self, err: MigrationError) -> Self {
            *self.execute_result.lock().unwrap() = Err(err);
            self
        }

        /// After the *next* `inspect()` call returns, subsequent calls will
        /// return `new_status`. Simulates concurrent filesystem changes.
        fn drifting_to(self, new_status: Status) -> Self {
            *self.status_drift_to.lock().unwrap() = Some(new_status);
            self
        }
    }

    impl Migration for FakeMigration {
        fn id(&self) -> &str {
            &self.id
        }
        fn inspect(&self) -> Status {
            let current = self.status.lock().unwrap().clone();
            if let Some(next) = self.status_drift_to.lock().unwrap().take() {
                *self.status.lock().unwrap() = next;
            }
            current
        }
        fn execute(&self) -> Result<(), MigrationError> {
            self.execution_log.lock().unwrap().push(self.id.clone());
            let mut slot = self.execute_result.lock().unwrap();
            std::mem::replace(&mut *slot, Ok(()))
        }
        fn prompt_line(&self) -> String {
            format!("(fake) {}", self.id)
        }
        fn conflict_message(&self) -> String {
            format!("(fake conflict) {}", self.id)
        }
        fn requires_confirmation(&self) -> bool {
            self.requires_confirmation
        }
    }

    fn new_log() -> std::sync::Arc<Mutex<Vec<String>>> {
        std::sync::Arc::new(Mutex::new(Vec::new()))
    }

    #[test]
    fn empty_plan_is_nothing_to_do() {
        let migrations: Vec<Box<dyn Migration>> = vec![];
        let outcome = run(&migrations, |_| panic!("should not prompt"));
        assert!(matches!(outcome, Outcome::NothingToDo));
    }

    #[test]
    fn all_nothing_and_already_applied_is_nothing_to_do() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::NothingToDo, log.clone())),
            Box::new(FakeMigration::new("b", Status::AlreadyApplied, log.clone())),
        ];
        let outcome = run(&migrations, |_| panic!("should not prompt"));
        assert!(matches!(outcome, Outcome::NothingToDo));
        assert!(log.lock().unwrap().is_empty(), "execute must not be called");
    }

    #[test]
    fn one_pending_prompts_and_runs() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![Box::new(FakeMigration::new(
            "a",
            Status::Pending,
            log.clone(),
        ))];
        let outcome = run(&migrations, |_| true);
        match outcome {
            Outcome::Completed { applied } => assert_eq!(applied, vec!["a".to_string()]),
            other => panic!("expected Completed, got {other:?}"),
        }
        assert_eq!(*log.lock().unwrap(), vec!["a".to_string()]);
    }

    #[test]
    fn decline_prevents_execution() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![Box::new(FakeMigration::new(
            "a",
            Status::Pending,
            log.clone(),
        ))];
        let outcome = run(&migrations, |_| false);
        assert!(matches!(outcome, Outcome::Declined));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn one_conflict_blocks_plan_even_if_others_pending() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("b", Status::Conflict, log.clone())),
            Box::new(FakeMigration::new("c", Status::Pending, log.clone())),
        ];
        let outcome = run(&migrations, |_| panic!("should not prompt"));
        match outcome {
            Outcome::Blocked { conflicts } => assert_eq!(conflicts, vec!["b".to_string()]),
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(log.lock().unwrap().is_empty(), "execute must not be called");
    }

    #[test]
    fn mixed_pending_and_already_applied_runs_only_pending() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("b", Status::AlreadyApplied, log.clone())),
            Box::new(FakeMigration::new("c", Status::Pending, log.clone())),
        ];
        let outcome = run(&migrations, |_| true);
        match outcome {
            Outcome::Completed { applied } => {
                assert_eq!(applied, vec!["a".to_string(), "c".to_string()])
            }
            other => panic!("expected Completed, got {other:?}"),
        }
        assert_eq!(*log.lock().unwrap(), vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn execution_preserves_registration_order() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("first", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("second", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("third", Status::Pending, log.clone())),
        ];
        let _ = run(&migrations, |_| true);
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string()
            ]
        );
    }

    #[test]
    fn state_drift_from_pending_to_conflict_fails_loudly() {
        let log = new_log();
        // inspect() returns Pending on the first call (so initial inspect()
        // in run() classifies it as Pending and asks the user), then drifts
        // to Conflict before the execute-loop re-checks.
        let m =
            FakeMigration::new("racy", Status::Pending, log.clone()).drifting_to(Status::Conflict);
        let migrations: Vec<Box<dyn Migration>> = vec![Box::new(m)];
        let outcome = run(&migrations, |_| true);
        match outcome {
            Outcome::Failed {
                applied,
                failed_id,
                error:
                    MigrationError::StateChanged {
                        observed_before,
                        observed_now,
                        ..
                    },
            } => {
                assert!(applied.is_empty());
                assert_eq!(failed_id, "racy");
                assert_eq!(observed_before, Status::Pending);
                assert_eq!(observed_now, Status::Conflict);
            }
            other => panic!("expected Failed::StateChanged, got {other:?}"),
        }
        assert!(
            log.lock().unwrap().is_empty(),
            "execute must not have been called after drift"
        );
    }

    #[test]
    fn state_drift_to_already_applied_also_fails_does_not_silently_skip() {
        // Even the "benign" drift (someone else finished the migration for us)
        // surfaces as StateChanged — we committed to running it, so the
        // ground rules changing underneath us is a signal to stop and let
        // the user re-verify.
        let log = new_log();
        let m = FakeMigration::new("racy", Status::Pending, log.clone())
            .drifting_to(Status::AlreadyApplied);
        let migrations: Vec<Box<dyn Migration>> = vec![Box::new(m)];
        let outcome = run(&migrations, |_| true);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                error: MigrationError::StateChanged { .. },
                ..
            }
        ));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn state_drift_preserves_earlier_successes_in_applied_list() {
        let log = new_log();
        let stable = Box::new(FakeMigration::new("stable", Status::Pending, log.clone()))
            as Box<dyn Migration>;
        let racy = Box::new(
            FakeMigration::new("racy", Status::Pending, log.clone()).drifting_to(Status::Conflict),
        ) as Box<dyn Migration>;
        let migrations: Vec<Box<dyn Migration>> = vec![stable, racy];
        let outcome = run(&migrations, |_| true);
        match outcome {
            Outcome::Failed {
                applied, failed_id, ..
            } => {
                assert_eq!(applied, vec!["stable".to_string()]);
                assert_eq!(failed_id, "racy");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(*log.lock().unwrap(), vec!["stable".to_string()]);
    }

    #[test]
    fn execute_stops_on_first_failure_and_reports_applied_so_far() {
        let log = new_log();
        let err = MigrationError::VerifyMismatch {
            migration_id: "second".to_string(),
            path: PathBuf::from("/nope"),
            expected_len: 10,
            got_len: 0,
        };
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("first", Status::Pending, log.clone())),
            Box::new(
                FakeMigration::new("second", Status::Pending, log.clone()).with_execute_error(err),
            ),
            Box::new(FakeMigration::new("third", Status::Pending, log.clone())),
        ];
        let outcome = run(&migrations, |_| true);
        match outcome {
            Outcome::Failed {
                applied, failed_id, ..
            } => {
                assert_eq!(applied, vec!["first".to_string()]);
                assert_eq!(failed_id, "second");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        // `third` must not have been executed.
        assert_eq!(
            *log.lock().unwrap(),
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn all_silent_pending_runs_without_prompt() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::Pending, log.clone()).silent()),
            Box::new(FakeMigration::new("b", Status::Pending, log.clone()).silent()),
        ];
        let outcome = run(&migrations, |_| panic!("should not prompt"));
        match outcome {
            Outcome::Completed { applied } => {
                assert_eq!(applied, vec!["a".to_string(), "b".to_string()])
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn mixed_silent_and_confirm_prompts_once_and_runs_both() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("silent", Status::Pending, log.clone()).silent()),
            Box::new(FakeMigration::new("asking", Status::Pending, log.clone())),
        ];
        let prompt_count = std::sync::Arc::new(Mutex::new(0usize));
        let pc = prompt_count.clone();
        let outcome = run(&migrations, move |_| {
            *pc.lock().unwrap() += 1;
            true
        });
        assert_eq!(*prompt_count.lock().unwrap(), 1);
        match outcome {
            Outcome::Completed { applied } => {
                assert_eq!(applied, vec!["silent".to_string(), "asking".to_string()])
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn mixed_silent_and_confirm_declined_runs_nothing() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("silent", Status::Pending, log.clone()).silent()),
            Box::new(FakeMigration::new("asking", Status::Pending, log.clone())),
        ];
        let outcome = run(&migrations, |_| false);
        assert!(matches!(outcome, Outcome::Declined));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn format_prompt_lists_each_confirm_required_pending() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("b", Status::AlreadyApplied, log.clone())),
            Box::new(FakeMigration::new("c", Status::Pending, log.clone()).silent()),
            Box::new(FakeMigration::new("d", Status::Pending, log.clone())),
        ];
        let text = format_prompt(&migrations);
        assert!(text.contains("[a]"));
        assert!(text.contains("[d]"));
        assert!(!text.contains("[b]"), "already-applied should not appear");
        assert!(
            !text.contains("[c]"),
            "silent migrations should not be in the prompt"
        );
    }

    #[test]
    fn format_conflict_error_lists_each_blocker() {
        let log = new_log();
        let migrations: Vec<Box<dyn Migration>> = vec![
            Box::new(FakeMigration::new("a", Status::Pending, log.clone())),
            Box::new(FakeMigration::new("b", Status::Conflict, log.clone())),
            Box::new(FakeMigration::new("c", Status::Conflict, log.clone())),
        ];
        let text = format_conflict_error(&migrations);
        assert!(text.contains("[b]"));
        assert!(text.contains("[c]"));
        assert!(!text.contains("[a]"));
    }

    // ---- shared helper tests (atomic_write / read_bytes / remove_if_exists) ----

    #[test]
    fn atomic_write_writes_bytes_exactly() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("sub/out.bin");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        atomic_write("t", &path, b"hello world").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello world");
        // tmp sibling must not linger
        assert!(!tmp.path().join("sub/out.bin.tmp").exists());
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("out.bin");
        fs::write(&path, b"old").unwrap();
        atomic_write("t", &path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn read_bytes_returns_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("r.bin");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(read_bytes("t", &path).unwrap(), b"abc");
    }

    #[test]
    fn read_bytes_missing_is_io_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nope.bin");
        match read_bytes("t", &path) {
            Err(MigrationError::Io { step, .. }) => assert_eq!(step, "read"),
            other => panic!("expected Io(read), got {other:?}"),
        }
    }

    #[test]
    fn remove_if_exists_is_ok_when_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nope.bin");
        remove_if_exists("t", &path).unwrap();
    }

    #[test]
    fn remove_if_exists_deletes_when_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("r.bin");
        fs::write(&path, b"x").unwrap();
        remove_if_exists("t", &path).unwrap();
        assert!(!path.exists());
    }
}
