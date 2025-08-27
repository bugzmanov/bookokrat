# HTML5ever Migration Plan for BookRat Text Generator

**Important**: This migration will create a **replacement implementation** that maintains the same external interface as the current `TextGenerator`. This allows easy switching between implementations for testing and comparison.

## Migration Strategy

Instead of modifying `src/text_generator.rs` directly, we will:
1. Create a new `src/html5ever_text_generator.rs` with identical public interface
2. Use a two-phase approach: HTML → Markdown AST → String
3. Leverage the existing `src/markdown.rs` model for structured representation
4. Add a feature flag or runtime switch to choose between implementations
5. Ensure both implementations can coexist during testing phase
6. Replace the old implementation only after thorough validation

---

## Two-Phase Architecture

The new implementation will follow this pipeline:

1. **Phase 1**: HTML → Markdown AST (using `src/markdown.rs` types)
   - Parse HTML with html5ever DOM tree
   - Convert DOM nodes to structured Markdown AST
   - Preserve semantic meaning in typed structures

2. **Phase 2**: Markdown AST → String (compatible output)
   - Render Markdown AST to plain text string
   - Apply dialog formatting and spacing rules
   - Maintain identical output format for compatibility

This approach provides:
- **Better maintainability**: Clear separation between parsing and rendering
- **Type safety**: Structured representation prevents formatting errors
- **Extensibility**: Easy to add new Markdown features later
- **Compatibility**: Same string output as current implementation

---

## Current Analysis

### Working Features (Must Preserve)
- **Image processing**: `img_tag_re` → `[image src="..."]` placeholders ✅
- **MathML support**: Via `mathml_renderer::mathml_to_ascii` with placeholder system ✅
- **HTML cleanup**: Removal of XML declarations, DOCTYPE, style/script tags ✅
- **Headings**: Conversion to Markdown format with h1 uppercase ✅
- **Paragraphs**: Proper `</p>` → `\n\n` conversion ✅
- **Dialog formatting**: Smart grouping of consecutive dialog lines ✅
- **Text formatting**: Bold (`**`), italic (`_`), links (`[text](url)`) ✅
- **Entity decoding**: HTML entities → Unicode characters ✅

### Buggy Features (To Drop Initially)
- **Tables**: Complex ratatui-based rendering (lines 468-670) 🗑️
- **Lists**: Regex-based ul/ol parsing (lines 149-187) 🗑️

## Implementation Plan

### Phase 1: Core Infrastructure Setup

**1. Add html5ever Dependency**
```toml
# In Cargo.toml
html5ever = "0.27"
markup5ever_rcdom = "0.3"
```

**2. Create New HTML to Markdown Converter**
```rust
// New file: src/html_to_markdown.rs
use crate::markdown::{Document, Node, Block, HeadingLevel, Text, Style, TextNode};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{RcDom, NodeData};
use std::rc::Rc;

pub struct HtmlToMarkdownConverter {
    // Conversion state and placeholders
    image_placeholders: Vec<(String, String)>,
    mathml_placeholders: Vec<(String, String)>,
}

impl HtmlToMarkdownConverter {
    pub fn new() -> Self { /* */ }
    pub fn convert(&mut self, html: &str) -> Document { /* */ }
    
    // Returns placeholder mappings for later substitution
    pub fn get_image_placeholders(&self) -> &[(String, String)] { /* */ }
    pub fn get_mathml_placeholders(&self) -> &[(String, String)] { /* */ }
}
```

**3. Create Markdown to String Renderer**
```rust
// New file: src/markdown_renderer.rs
use crate::markdown::{Document, Node, Block, HeadingLevel, Text, TextNode};

pub struct MarkdownRenderer {
    // Rendering configuration
}

impl MarkdownRenderer {
    pub fn new() -> Self { /* */ }
    pub fn render(&self, doc: &Document) -> String { /* */ }
    pub fn render_with_dialog_formatting(&self, doc: &Document) -> String { /* */ }
    
    // Apply the same dialog grouping logic as current implementation
    fn format_text_with_spacing(&self, text: &str) -> String { /* */ }
    fn is_dialog_line(&self, text: &str) -> bool { /* */ }
    fn is_list_item(&self, text: &str) -> bool { /* */ }
}
```

