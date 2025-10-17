use std::process::Command;

/// Trait for executing system commands (mockable for testing)
pub trait SystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String>;
    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String>;
    fn open_url(&self, url: &str) -> Result<(), String>;
    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct RealSystemCommandExecutor;

impl SystemCommandExecutor for RealSystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String> {
        self.open_file_at_chapter(path, 0)
    }

    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String> {
        use std::path::PathBuf;

        let absolute_path = if std::path::Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {e}"))?
                .join(path)
        };

        if !absolute_path.exists() {
            return Err(format!("File does not exist: {}", absolute_path.display()));
        }

        let absolute_path_str = absolute_path.to_string_lossy();

        let result = if cfg!(target_os = "macos") {
            self.open_with_macos_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| Command::new("open").arg(absolute_path_str.as_ref()).spawn())
        } else if cfg!(target_os = "windows") {
            self.open_with_windows_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| {
                    Command::new("cmd")
                        .args(["/C", "start", "", absolute_path_str.as_ref()])
                        .spawn()
                })
        } else {
            self.open_with_linux_epub_reader(absolute_path_str.as_ref(), chapter)
                .or_else(|_| {
                    Command::new("xdg-open")
                        .arg(absolute_path_str.as_ref())
                        .spawn()
                })
        };

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "Failed to open file '{}': {}",
                absolute_path.display(),
                e
            )),
        }
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        let (command, args) = if cfg!(target_os = "macos") {
            ("open", vec![url])
        } else if cfg!(target_os = "windows") {
            ("cmd", vec!["/C", "start", "", url])
        } else {
            // Linux and other Unix-like systems
            ("xdg-open", vec![url])
        };

        Command::new(command)
            .args(&args)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {e}"))?;

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl RealSystemCommandExecutor {
    fn open_with_macos_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }
        if let Ok(child) = self.try_clearview(path, chapter) {
            return Ok(child);
        }
        if let Ok(child) = self.try_skim(path, chapter) {
            return Ok(child);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    fn open_with_windows_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    fn open_with_linux_epub_reader(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        if let Ok(child) = self.try_calibre_viewer(path, chapter) {
            return Ok(child);
        }
        if let Ok(child) = self.try_fbreader(path, chapter) {
            return Ok(child);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No compatible EPUB reader found",
        ))
    }

    fn try_clearview(
        &self,
        path: &str,
        _chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        Command::new("open").args(["-a", "ClearView", path]).spawn()
    }

    fn try_calibre_viewer(
        &self,
        path: &str,
        chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        if chapter > 0 {
            let chapter_patterns = [
                format!("toc:Chapter {}", chapter + 1),
                format!("toc:Ch {}", chapter + 1),
                format!("toc:{}", chapter + 1),
                format!("toc:Chapter{}", chapter + 1),
            ];

            for pattern in &chapter_patterns {
                if let Ok(child) = Command::new("ebook-viewer")
                    .arg(format!("--goto={pattern}"))
                    .arg(path)
                    .spawn()
                {
                    return Ok(child);
                }
            }
        }
        Command::new("ebook-viewer").arg(path).spawn()
    }

    fn try_skim(&self, path: &str, _chapter: usize) -> Result<std::process::Child, std::io::Error> {
        Command::new("open").args(["-a", "Skim", path]).spawn()
    }

    fn try_fbreader(
        &self,
        path: &str,
        _chapter: usize,
    ) -> Result<std::process::Child, std::io::Error> {
        Command::new("fbreader").arg(path).spawn()
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub struct MockSystemCommandExecutor {
    pub executed_commands: std::cell::RefCell<Vec<String>>,
    pub should_fail: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for MockSystemCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl MockSystemCommandExecutor {
    pub fn new() -> Self {
        Self {
            executed_commands: std::cell::RefCell::new(Vec::new()),
            should_fail: false,
        }
    }

    pub fn new_with_failure() -> Self {
        Self {
            executed_commands: std::cell::RefCell::new(Vec::new()),
            should_fail: true,
        }
    }

    pub fn get_executed_commands(&self) -> Vec<String> {
        self.executed_commands.borrow().clone()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl SystemCommandExecutor for MockSystemCommandExecutor {
    fn open_file(&self, path: &str) -> Result<(), String> {
        self.executed_commands.borrow_mut().push(path.to_string());
        if self.should_fail {
            Err("Mock failure".to_string())
        } else {
            Ok(())
        }
    }

    fn open_file_at_chapter(&self, path: &str, chapter: usize) -> Result<(), String> {
        self.executed_commands
            .borrow_mut()
            .push(format!("{path}@chapter{chapter}"));
        if self.should_fail {
            Err("Mock failure".to_string())
        } else {
            Ok(())
        }
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        self.executed_commands
            .borrow_mut()
            .push(format!("URL:{url}"));
        if self.should_fail {
            Err("Mock failure".to_string())
        } else {
            Ok(())
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
