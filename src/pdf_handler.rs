use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use oxidize_pdf::parser::{PdfDocument as OxidizePdfDocument, PdfReader};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::{any::Any, panic::AssertUnwindSafe, process::Command};

#[cfg(unix)]
fn suppress_stderr<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        let null_fd = libc::open(b"/dev/null\0".as_ptr() as *const i8, libc::O_WRONLY);
        if null_fd < 0 {
            return f();
        }
        let old_stderr = libc::dup(libc::STDERR_FILENO);
        if old_stderr < 0 {
            libc::close(null_fd);
            return f();
        }

        struct StderrGuard(i32);
        impl Drop for StderrGuard {
            fn drop(&mut self) {
                unsafe {
                    libc::dup2(self.0, libc::STDERR_FILENO);
                    libc::close(self.0);
                }
            }
        }

        libc::dup2(null_fd, libc::STDERR_FILENO);
        libc::close(null_fd);
        let guard = StderrGuard(old_stderr);

        let result = std::panic::catch_unwind(AssertUnwindSafe(f));
        drop(guard);

        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
}

#[cfg(not(unix))]
fn suppress_stderr<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}

static PDF_PROGRESS_CALLBACK: OnceLock<Arc<Mutex<Option<Box<dyn Fn(String, u16) + Send>>>>> =
    OnceLock::new();

fn get_callback_arc() -> Arc<Mutex<Option<Box<dyn Fn(String, u16) + Send>>>> {
    PDF_PROGRESS_CALLBACK
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

pub fn set_pdf_progress_callback<F: Fn(String, u16) + Send + 'static>(callback: F) {
    let callbacks = get_callback_arc();
    if let Ok(mut cbs) = callbacks.lock() {
        *cbs = Some(Box::new(callback));
    }
}

pub fn clear_pdf_progress_callback() {
    let callbacks = get_callback_arc();
    if let Ok(mut cbs) = callbacks.lock() {
        *cbs = None;
    }
}

fn emit_pdf_progress(message: &str, progress: u16) {
    let callbacks = get_callback_arc();
    if let Ok(cbs) = callbacks.lock() {
        if let Some(ref cb) = *cbs {
            cb(message.to_string(), progress);
        }
    }
}

