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

    // Once a copy doesn't fit, it and every older one go, even smaller ones
    // that would: keeping those would evict out of recency order.
    let mut over_limit = false;
    for (used, len, path) in others {
        let stale = now.duration_since(used).is_ok_and(|age| age > max_unused);
        over_limit = over_limit || (!stale && total + len > max_total);
        if stale || over_limit {
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
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
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

    /// Yields `data` until `fail_at` bytes, then fails like a dropped connection.
    struct FailingReader {
        data: Vec<u8>,
        pos: usize,
        fail_at: usize,
    }

    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos >= self.fail_at {
                return Err(io::Error::other("connection reset"));
            }
            let n = buf.len().min(self.fail_at - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[test]
    fn failed_read_leaves_nothing_behind() {
        let data = epub_bytes(0);
        let sniff = SNIFF_LEN as usize;
        for fail_at in [0, 100, sniff - 1, sniff, sniff + 1, data.len() - 1] {
            let dir = TempDir::new().unwrap();
            let reader = FailingReader {
                data: data.clone(),
                pos: 0,
                fail_at,
            };

            let err = copy_to_dir(reader, dir.path()).unwrap_err();

            assert!(
                format!("{err:#}").contains("connection reset"),
                "fail at {fail_at}: {err:#}"
            );
            assert_eq!(file_count(dir.path()), 0, "fail at {fail_at}");
        }
    }

    /// Hands out `data` a few bytes at a time, with spurious `Interrupted` errors.
    #[cfg(feature = "pdf")]
    struct ChoppyReader {
        data: Vec<u8>,
        pos: usize,
        rng: StdRng,
    }

    #[cfg(feature = "pdf")]
    impl Read for ChoppyReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.rng.gen_bool(0.2) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = self
                .rng
                .gen_range(1..=7)
                .min(buf.len())
                .min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn choppy_reads_produce_the_same_copy() {
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut data = b"%PDF-1.7\n".to_vec();
            data.extend((0..rng.gen_range(0..20_000)).map(|_| rng.gen_range(0..=255u8)));
            let whole_dir = TempDir::new().unwrap();
            let choppy_dir = TempDir::new().unwrap();

            let whole = copy_to_dir(Cursor::new(data.clone()), whole_dir.path()).unwrap();
            let choppy = ChoppyReader {
                data: data.clone(),
                pos: 0,
                rng,
            };
            let pieces = copy_to_dir(choppy, choppy_dir.path()).unwrap();

            assert_eq!(whole.file_name(), pieces.file_name(), "seed {seed}");
            assert_eq!(fs::read(&pieces).unwrap(), data, "seed {seed}");
        }
    }

    #[test]
    fn unwritable_cache_dir_is_an_error() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing");

        let err = copy_to_dir(Cursor::new(epub_bytes(0)), &missing).unwrap_err();

        assert!(
            format!("{err:#}").contains("failed to create a file in"),
            "{err:#}"
        );
        assert!(!missing.exists());
    }

    /// Accepts at most 3 bytes per write call.
    struct TrickleWriter(Vec<u8>);

    impl Write for TrickleWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = buf.len().min(3);
            self.0.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn hash_covers_exactly_the_bytes_written() {
        let data = epub_bytes(7);
        let mut writer = HashingWriter {
            inner: TrickleWriter(Vec::new()),
            hasher: md5::Context::new(),
        };

        io::copy(&mut Cursor::new(&data), &mut writer).unwrap();

        assert_eq!(writer.inner.0, data);
        assert_eq!(
            format!("{:x}", writer.hasher.compute()),
            format!("{:x}", md5::compute(&data))
        );
    }

    fn set_used(path: &Path, used: SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(FileTimes::new().set_accessed(used).set_modified(used))
            .unwrap();
    }

    fn last_used(path: &Path) -> SystemTime {
        let meta = fs::metadata(path).unwrap();
        meta.accessed().unwrap().max(meta.modified().unwrap())
    }

    #[test]
    fn eviction_keeps_the_most_recently_used_copies_that_fit() {
        let now = SystemTime::now();
        let max_unused = Duration::from_secs(30 * 24 * 60 * 60);
        let is_stale =
            |used: SystemTime| now.duration_since(used).is_ok_and(|age| age > max_unused);

        for seed in 0..300 {
            let mut rng = StdRng::seed_from_u64(seed);
            let dir = TempDir::new().unwrap();
            let count = rng.gen_range(1..12);
            let copies: Vec<(PathBuf, SystemTime, u64)> = (0..count)
                .map(|i| {
                    let path = dir.path().join(format!("stdin-{i}.pdf"));
                    fs::write(&path, vec![0u8; rng.gen_range(0..1000)]).unwrap();
                    let age = Duration::from_secs(rng.gen_range(0..60 * 24 * 60 * 60));
                    set_used(&path, now - age);
                    let len = fs::metadata(&path).unwrap().len();
                    (path.clone(), last_used(&path), len)
                })
                .collect();
            let (keep, _, keep_len) = copies[rng.gen_range(0..count)].clone();
            let max_total = rng.gen_range(0..4000);

            evict_copies(dir.path(), &keep, now, max_unused, max_total);

            let ctx = format!("seed {seed}, limit {max_total}");
            assert!(keep.exists(), "{ctx}: deleted the copy being opened");
            let (kept, deleted): (Vec<_>, Vec<_>) = copies
                .iter()
                .filter(|(path, ..)| *path != keep)
                .partition(|(path, ..)| path.exists());
            let kept_total = keep_len + kept.iter().map(|(.., len)| len).sum::<u64>();
            assert!(
                kept.is_empty() || kept_total <= max_total,
                "{ctx}: kept {kept_total} bytes"
            );
            assert!(
                kept.iter().all(|(_, used, _)| !is_stale(*used)),
                "{ctx}: kept a copy unused for too long"
            );
            let deleted_for_size: Vec<_> = deleted
                .iter()
                .filter(|(_, used, _)| !is_stale(*used))
                .collect();
            for (kept_path, kept_used, _) in &kept {
                for (deleted_path, deleted_used, _) in &deleted_for_size {
                    assert!(
                        kept_used >= deleted_used,
                        "{ctx}: kept {kept_path:?} but deleted the more recently used {deleted_path:?}"
                    );
                }
            }
            if let Some((_, _, len)) = deleted_for_size.iter().max_by_key(|(_, used, _)| *used) {
                assert!(
                    kept_total + len > max_total,
                    "{ctx}: deleted a copy that still fit"
                );
            }
        }
    }
}
