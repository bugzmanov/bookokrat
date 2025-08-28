use crate::mathml_renderer::mathml_to_ascii;
use crate::table_of_contents::TocItem;
use crate::toc_parser::TocParser;
use epub::doc::EpubDoc;
use log::{debug, warn};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Constraint, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};
use regex::Regex;
use std::io::BufReader;

pub struct TextGenerator {
    p_tag_re: Regex,
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
    img_tag_re: Regex,
    table_re: Regex,
    mathml_re: Regex,
    toc_parser: TocParser,
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            p_tag_re: Regex::new(r"<p[^>]*>").expect("Failed to compile paragraph tag regex"),
            multi_space_re: Regex::new(r" +").expect("Failed to compile multi space regex"),
            multi_newline_re: Regex::new(r"\n{3,}").expect("Failed to compile multi newline regex"),
            leading_space_re: Regex::new(r"^ +").expect("Failed to compile leading space regex"),
            line_leading_space_re: Regex::new(r"\n +")
                .expect("Failed to compile line leading space regex"),
            img_tag_re: Regex::new(r#"<img[^>]*\ssrc\s*=\s*["']([^"']+)["'][^>]*>"#)
                .expect("Failed to compile image tag regex"),
            table_re: Regex::new(r"(?s)<table[^>]*>.*?</table>")
                .expect("Failed to compile table regex"),
            mathml_re: Regex::new(r"(?s)<math[^>]*>.*?</math>")
                .expect("Failed to compile MathML regex"),
            toc_parser: TocParser::new(),
        }
    }

    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        let h1_pattern = Regex::new(r"(?s)<h1[^>]*>(.*?)</h1>").ok()?;
        let h2_pattern = Regex::new(r"(?s)<h2[^>]*>(.*?)</h2>").ok()?;
        let h3_pattern = Regex::new(r"(?s)<h3[^>]*>(.*?)</h3>").ok()?;
        let title_pattern = Regex::new(r"(?s)<title[^>]*>(.*?)</title>").ok()?;

        for re in vec![h1_pattern, h2_pattern, h3_pattern, title_pattern] {
            if let Some(captures) = re.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = self.extract_text_from_html(title_match.as_str());
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title);
                    }
                }
            }
        }

        None
    }

    /// Helper function to extract plain text from HTML, removing tags but keeping content
    fn extract_text_from_html(&self, html: &str) -> String {
        // Remove all HTML tags but keep their content, don't add spaces
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let text = tag_re.replace_all(html, " ");

        // Clean up all whitespace (spaces, newlines, tabs) and collapse to single space
        let whitespace_re = Regex::new(r"\s+").unwrap();
        let cleaned = whitespace_re.replace_all(&text, " ");

        cleaned.trim().to_string()
    }

    /// Normalize href for comparison by removing relative path prefixes, OEBPS directory, and fragments
    pub fn normalize_href(&self, href: &str) -> String {
        let normalized = href
            .trim_start_matches("../")
            .trim_start_matches("./")
            .trim_start_matches("OEBPS/");

        // Remove fragment identifiers (e.g., "#ch1", "#tit") for matching
        if let Some(fragment_pos) = normalized.find('#') {
            normalized[..fragment_pos].to_string()
        } else {
            normalized.to_string()
        }
    }

    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        let result = self.toc_parser.parse_toc_structure(doc);
        result
    }

    pub fn process_chapter_content(
        &self,
        doc: &mut EpubDoc<BufReader<std::fs::File>>,
    ) -> Result<(String, Option<String>), String> {
        let content = doc
            .get_current_str()
            .map_err(|e| format!("Failed to get chapter content: {}", e))?;
        debug!("Raw content length: {} bytes", content.len());

        let chapter_title = self.extract_chapter_title(&content);
        // Use a default width of 120 for chapter extraction (will be properly wrapped later)
        let processed_text = self.clean_html_content(&content, 120);

        if processed_text.is_empty() {
            warn!("Converted text is empty");
            Ok((
                "No content available in this chapter.".to_string(),
                chapter_title,
            ))
        } else {
            debug!("Final text length: {} bytes", processed_text.len());
            let formatted_text = self.format_text_with_spacing(&processed_text);

            Ok((formatted_text, chapter_title))
        }
    }

    /// Convert HTML headings to Markdown format
    fn convert_headings_to_markdown(&self, text: &str) -> String {
        let heading_re = Regex::new(r"(?s)<h([1-7])[^>]*>(.*?)</h[1-7]>").unwrap();

        heading_re
            .replace_all(text, |caps: &regex::Captures| {
                let level = caps.get(1).unwrap().as_str();
                let content = caps.get(2).unwrap().as_str().trim();

                let level_num: usize = level.parse().unwrap_or(1);
                let hashes = "#".repeat(level_num);
                let final_content = if level_num == 1 {
                    content.to_uppercase()
                } else {
                    content.to_string()
                };

                format!("\n\n{} {}\n\n", hashes, final_content)
            })
            .to_string()
    }

    /// Convert HTML lists to markdown format
    fn convert_lists_to_markdown(&self, text: &str) -> String {
        // Handle unordered lists
        let ul_re = Regex::new(r"(?s)<ul[^>]*>(.*?)</ul>").unwrap();
        let mut content = ul_re
            .replace_all(text, |caps: &regex::Captures| {
                let list_content = caps.get(1).unwrap().as_str();
                let li_re = Regex::new(r"(?s)<li[^>]*>(.*?)</li>").unwrap();
                let mut result = String::new();
                for li_cap in li_re.captures_iter(list_content) {
                    if let Some(item_content) = li_cap.get(1) {
                        let cleaned_item = item_content.as_str().trim();
                        result.push_str(&format!("• {}\n", cleaned_item));
                    }
                }
                format!("\n{}\n", result)
            })
            .to_string();

        // Handle ordered lists
        let ol_re = Regex::new(r"(?s)<ol[^>]*>(.*?)</ol>").unwrap();
        content = ol_re
            .replace_all(&content, |caps: &regex::Captures| {
                let list_content = caps.get(1).unwrap().as_str();
                let li_re = Regex::new(r"(?s)<li[^>]*>(.*?)</li>").unwrap();
                let mut result = String::new();
                let mut index = 1;
                for li_cap in li_re.captures_iter(list_content) {
                    if let Some(item_content) = li_cap.get(1) {
                        let cleaned_item = item_content.as_str().trim();
                        result.push_str(&format!("{}. {}\n", index, cleaned_item));
                        index += 1;
                    }
                }
                format!("\n{}\n", result)
            })
            .to_string();

        content
    }

    fn clean_html_content(&self, content: &str, terminal_width: usize) -> String {
        // First remove XML declaration and DOCTYPE
        let xml_decl_re = Regex::new(r"<\?xml[^?]*\?>").unwrap();
        let doctype_re = Regex::new(r"(?i)<!DOCTYPE[^>]*>").unwrap();
        let mut content = xml_decl_re.replace_all(content, "").into_owned();
        content = doctype_re.replace_all(&content, "").into_owned();

        // Remove style and script tags
        let style_re = Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
        let script_re = Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
        content = style_re.replace_all(&content, "").into_owned();
        content = script_re.replace_all(&content, "").into_owned();

        // Remove head tag and all its contents
        let head_re = Regex::new(r"(?s)<head[^>]*>.*?</head>").unwrap();
        content = head_re.replace_all(&content, "").into_owned();

        // Remove html and body tags (but keep their content)
        let html_open_re = Regex::new(r"</?html[^>]*>").unwrap();
        let body_open_re = Regex::new(r"</?body[^>]*>").unwrap();
        content = html_open_re.replace_all(&content, "").into_owned();
        content = body_open_re.replace_all(&content, "").into_owned();

        // Process links before removing tags - replace with markdown-style format
        let link_re = Regex::new(r#"<a[^>]*\shref\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#).unwrap();
        content = link_re.replace_all(&content, "[$2]($1)").into_owned();

        // Convert lists to markdown format before removing tags
        content = self.convert_lists_to_markdown(&content);

        // Process tables BEFORE entity replacement and tag removal
        // Tables handle their own entity processing internally
        // We'll use placeholders to protect table content
        let mut table_placeholders = Vec::new();
        let mut tables_processed = content.clone();

        for (idx, table_match) in self.table_re.find_iter(&content).enumerate() {
            let placeholder = format!("__TABLE_PLACEHOLDER_{}__", idx);
            let table_text = self.parse_table(table_match.as_str(), terminal_width);
            table_placeholders.push((placeholder.clone(), table_text));

            // Replace table HTML with placeholder
            tables_processed = tables_processed.replace(table_match.as_str(), &placeholder);
        }

        content = tables_processed;

        // Process MathML formulas BEFORE entity replacement and tag removal
        // Use placeholders to protect MathML content
        let mut mathml_placeholders = Vec::new();
        let mut mathml_processed = content.clone();

        for (idx, mathml_match) in self.mathml_re.find_iter(&content).enumerate() {
            let placeholder = format!("__MATHML_PLACEHOLDER_{}__", idx);
            match mathml_to_ascii(mathml_match.as_str(), true) {
                Ok(ascii_math) => {
                    mathml_placeholders.push((placeholder.clone(), ascii_math));
                }
                Err(e) => {
                    debug!("Failed to convert MathML to ASCII: {}", e);
                    // Fallback to original MathML content without tags
                    let fallback = mathml_match
                        .as_str()
                        .replace("<math", "")
                        .replace("</math>", "")
                        .trim()
                        .to_string();
                    mathml_placeholders.push((placeholder.clone(), fallback));
                }
            }

            // Replace MathML HTML with placeholder
            mathml_processed = mathml_processed.replace(mathml_match.as_str(), &placeholder);
        }

        content = mathml_processed;

        // Process img tags into text placeholders
        content = self
            .img_tag_re
            .replace_all(&content, "\n\n[image src=\"$1\"]\n\n")
            .into_owned();

        // Process HTML entities for the rest of the content
        let text = content
            .replace("&nbsp;", " ")
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
            .replace("&rsquo;", "\u{2019}");

        let text = self.p_tag_re.replace_all(&text, "").to_string();

        let text = text
            .replace("</p>", "\n\n")
            .replace("<br>", "\n")
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<blockquote>", "\n    ")
            .replace("</blockquote>", "\n")
            .replace("<em>", "_")
            .replace("</em>", "_")
            .replace("<i>", "_")
            .replace("</i>", "_")
            .replace("<strong>", "**")
            .replace("</strong>", "**")
            .replace("<b>", "**")
            .replace("</b>", "**")
            .replace("<div>", "")
            .replace("</div>", "\n");

        let text = self.convert_headings_to_markdown(&text);

        // Only remove HTML tags, not content that looks like tags (e.g., <BOS>)
        // This regex only matches actual HTML tags starting with a letter or /
        let safe_tag_re = Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();
        let text = safe_tag_re.replace_all(&text, "").to_string();

        let text = self.multi_space_re.replace_all(&text, " ").to_string();
        let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
        let text = Regex::new(r"\n\s*\n")
            .unwrap()
            .replace_all(&text, "\n\n")
            .to_string();
        let text = self.multi_newline_re.replace_all(&text, "\n\n").to_string();
        let text = self.leading_space_re.replace_all(&text, "").to_string();
        let mut text = self
            .line_leading_space_re
            .replace_all(&text, "\n")
            .to_string();

        // Restore table content from placeholders
        for (placeholder, table_text) in table_placeholders {
            text = text.replace(&placeholder, &table_text);
        }

        // Restore MathML content from placeholders
        for (placeholder, mathml_text) in mathml_placeholders {
            text = text.replace(&placeholder, &mathml_text);
        }

        text.trim().to_string()
    }

    fn format_text_with_spacing(&self, text: &str) -> String {
        let mut formatted = String::new();
        let normalized_text = self.multi_newline_re.replace_all(text, "\n\n");
        let paragraphs: Vec<&str> = normalized_text.split("\n\n").collect();

        let mut i = 0;
        while i < paragraphs.len() {
            let paragraph = paragraphs[i];
            if paragraph.trim().is_empty() {
                i += 1;
                continue;
            }

            // Check if this is the start of a list block
            if self.is_list_item(paragraph) {
                // Collect consecutive list items
                let mut list_items = vec![paragraph];
                let mut j = i + 1;

                while j < paragraphs.len() && self.is_list_item(paragraphs[j]) {
                    list_items.push(paragraphs[j]);
                    j += 1;
                }

                // Format list block without empty lines between items
                for (idx, list_item) in list_items.iter().enumerate() {
                    // Process each line in the list item (in case it spans multiple lines)
                    let lines: Vec<&str> = list_item.lines().collect();
                    for (line_idx, line) in lines.iter().enumerate() {
                        formatted.push_str(line);
                        if line_idx < lines.len() - 1 {
                            formatted.push('\n');
                        }
                    }
                    if idx < list_items.len() - 1 {
                        formatted.push('\n');
                    }
                }

                // Add empty line after list block
                if j < paragraphs.len() {
                    formatted.push_str("\n\n");
                }

                i = j;
                continue;
            }

            // Check if this is the start of a dialog block
            if self.is_dialog_line(paragraph) {
                // Collect consecutive dialog lines
                let mut dialog_lines = vec![paragraph];
                let mut j = i + 1;

                while j < paragraphs.len() && self.is_dialog_line(paragraphs[j]) {
                    dialog_lines.push(paragraphs[j]);
                    j += 1;
                }

                // Only treat as dialog if we have at least 2 consecutive dialog lines
                if dialog_lines.len() >= 2 {
                    // Format dialog block without empty lines between responses
                    for (idx, dialog_line) in dialog_lines.iter().enumerate() {
                        formatted.push_str(dialog_line);
                        if idx < dialog_lines.len() - 1 {
                            formatted.push('\n');
                        }
                    }

                    // Add empty line after dialog block
                    if j < paragraphs.len() {
                        formatted.push_str("\n\n");
                    }

                    i = j;
                    continue;
                }
            }

            // Regular paragraph formatting
            let lines: Vec<&str> = paragraph.lines().collect();
            for (j, line) in lines.iter().enumerate() {
                formatted.push_str(line);
                if j < lines.len() - 1 {
                    formatted.push('\n');
                }
            }

            if i < paragraphs.len() - 1 {
                formatted.push_str("\n\n");
            }

            i += 1;
        }

        formatted
    }

    /// Check if a line is a list item (bullet or numbered)
    fn is_list_item(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        // Check for bullet points
        if trimmed.starts_with("• ") || trimmed.starts_with("- ") {
            return true;
        }
        // Check for numbered lists
        if let Some(captures) = Regex::new(r"^\d+\. ").unwrap().captures(trimmed) {
            return captures.get(0).is_some();
        }
        false
    }

    /// Check if a line starts with a dialog marker (various types of dashes)
    fn is_dialog_line(&self, text: &str) -> bool {
        let trimmed = text.trim_start();
        // Check for various dash types that indicate dialog
        trimmed.starts_with('-') ||      // Regular hyphen
        trimmed.starts_with('–') ||      // En dash
        trimmed.starts_with('—') ||      // Em dash
        trimmed.starts_with('\u{2010}') || // Hyphen
        trimmed.starts_with('\u{2011}') || // Non-breaking hyphen
        trimmed.starts_with('\u{2012}') || // Figure dash
        trimmed.starts_with('\u{2013}') || // En dash
        trimmed.starts_with('\u{2014}') // Em dash
    }

    /// Parse HTML table and render it using ratatui Table widget
    fn parse_table(&self, table_html: &str, max_width: usize) -> String {
        let mut result = String::new();

        // Extract caption if present
        let caption_re = Regex::new(r"(?s)<caption[^>]*>(.*?)</caption>").unwrap();
        if let Some(captures) = caption_re.captures(table_html) {
            if let Some(caption_match) = captures.get(1) {
                let caption = self.clean_table_cell(caption_match.as_str());
                if !caption.is_empty() {
                    // Wrap caption to fit width
                    let wrapped_caption = textwrap::wrap(&caption, max_width);
                    for line in wrapped_caption {
                        result.push_str(&line);
                        result.push('\n');
                    }
                }
            }
        }

        // Parse table rows
        let row_re = Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").unwrap();
        let cell_re = Regex::new(r"(?s)<t[hd][^>]*>(.*?)</t[hd]>").unwrap();

        let mut rows_data: Vec<Vec<String>> = Vec::new();
        let mut header_row: Option<Vec<String>> = None;
        let has_thead = table_html.contains("<thead");

        // Collect all cells
        for (row_idx, row_match) in row_re.captures_iter(table_html).enumerate() {
            if let Some(row_content) = row_match.get(1) {
                let mut cells: Vec<String> = Vec::new();

                for cell_match in cell_re.captures_iter(row_content.as_str()) {
                    if let Some(cell_content) = cell_match.get(1) {
                        let cleaned = self.clean_table_cell(cell_content.as_str());
                        cells.push(cleaned);
                    }
                }

                if !cells.is_empty() {
                    if row_idx == 0 && has_thead {
                        header_row = Some(cells);
                    } else {
                        rows_data.push(cells);
                    }
                }
            }
        }

        // Determine the number of columns
        let num_columns = header_row
            .as_ref()
            .map(|h| h.len())
            .unwrap_or_else(|| rows_data.iter().map(|r| r.len()).max().unwrap_or(0));

        if num_columns == 0 {
            return result;
        }

        // Create constraints for equal column widths
        let column_width = max_width.saturating_sub(num_columns + 1) / num_columns; // Account for borders
        let constraints: Vec<Constraint> =
            vec![Constraint::Length(column_width as u16); num_columns];

        let num_data_rows = rows_data.len();

        // Create data rows with alternating colors
        let rows: Vec<Row> = rows_data
            .into_iter()
            .enumerate()
            .map(|(i, row_data)| {
                let row_style = if i % 2 == 0 {
                    // Even rows: normal background
                    Style::default().fg(Color::White).bg(Color::Black)
                } else {
                    // Odd rows: darker background for contrast
                    Style::default().fg(Color::Gray).bg(Color::DarkGray)
                };

                Row::new(
                    row_data
                        .into_iter()
                        .map(|cell| Cell::from(cell))
                        .collect::<Vec<_>>(),
                )
                .style(row_style)
                .height(1)
            })
            .collect();

        // Create the table with visible borders and frame
        let mut table = Table::new(rows, constraints)
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::default().fg(Color::White)),
            );

        let has_header = header_row.is_some();
        if let Some(header_data) = header_row {
            let header = Row::new(
                header_data
                    .into_iter()
                    .map(|h| Cell::from(h))
                    .collect::<Vec<_>>(),
            )
            .style(Style::default().fg(Color::Yellow).bg(Color::Blue)) // Make header more prominent
            .height(1);
            table = table.header(header);
        }

        // Render the table to a buffer to extract the text
        let buffer = self.render_table_to_text(
            table,
            max_width as u16,
            num_data_rows as u16 + if has_header { 1 } else { 0 },
        );
        result.push_str(&buffer);
        result.push('\n');
        result
    }

    /// Render a ratatui Table to text format
    fn render_table_to_text(&self, table: Table, width: u16, height: u16) -> String {
        // Create a test backend to render the table
        let backend = TestBackend::new(width, height.saturating_add(2)); // Add some height buffer
        let mut terminal = Terminal::new(backend).unwrap();

        let mut table_state = TableState::default();
        let _rendered_buffer = terminal
            .draw(|f| {
                let area = Rect {
                    x: 0,
                    y: 0,
                    width,
                    height: height.saturating_add(2),
                };
                f.render_stateful_widget(table, area, &mut table_state);
            })
            .unwrap();

        // Extract text from the buffer
        let buffer = terminal.backend().buffer();
        let mut result = String::new();

        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                line.push_str(cell.symbol());
            }
            // Remove trailing spaces and add newline
            let trimmed_line = line.trim_end();
            if !trimmed_line.is_empty() {
                result.push_str(trimmed_line);
            }
            result.push('\n');
        }

        result
    }

    /// Clean table cell content
    fn clean_table_cell(&self, cell_html: &str) -> String {
        // Remove HTML tags first (but not entities)
        // Remove code tags but keep their content
        let code_re = Regex::new(r"</?code[^>]*>").unwrap();
        let mut cleaned = code_re.replace_all(cell_html, "").into_owned();

        // Remove span tags but keep their content
        let span_re = Regex::new(r"</?span[^>]*>").unwrap();
        cleaned = span_re.replace_all(&cleaned, "").into_owned();

        // Remove other HTML tags (but not things that look like <BOS> after entity decoding)
        // This regex only matches actual HTML tags, not arbitrary text between < and >
        let tag_re = Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();
        cleaned = tag_re.replace_all(&cleaned, "").into_owned();

        // Now process HTML entities - these should be decoded to their actual characters
        cleaned = cleaned
            .replace("&nbsp;", " ")
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
            .replace("&rsquo;", "\u{2019}");

        // Clean up whitespace
        cleaned = cleaned.replace('\n', " ");
        let multi_space_re = Regex::new(r" +").unwrap();
        cleaned = multi_space_re.replace_all(&cleaned, " ").into_owned();

        cleaned.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_conversion_to_markdown() {
        let generator = TextGenerator::new();

        // Test h1 conversion (should be uppercase)
        let html = "<h1>Main Title</h1>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n# MAIN TITLE\n\n");

        // Test h2 conversion
        let html = "<h2>Chapter Title</h2>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n## Chapter Title\n\n");

        // Test h3 conversion
        let html = "<h3>Section Title</h3>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n### Section Title\n\n");

        // Test h4 conversion
        let html = "<h4>Subsection</h4>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n#### Subsection\n\n");

        // Test h5 conversion
        let html = "<h5>Small Heading</h5>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n##### Small Heading\n\n");

        // Test h6 conversion
        let html = "<h6>Tiny Heading</h6>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n###### Tiny Heading\n\n");

        // Test h7 conversion (rare but supported)
        let html = "<h7>Very Tiny Heading</h7>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n####### Very Tiny Heading\n\n");
    }

    #[test]
    fn test_heading_with_attributes() {
        let generator = TextGenerator::new();

        // Test heading with class attribute
        let html = r#"<h2 class="chapter-title">Chapter One</h2>"#;
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n## Chapter One\n\n");

        // Test heading with id attribute
        let html = r#"<h3 id="section-1">Introduction</h3>"#;
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n### Introduction\n\n");

        // Test heading with multiple attributes (h1 should be uppercase)
        let html = r#"<h1 class="title" id="main" style="color: red;">Book Title</h1>"#;
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n# BOOK TITLE\n\n");
    }

    #[test]
    fn test_heading_with_nested_content() {
        let generator = TextGenerator::new();

        // Test heading with nested emphasis
        let html = "<h2>Chapter <em>One</em></h2>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n## Chapter <em>One</em>\n\n");

        // Test heading with nested strong
        let html = "<h3>Important <strong>Section</strong></h3>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n### Important <strong>Section</strong>\n\n");

        // Test heading with nested span
        let html = r#"<h2>Chapter <span class="number">2</span>: The Journey</h2>"#;
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(
            result,
            "\n\n## Chapter <span class=\"number\">2</span>: The Journey\n\n"
        );
    }

    #[test]
    fn test_multiple_headings() {
        let generator = TextGenerator::new();

        // Test multiple headings in sequence (h1 should be uppercase)
        let html = "<h1>Title</h1><p>Some text</p><h2>Chapter</h2><p>More text</p><h3>Section</h3>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(
            result,
            "\n\n# TITLE\n\n<p>Some text</p>\n\n## Chapter\n\n<p>More text</p>\n\n### Section\n\n"
        );
    }

    #[test]
    fn test_heading_with_whitespace() {
        let generator = TextGenerator::new();

        // Test heading with extra whitespace
        let html = "<h2>  Chapter Title  </h2>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n## Chapter Title\n\n");

        // Test heading with newlines
        let html = "<h3>\n    Section Title\n    </h3>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n### Section Title\n\n");
    }

    #[test]
    fn test_heading_edge_cases() {
        let generator = TextGenerator::new();

        // Test empty heading
        let html = "<h2></h2>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n## \n\n");

        // Test heading with only whitespace
        let html = "<h3>   </h3>";
        let result = generator.convert_headings_to_markdown(html);
        assert_eq!(result, "\n\n### \n\n");
    }

    #[test]
    fn test_duplicate_title_removal() {
        let generator = TextGenerator::new();

        // Simulate the HTML structure found in careless.epub
        // This reproduces the exact scenario where title appears twice:
        // 1. Title extraction finds "Simpleminded Hope" from h1 tag
        // 2. But after HTML processing, the h1 content still appears in text
        // 3. The duplicate removal logic fails because it tries to remove the h1 tag but content remains
        let html_content = r#"
<!DOCTYPE html>
<html>
<head>
    <title>1. Simpleminded Hope</title>
</head>
<body>
    <h1 class="CHAPTER" id="ch1">
        <a href="contents.xhtml#c_ch1"><span class="CN">1</span>
        <span class="CT">Simpleminded Hope</span></a>
    </h1>
    <p>This is the chapter content that should not be duplicated.</p>
</body>
</html>
        "#;

        let extracted_title = generator.extract_chapter_title(html_content);
        // The h1 tag contains "1 Simpleminded Hope" (without period), which should be prioritized over title tag
        assert_eq!(extracted_title, Some("1 Simpleminded Hope".to_string()));

        let cleaned_content = generator.clean_html_content(html_content, 80);

        assert!(
            !cleaned_content.contains("Simpleminded Hope"),
            "DUPLICATE TITLE BUG: Content contains duplicate title text in: '{}'",
            cleaned_content
        );

        assert!(
            cleaned_content.contains("This is the chapter content"),
            "Content should contain the actual chapter text but found: '{}'",
            cleaned_content
        );
    }

    #[test]
    fn test_dialog_formatting() {
        let generator = TextGenerator::new();

        // Test case with dialog (should be formatted without empty lines between)
        let text_with_dialog = "Some narrative text here.\n\n— А кем работаешь?\n\n— Я аналитик, работаю на рынке ценных бумаг.\n\n— Пирамиды, что ли? Ваучеры?\n\n— Нет, что вы... Я работаю в брокере...\n\n— Ставки на спорт?! Ты кого привела в наш дом?!\n\nMore narrative text here.";

        let formatted = generator.format_text_with_spacing(text_with_dialog);

        // Dialog lines should be on consecutive lines without empty lines between them
        assert!(formatted.contains("— А кем работаешь?\n— Я аналитик, работаю на рынке ценных бумаг.\n— Пирамиды, что ли? Ваучеры?\n— Нет, что вы... Я работаю в брокере...\n— Ставки на спорт?! Ты кого привела в наш дом?!"));

        // There should be empty lines before and after the dialog block
        assert!(formatted.contains("Some narrative text here.\n\n— А кем работаешь?"));
        assert!(formatted.contains(
            "— Ставки на спорт?! Ты кого привела в наш дом?!\n\nMore narrative text here."
        ));
    }

    #[test]
    fn test_dialog_detection_various_dashes() {
        let generator = TextGenerator::new();

        // Test various dash types
        assert!(generator.is_dialog_line("- This is dialog"));
        assert!(generator.is_dialog_line("– This is dialog"));
        assert!(generator.is_dialog_line("— This is dialog"));
        assert!(generator.is_dialog_line("  — This is dialog")); // With leading spaces
        assert!(!generator.is_dialog_line("This is not dialog"));
        assert!(!generator.is_dialog_line("This has dash - in middle"));
    }

    #[test]
    fn test_dialog_minimum_lines() {
        let generator = TextGenerator::new();

        // Test that single dialog line is treated as regular paragraph
        let text_with_one_dash = "Some text.\n\n— Single line.\n\nMore text.";
        let formatted = generator.format_text_with_spacing(text_with_one_dash);

        // Should have empty lines around single dash line (not treated as dialog)
        assert!(formatted.contains("Some text.\n\n— Single line.\n\nMore text."));

        // Test that 2+ dialog lines are treated as dialog
        let text_with_two_dashes = "Some text.\n\n— First line.\n\n— Second line.\n\nMore text.";
        let formatted = generator.format_text_with_spacing(text_with_two_dashes);

        // Should NOT have empty lines between dialog lines
        assert!(formatted.contains("— First line.\n— Second line."));
        assert!(formatted.contains("— Second line.\n\nMore text."));

        // Test that 3+ dialog lines are also treated as dialog
        let text_with_three_dashes =
            "Some text.\n\n— First line.\n\n— Second line.\n\n— Third line.\n\nMore text.";
        let formatted = generator.format_text_with_spacing(text_with_three_dashes);

        // Should NOT have empty lines between dialog lines
        assert!(formatted.contains("— First line.\n— Second line.\n— Third line."));
    }

    #[test]
    fn test_table_parsing() {
        let generator = TextGenerator::new();

        // Test table parsing with the example from the user
        let html_with_table = r#"
        <p>Some text before the table.</p>
        <table id="ch01_table_1_1730130814941480">
            <caption><span class="label">Table 1-1. </span>Training samples from the sentence "I love street food."</caption>
            <thead>
                <tr>
                    <th>Input (context)</th>
                    <th>Output (next token)</th>
                </tr>
            </thead>
            <tr>
                <td><code>&lt;BOS&gt;</code></td>
                <td><code>I</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I</code></td>
                <td><code>love</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I, love</code></td>
                <td><code>street</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I, love, street</code></td>
                <td><code>food</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I, love, street, food</code></td>
                <td><code>.</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I, love, street, food, .</code></td>
                <td><code>&lt;EOS&gt;</code></td>
            </tr>
        </table>
        <p>Some text after the table.</p>
        "#;

        let cleaned = generator.clean_html_content(html_with_table, 80);

        // Check that the table caption is present
        assert!(
            cleaned
                .contains("Table 1-1. Training samples from the sentence \"I love street food.\"")
        );

        // Check that table headers are present
        assert!(cleaned.contains("Input (context)"));
        assert!(cleaned.contains("Output (next token)"));

        // Check that table data is present
        assert!(
            cleaned.contains("<BOS>"),
            "Missing <BOS> in output:\n{}",
            cleaned
        );
        assert!(cleaned.contains("<EOS>"));
        assert!(cleaned.contains("love"));
        assert!(cleaned.contains("street"));

        // Check that the text before and after table is preserved
        assert!(cleaned.contains("Some text before the table"));
        assert!(cleaned.contains("Some text after the table"));

        // Check that table formatting is present (Ratatui creates clean aligned tables)
        assert!(cleaned.contains("Input (context)"));
        assert!(cleaned.contains("Output (next token)"));
    }

    #[test]
    fn test_simple_table() {
        let generator = TextGenerator::new();

        let html_with_table = r#"
        <table>
            <tr>
                <th>Name</th>
                <th>Age</th>
            </tr>
            <tr>
                <td>Alice</td>
                <td>30</td>
            </tr>
            <tr>
                <td>Bob</td>
                <td>25</td>
            </tr>
        </table>
        "#;

        let cleaned = generator.clean_html_content(html_with_table, 80);

        // Check table structure
        assert!(cleaned.contains("Name"));
        assert!(cleaned.contains("Age"));
        assert!(cleaned.contains("Alice"));
        assert!(cleaned.contains("30"));
        assert!(cleaned.contains("Bob"));
        assert!(cleaned.contains("25"));
    }

    #[test]
    fn test_table_cell_with_entities() {
        let generator = TextGenerator::new();

        // Test a single cell with entities
        let cell_html = "<code>&lt;BOS&gt;</code>";
        let cleaned_cell = generator.clean_table_cell(cell_html);
        println!("Cell HTML: {}", cell_html);
        println!("Cleaned cell: {}", cleaned_cell);
        assert_eq!(cleaned_cell, "<BOS>");

        // Test another cell with comma
        let cell_html2 = "<code>&lt;BOS&gt;, I</code>";
        let cleaned_cell2 = generator.clean_table_cell(cell_html2);
        println!("Cell HTML 2: {}", cell_html2);
        println!("Cleaned cell 2: {}", cleaned_cell2);
        assert_eq!(cleaned_cell2, "<BOS>, I");
    }

    #[test]
    fn test_table_row_parsing() {
        let generator = TextGenerator::new();

        // Test parsing a full table with BOS/EOS
        let table_html = r#"
        <table>
            <tr>
                <td><code>&lt;BOS&gt;</code></td>
                <td><code>I</code></td>
            </tr>
            <tr>
                <td><code>&lt;BOS&gt;, I</code></td>
                <td><code>love</code></td>
            </tr>
        </table>
        "#;

        let parsed = generator.parse_table(table_html, 80);
        println!("Parsed table:\n{}", parsed);

        assert!(parsed.contains("<BOS>"));
        assert!(parsed.contains("<BOS>, I"));
        assert!(parsed.contains("love"));
        // Check for table content (Ratatui creates clean aligned tables)
        assert!(parsed.len() > 0);
    }

    #[test]
    fn test_link_extraction() {
        let generator = TextGenerator::new();

        // Test HTML with links
        let html = r#"<p>Text with <a href="https://example.com">external link</a> and <a href="chapter2.html">internal link</a>.</p>"#;

        let cleaned = generator.clean_html_content(html, 80);

        // Should contain markdown-style links
        assert!(
            cleaned.contains("[external link](https://example.com)"),
            "External link not converted. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("[internal link](chapter2.html)"),
            "Internal link not converted. Got: {}",
            cleaned
        );
    }

    #[test]
    fn test_heading_conversion_in_clean_html() {
        let generator = TextGenerator::new();

        // Test full HTML processing with headings
        let html = r#"
            <html>
            <body>
                <h1>Book Title</h1>
                <p>This is the introduction.</p>
                <h2>Chapter 1: Getting Started</h2>
                <p>Welcome to chapter one.</p>
                <h3>Section 1.1: Prerequisites</h3>
                <p>Before we begin, you need to know...</p>
                <h4>Subsection: Tools</h4>
                <p>The following tools are required.</p>
            </body>
            </html>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check that headings are converted to Markdown format (h1 should be uppercase)
        assert!(
            cleaned.contains("# BOOK TITLE"),
            "H1 not converted to Markdown or not uppercase. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("## Chapter 1: Getting Started"),
            "H2 not converted to Markdown. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("### Section 1.1: Prerequisites"),
            "H3 not converted to Markdown. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("#### Subsection: Tools"),
            "H4 not converted to Markdown. Got: {}",
            cleaned
        );

        // Ensure paragraph text is preserved
        assert!(cleaned.contains("This is the introduction"));
        assert!(cleaned.contains("Welcome to chapter one"));
        assert!(cleaned.contains("Before we begin, you need to know"));
        assert!(cleaned.contains("The following tools are required"));
    }

    #[test]
    fn test_heading_conversion_with_mixed_content() {
        let generator = TextGenerator::new();

        // Test headings mixed with other HTML elements
        let html = r#"
            <h2>Chapter <em>Two</em>: The <strong>Adventure</strong></h2>
            <p>Some text with <a href="link.html">a link</a>.</p>
            <h3>Section with <code>code</code></h3>
            <blockquote>A quote here</blockquote>
            <h4>Lists and More</h4>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
            </ul>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check headings are converted properly even with nested tags
        assert!(
            cleaned.contains("##"),
            "H2 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("###"),
            "H3 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("####"),
            "H4 markdown not found. Got: {}",
            cleaned
        );
    }

    #[test]
    fn test_real_world_chapter_with_labels() {
        let generator = TextGenerator::new();

        // Test real-world HTML from a technical book with chapter labels and complex structure
        let html = r#"
            <h1><span class="label">Chapter 1. </span>Introduction to Building AI Applications with Foundation Models</h1>
            <p><a contenteditable="false" data-primary="application building" data-type="indexterm" id="ch01.html0"></a>If I could use only one word to describe AI post-2020, it'd be <em>scale</em>. The AI models behind applications like ChatGPT, Google's Gemini, and Midjourney are at such a scale that they're consuming <a href="https://oreil.ly/J0IyO">a nontrivial portion</a> of the world's electricity, and we're at risk of <a href="https://arxiv.org/abs/2211.04325">running out of publicly available internet data</a> to train them.</p>
            <p>The scaling up of AI models has two major consequences. First, AI models are becoming more powerful and capable of more tasks, enabling more applications. More people and teams leverage AI to increase productivity, create economic value, and improve quality of life.</p>
            <p>Second, training large language models (LLMs) requires data, compute resources, and specialized talent that only a few organizations can afford. This has led to the emergence of <em>model as a service</em>: models developed by these few organizations are made available for others to use as a service. Anyone who wishes to leverage AI to build applications can now use these models to do so without having to invest up front in building a model.</p>
            <p>In short, the demand for AI applications has increased while the barrier to entry for building AI applications has decreased. This has turned <em>AI engineering</em>—the process of building applications on top of readily available models—into one of the fastest-growing engineering disciplines.</p>
            <p>Building applications on top of machine learning (ML) models isn't new. Long before LLMs became prominent, AI was already powering many applications, including product recommendations, fraud detection, and churn prediction. While many principles of productionizing AI applications remain the same, the new generation of large-scale, readily available models brings about new possibilities and new challenges, which are the focus of this book.</p>
            <p>This chapter begins with an overview of foundation models, the key catalyst behind the explosion of AI engineering. I'll then discuss a range of successful AI use cases, each illustrating what AI is good and not yet good at. As AI's capabilities expand daily, predicting its future possibilities becomes increasingly challenging. However, existing application patterns can help uncover opportunities today and offer clues about how AI may continue to be used in the future.</p>
            <p>To close out the chapter, I'll provide an overview of the new AI stack, including what has changed with foundation models, what remains the same, and how the role of an AI engineer today differs from that of a traditional ML engineer.<sup><a data-type="noteref" id="id534-marker" href="ch01.html#id534">1</a></sup></p>
            <section data-type="sect1" data-pdf-bookmark="The Rise of AI Engineering"><div class="sect1" id="ch01_the_rise_of_ai_engineering_1730130814984854">
                <h1>The Rise of AI Engineering</h1>
                <p><a contenteditable="false" data-primary="AI engineering (AIE)" data-secondary="rise of AI engineering" data-type="indexterm" id="ch01.html1"></a><a contenteditable="false" data-primary="application building" data-secondary="rise of AI engineering" data-type="indexterm" id="ch01.html2"></a>Foundation models emerged from large language models, which, in turn, originated as just language models. While applications like ChatGPT and GitHub's Copilot may seem to have come out of nowhere, they are the culmination of decades of technology advancements, with the first language models emerging in the 1950s. This section traces the key breakthroughs that enabled the evolution from language models to AI engineering.</p>
                <section data-type="sect2" data-pdf-bookmark="From Language Models to Large Language Models"><div class="sect2" id="ch01_from_language_models_to_large_language_models_1730130814984966">
                    <h2>From Language Models to Large Language Models</h2>
                </div></section>
            </div></section>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Use multiline regex to check that key elements are present in the output
        let expected_patterns = vec![
            r"#.*CHAPTER 1.*INTRODUCTION TO BUILDING AI APPLICATIONS", // H1 heading
            r"#.*THE RISE OF AI ENGINEERING",                          // Nested H1
            r"##.*From Language Models to Large Language Models",      // H2 heading
            r"_scale_",                                                // Emphasis preserved
            r"\[a nontrivial portion\]\(https://oreil\.ly/J0IyO\)",    // Link 1
            r"\[running out of publicly available internet data\]\(https://arxiv\.org/abs/2211\.04325\)", // Link 2
            r"If I could use only one word to describe AI post-2020", // Text content
            r"model as a service",                                    // Key phrase
            r"AI engineering",                                        // Key phrase
            r"traditional ML engineer",                               // Text with footnote
        ];

        for pattern in expected_patterns {
            let re = Regex::new(pattern).unwrap();
            assert!(
                re.is_match(&cleaned),
                "Pattern '{}' not found in output. Got:\n{}",
                pattern,
                cleaned
            );
        }
    }

    #[test]
    fn test_complex_nested_heading_structure() {
        let generator = TextGenerator::new();

        // Test various complex heading structures that might appear in real EPUBs
        let html = r#"
            <h1><span class="part-number">Part I.</span> <span class="part-title">Getting Started</span></h1>
            <h2><span class="chapter-number">1.</span> <span class="chapter-title">Introduction</span></h2>
            <h3><span class="section-number">1.1</span> <span class="section-title">Overview</span></h3>
            <h4><span>1.1.1</span> Background</h4>
            <h5>Details</h5>
            <h6>Fine Print</h6>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // All headings should be converted to appropriate markdown levels
        assert!(
            cleaned.contains("#"),
            "No markdown headings found. Got: {}",
            cleaned
        );

        // Check that nested spans are preserved in the heading content
        assert!(
            cleaned.contains("PART I") || cleaned.contains("PART I."),
            "Part number not found in output. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("GETTING STARTED"),
            "Part title not found in output. Got: {}",
            cleaned
        );

        // Verify heading hierarchy is preserved
        assert!(
            cleaned.contains("##"),
            "H2 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("###"),
            "H3 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("####"),
            "H4 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("#####"),
            "H5 markdown not found. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("######"),
            "H6 markdown not found. Got: {}",
            cleaned
        );
    }

    #[test]
    fn test_unordered_list_conversion() {
        let generator = TextGenerator::new();

        // Test simple unordered list
        let html = r#"
        <p>Some text before list.</p>
        <ul>
            <li>First item</li>
            <li>Second item</li>
            <li>Third item</li>
        </ul>
        <p>Some text after list.</p>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check that list items are converted to bullet points
        assert!(
            cleaned.contains("• First item"),
            "First item not converted to bullet. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("• Second item"),
            "Second item not converted to bullet. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("• Third item"),
            "Third item not converted to bullet. Got: {}",
            cleaned
        );

        // Check that surrounding text is preserved
        assert!(cleaned.contains("Some text before list"));
        assert!(cleaned.contains("Some text after list"));
    }

    #[test]
    fn test_ordered_list_conversion() {
        let generator = TextGenerator::new();

        // Test simple ordered list
        let html = r#"
        <p>Steps to follow:</p>
        <ol>
            <li>Open the file</li>
            <li>Edit the content</li>
            <li>Save and close</li>
        </ol>
        <p>That's it!</p>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check that list items are converted to numbered format
        assert!(
            cleaned.contains("1. Open the file"),
            "First item not numbered. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("2. Edit the content"),
            "Second item not numbered. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("3. Save and close"),
            "Third item not numbered. Got: {}",
            cleaned
        );

        // Check that surrounding text is preserved
        assert!(cleaned.contains("Steps to follow"));
        assert!(cleaned.contains("That's it!"));
    }

    #[test]
    fn test_list_formatting_no_empty_lines() {
        let generator = TextGenerator::new();

        // Test that list items are not separated by empty lines
        let text_with_list = "Some paragraph.\n\n• First item\n\n• Second item\n\n• Third item\n\nAnother paragraph.";

        let formatted = generator.format_text_with_spacing(text_with_list);

        // List items should be on consecutive lines
        assert!(
            formatted.contains("• First item\n• Second item\n• Third item"),
            "List items should not have empty lines between them. Got: {}",
            formatted
        );

        // There should be empty lines before and after the list block
        assert!(formatted.contains("Some paragraph.\n\n• First item"));
        assert!(formatted.contains("• Third item\n\nAnother paragraph"));
    }

    #[test]
    fn test_numbered_list_formatting() {
        let generator = TextGenerator::new();

        // Test that numbered list items are not separated by empty lines
        let text_with_list =
            "Introduction.\n\n1. First step\n\n2. Second step\n\n3. Third step\n\nConclusion.";

        let formatted = generator.format_text_with_spacing(text_with_list);

        // List items should be on consecutive lines
        assert!(
            formatted.contains("1. First step\n2. Second step\n3. Third step"),
            "Numbered list items should not have empty lines between them. Got: {}",
            formatted
        );
    }

    #[test]
    fn test_mixed_list_types() {
        let generator = TextGenerator::new();

        // Test HTML with both list types
        let html = r#"
        <p>Here are unordered items:</p>
        <ul>
            <li>Apple</li>
            <li>Banana</li>
        </ul>
        <p>And ordered items:</p>
        <ol>
            <li>First</li>
            <li>Second</li>
        </ol>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check both list types are converted
        assert!(cleaned.contains("• Apple"));
        assert!(cleaned.contains("• Banana"));
        assert!(cleaned.contains("1. First"));
        assert!(cleaned.contains("2. Second"));
    }

    #[test]
    fn test_nested_html_in_list_items() {
        let generator = TextGenerator::new();

        // Test list items with nested HTML
        let html = r#"
        <ul>
            <li>Item with <strong>bold</strong> text</li>
            <li>Item with <em>italic</em> text</li>
            <li>Item with <a href="link.html">a link</a></li>
        </ul>
        "#;

        let cleaned = generator.clean_html_content(html, 80);

        // Check that list items are converted and nested tags are processed
        assert!(
            cleaned.contains("• Item with **bold** text"),
            "Bold not preserved in list. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("• Item with _italic_ text"),
            "Italic not preserved in list. Got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("• Item with [a link](link.html)"),
            "Link not preserved in list. Got: {}",
            cleaned
        );
    }

    #[test]
    fn test_mathml_integration() {
        let generator = TextGenerator::new();

        // Test HTML with MathML formula
        let html_with_mathml = r#"
        <p>This is the quadratic formula:</p>
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mrow>
                <mi>x</mi>
                <mo>=</mo>
                <mfrac>
                    <mrow>
                        <mo>-</mo>
                        <mi>b</mi>
                        <mo>±</mo>
                        <msqrt>
                            <mrow>
                                <msup>
                                    <mi>b</mi>
                                    <mn>2</mn>
                                </msup>
                                <mo>-</mo>
                                <mn>4</mn>
                                <mi>a</mi>
                                <mi>c</mi>
                            </mrow>
                        </msqrt>
                    </mrow>
                    <mrow>
                        <mn>2</mn>
                        <mi>a</mi>
                    </mrow>
                </mfrac>
            </mrow>
        </math>
        <p>Where a, b, and c are coefficients.</p>
        "#;

        let cleaned = generator.clean_html_content(html_with_mathml, 80);

        // Check that the text before and after is preserved
        assert!(cleaned.contains("This is the quadratic formula"));
        assert!(cleaned.contains("Where a, b, and c are coefficients"));

        // Check that some mathematical content is present
        // The exact format will depend on the MathML renderer output
        assert!(cleaned.contains("x") || cleaned.contains("="));

        println!("MathML test result:\n{}", cleaned);
    }

    #[test]
    fn test_wide_table_wrapping() {
        let generator = TextGenerator::new();

        let table_html = r#"
        <table>
            <caption>Table 1-4. How different responsibilities of model development have changed with foundation models.</caption>
            <tr>
                <th>Category</th>
                <th>Building with traditional ML</th>
                <th>Building with foundation models</th>
            </tr>
            <tr>
                <td>Modeling and training</td>
                <td>ML knowledge is required for training a model from scratch</td>
                <td>ML knowledge is a nice-to-have, not a must-have</td>
            </tr>
            <tr>
                <td>Dataset engineering</td>
                <td>More about feature engineering, especially with tabular data</td>
                <td>Less about feature engineering and more about data deduplication, tokenization, context retrieval, and quality control</td>
            </tr>
        </table>
        "#;

        // Test with narrow width to force wrapping
        let parsed = generator.parse_table(table_html, 60);
        println!("Wrapped table (60 chars):\n{}", parsed);

        // Should contain wrapped text
        assert!(parsed.contains("Table 1-4"));
        assert!(parsed.contains("foundation models"));

        // Test that table wrapping works (borders may extend slightly beyond target width)
        // The wrapped text shows good column distribution
        println!("Table lines:");
        for line in parsed.lines() {
            println!("{} chars: {}", line.len(), line);
        }
    }
}
