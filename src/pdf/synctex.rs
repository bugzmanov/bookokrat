use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Conversion factor from TeX scaled points to PDF points (72 dpi).
const SP_TO_PT: f64 = 65781.76;

/// Parsed SyncTeX data, ready for forward/inverse queries.
///
/// Parses `.synctex` or `.synctex.gz` files produced by `pdflatex --synctex=1`
/// and provides bidirectional mapping between LaTeX source locations and PDF coordinates.
pub struct SyncTexScanner {
    inputs: HashMap<u32, String>,
    basename_to_tag: HashMap<String, u32>,
    /// Forward search index: file_tag -> line -> Vec<IndexEntry>
    /// Entries come from BOTH hboxes and leaf elements (using parent hbox geometry).
    index: HashMap<u32, HashMap<u32, Vec<IndexEntry>>>,
    /// All hbox elements for inverse search
    page_elements: Vec<PageElement>,
    page_count: usize,
}

#[derive(Clone, Debug)]
struct IndexEntry {
    page: usize,
    /// Left edge in PDF points
    h: f64,
    /// Bottom of box in PDF points (raw_v + raw_depth) / SP_TO_PT
    v: f64,
    /// Width in PDF points
    width: f64,
    /// Total visual height in PDF points (raw_height + raw_depth) / SP_TO_PT
    height: f64,
}

#[derive(Clone, Debug)]
struct PageElement {
    page: usize,
    file_tag: u32,
    line: u32,
    h: f64,
    v: f64,
    width: f64,
    height: f64,
}

/// Result of a forward search query (source -> PDF).
#[derive(Clone, Debug)]
pub struct ForwardResult {
    /// 1-indexed page number
    pub page: usize,
    /// Horizontal position in PDF points (left edge)
    pub h: f64,
    /// Vertical position in PDF points (bottom of box)
    pub v: f64,
    /// Width in PDF points
    pub width: f64,
    /// Total visual height in PDF points
    pub height: f64,
}

/// Result of an inverse search query (PDF -> source).
#[derive(Clone, Debug)]
pub struct InverseResult {
    pub file: String,
    pub line: u32,
}

impl SyncTexScanner {
    /// Open and parse a `.synctex` or `.synctex.gz` file.
    pub fn open(path: &Path) -> Result<Self> {
        let content = if path.to_string_lossy().ends_with(".synctex.gz") {
            let file = std::fs::File::open(path)
                .with_context(|| format!("Failed to open synctex file: {}", path.display()))?;
            let mut decoder = flate2::read::GzDecoder::new(file);
            let mut content = String::new();
            decoder.read_to_string(&mut content).with_context(|| {
                format!("Failed to decompress synctex file: {}", path.display())
            })?;
            content
        } else {
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read synctex file: {}", path.display()))?
        };
        Self::parse(&content)
    }

    /// Parse synctex content from a string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut inputs: HashMap<u32, String> = HashMap::new();
        let mut index: HashMap<u32, HashMap<u32, Vec<IndexEntry>>> = HashMap::new();
        let mut page_elements: Vec<PageElement> = Vec::new();
        let mut page_count: usize = 0;

        let mut current_page: usize = 0;
        let mut in_content = false;
        let mut box_stack: Vec<BoxContext> = Vec::new();

        for line in content.lines() {
            if !in_content {
                if let Some(rest) = line.strip_prefix("Input:") {
                    if let Some((tag_str, path)) = rest.split_once(':') {
                        if let Ok(tag) = tag_str.parse::<u32>() {
                            inputs.insert(tag, path.to_string());
                        }
                    }
                } else if line == "Content:" {
                    in_content = true;
                }
                continue;
            }

            if line.starts_with("Postamble:") {
                break;
            }
            if line.starts_with('!') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('{') {
                if let Ok(p) = rest.parse::<usize>() {
                    current_page = p;
                    if p > page_count {
                        page_count = p;
                    }
                    box_stack.clear();
                }
                continue;
            }

            if line.starts_with('}') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                if let Some(ctx) = parse_box(rest) {
                    box_stack.push(ctx);
                }
                continue;
            }

            if line == "]" {
                box_stack.pop();
                continue;
            }

