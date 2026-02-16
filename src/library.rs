use anyhow::{Context, Result};
use log::warn;
use std::fs;
use std::path::{Path, PathBuf};

pub struct LibraryPaths {
    pub bookmarks_file: PathBuf,
    pub comments_dir: PathBuf,
    pub image_cache_dir: PathBuf,
}

/// Compute a slug that uniquely identifies a library directory.
/// Format: `<md5_first_12>_<slugified_last_2_path_components>`
pub fn library_slug(abs_path: &Path) -> String {
    let path_str = abs_path.to_string_lossy();
    let digest = md5::compute(path_str.as_bytes());
    let hash_prefix = &format!("{digest:x}")[..12];

    let components: Vec<&str> = abs_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    let slug_parts: Vec<&str> = if components.len() >= 2 {
        components[components.len() - 2..].to_vec()
    } else if components.len() == 1 {
        vec![components[0]]
    } else {
        vec!["root"]
    };

    let slugified: Vec<String> = slug_parts
        .iter()
        .map(|s| {
            s.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .to_lowercase()
        })
        .collect();

    format!("{hash_prefix}_{}", slugified.join("_"))
}

/// Compute XDG-compliant paths for a library directory.
/// Creates the directories if they don't exist.
pub fn resolve_library_paths(library_dir: &Path) -> Result<LibraryPaths> {
    let abs_path = if library_dir.is_absolute() {
        library_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .context("Failed to get current directory")?
            .join(library_dir)
    };

    let slug = library_slug(&abs_path);

    let data_dir = dirs::data_dir()
        .context("Could not determine data directory")?
        .join("bookokrat")
        .join("libraries")
        .join(&slug);

    let cache_dir = dirs::cache_dir()
        .context("Could not determine cache directory")?
        .join("bookokrat")
        .join("libraries")
        .join(&slug);

    let bookmarks_file = data_dir.join("bookmarks.json");
    let comments_dir = data_dir.join("comments");
    let image_cache_dir = cache_dir.join("temp_images");

    fs::create_dir_all(&data_dir)
        .with_context(|| format!("Failed to create data directory: {data_dir:?}"))?;
    fs::create_dir_all(&comments_dir)
        .with_context(|| format!("Failed to create comments directory: {comments_dir:?}"))?;
    fs::create_dir_all(&image_cache_dir)
        .with_context(|| format!("Failed to create image cache directory: {image_cache_dir:?}"))?;

    Ok(LibraryPaths {
        bookmarks_file,
        comments_dir,
        image_cache_dir,
    })
}

/// Compute the XDG-compliant log file path (not per-library).
/// Uses `state_dir` on platforms that have it, falls back to `cache_dir`.
pub fn resolve_log_path() -> Result<PathBuf> {
    let base = dirs::state_dir()
        .or_else(dirs::cache_dir)
        .context("Could not determine state or cache directory")?;

    let log_dir = base.join("bookokrat");
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {log_dir:?}"))?;

    Ok(log_dir.join("bookokrat.log"))
}

/// Migrate old CWD-local files to XDG locations.
/// Non-fatal: prints progress to stdout, returns Ok even on partial failure.
pub fn migrate_if_needed(library_dir: &Path, paths: &LibraryPaths) -> Result<()> {
    let old_bookmarks = library_dir.join("bookmarks.json");
    let old_comments = library_dir.join(".bookokrat_comments");
    let old_images = library_dir.join("temp_images");
    let old_log = library_dir.join("bookokrat.log");

    if old_bookmarks.exists() && !paths.bookmarks_file.exists() {
        println!(
            "Migrating bookmarks.json → {}",
            paths.bookmarks_file.display()
        );
        if let Err(e) = move_path(&old_bookmarks, &paths.bookmarks_file) {
            warn!("Failed to migrate bookmarks: {e}");
        }
    }

    if old_comments.is_dir() {
        println!(
            "Migrating .bookokrat_comments/ → {}",
            paths.comments_dir.display()
        );
        if let Err(e) = merge_directory(&old_comments, &paths.comments_dir) {
            warn!("Failed to migrate comments: {e}");
        } else if let Err(e) = fs::remove_dir_all(&old_comments) {
            warn!("Failed to remove old comments directory: {e}");
        }
    }

    if old_images.is_dir() {
        println!(
            "Migrating temp_images/ → {}",
            paths.image_cache_dir.display()
        );
        if let Err(e) = merge_directory(&old_images, &paths.image_cache_dir) {
            warn!("Failed to migrate images: {e}");
        } else if let Err(e) = fs::remove_dir_all(&old_images) {
            warn!("Failed to remove old images directory: {e}");
        }
    }

    if old_log.exists() {
        if let Err(e) = fs::remove_file(&old_log) {
            warn!("Failed to remove old log file: {e}");
        }
    }

    Ok(())
}

