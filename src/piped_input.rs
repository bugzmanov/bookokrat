//! Documents given as `-` (stdin) or as a pipe path such as `<(curl …)`.
//!
//! Readers need a seekable file whose extension names the format (MuPDF
//! workers and the EPUB loader all reopen the document by path), so the
//! stream is copied into a private temp directory that is removed at exit.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use bookokrat::book_manager::BookManager;

/// Enough for every signature `BookManager::sniff_format` looks at.
#[cfg_attr(not(unix), allow(dead_code))]
const SNIFF_LEN: u64 = 4096;

#[cfg(unix)]
static TEMP_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// True for `-` and for paths that are pipes rather than regular files.
pub fn is_stream_arg(arg: &str) -> bool {
    if arg == "-" {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(arg).is_ok_and(|m| m.file_type().is_fifo())
    }
    #[cfg(not(unix))]
    false
}

pub fn describe(arg: &str) -> &str {
    if arg == "-" { "stdin" } else { arg }
}

/// Directory holding the copied document, once `materialize` has run.
pub fn temp_dir() -> Option<&'static Path> {
    #[cfg(unix)]
    {
        TEMP_DIR.get().map(PathBuf::as_path)
    }
    #[cfg(not(unix))]
    None
}

/// Copy the stream named by `arg` into the temp directory and return the copy's path.
#[cfg(unix)]
pub fn materialize(arg: &str) -> Result<PathBuf> {
    use std::io::IsTerminal;

    let reader: Box<dyn Read> = if arg == "-" {
        if io::stdin().is_terminal() {
            bail!("nothing was piped to stdin");
        }
        Box::new(io::stdin().lock())
    } else {
        Box::new(File::open(arg)?)
    };
    copy_to_dir(reader, create_temp_dir()?)
}

#[cfg(not(unix))]
pub fn materialize(_arg: &str) -> Result<PathBuf> {
    bail!("reading documents from stdin or pipes is not supported on this platform")
}

#[cfg(unix)]
fn create_temp_dir() -> Result<&'static Path> {
    extern "C" fn remove_temp_dir() {
        if let Some(dir) = TEMP_DIR.get() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    if let Some(dir) = TEMP_DIR.get() {
        return Ok(dir);
    }
    let tmp = std::env::temp_dir();
    remove_orphaned_temp_dirs(&tmp);
    let dir = tempfile::Builder::new()
        .prefix(&format!("{TEMP_DIR_PREFIX}{}-", std::process::id()))
        .tempdir_in(&tmp)
        .context("failed to create temp directory")?
        .keep();
    let dir = TEMP_DIR.get_or_init(|| dir);
    // atexit also covers the many `std::process::exit` paths, which skip destructors.
    // SAFETY: registering a plain extern "C" fn with no captured state.
    unsafe {
        libc::atexit(remove_temp_dir);
    }
    Ok(dir)
}

#[cfg(unix)]
const TEMP_DIR_PREFIX: &str = "bookokrat-stdin-";

/// Remove copies left by sessions killed by a signal (e.g. a closed terminal
/// window), which never reach atexit. Directory names carry the owner's pid.
#[cfg(unix)]
fn remove_orphaned_temp_dirs(tmp: &Path) {
    let Ok(entries) = std::fs::read_dir(tmp) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|n| n.strip_prefix(TEMP_DIR_PREFIX))
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<libc::pid_t>().ok());
        if pid.is_some_and(|pid| !process_exists(pid)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(unix)]
fn process_exists(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 performs only the existence and permission check.
    let sent = unsafe { libc::kill(pid, 0) } == 0;
    sent || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg_attr(not(unix), allow(dead_code))]
fn copy_to_dir(mut reader: impl Read, dir: &Path) -> Result<PathBuf> {
    let mut head = Vec::new();
    reader.by_ref().take(SNIFF_LEN).read_to_end(&mut head)?;
    if head.is_empty() {
        bail!("no data received");
    }
    let format =
        BookManager::sniff_format(&head).ok_or_else(|| anyhow!("unrecognized document format"))?;

    let path = dir.join(format!("stdin.{}", format.extension()));
    let mut file = File::create(&path).with_context(|| format!("failed to create {path:?}"))?;
    file.write_all(&head)?;
    io::copy(&mut reader, &mut file)?;
    Ok(path)
}

/// Point fd 0 back at the terminal when it is a pipe or a file.
///
/// Key input and terminal query replies (graphics probes, the PDF event
/// loop) are read straight from fd 0, not just through crossterm.
#[cfg(unix)]
pub fn reattach_stdin_to_tty() -> Result<()> {
    use std::io::IsTerminal;

    if io::stdin().is_terminal() {
        return Ok(());
    }
    // Prefer the terminal's own device over /dev/tty: macOS can't poll()
    // /dev/tty, which would leave the event loop spinning with no input.
    let path = terminal_device_path().unwrap_or_else(|| PathBuf::from("/dev/tty"));
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("stdin is not a terminal and {path:?} cannot be opened"))?;
    rustix::stdio::dup2_stdin(&tty)
        .with_context(|| format!("failed to attach {path:?} to stdin"))?;
    Ok(())
}

#[cfg(unix)]
fn terminal_device_path() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    [libc::STDOUT_FILENO, libc::STDERR_FILENO]
        .into_iter()
        .find_map(|fd| {
            let mut buf = [0 as libc::c_char; 256];
            // SAFETY: buf is writable for its full length; on success ttyname_r
            // stores a NUL-terminated path in it.
            if unsafe { libc::ttyname_r(fd, buf.as_mut_ptr(), buf.len()) } != 0 {
                return None;
            }
            let name = unsafe { CStr::from_ptr(buf.as_ptr()) };
            Some(PathBuf::from(OsStr::from_bytes(name.to_bytes())))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::TempDir;

    #[test]
    fn copies_whole_stream_and_names_file_by_format() {
        let dir = TempDir::new().unwrap();
        let mut data = b"PK\x03\x04".to_vec();
        data.extend((0..20_000u32).map(|i| (i % 251) as u8));

        let path = copy_to_dir(Cursor::new(data.clone()), dir.path()).unwrap();

        assert_eq!(path, dir.path().join("stdin.epub"));
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[test]
    fn rejects_empty_and_unknown_input() {
        let dir = TempDir::new().unwrap();
        let empty = copy_to_dir(Cursor::new(Vec::new()), dir.path()).unwrap_err();
        assert_eq!(empty.to_string(), "no data received");
        let unknown = copy_to_dir(Cursor::new(b"hello".to_vec()), dir.path()).unwrap_err();
        assert_eq!(unknown.to_string(), "unrecognized document format");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn removes_only_temp_dirs_of_dead_processes() {
        let tmp = TempDir::new().unwrap();
        // Above every platform's pid limit, so never a live process.
        let dead = tmp.path().join(format!("{TEMP_DIR_PREFIX}99999999-aB3x"));
        let alive = tmp
            .path()
            .join(format!("{TEMP_DIR_PREFIX}{}-Qz9k", std::process::id()));
        let unrelated = tmp.path().join(format!("{TEMP_DIR_PREFIX}notapid"));
        for dir in [&dead, &alive, &unrelated] {
            std::fs::create_dir(dir).unwrap();
            std::fs::write(dir.join("stdin.pdf"), b"%PDF-").unwrap();
        }

        remove_orphaned_temp_dirs(tmp.path());

        assert!(!dead.exists());
        assert!(alive.exists());
        assert!(unrelated.exists());
    }
}
