//! Documents given as `-` (stdin) or as a pipe path such as `<(curl …)`.
//!
//! Readers need a seekable file whose extension names the format (MuPDF
//! workers and the EPUB loader reopen the document by path), so the stream is
//! copied into the cache and named after a hash of its content. Piping the
//! same bytes again yields the same path, so the reading position, comments
//! and highlights saved for that path come back like for any other book.

use std::fs::{self, File, FileTimes};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use bookokrat::book_manager::BookManager;
use log::warn;

/// Enough for every signature `BookManager::sniff_format` looks at.
#[cfg_attr(not(unix), allow(dead_code))]
const SNIFF_LEN: u64 = 4096;
/// Copies not opened for this long are deleted.
#[cfg(unix)]
const MAX_UNUSED: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Beyond this total size, the least recently opened copies are deleted.
#[cfg(unix)]
const MAX_TOTAL_BYTES: u64 = 1 << 30;

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

/// Copy the stream named by `arg` into the piped documents cache and return the copy's path.
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
    let dir = bookokrat::library::piped_documents_dir()?;
    let path = copy_to_dir(reader, &dir)?;
    evict_copies(&dir, &path, SystemTime::now(), MAX_UNUSED, MAX_TOTAL_BYTES);
    Ok(path)
}

#[cfg(not(unix))]
pub fn materialize(_arg: &str) -> Result<PathBuf> {
    bail!("reading documents from stdin or pipes is not supported on this platform")
}

struct HashingWriter<W> {
    inner: W,
    hasher: md5::Context,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.consume(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Write `reader` to `dir/stdin-<hash>.<ext>`, reusing an identical earlier copy.
#[cfg_attr(not(unix), allow(dead_code))]
fn copy_to_dir(mut reader: impl Read, dir: &Path) -> Result<PathBuf> {
    let mut head = Vec::new();
    reader.by_ref().take(SNIFF_LEN).read_to_end(&mut head)?;
    if head.is_empty() {
        bail!("no data received");
    }
    let format =
        BookManager::sniff_format(&head).ok_or_else(|| anyhow!("unrecognized document format"))?;

    let partial = tempfile::Builder::new()
        .prefix(".partial-")
        .tempfile_in(dir)
        .with_context(|| format!("failed to create a file in {dir:?}"))?;
    let mut writer = HashingWriter {
        inner: partial,
        hasher: md5::Context::new(),
    };
    writer.write_all(&head)?;
    io::copy(&mut reader, &mut writer)?;
    let digest = format!("{:x}", writer.hasher.compute());

    let path = dir.join(format!("stdin-{}.{}", &digest[..16], format.extension()));
    // An existing copy has the same content and may be open in another session.
    if !path.exists() {
        writer
            .inner
            .persist(&path)
            .map_err(|e| e.error)
            .with_context(|| format!("failed to save {path:?}"))?;
    }
    if let Err(e) = mark_used(&path) {
        warn!("Failed to update access time of {path:?}: {e}");
    }
    Ok(path)
}

/// Record the open in the access time. The modified time has to keep meaning
/// "content written": the EPUB image cache is invalidated by a newer one.
#[cfg_attr(not(unix), allow(dead_code))]
fn mark_used(path: &Path) -> io::Result<()> {
    File::options()
        .write(true)
        .open(path)?
        .set_times(FileTimes::new().set_accessed(SystemTime::now()))
}

/// Delete copies (and leftover partial ones) not used within `max_unused`, then
/// the least recently used until the rest fit in `max_total` bytes. `keep`, the
/// copy being opened, is never deleted.
#[cfg_attr(not(unix), allow(dead_code))]
fn evict_copies(dir: &Path, keep: &Path, now: SystemTime, max_unused: Duration, max_total: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut total = fs::metadata(keep).map_or(0, |m| m.len());
    let mut others: Vec<(SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.path() != keep)
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(|m| m.is_file())?;
            let used = meta.accessed().ok().max(meta.modified().ok())?;
            Some((used, meta.len(), entry.path()))
        })
        .collect();
    others.sort_by(|a, b| b.0.cmp(&a.0));

    for (used, len, path) in others {
        let stale = now.duration_since(used).is_ok_and(|age| age > max_unused);
        if stale || total + len > max_total {
            if let Err(e) = fs::remove_file(&path) {
                warn!("Failed to remove piped document copy {path:?}: {e}");
            }
        } else {
            total += len;
        }
    }
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

    fn epub_bytes(seed: u32) -> Vec<u8> {
        let mut data = b"PK\x03\x04".to_vec();
        data.extend((0..20_000u32).map(|i| ((i + seed) % 251) as u8));
        data
    }

    fn file_count(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn names_copy_by_content_hash_and_format() {
        let dir = TempDir::new().unwrap();
        let data = epub_bytes(0);

        let path = copy_to_dir(Cursor::new(data.clone()), dir.path()).unwrap();

        let hash = format!("{:x}", md5::compute(&data));
        assert_eq!(path, dir.path().join(format!("stdin-{}.epub", &hash[..16])));
        assert_eq!(fs::read(&path).unwrap(), data);
        assert_eq!(file_count(dir.path()), 1, "no partial file left behind");
    }

    #[test]
    fn same_content_reuses_the_copy() {
        let dir = TempDir::new().unwrap();

        let first = copy_to_dir(Cursor::new(epub_bytes(0)), dir.path()).unwrap();
        let again = copy_to_dir(Cursor::new(epub_bytes(0)), dir.path()).unwrap();
        let other = copy_to_dir(Cursor::new(epub_bytes(1)), dir.path()).unwrap();

        assert_eq!(first, again);
        assert_ne!(first, other);
        assert_eq!(file_count(dir.path()), 2);
    }

    #[test]
    fn rejects_empty_and_unknown_input() {
        let dir = TempDir::new().unwrap();
        let empty = copy_to_dir(Cursor::new(Vec::new()), dir.path()).unwrap_err();
        assert_eq!(empty.to_string(), "no data received");
        let unknown = copy_to_dir(Cursor::new(b"hello".to_vec()), dir.path()).unwrap_err();
        assert_eq!(unknown.to_string(), "unrecognized document format");
        assert_eq!(file_count(dir.path()), 0);
    }

    #[test]
    fn evicts_unused_then_least_recently_used_copies() {
        let dir = TempDir::new().unwrap();
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let copy = |name: &str, used_days_ago: u32| {
            let path = dir.path().join(name);
            fs::write(&path, vec![0u8; 300]).unwrap();
            let used = now - day * used_days_ago;
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(FileTimes::new().set_accessed(used).set_modified(used))
                .unwrap();
            path
        };
        let stale = copy("stdin-stale.pdf", 40);
        let older = copy("stdin-older.pdf", 2);
        let recent = copy("stdin-recent.pdf", 1);
        let opened = copy("stdin-opened.pdf", 50);

        evict_copies(dir.path(), &opened, now, day * 30, 700);

        assert!(opened.exists(), "the copy being opened is kept");
        assert!(recent.exists());
        assert!(!older.exists(), "over the size limit");
        assert!(!stale.exists(), "unused for too long");
    }
}