/// Move a single file, falling back to copy+delete for cross-device moves.
fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(from, to)?;
            fs::remove_file(from)?;
            Ok(())
        }
    }
}

/// Merge source directory contents into target, moving individual files.
/// Skips files that already exist in target.
fn merge_directory(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if src_path.is_dir() {
            merge_directory(&src_path, &dst_path)?;
        } else if !dst_path.exists() {
            move_path(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_library_slug_format() {
        let slug = library_slug(Path::new("/Users/rafael/books/fiction"));
        assert!(slug.starts_with(&slug[..12]));
        assert!(slug.contains("books_fiction"));
    }

    #[test]
    fn test_library_slug_single_component() {
        let slug = library_slug(Path::new("/books"));
        assert!(slug.contains("books"));
    }

    #[test]
    fn test_library_slug_deterministic() {
        let a = library_slug(Path::new("/a/b/c"));
        let b = library_slug(Path::new("/a/b/c"));
        assert_eq!(a, b);
    }

    #[test]
    fn test_library_slug_different_paths() {
        let a = library_slug(Path::new("/a/b/c"));
        let b = library_slug(Path::new("/x/y/z"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_resolve_library_paths_creates_dirs() {
        let tmp = TempDir::new().unwrap();
        let paths = resolve_library_paths(tmp.path()).unwrap();
        assert!(paths.bookmarks_file.parent().unwrap().exists());
        assert!(paths.comments_dir.exists());
        assert!(paths.image_cache_dir.exists());
    }

    #[test]
    fn test_resolve_log_path() {
        let log_path = resolve_log_path().unwrap();
        assert!(log_path.ends_with("bookokrat.log"));
        assert!(log_path.parent().unwrap().exists());
    }

    #[test]
    fn test_migrate_bookmarks() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        let old_bm = src.path().join("bookmarks.json");
        fs::write(&old_bm, r#"{"books":{}}"#).unwrap();

        let paths = LibraryPaths {
            bookmarks_file: dst.path().join("bookmarks.json"),
            comments_dir: dst.path().join("comments"),
            image_cache_dir: dst.path().join("temp_images"),
        };
        fs::create_dir_all(&paths.comments_dir).unwrap();
        fs::create_dir_all(&paths.image_cache_dir).unwrap();

        migrate_if_needed(src.path(), &paths).unwrap();

        assert!(!old_bm.exists());
        assert!(paths.bookmarks_file.exists());
    }

    #[test]
    fn test_migrate_comments_merge() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        let old_comments = src.path().join(".bookokrat_comments");
        fs::create_dir_all(&old_comments).unwrap();
        fs::write(old_comments.join("book_abc.yaml"), "comments").unwrap();

        let comments_dir = dst.path().join("comments");
        fs::create_dir_all(&comments_dir).unwrap();
        fs::write(comments_dir.join("book_existing.yaml"), "existing").unwrap();

        let paths = LibraryPaths {
            bookmarks_file: dst.path().join("bookmarks.json"),
            comments_dir: comments_dir.clone(),
            image_cache_dir: dst.path().join("temp_images"),
        };
        fs::create_dir_all(&paths.image_cache_dir).unwrap();

        migrate_if_needed(src.path(), &paths).unwrap();

        assert!(!old_comments.exists());
        assert!(comments_dir.join("book_abc.yaml").exists());
        assert!(comments_dir.join("book_existing.yaml").exists());
    }

    #[test]
    fn test_migrate_skips_existing_target() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        let old_bm = src.path().join("bookmarks.json");
        fs::write(&old_bm, r#"{"old": true}"#).unwrap();

        let paths = LibraryPaths {
            bookmarks_file: dst.path().join("bookmarks.json"),
            comments_dir: dst.path().join("comments"),
            image_cache_dir: dst.path().join("temp_images"),
        };
        fs::create_dir_all(&paths.comments_dir).unwrap();
        fs::create_dir_all(&paths.image_cache_dir).unwrap();
        fs::write(&paths.bookmarks_file, r#"{"new": true}"#).unwrap();

        migrate_if_needed(src.path(), &paths).unwrap();

        // Old file should remain (target already exists)
        assert!(old_bm.exists());
        let content = fs::read_to_string(&paths.bookmarks_file).unwrap();
        assert!(content.contains("new"));
    }
}