fn describe_panic(payload: Box<dyn Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        msg.to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

pub struct PdfDocument {
    page_count: usize,
    file_size: u64,
    path: String,
}

pub type ProgressCallback = Box<dyn Fn(&str) + Send>;

impl PdfDocument {
    pub fn load(path: &str) -> Result<Self> {
        Self::load_with_progress(path, Box::new(|_| {}))
    }

    pub fn load_with_progress(path: &str, _progress: ProgressCallback) -> Result<Self> {
        info!("Loading PDF from path: {path}");

        emit_pdf_progress("Reading PDF metadata...", 10);
        std::thread::sleep(std::time::Duration::from_millis(150));

        let metadata = std::fs::metadata(path).context("Failed to read PDF file metadata")?;
        let file_size = metadata.len();

        emit_pdf_progress("Parsing PDF structure...", 30);
        std::thread::sleep(std::time::Duration::from_millis(150));

        let page_count = crate::panic_handler::with_panic_exit_suppressed(|| {
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                suppress_stderr(|| Self::get_page_count(path))
            }))
        });
        match page_count {
            Ok(Ok(page_count)) => {
                emit_pdf_progress(&format!("Found {page_count} pages"), 60);
                std::thread::sleep(std::time::Duration::from_millis(150));
                info!(
                    "PdfDocument::load succeeded for {path}: pages={page_count}, size={file_size} bytes"
                );
                Ok(PdfDocument {
                    page_count,
                    file_size,
                    path: path.to_string(),
                })
            }
            Ok(Err(e)) => {
                warn!("Could not read PDF page count for {path}: {e}. Using default fallback.");
                emit_pdf_progress("Could not determine page count, using default", 60);
                std::thread::sleep(std::time::Duration::from_millis(150));
                Ok(PdfDocument {
                    page_count: 1,
                    file_size,
                    path: path.to_string(),
                })
            }
            Err(payload) => {
                let message = describe_panic(payload);
                error!("PdfDocument::load panicked while counting pages for {path}: {message}");
                emit_pdf_progress("Could not determine page count due to parser error", 60);
                std::thread::sleep(std::time::Duration::from_millis(150));
                Ok(PdfDocument {
                    page_count: 1,
                    file_size,
                    path: path.to_string(),
                })
            }
        }
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Extract text content from PDF using oxidize-pdf with CJK support and panic protection
    pub fn extract_text(&self) -> Result<String> {
        self.extract_text_with_progress(Box::new(|_| {}))
    }

    pub fn extract_text_with_progress(&self, _progress: ProgressCallback) -> Result<String> {
        let path_owned = self.path.clone();

        emit_pdf_progress("Extracting text from PDF...", 70);
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Skip subprocess extraction - it's unreliable and often hangs
        // Go directly to inline text extraction which is more straightforward
        // (Subprocess extraction was causing 70% stall)
        debug!("Skipping subprocess extraction, using inline method directly");

        // Fallback to inline processing
        let handle = std::thread::spawn(move || {
            crate::panic_handler::with_panic_exit_suppressed(|| {
                suppress_stderr(|| {
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        match PdfReader::open(&path_owned) {
                            Ok(reader) => {
                                let pdf_doc = OxidizePdfDocument::new(reader);

                                match pdf_doc.extract_text() {
                                    Ok(text_pages) => {
                                        let mut full_text = String::new();

                                        for page in text_pages.iter() {
                                            if !page.text.is_empty() {
                                                full_text.push_str(&page.text);
                                                full_text.push_str("\n--- Page Break ---\n");
                                            }
                                        }

                                        if full_text.is_empty() {
                                            None
                                        } else {
                                            Some(full_text)
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to extract text from PDF: {}", e);
                                        None
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to open PDF file: {}", e);
                                None
                            }
                        }
                    }))
                })
            })
        });

        emit_pdf_progress("Processing extracted content...", 85);
        std::thread::sleep(std::time::Duration::from_millis(150));

        match handle.join() {
            Ok(Ok(Some(text))) => {
                emit_pdf_progress("PDF loaded successfully", 100);
                std::thread::sleep(std::time::Duration::from_millis(150));
                let char_count = text.chars().count();
                info!(
                    "PdfDocument::extract_text succeeded for {} with {} bytes ({} chars)",
                    self.path,
                    text.len(),
                    char_count
                );
                Ok(text)
            }
            Ok(Ok(None)) => {
                emit_pdf_progress("PDF loaded (no text content)", 100);
                std::thread::sleep(std::time::Duration::from_millis(150));
                warn!(
                    "PdfDocument::extract_text returned no text for {}. Falling back to summary message.",
                    self.path
                );
                self.get_fallback_message()
            }
            Ok(Err(payload)) => {
                let message = describe_panic(payload);
                error!(
                    "PDF text extraction panicked for {} with message: {}",
                    self.path, message
                );
                emit_pdf_progress("PDF parser crashed during extraction", 100);
                std::thread::sleep(std::time::Duration::from_millis(150));
                self.get_fallback_message()
            }
            Err(_) => {
                error!("PDF text extraction thread panicked (possible stack overflow)");
                emit_pdf_progress("PDF loaded with errors", 100);
                std::thread::sleep(std::time::Duration::from_millis(150));
                self.get_fallback_message()
            }
        }
    }

    /// Extract text using subprocess (safer from SIGABRT crashes)
    fn extract_text_subprocess(&self) -> Result<String> {
        use std::process::Stdio as StdioType;

        let mut child = Command::new(std::env::current_exe()?)
            .arg("--debug-pdf")
            .arg(&self.path)
            .stdout(StdioType::piped())
            .stderr(StdioType::piped())
            .spawn()
            .context("Failed to spawn PDF parser subprocess")?;

        // Wait with a 120-second timeout for PDF text extraction
        let timeout = std::time::Duration::from_secs(120);
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process finished
                    if !status.success() {
                        return Err(anyhow::anyhow!(
                            "PDF parser subprocess failed with status: {:?}",
                            status
                        ));
                    }

                    let output = child
                        .wait_with_output()
                        .context("Failed to read subprocess output")?;
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    // Extract text from the preview section of the debug output
                    let mut text_content = String::new();
                    let mut in_preview = false;

                    for line in stdout.lines() {
                        if line.contains("--- Text preview") {
                            in_preview = true;
                            continue;
                        }
                        if line.contains("--- end preview ---") {
                            break;
                        }
                        if in_preview && !line.is_empty() {
                            text_content.push_str(line);
                            text_content.push('\n');
                        }
                    }

                    if !text_content.is_empty() {
                        return Ok(text_content);
                    } else {
                        return Err(anyhow::anyhow!("No text extracted from subprocess"));
                    }
                }
                Ok(None) => {
                    // Process still running
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        error!(
                            "PDF text extraction subprocess timed out (120s) for: {}",
                            self.path
                        );
                        return Err(anyhow::anyhow!(
                            "PDF text extraction timeout (possible infinite loop or corrupted structure)"
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to check subprocess status: {}", e));
                }
            }
        }
    }

    fn get_fallback_message(&self) -> Result<String> {
        let size_mb = self.file_size as f64 / (1024.0 * 1024.0);
        let filename = Path::new(&self.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");

        Ok(format!(
            "# PDF Document\n\n\
             **File:** {}\n\n\
             **Pages:** {}\n\n\
             **Size:** {:.1} MB\n\n\
             ---\n\n\
             **Note:** This PDF does not contain extractable text.\n\n\
             This typically means:\n\n\
             • The PDF is a **scanned image** (photograph of pages)\n\n\
             • Text is embedded in a way that this reader cannot extract\n\n\
             • The PDF uses non-standard or proprietary encoding\n\n\
             To read this content, you would need:\n\n\
             • An OCR tool to convert images to text\n\n\
             • The original source document\n\n\
             • Or a dedicated PDF reader application",
            filename, self.page_count, size_mb
        ))
    }

    /// Get page count by parsing the PDF with subprocess protection against crashes
    fn get_page_count(path: &str) -> Result<usize> {
        // Check if we're already in debug mode to avoid subprocess recursion
        if std::env::var("BOOKOKRAT_DEBUG_PDF_MODE").is_ok() {
            return Self::get_page_count_inline(path);
        }

        // Use subprocess to isolate potential crashes from oxidize-pdf
        Self::get_page_count_subprocess(path).or_else(|_| Self::get_page_count_inline(path))
    }

    /// Inline page count parsing without subprocess (used in debug mode)
    fn get_page_count_inline(path: &str) -> Result<usize> {
        let path_owned = path.to_string();

        let handle = std::thread::spawn(move || {
            crate::panic_handler::with_panic_exit_suppressed(|| {
                suppress_stderr(|| match PdfReader::open(&path_owned) {
                    Ok(reader) => {
                        let pdf_doc = OxidizePdfDocument::new(reader);

                        match pdf_doc.extract_text() {
                            Ok(text_pages) => Ok(text_pages.len()),
                            Err(e) => {
                                warn!("Could not determine page count: {}", e);
                                Err(anyhow::anyhow!("Failed to parse PDF: {}", e))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Could not open PDF file: {}", e);
                        Err(anyhow::anyhow!("Failed to open PDF: {}", e))
                    }
                })
            })
        });

        match handle.join() {
            Ok(result) => result,
            Err(_) => {
                error!("PDF parsing thread panicked (possible stack overflow or recursion limit)");
                Err(anyhow::anyhow!(
                    "PDF parsing failed: possible stack overflow in malformed PDF"
                ))
            }
        }
    }

    /// Get page count using a subprocess (safer from SIGABRT crashes)
    fn get_page_count_subprocess(path: &str) -> Result<usize> {
        use std::process::Stdio as StdioType;

        let mut child = Command::new(std::env::current_exe()?)
            .arg("--debug-pdf")
            .arg(path)
            .stdout(StdioType::piped())
            .stderr(StdioType::piped())
            .spawn()
            .context("Failed to spawn PDF parser subprocess")?;

        // Wait with a 60-second timeout for PDF page count parsing
        let timeout = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process finished, read output before we lose access to stdout
                    if !status.success() {
                        return Err(anyhow::anyhow!(
                            "PDF parser subprocess failed with status: {:?}",
                            status
                        ));
                    }

                    // We can still read from stdout since we have pipes
                    let output = child
                        .wait_with_output()
                        .context("Failed to read subprocess output")?;
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    // Parse the output to find page count
                    for line in stdout.lines() {
                        if line.contains("Reported page count:") {
                            if let Some(count_str) = line.split(':').nth(1) {
                                if let Ok(count) = count_str.trim().parse::<usize>() {
                                    return Ok(count);
                                }
                            }
                        }
                    }

                    return Err(anyhow::anyhow!(
                        "Could not parse page count from subprocess output"
                    ));
                }
                Ok(None) => {
                    // Process still running
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        error!("PDF page count subprocess timed out (60s) for: {}", path);
                        return Err(anyhow::anyhow!(
                            "PDF page count parsing timeout (possible infinite loop or corrupted structure)"
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to check subprocess status: {}", e));
                }
            }
        }
    }
}