            if let Some(rest) = line.strip_prefix('(') {
                if let Some(ctx) = parse_box(rest) {
                    let entry = IndexEntry {
                        page: current_page,
                        h: ctx.h / SP_TO_PT,
                        v: (ctx.v + ctx.depth) / SP_TO_PT,
                        width: ctx.width / SP_TO_PT,
                        height: (ctx.height + ctx.depth) / SP_TO_PT,
                    };

                    index
                        .entry(ctx.tag)
                        .or_default()
                        .entry(ctx.line)
                        .or_default()
                        .push(entry);

                    // hbox entries are NOT added to page_elements — only leaf
                    // elements go there, because leaf elements carry the most
                    // precise source line attribution for inverse search.

                    box_stack.push(ctx);
                }
                continue;
            }

            if line == ")" {
                box_stack.pop();
                continue;
            }

            // Leaf elements: x, k, g, $, h, v
            let first_byte = line.as_bytes().first().copied();
            match first_byte {
                Some(b'x') | Some(b'k') | Some(b'g') | Some(b'$') | Some(b'h') | Some(b'v') => {
                    if let Some(elem) = parse_element(&line[1..]) {
                        // Index leaf elements using parent hbox geometry for forward search.
                        // This is critical: a leaf element tagged as line 8 may live inside
                        // an hbox tagged as line 10. The synctex CLI finds leaf elements
                        // by line and returns the parent hbox's geometry.
                        if let Some(parent) = box_stack.last() {
                            let entry = IndexEntry {
                                page: current_page,
                                h: parent.h / SP_TO_PT,
                                v: (parent.v + parent.depth) / SP_TO_PT,
                                width: parent.width / SP_TO_PT,
                                height: (parent.height + parent.depth) / SP_TO_PT,
                            };
                            index
                                .entry(elem.tag)
                                .or_default()
                                .entry(elem.line)
                                .or_default()
                                .push(entry);
                        }

                        // For inverse search, store leaf elements with their own position
                        let parent_height = box_stack
                            .last()
                            .map(|b| (b.height + b.depth) / SP_TO_PT)
                            .unwrap_or(0.0);
                        let parent_v_bottom = box_stack
                            .last()
                            .map(|b| (b.v + b.depth) / SP_TO_PT)
                            .unwrap_or(elem.v / SP_TO_PT);

                        page_elements.push(PageElement {
                            page: current_page,
                            file_tag: elem.tag,
                            line: elem.line,
                            h: elem.h / SP_TO_PT,
                            v: parent_v_bottom,
                            width: elem.width.unwrap_or(0.0) / SP_TO_PT,
                            height: parent_height,
                        });
                    }
                }
                _ => {}
            }
        }

        let mut basename_to_tag = HashMap::new();
        for (&tag, path) in &inputs {
            let basename = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            basename_to_tag.insert(basename, tag);
        }

        Ok(SyncTexScanner {
            inputs,
            basename_to_tag,
            index,
            page_elements,
            page_count,
        })
    }

    /// Find a synctex file for the given PDF path.
    pub fn find_synctex_file(pdf_path: &Path) -> Option<PathBuf> {
        let stem = pdf_path.file_stem()?;
        let dir = pdf_path.parent()?;

        let gz_path = dir.join(format!("{}.synctex.gz", stem.to_string_lossy()));
        if gz_path.exists() {
            return Some(gz_path);
        }
        let plain_path = dir.join(format!("{}.synctex", stem.to_string_lossy()));
        if plain_path.exists() {
            return Some(plain_path);
        }
        None
    }

    /// Forward search: find PDF position for a source location.
    ///
    /// `file` can be a full path or just a filename (basename matching).
    /// `line` is 1-indexed.
    pub fn forward_search(&self, file: &str, line: u32, _column: u32) -> Option<ForwardResult> {
        let tag = self.resolve_file_tag(file)?;
        let line_map = self.index.get(&tag)?;

        for offset in [0i32, 1, -1, 2, -2, 3, -3] {
            let target_line = (line as i32 + offset) as u32;
            if let Some(entries) = line_map.get(&target_line) {
                if let Some(entry) = entries.first() {
                    return Some(ForwardResult {
                        page: entry.page,
                        h: entry.h,
                        v: entry.v,
                        width: entry.width,
                        height: entry.height,
                    });
                }
            }
        }
        None
    }

    /// Inverse search: find source location for a PDF position.
    ///
    /// `page` is 1-indexed. `x` and `y` are in PDF points.
    pub fn inverse_search(&self, page: usize, x: f64, y: f64) -> Option<InverseResult> {
        let mut best: Option<(f64, &PageElement)> = None;

        for elem in &self.page_elements {
            if elem.page != page {
                continue;
            }
            if elem.width <= 0.0 && elem.height <= 0.0 {
                continue;
            }

            let elem_top = elem.v - elem.height;
            let elem_bottom = elem.v;

            let vert_dist = if y >= elem_top && y <= elem_bottom {
                0.0
            } else {
                (y - elem.v).abs().min((y - elem_top).abs())
            };

            let horiz_dist = if x >= elem.h && x <= elem.h + elem.width {
                0.0
            } else {
                (x - elem.h).abs().min((x - (elem.h + elem.width)).abs())
            };

            let dist = vert_dist * 10.0 + horiz_dist;

            if best.is_none() || dist < best.unwrap().0 {
                best = Some((dist, elem));
            }
        }

        let elem = best?.1;
        let file = self.inputs.get(&elem.file_tag)?.clone();
        Some(InverseResult {
            file,
            line: elem.line,
        })
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn inputs(&self) -> &HashMap<u32, String> {
        &self.inputs
    }

    fn resolve_file_tag(&self, file: &str) -> Option<u32> {
        for (&tag, path) in &self.inputs {
            if path == file || path.ends_with(file) {
                return Some(tag);
            }
        }
        let basename = Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string());
        self.basename_to_tag.get(&basename).copied()
    }
}