**4. Create Replacement TextGenerator**
```rust
// New file: src/html5ever_text_generator.rs
use crate::html_to_markdown::HtmlToMarkdownConverter;
use crate::markdown_renderer::MarkdownRenderer;
use crate::mathml_renderer::mathml_to_ascii;
use crate::table_of_contents::TocItem;
use crate::toc_parser::TocParser;
use epub::doc::EpubDoc;
use regex::Regex;
use std::io::BufReader;

pub struct TextGenerator {
    html_converter: HtmlToMarkdownConverter,
    markdown_renderer: MarkdownRenderer,
    toc_parser: TocParser,
    // Keep utility regexes for post-processing compatibility
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
}

impl TextGenerator {
    // IDENTICAL public interface to original TextGenerator
    pub fn new() -> Self { /* */ }
    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> { /* */ }
    pub fn normalize_href(&self, href: &str) -> String { /* */ }
    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> { /* */ }
    pub fn process_chapter_content(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Result<(String, Option<String>), String> {
        // Two-phase processing:
        // 1. HTML → Markdown AST
        // 2. Markdown AST → String (with dialog formatting)
    }
    
    // Private methods will use the new pipeline
    fn clean_html_content(&self, content: &str, terminal_width: usize) -> String {
        // 1. Convert HTML to Markdown AST
        let mut converter = HtmlToMarkdownConverter::new();
        let markdown_doc = converter.convert(content);
        
        // 2. Render Markdown AST to string with formatting
        let rendered = self.markdown_renderer.render_with_dialog_formatting(&markdown_doc);
        
        // 3. Apply placeholder substitutions
        self.substitute_placeholders(rendered, &converter)
    }
}
```

**4. Add Implementation Switcher**
```rust
// In src/main_app.rs or wherever TextGenerator is used
#[cfg(feature = "html5ever")]
use crate::html5ever_text_generator::TextGenerator;

#[cfg(not(feature = "html5ever"))]
use crate::text_generator::TextGenerator;

// Alternative runtime approach:
pub enum TextGeneratorImpl {
    Regex(crate::text_generator::TextGenerator),
    Html5ever(crate::html5ever_text_generator::TextGenerator),
}
```

### Phase 2: DOM Tree Walking Implementation

**5. Implement Node Visitor Pattern**
```rust
// In html_parser.rs
struct MarkdownVisitor {
    output: String,
    image_placeholders: Vec<(String, String)>,
    mathml_placeholders: Vec<(String, String)>,
    in_heading: Option<u8>, // heading level
}

impl MarkdownVisitor {
    fn visit_node(&mut self, node: &Rc<Node>) {
        match node.data {
            NodeData::Element { ref name, ref attrs, .. } => {
                self.visit_element(name, attrs, node);
            }
            NodeData::Text { ref contents } => {
                self.visit_text(contents);
            }
            _ => {} // Document, Comment, etc.
        }
    }
}
```

**6. Element-Specific Handlers**
```rust
impl MarkdownVisitor {
    fn visit_element(&mut self, name: &QualName, attrs: &RefCell<Vec<Attribute>>, node: &Rc<Node>) {
        match name.local.as_ref() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.handle_heading(name, node),
            "p" => self.handle_paragraph(node),
            "img" => self.handle_image(attrs),
            "math" => self.handle_mathml(node),
            "a" => self.handle_link(attrs, node),
            "strong" | "b" => self.handle_bold(node),
            "em" | "i" => self.handle_italic(node),
            "br" => self.handle_break(),
            "div" => self.handle_div(node),
            "blockquote" => self.handle_blockquote(node),
            // Skip these entirely
            "style" | "script" | "head" => return,
            // Drop table/list support initially
            "table" | "ul" | "ol" | "li" => return,
            _ => self.visit_children(node),
        }
    }
}
```

### Phase 3: Feature Implementation

**7. Preserve Image Processing**
```rust
fn handle_image(&mut self, attrs: &RefCell<Vec<Attribute>>) {
    if let Some(src) = self.get_attr_value(attrs, "src") {
        self.output.push_str(&format!("\n\n[image src=\"{}\"]\n\n", src));
    }
}
```

**8. Preserve MathML Support**
```rust
fn handle_mathml(&mut self, node: &Rc<Node>) {
    let mathml_html = self.serialize_node(node);
    let placeholder = format!("__MATHML_PLACEHOLDER_{}__", self.mathml_placeholders.len());
    
    match mathml_to_ascii(&mathml_html, true) {
        Ok(ascii_math) => {
            self.mathml_placeholders.push((placeholder.clone(), ascii_math));
        }
        Err(_) => {
            let fallback = self.extract_text_content(node);
            self.mathml_placeholders.push((placeholder.clone(), fallback));
        }
    }
    
    self.output.push_str(&placeholder);
}
```

