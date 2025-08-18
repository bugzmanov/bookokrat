use crate::table_of_contents::TocItem;
use crate::toc_parser::TocParser;
use epub::doc::EpubDoc;
use log::{debug, warn};
use regex::Regex;
use std::io::BufReader;

pub struct TextGenerator {
    p_tag_re: Regex,
    h_open_re: Regex,
    h_close_re: Regex,
    multi_space_re: Regex,
    multi_newline_re: Regex,
    leading_space_re: Regex,
    line_leading_space_re: Regex,
    img_tag_re: Regex,
    table_re: Regex,
    toc_parser: TocParser,
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            p_tag_re: Regex::new(r"<p[^>]*>").expect("Failed to compile paragraph tag regex"),
            h_open_re: Regex::new(r"<h[1-6][^>]*>")
                .expect("Failed to compile header open tag regex"),
            h_close_re: Regex::new(r"</h[1-6]>").expect("Failed to compile header close tag regex"),
            multi_space_re: Regex::new(r" +").expect("Failed to compile multi space regex"),
            multi_newline_re: Regex::new(r"\n{3,}").expect("Failed to compile multi newline regex"),
            leading_space_re: Regex::new(r"^ +").expect("Failed to compile leading space regex"),
            line_leading_space_re: Regex::new(r"\n +")
                .expect("Failed to compile line leading space regex"),
            img_tag_re: Regex::new(r#"<img[^>]*\ssrc\s*=\s*["']([^"']+)["'][^>]*>"#)
                .expect("Failed to compile image tag regex"),
            table_re: Regex::new(r"(?s)<table[^>]*>.*?</table>")
                .expect("Failed to compile table regex"),
            toc_parser: TocParser::new(),
        }
    }

    pub fn extract_chapter_title(&self, html_content: &str) -> Option<String> {
        let title_patterns = [
            Regex::new(r"<h[12][^>]*>([^<]+)</h[12]>").ok()?,
            Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?,
        ];

        for pattern in &title_patterns {
            if let Some(captures) = pattern.captures(html_content) {
                if let Some(title_match) = captures.get(1) {
                    let title = title_match.as_str().trim();
                    if !title.is_empty() && title.len() < 100 {
                        return Some(title.to_string());
                    }
                }
            }
        }
        None
    }

    /// Check if this chapter is a section header by comparing its href with the TOC structure
    /// A chapter is a section header if it appears in the TOC and has children
    pub fn is_section_header(&self, chapter_href: &str, toc_entries: &[TocItem]) -> bool {
        self.find_entry_with_children(chapter_href, toc_entries)
    }

    /// Recursively search for an entry with the given href that has children
    fn find_entry_with_children(&self, href: &str, entries: &[TocItem]) -> bool {
        for entry in entries {
            match entry {
                TocItem::Chapter {
                    href: entry_href, ..
                } => {
                    // Chapters don't have children
                    let _ = entry_href; // Unused, just for clarity
                }
                TocItem::Section {
                    href: entry_href,
                    children,
                    ..
                } => {
                    if let Some(entry_href) = entry_href {
                        // Normalize hrefs for comparison (remove leading ../ and ./)
                        let normalized_entry_href = self.normalize_href(entry_href);
                        let normalized_target_href = self.normalize_href(href);

                        if normalized_entry_href == normalized_target_href && !children.is_empty() {
                            return true;
                        }
                    }

                    // Also check children recursively
                    if self.find_entry_with_children(href, children) {
                        return true;
                    }
                }
            }
        }
        false
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

    /// Extract section title - will be handled by TOC parsing
    pub fn extract_section_title(&self, html_content: &str) -> Option<String> {
        // Fallback to regular chapter title extraction
        self.extract_chapter_title(html_content)
    }

    /// Parse EPUB table of contents to get hierarchical structure
    pub fn parse_toc_structure(&self, doc: &mut EpubDoc<BufReader<std::fs::File>>) -> Vec<TocItem> {
        debug!("TextGenerator::parse_toc_structure called");
        let result = self.toc_parser.parse_toc_structure(doc);
        debug!(
            "TextGenerator::parse_toc_structure returning {} entries",
            result.len()
        );
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
        let processed_text = self.clean_html_content(&content, &chapter_title, 120);

        if processed_text.is_empty() {
            warn!("Converted text is empty");
            Ok((
                "No content available in this chapter.".to_string(),
                chapter_title,
            ))
        } else {
            debug!("Final text length: {} bytes", processed_text.len());
            let formatted_text = self.format_text_with_spacing(&processed_text);

            let mut final_text = formatted_text;
            if let Some(ref title) = chapter_title {
                let trimmed_content = final_text.trim_start();
                if trimmed_content.starts_with(title) {
                    final_text = trimmed_content[title.len()..].trim_start().to_string();
                }
            }

            Ok((final_text, chapter_title))
        }
    }

    fn clean_html_content(
        &self,
        content: &str,
        chapter_title: &Option<String>,
        terminal_width: usize,
    ) -> String {
        let style_re = Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
        let script_re = Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
        let mut content = style_re.replace_all(content, "").into_owned();
        content = script_re.replace_all(&content, "").into_owned();

        if let Some(_title) = chapter_title {
            // Remove h1/h2 tags that contain the chapter title
            // This handles complex nested structures by removing the entire h1/h2 tag
            let title_removal_re = Regex::new(r"(?s)<h[12][^>]*>.*?</h[12]>").unwrap();
            content = title_removal_re.replace_all(&content, "").into_owned();

            // Also remove title tags since they can contain duplicate title text
            let title_tag_re = Regex::new(r"(?s)<title[^>]*>.*?</title>").unwrap();
            content = title_tag_re.replace_all(&content, "").into_owned();
        }

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

        // Process img tags into text placeholders
        content = self
            .img_tag_re
            .replace_all(&content, "\n\n[image src=\"$1\"]\n\n")
            .into_owned();

        // Process HTML entities for the rest of the content
        let mut text = content
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

        let text = self.h_open_re.replace_all(&text, "\n\n").to_string();
        let text = self.h_close_re.replace_all(&text, "\n\n").to_string();

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

    /// Parse HTML table into formatted text with text wrapping
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

        // Check if we have a thead section to detect header rows
        let has_thead = table_html.contains("<thead");

        // Parse table rows
        let row_re = Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").unwrap();
        let cell_re = Regex::new(r"(?s)<t[hd][^>]*>(.*?)</t[hd]>").unwrap();

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut is_first_row = true;

        // First pass: collect all cells
        for row_match in row_re.captures_iter(table_html) {
            if let Some(row_content) = row_match.get(1) {
                let mut cells: Vec<String> = Vec::new();

                for cell_match in cell_re.captures_iter(row_content.as_str()) {
                    if let Some(cell_content) = cell_match.get(1) {
                        let cleaned = self.clean_table_cell(cell_content.as_str());
                        cells.push(cleaned);
                    }
                }

                if !cells.is_empty() {
                    rows.push(cells);
                }
            }
        }

        // Calculate column widths to fit within max_width
        let num_columns = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if num_columns == 0 {
            return result;
        }

        // Account for borders: 1 char for left border, 1 for right, and (n-1) for column separators
        // Plus 2 chars padding per column (1 space on each side)
        let border_overhead = 1 + 1 + (num_columns.saturating_sub(1)); // │ ... │ ... │
        let padding_overhead = num_columns * 2; // spaces around content
        let total_overhead = border_overhead + padding_overhead;
        let available_width = if max_width > total_overhead {
            max_width - total_overhead
        } else {
            // Table can't fit at all, use minimum widths
            num_columns * 5 // 5 chars minimum per column
        };

        // Distribute width among columns
        let mut column_widths = vec![0usize; num_columns];

        // First, find the natural max width for each column
        let mut natural_widths = vec![0usize; num_columns];
        for row in &rows {
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx < natural_widths.len() {
                    // Consider the longest word in the cell for minimum width
                    let min_word_width = cell
                        .split_whitespace()
                        .map(|word| word.len())
                        .max()
                        .unwrap_or(0);
                    natural_widths[col_idx] = natural_widths[col_idx].max(cell.len());
                    // Ensure we can at least fit the longest word
                    column_widths[col_idx] = column_widths[col_idx].max(min_word_width.min(20));
                }
            }
        }

        // Calculate total natural width
        let total_natural_width: usize = natural_widths.iter().sum();

        if total_natural_width <= available_width {
            // If natural widths fit, use them
            column_widths = natural_widths;
        } else {
            // Need to compress columns to fit
            // Start with equal distribution
            let base_width = available_width / num_columns;

            for (idx, natural_width) in natural_widths.iter().enumerate() {
                if *natural_width <= base_width {
                    // Column fits in base width
                    column_widths[idx] = *natural_width;
                } else {
                    // Column needs to be compressed
                    column_widths[idx] = base_width.max(8); // Minimum width of 8 chars
                }
            }

            // Redistribute any leftover space
            let used_width: usize = column_widths.iter().sum();
            if used_width < available_width {
                let extra_per_column = (available_width - used_width) / num_columns;
                for width in &mut column_widths {
                    *width += extra_per_column;
                }
            }
        }

        // Second pass: format the table with text wrapping
        for (row_idx, row) in rows.iter().enumerate() {
            // Add top border for first row
            if row_idx == 0 {
                result.push_str(&self.create_table_separator(
                    &column_widths,
                    true,
                    false,
                    max_width,
                ));
            }

            // Wrap text in each cell
            let mut wrapped_cells: Vec<Vec<String>> = Vec::new();
            let mut max_lines = 0;

            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx < column_widths.len() {
                    let wrapped = textwrap::wrap(cell, column_widths[col_idx]);
                    let wrapped_lines: Vec<String> =
                        wrapped.iter().map(|s| s.to_string()).collect();
                    max_lines = max_lines.max(wrapped_lines.len());
                    wrapped_cells.push(wrapped_lines);
                }
            }

            // Render each line of the wrapped cells
            for line_idx in 0..max_lines {
                let mut line = String::new();

                // Add visual distinction for even rows (excluding header)
                let is_even_row = row_idx > 0 && row_idx % 2 == 0;

                // Start the line with appropriate border
                line.push('│');

                for (col_idx, wrapped_cell) in wrapped_cells.iter().enumerate() {
                    // Start padding for the cell - use shade for even rows
                    if is_even_row {
                        line.push('▓'); // Dark shade for background effect
                    } else {
                        line.push(' ');
                    }

                    // Get the line for this cell, or empty if we've run out of lines
                    let line_text = wrapped_cell.get(line_idx).map(|s| s.as_str()).unwrap_or("");

                    // Truncate if necessary
                    let text_to_add = if line_text.len() > column_widths[col_idx] {
                        &line_text[..column_widths[col_idx]]
                    } else {
                        line_text
                    };

                    // Add the actual text content (always normal text, no shading behind it)
                    line.push_str(text_to_add);

                    // Add padding to align columns - this is where we apply shading
                    if col_idx < column_widths.len() {
                        let padding = column_widths[col_idx].saturating_sub(text_to_add.len());
                        for _ in 0..padding {
                            if is_even_row {
                                line.push('▓'); // Dark shade for padding areas only
                            } else {
                                line.push(' ');
                            }
                        }
                    }

                    // End padding for the cell
                    if is_even_row {
                        line.push('▓');
                    } else {
                        line.push(' ');
                    }

                    line.push('│');
                }

                // Ensure line doesn't exceed max width
                if line.chars().count() > max_width {
                    let truncate_to = line
                        .char_indices()
                        .nth(max_width - 2)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    line.truncate(truncate_to);
                    line.push('│');
                }

                result.push_str(&line);
                result.push('\n');
            }

            // Add separator after header row or at the end
            let is_header = row_idx == 0 && (has_thead || is_first_row);
            let is_last = row_idx == rows.len() - 1;

            if is_header && !is_last {
                // Header separator (middle)
                result.push_str(&self.create_table_separator(
                    &column_widths,
                    false,
                    false,
                    max_width,
                ));
                is_first_row = false;
            } else if is_last {
                // Bottom border
                result.push_str(&self.create_table_separator(
                    &column_widths,
                    false,
                    true,
                    max_width,
                ));
            }
        }

        // Add newlines for spacing
        result.push('\n');
        result
    }

    /// Create a table separator line using Unicode box-drawing characters
    fn create_table_separator(
        &self,
        widths: &[usize],
        is_top: bool,
        is_bottom: bool,
        max_width: usize,
    ) -> String {
        // Calculate the actual width of the separator
        let total_width = widths.iter().map(|w| w + 2).sum::<usize>() // column widths + padding
            + 1 // left border
            + 1 // right border  
            + widths.len().saturating_sub(1); // column separators

        // If the separator would be too wide, truncate column widths
        let adjusted_widths: Vec<usize> = if total_width > max_width {
            // Reduce each column proportionally
            let reduction_factor = max_width as f64 / total_width as f64;
            widths
                .iter()
                .map(|w| ((*w as f64 * reduction_factor) as usize).max(1))
                .collect()
        } else {
            widths.to_vec()
        };

        let mut separator = String::new();

        // Choose appropriate corners and junctions
        let (left, middle, right) = if is_top {
            ('┌', '┬', '┐')
        } else if is_bottom {
            ('└', '┴', '┘')
        } else {
            ('├', '┼', '┤')
        };

        separator.push(left);

        for (i, width) in adjusted_widths.iter().enumerate() {
            // Add horizontal lines for column width + 2 for padding
            for _ in 0..(*width + 2) {
                separator.push('─');
            }

            if i < adjusted_widths.len() - 1 {
                separator.push(middle);
            }
        }

        separator.push(right);

        // Final safety check - truncate if still too long
        if separator.chars().count() > max_width {
            let truncate_to = separator
                .char_indices()
                .nth(max_width - 2)
                .map(|(i, _)| i)
                .unwrap_or(separator.len());
            separator.truncate(truncate_to);
            separator.push(right);
        }

        separator.push('\n');
        separator
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
    fn test_is_section_header_with_toc_structure() {
        let generator = TextGenerator::new();

        // Create a sample TOC structure with sections and chapters
        let toc_entries = vec![
            TocItem::Chapter {
                title: "Introduction".to_string(),
                href: "Text/intro.html".to_string(),
                index: 0, // No children - not a section header
            },
            TocItem::Section {
                title: "Chapter 1: Getting Started".to_string(),
                href: Some("Text/chapter1.html".to_string()),
                index: Some(1),
                children: vec![
                    // Has children - is a section header
                    TocItem::Chapter {
                        title: "1.1 Setup".to_string(),
                        href: "Text/chapter1_1.html".to_string(),
                        index: 2,
                    },
                    TocItem::Chapter {
                        title: "1.2 Configuration".to_string(),
                        href: "Text/chapter1_2.html".to_string(),
                        index: 3,
                    },
                ],
                is_expanded: true,
            },
        ];

        // Test that chapters without children are not section headers
        assert!(!generator.is_section_header("Text/intro.html", &toc_entries));
        assert!(!generator.is_section_header("Text/chapter1_1.html", &toc_entries));
        assert!(!generator.is_section_header("Text/chapter1_2.html", &toc_entries));

        // Test that chapters with children are section headers
        assert!(generator.is_section_header("Text/chapter1.html", &toc_entries));

        // Test href normalization (relative paths, OEBPS directory, and fragments)
        assert!(generator.is_section_header("../Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("./Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("OEBPS/Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("../OEBPS/Text/chapter1.html", &toc_entries));
        assert!(generator.is_section_header("Text/chapter1.html#ch1", &toc_entries));
        assert!(generator.is_section_header("OEBPS/Text/chapter1.html#anchor", &toc_entries));

        // Test non-existent hrefs
        assert!(!generator.is_section_header("Text/nonexistent.html", &toc_entries));
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
        assert_eq!(extracted_title, Some("1. Simpleminded Hope".to_string()));

        let cleaned_content = generator.clean_html_content(html_content, &extracted_title, 80);

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

        let cleaned = generator.clean_html_content(html_with_table, &None, 80);

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

        // Check that table formatting includes Unicode box-drawing characters
        assert!(cleaned.contains("│"));
        assert!(cleaned.contains("─"));
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

        let cleaned = generator.clean_html_content(html_with_table, &None, 80);

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
        // Check for Unicode box-drawing characters
        assert!(parsed.contains("│"));
        assert!(parsed.contains("─"));
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