#[derive(Clone, Debug)]
struct BoxContext {
    tag: u32,
    line: u32,
    h: f64,
    v: f64,
    width: f64,
    height: f64,
    depth: f64,
}

struct ElementData {
    tag: u32,
    line: u32,
    h: f64,
    v: f64,
    width: Option<f64>,
}

fn parse_box(s: &str) -> Option<BoxContext> {
    let (link, rest) = s.split_once(':')?;
    let (tag_str, line_str) = link.split_once(',')?;
    let tag = tag_str.parse::<u32>().ok()?;
    let line = line_str.parse::<u32>().ok()?;

    let (point, size) = rest.split_once(':')?;
    let (x_str, y_str) = point.split_once(',')?;
    let h = x_str.parse::<f64>().ok()?;
    let v = y_str.parse::<f64>().ok()?;

    let size_parts: Vec<&str> = size.split(',').collect();
    if size_parts.len() < 3 {
        return None;
    }
    let width = size_parts[0].parse::<f64>().ok()?;
    let height = size_parts[1].parse::<f64>().ok()?;
    let depth = size_parts[2].parse::<f64>().ok()?;

    Some(BoxContext {
        tag,
        line,
        h,
        v,
        width,
        height,
        depth,
    })
}

fn parse_element(s: &str) -> Option<ElementData> {
    let (link, rest) = s.split_once(':')?;
    let (tag_str, line_str) = link.split_once(',')?;
    let tag = tag_str.parse::<u32>().ok()?;
    let line = line_str.parse::<u32>().ok()?;

    let parts: Vec<&str> = rest.splitn(3, ':').collect();
    let (x_str, y_str) = parts[0].split_once(',')?;
    let h = x_str.parse::<f64>().ok()?;
    let v = y_str.parse::<f64>().ok()?;

    let width = if parts.len() > 1 {
        parts[1].parse::<f64>().ok()
    } else {
        None
    };

    Some(ElementData {
        tag,
        line,
        h,
        v,
        width,
    })
}

// ---------------------------------------------------------------------------
// Socket-based IPC for editor integration
// ---------------------------------------------------------------------------

/// Command received from an editor via the Unix socket.
#[derive(Debug, Clone)]
pub enum SyncTexCommand {
    /// Forward search: navigate to the PDF position for the given source location.
    Forward {
        file: String,
        line: u32,
        column: u32,
    },
}

/// Compute the deterministic socket path for a given PDF file.
pub fn synctex_socket_path(pdf_path: &Path) -> PathBuf {
    let stem = pdf_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let dir = std::env::temp_dir();
    dir.join(format!("bookokrat-synctex-{stem}.sock"))
}