**9. Implement Heading Conversion**
```rust
fn handle_heading(&mut self, name: &QualName, node: &Rc<Node>) {
    let level = match name.local.as_ref() {
        "h1" => 1,
        "h2" => 2,
        // ... etc
        _ => 1,
    };
    
    self.in_heading = Some(level);
    self.output.push_str("\n\n");
    self.output.push_str(&"#".repeat(level));
    self.output.push(' ');
    
    let content = self.extract_text_content(node);
    if level == 1 {
        self.output.push_str(&content.to_uppercase());
    } else {
        self.output.push_str(&content);
    }
    
    self.output.push_str("\n\n");
    self.in_heading = None;
}
```

**10. Maintain Entity Decoding**
```rust
fn process_text(&self, text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "...")
        .replace("&ldquo;", "\u{201C}")
        .replace("&rdquo;", "\u{201D}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rsquo;", "\u{2019}")
}
```

### Phase 4: Integration & Testing

**11. Preserve Dialog Formatting**
Keep the existing `format_text_with_spacing` and dialog detection logic unchanged since it operates on the final text output.

**12. Create Test Harness**
```rust
// In tests or examples
fn test_both_implementations() {
    let regex_gen = crate::text_generator::TextGenerator::new();
    let html5ever_gen = crate::html5ever_text_generator::TextGenerator::new();
    
    let test_html = "<h1>Test</h1><p>Content</p>";
    
    let regex_result = regex_gen.clean_html_content(test_html, 80);
    let html5ever_result = html5ever_gen.clean_html_content(test_html, 80);
    
    println!("Regex result: {}", regex_result);
    println!("HTML5ever result: {}", html5ever_result);
    
    // Compare results
}
```

**13. Cargo Features for Easy Switching**
```toml
# In Cargo.toml
[features]
default = ["regex-parser"]
regex-parser = []
html5ever-parser = ["html5ever", "markup5ever_rcdom"]

# Build with: cargo build --features html5ever-parser --no-default-features
```

## Testing Strategy

### A/B Testing Approach
1. **Identical Interface**: Both implementations expose identical public methods
2. **Side-by-side comparison**: Test both implementations with same input
3. **Regression testing**: Ensure html5ever version produces equivalent output
4. **Performance testing**: Compare parsing speed and memory usage

### Test Coverage
- Run existing test suite against both implementations
- Focus on: headings, images, mathml, basic formatting
- Verify chapter title extraction compatibility
- Confirm dialog formatting remains identical
- Test edge cases that break regex parsing

## Implementation Steps

1. **Add html5ever dependency and feature flags**
2. **Create `src/html_parser.rs` with basic structure**
3. **Create `src/html5ever_text_generator.rs` with identical interface**
4. **Implement DOM walking and visitor pattern**
5. **Port element handlers one by one (start with headings, paragraphs)**
6. **Integrate image and mathml preservation**
7. **Add implementation switcher in main app**
8. **Run side-by-side tests and fix compatibility issues**
9. **Performance benchmarking**
10. **Gradual rollout with feature flag**

## Benefits

- **Risk-free migration**: Can instantly switch back if issues arise
- **Robust HTML parsing**: Handles malformed HTML gracefully
- **Proper DOM tree**: No more regex edge cases
- **Maintainable**: Clear separation of concerns
- **Extensible**: Easy to add new HTML elements later
- **Standards compliant**: Follows HTML5 parsing rules

## Migration Timeline

- **Phase 1**: 2-3 hours (infrastructure, identical interface setup)
- **Phase 2**: 3-4 hours (DOM walking implementation)  
- **Phase 3**: 3-4 hours (feature porting)
- **Phase 4**: 2-3 hours (integration, testing harness)

**Total estimated time**: 10-14 hours

## Rollout Plan

1. **Development**: Create html5ever implementation alongside existing code
2. **Testing**: Extensive A/B testing with real EPUB files
3. **Soft launch**: Feature flag for early adopters
4. **Full rollout**: Switch default implementation
5. **Cleanup**: Remove old regex-based implementation after confidence period

This approach ensures zero downtime and maximum safety during the migration process.