/// Parse a text command line into a SyncTexCommand.
///
/// Format: `forward <line> <column> <file>`
fn parse_command(line: &str) -> Option<SyncTexCommand> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("forward ") {
        let parts: Vec<&str> = rest.splitn(3, ' ').collect();
        if parts.len() == 3 {
            let line_num = parts[0].parse::<u32>().ok()?;
            let column = parts[1].parse::<u32>().ok()?;
            let file = parts[2].to_string();
            return Some(SyncTexCommand::Forward {
                file,
                line: line_num,
                column,
            });
        }
    }
    None
}

/// Listens on a Unix domain socket for SyncTeX commands from editors.
///
/// Runs a background thread that accepts connections and sends parsed
/// commands to the main event loop via a flume channel.
pub struct SyncTexListener {
    socket_path: PathBuf,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[allow(dead_code)]
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl SyncTexListener {
    /// Start listening on the given socket path.
    ///
    /// Commands are sent to `tx`. The listener thread runs until dropped.
    pub fn start(socket_path: PathBuf, tx: flume::Sender<SyncTexCommand>) -> Result<Self> {
        // Remove stale socket if it exists
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .with_context(|| format!("Failed to bind synctex socket: {}", socket_path.display()))?;

        // Set non-blocking so the thread can check the shutdown flag
        listener.set_nonblocking(true)?;

        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let path_clone = socket_path.clone();

        let join_handle = std::thread::Builder::new()
            .name("synctex-listener".into())
            .spawn(move || {
                Self::listener_loop(listener, tx, shutdown_clone, &path_clone);
            })
            .with_context(|| "Failed to spawn synctex listener thread")?;

        log::info!("SyncTeX listener started on {}", socket_path.display());

        Ok(SyncTexListener {
            socket_path,
            shutdown,
            join_handle: Some(join_handle),
        })
    }

    fn listener_loop(
        listener: std::os::unix::net::UnixListener,
        tx: flume::Sender<SyncTexCommand>,
        shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
        _path: &Path,
    ) {
        use std::io::BufRead;

        while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // Set blocking with a timeout for reading
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let reader = std::io::BufReader::new(&stream);

                    for line_result in reader.lines() {
                        let Ok(line) = line_result else { break };
                        if let Some(cmd) = parse_command(&line) {
                            if tx.send(cmd).is_err() {
                                return;
                            }
                        } else {
                            log::warn!("Unknown synctex command: {line}");
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection waiting — sleep briefly and retry
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    log::error!("SyncTeX listener accept error: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }

    /// Get the socket path this listener is bound to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for SyncTexListener {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // Connect briefly to unblock the accept() call
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        log::info!(
            "SyncTeX listener stopped, removed {}",
            self.socket_path.display()
        );
    }
}

/// Send a forward search command to a running Bookokrat instance.
///
/// Used by `--synctex-forward` CLI mode.
pub fn send_forward_command(
    socket_path: &Path,
    file: &str,
    line: u32,
    column: u32,
) -> Result<String> {
    use std::io::Write;

    let mut stream = std::os::unix::net::UnixStream::connect(socket_path).with_context(|| {
        format!(
            "Failed to connect to synctex socket: {}",
            socket_path.display()
        )
    })?;

    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;

    writeln!(stream, "forward {line} {column} {file}")?;
    stream.flush()?;

    let mut response = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(&stream), &mut response)?;
    Ok(response.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct GoldenTestCase {
        #[allow(dead_code)]
        description: String,
        #[allow(dead_code)]
        source_file: String,
        forward_search: Vec<ForwardSearchCase>,
        inverse_search: Vec<InverseSearchCase>,
    }

    #[derive(Deserialize)]
    struct ForwardSearchCase {
        description: String,
        query: ForwardQuery,
        expected: ForwardExpected,
    }

    #[derive(Deserialize)]
    struct ForwardQuery {
        file: String,
        line: u32,
        column: u32,
    }

    #[derive(Deserialize)]
    #[allow(non_snake_case)]
    struct ForwardExpected {
        page: usize,
        h: f64,
        v: f64,
        W: f64,
        H: f64,
    }

    #[derive(Deserialize)]
    struct InverseSearchCase {
        description: String,
        query: InverseQuery,
        expected: InverseExpected,
    }

    #[derive(Deserialize)]
    struct InverseQuery {
        page: usize,
        x: f64,
        y: f64,
    }

    #[derive(Deserialize)]
    struct InverseExpected {
        file: String,
        line: u32,
    }

    fn assert_approx_eq(actual: f64, expected: f64, epsilon: f64, context: &str) {
        assert!(
            (actual - expected).abs() < epsilon,
            "{context}: expected {expected}, got {actual} (diff: {})",
            (actual - expected).abs()
        );
    }

    fn load_synctex(name: &str) -> SyncTexScanner {
        let path = format!("tests/testdata/synctex/{name}.synctex.gz");
        SyncTexScanner::open(Path::new(&path))
            .unwrap_or_else(|e| panic!("Failed to open {path}: {e}"))
    }

    fn load_golden(name: &str) -> GoldenTestCase {
        let path = format!("tests/testdata/synctex/{name}_golden.json");
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {path}: {e}"))
    }

    fn run_forward_golden(scanner: &SyncTexScanner, golden: &GoldenTestCase) {
        for case in &golden.forward_search {
            let result = scanner
                .forward_search(&case.query.file, case.query.line, case.query.column)
                .unwrap_or_else(|| {
                    panic!(
                        "Forward search returned None for: {} (line {})",
                        case.description, case.query.line
                    )
                });

            assert_eq!(
                result.page, case.expected.page,
                "{}: page mismatch",
                case.description
            );
            assert_approx_eq(result.h, case.expected.h, 0.01, &case.description);
            assert_approx_eq(result.v, case.expected.v, 0.01, &case.description);
            assert_approx_eq(result.width, case.expected.W, 0.01, &case.description);
            assert_approx_eq(result.height, case.expected.H, 0.01, &case.description);
        }
    }

    fn run_inverse_golden(scanner: &SyncTexScanner, golden: &GoldenTestCase) {
        for case in &golden.inverse_search {
            let result = scanner
                .inverse_search(case.query.page, case.query.x, case.query.y)
                .unwrap_or_else(|| {
                    panic!(
                        "Inverse search returned None for: {} (page {}, x={}, y={})",
                        case.description, case.query.page, case.query.x, case.query.y
                    )
                });

            assert!(
                result.file.ends_with(&case.expected.file),
                "{}: file mismatch - expected path ending with '{}', got '{}'",
                case.description,
                case.expected.file,
                result.file
            );
            assert_eq!(
                result.line, case.expected.line,
                "{}: line mismatch",
                case.description
            );
        }
    }

    // -- test_main (3-page simple document) -----------------------------------

    #[test]
    fn test_main_parse() {
        let scanner = load_synctex("test_main");
        assert_eq!(scanner.page_count(), 3);
        let input1 = scanner.inputs().get(&1).expect("Missing input 1");
        assert!(input1.ends_with("test_main.tex"));
    }

    #[test]
    fn test_main_forward_golden() {
        let scanner = load_synctex("test_main");
        let golden = load_golden("test_main");
        run_forward_golden(&scanner, &golden);
    }

    #[test]
    fn test_main_inverse_golden() {
        let scanner = load_synctex("test_main");
        let golden = load_golden("test_main");
        run_inverse_golden(&scanner, &golden);
    }

    // -- test_comprehensive (23-page document with math, code, tikz, tables) --

    #[test]
    fn test_comprehensive_parse() {
        let scanner = load_synctex("test_comprehensive");
        assert_eq!(scanner.page_count(), 20);
        let input1 = scanner.inputs().get(&1).expect("Missing input 1");
        assert!(input1.ends_with("test_comprehensive.tex"));
    }

    #[test]
    fn test_comprehensive_forward_golden() {
        let scanner = load_synctex("test_comprehensive");
        let golden = load_golden("test_comprehensive");
        run_forward_golden(&scanner, &golden);
    }

    #[test]
    fn test_comprehensive_inverse_golden() {
        let scanner = load_synctex("test_comprehensive");
        let golden = load_golden("test_comprehensive");
        run_inverse_golden(&scanner, &golden);
    }

    #[test]
    fn test_forward_search_nonexistent_file() {
        let scanner = load_synctex("test_main");
        assert!(scanner.forward_search("nonexistent.tex", 1, 0).is_none());
    }

    #[test]
    fn test_forward_search_nonexistent_line() {
        let scanner = load_synctex("test_main");
        assert!(scanner.forward_search("test_main.tex", 999, 0).is_none());
    }

    #[test]
    fn test_inverse_search_nonexistent_page() {
        let scanner = load_synctex("test_main");
        assert!(scanner.inverse_search(99, 100.0, 100.0).is_none());
    }

    #[test]
    fn test_find_synctex_file() {
        let pdf_path = Path::new("tests/testdata/synctex/test_main.pdf");
        let result = SyncTexScanner::find_synctex_file(pdf_path);
        assert!(result.is_some());
        let synctex_path = result.unwrap();
        assert!(synctex_path.to_string_lossy().ends_with(".synctex.gz"));
    }

    #[test]
    fn test_gzip_and_plain_parse_identical() {
        let gz_scanner = load_synctex("test_main");

        let gz_path = Path::new("tests/testdata/synctex/test_main.synctex.gz");
        let file = std::fs::File::open(gz_path).unwrap();
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut content = String::new();
        decoder.read_to_string(&mut content).unwrap();
        let plain_scanner = SyncTexScanner::parse(&content).unwrap();

        assert_eq!(gz_scanner.page_count(), plain_scanner.page_count());
        assert_eq!(gz_scanner.inputs().len(), plain_scanner.inputs().len());

        let result_gz = gz_scanner.forward_search("test_main.tex", 8, 1);
        let result_plain = plain_scanner.forward_search("test_main.tex", 8, 1);
        assert!(result_gz.is_some());
        assert!(result_plain.is_some());
        let rg = result_gz.unwrap();
        let rp = result_plain.unwrap();
        assert_eq!(rg.page, rp.page);
        assert_approx_eq(rg.h, rp.h, 0.001, "gz vs plain h");
        assert_approx_eq(rg.v, rp.v, 0.001, "gz vs plain v");
    }

    #[test]
    fn test_parse_empty_content() {
        let content = "SyncTeX Version:1\nContent:\nPostamble:\nCount:0\n";
        let scanner = SyncTexScanner::parse(content).unwrap();
        assert_eq!(scanner.page_count(), 0);
        assert!(scanner.inputs().is_empty());
    }

    #[test]
    fn test_parse_command() {
        let cmd = parse_command("forward 42 1 main.tex");
        assert!(cmd.is_some());
        match cmd.unwrap() {
            SyncTexCommand::Forward { file, line, column } => {
                assert_eq!(file, "main.tex");
                assert_eq!(line, 42);
                assert_eq!(column, 1);
            }
        }

        let cmd = parse_command("forward 8 0 /path/to/chapter1.tex\n");
        assert!(cmd.is_some());
        match cmd.unwrap() {
            SyncTexCommand::Forward { file, line, column } => {
                assert_eq!(file, "/path/to/chapter1.tex");
                assert_eq!(line, 8);
                assert_eq!(column, 0);
            }
        }

        assert!(parse_command("unknown command").is_none());
        assert!(parse_command("forward").is_none());
        assert!(parse_command("forward 1").is_none());
        assert!(parse_command("forward 1 2").is_none());
        assert!(parse_command("").is_none());
    }

    #[test]
    fn test_synctex_socket_path() {
        let path = synctex_socket_path(Path::new("/home/user/documents/thesis.pdf"));
        assert!(
            path.to_string_lossy()
                .contains("bookokrat-synctex-thesis.sock")
        );
    }

    #[test]
    fn test_socket_listener_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let socket_path = tmp.path().join("test.sock");

        let (tx, rx) = flume::unbounded();
        let listener = SyncTexListener::start(socket_path.clone(), tx).unwrap();

        // Connect as a client and send a command
        {
            use std::io::Write;
            let mut stream = std::os::unix::net::UnixStream::connect(&socket_path).unwrap();
            writeln!(stream, "forward 42 1 main.tex").unwrap();
            stream.flush().unwrap();
        }

        // Give the listener thread a moment to process
        std::thread::sleep(std::time::Duration::from_millis(200));

        let cmd = rx.try_recv().expect("Should have received a command");
        match cmd {
            SyncTexCommand::Forward { file, line, column } => {
                assert_eq!(file, "main.tex");
                assert_eq!(line, 42);
                assert_eq!(column, 1);
            }
        }

        drop(listener);
        assert!(!socket_path.exists(), "Socket file should be cleaned up");
    }
}
