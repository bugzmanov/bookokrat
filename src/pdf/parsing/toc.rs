//! PDF table of contents structures and extraction helpers.

use mupdf::Document;

/// Target of a TOC entry
#[derive(Clone, Debug)]
pub enum TocTarget {
    /// Internal page (0-indexed)
    InternalPage(usize),
    /// External URI
    External(String),
    /// Printed page number (for display)
    PrintedPage(usize),
}

/// A single entry in the table of contents
#[derive(Clone, Debug)]
pub struct TocEntry {
    /// Display title
    pub title: String,
    /// Nesting level (0 = top level)
    pub level: usize,
    /// Navigation target
    pub target: TocTarget,
}

/// Extract table of contents from document outlines or by scanning pages.
pub fn extract_toc(doc: &Document, page_count: usize) -> Vec<TocEntry> {
    if let Ok(outlines) = doc.outlines() {
        if !outlines.is_empty() {
            let mut entries = Vec::new();
            flatten_outlines(&outlines, 0, &mut entries);
            if looks_like_valid_toc(&entries) {
                return entries;
            }
        }
    }

    // Fallback: scan pages for TOC entries when outlines unavailable
    extract_toc_from_pages(doc, page_count)
}

/// Check if extracted outline entries look like a valid table of contents.
/// A real TOC should have multiple short entries, not random body text.
fn looks_like_valid_toc(entries: &[TocEntry]) -> bool {
    // Need at least 3 entries for a real TOC
    if entries.len() < 3 {
        return false;
    }

    // Check that entries look like chapter titles (not body text)
    let mut valid_count = 0;
    for entry in entries {
        let title_len = entry.title.chars().count();
        // Chapter titles are typically under 100 characters
        // Real body text tends to have multiple sentences
        let sentence_count = entry
            .title
            .matches(". ")
            .count()
            .saturating_add(entry.title.matches("? ").count())
            .saturating_add(entry.title.matches("! ").count());

        // Allow titles up to 100 chars with at most one sentence break
        // (some titles legitimately have periods, e.g., "Example: Using ASP.NET Core")
        if title_len <= 100 && sentence_count <= 1 {
            valid_count += 1;
        }
    }

    // At least 70% of entries should look like valid chapter titles
    valid_count * 100 / entries.len() >= 70
}

fn flatten_outlines(outlines: &[mupdf::Outline], level: usize, entries: &mut Vec<TocEntry>) {
    for outline in outlines {
        let target = if let Some(dest) = outline.dest {
            Some(TocTarget::InternalPage(dest.loc.page_number as usize))
        } else {
            outline
                .uri
                .as_ref()
                .map(|uri| TocTarget::External(uri.clone()))
        };

        if let Some(target) = target {
            let title = strip_outline_leader_chars(outline.title.trim());
            if !title.is_empty() {
                entries.push(TocEntry {
                    title,
                    level,
                    target,
                });
            }
        }

        if !outline.down.is_empty() {
            flatten_outlines(&outline.down, level + 1, entries);
        }
    }
}

/// Strip trailing leader characters from outline titles.
/// PDFs often embed visual leader dots (◆, ·, ., etc.) in outline titles.
fn strip_outline_leader_chars(title: &str) -> String {
    let chars: Vec<char> = title.chars().collect();
    if chars.len() < 3 {
        return title.to_string();
    }

    // Find trailing run of repeated characters (leader dots pattern)
    let last_char = chars[chars.len() - 1];
    let mut run_start = chars.len() - 1;

    // Count how many consecutive identical characters at the end
    while run_start > 0 && chars[run_start - 1] == last_char {
        run_start -= 1;
    }

    let run_length = chars.len() - run_start;

    // If we have 3+ repeated non-alphanumeric chars at the end, strip them
    if run_length >= 3 && !last_char.is_alphanumeric() && !last_char.is_whitespace() {
        let result: String = chars[..run_start].iter().collect();
        return result.trim_end().to_string();
    }

    title.to_string()
}

// ============================================================================
// TOC Fallback Extraction (when PDF has no embedded outlines)
// ============================================================================

/// Extract TOC by scanning page content when outlines are unavailable.
fn extract_toc_from_pages(doc: &Document, n_pages: usize) -> Vec<TocEntry> {
    let max_scan = n_pages.min(30);
    let Some(start_idx) = find_toc_start(doc, max_scan) else {
        return Vec::new();
    };
    let start_idx = backtrack_toc_start(doc, start_idx);

    let mut entries = Vec::new();
    let mut saw_entries = false;
    let end = (start_idx + 5).min(n_pages);

    for page_idx in start_idx..end {
        let Ok(page) = doc.load_page(page_idx as i32) else {
            continue;
        };
        let page_entries = extract_toc_entries_from_page(&page);
        if !page_entries.is_empty() {
            entries.extend(page_entries);
            saw_entries = true;
        } else if saw_entries {
            break;
        }
    }

    // Infer hierarchy from section numbering patterns (e.g., "1", "1.1", "2")
    infer_toc_hierarchy(&mut entries);

    entries
}

/// Find the page where TOC starts.
fn find_toc_start(doc: &Document, max_pages: usize) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    let mut earliest: Option<(usize, usize)> = None;
    let mut headings = Vec::new();

    for page_idx in 0..max_pages {
        let Ok(page) = doc.load_page(page_idx as i32) else {
            continue;
        };
        let Ok(bounds) = page.bounds() else {
            continue;
        };
        let page_height = bounds.y1 - bounds.y0;
        let line_bounds = super::super::worker::extract_line_bounds(&page, 1.0);

        let mut has_heading = false;
        for line in &line_bounds {
            if line.y0 > page_height * 0.3 {
                continue;
            }
            let text = line_text(line);
            if is_toc_heading(&text) {
                has_heading = true;
                break;
            }
        }

        if !has_heading {
            continue;
        }

        headings.push(page_idx);

        let mut score = 0;
        for line in &line_bounds {
            let text = line_text(line);
            if let Some((_, number_start)) = extract_trailing_page_number(&text) {
                if strip_toc_title(&text, number_start).is_some() {
                    score += 1;
                }
            }
        }

        if score >= 3 && earliest.is_none_or(|(idx, _)| page_idx < idx) {
            earliest = Some((page_idx, score));
        }

        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((page_idx, score));
        }
    }

    let best_idx = best.map(|(idx, _)| idx);
    if let Some(best_idx) = best_idx {
        let earliest_near = headings
            .into_iter()
            .filter(|idx| *idx <= best_idx && best_idx.saturating_sub(*idx) <= 2)
            .min();
        if let Some(idx) = earliest_near {
            return Some(idx);
        }
    }

    earliest.or(best).map(|(idx, _)| idx)
}

/// Check if TOC starts on an earlier page.
fn backtrack_toc_start(doc: &Document, start_idx: usize) -> usize {
    let mut candidate = start_idx;
    let scan_start = start_idx.saturating_sub(2);

    for page_idx in (scan_start..start_idx).rev() {
        let Ok(page) = doc.load_page(page_idx as i32) else {
            continue;
        };
        let entries = extract_toc_entries_from_page(&page);
        if entries.len() >= 2 {
            candidate = page_idx;
        } else if candidate != start_idx {
            break;
        }
    }

    candidate
}

/// Extract TOC entries from a single page.
fn extract_toc_entries_from_page(page: &mupdf::Page) -> Vec<TocEntry> {
    let Ok(bounds) = page.bounds() else {
        return Vec::new();
    };
    let page_height = bounds.y1 - bounds.y0;
    let page_width = bounds.x1 - bounds.x0;
    let line_bounds = super::super::worker::extract_line_bounds(page, 1.0);
    let link_rects = super::super::worker::extract_link_rects(page, 1.0);

    let mut ordered_lines: Vec<_> = line_bounds.iter().collect();
    ordered_lines.sort_by(|a, b| {
        a.y0.partial_cmp(&b.y0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });

    // Find minimum x position to establish left margin baseline
    let min_x = ordered_lines
        .iter()
        .filter(|l| l.y0 >= page_height * 0.08 && l.y1 <= page_height * 0.92)
        .map(|l| l.x0)
        .fold(f32::MAX, f32::min);
    let indent_step = page_width * 0.025; // ~2.5% of page width per indent level

    let mut entries = Vec::new();
    let mut pending_title: Option<(String, usize, f32)> = None;
    let mut pending_prefix: Option<(String, f32)> = None;

    for line in ordered_lines {
        if line.y0 < page_height * 0.08 || line.y1 > page_height * 0.92 {
            continue;
        }

        let raw_text = line_text(line);
        if raw_text.trim().is_empty() {
            continue;
        }

        if is_toc_heading(&raw_text) {
            continue;
        }

        if line_is_roman_numeral_only(&raw_text) && line.y0 < page_height * 0.2 {
            continue;
        }

        // Compute indent level from physical x-position
        let base_level = ((line.x0 - min_x).max(0.0) / indent_step) as usize;

        let line_height = (line.y1 - line.y0).max(1.0);
        let max_gap = line_height * 12.0;

        if let Some((_, _, y1)) = pending_title {
            if line.y0 > y1 + max_gap {
                pending_title = None;
            }
        }
        if let Some((_, y1)) = pending_prefix {
            if line.y0 > y1 + max_gap * 2.0 {
                pending_prefix = None;
            }
        }

        // Split line on bullet separators (■, •, etc.) for multi-entry lines
        let segments = split_on_bullet_separators(&raw_text);

        for text in segments {
            // Use physical indent level from line position
            let level = base_level;

            if let Some((printed_page, number_start)) = extract_trailing_page_number(&text) {
                let use_pending = pending_title
                    .as_ref()
                    .is_some_and(|(_, _, y1)| (line.y0 - *y1).abs() <= max_gap);
                let (mut title, title_level) = if use_pending {
                    let (pending, pending_level, _) = pending_title.take().unwrap();
                    (Some(pending), pending_level)
                } else {
                    (strip_toc_title(&text, number_start), level)
                };

                if let Some((prefix, y1)) = pending_prefix.take() {
                    if (line.y0 - y1).abs() <= max_gap * 2.0 {
                        if let Some(title_text) = title.take() {
                            if !starts_with_digit(&title_text) {
                                title = Some(format!("{prefix} {title_text}"));
                            } else {
                                title = Some(title_text);
                            }
                        }
                    } else {
                        pending_prefix = Some((prefix, y1));
                    }
                }

                let Some(title) = title else {
                    continue;
                };

                let target = if let Some(link) = link_for_line(line, &link_rects) {
                    match &link.target {
                        super::super::types::LinkTarget::Internal { page } => {
                            TocTarget::InternalPage(*page)
                        }
                        super::super::types::LinkTarget::External { uri } => {
                            TocTarget::External(uri.clone())
                        }
                    }
                } else {
                    TocTarget::PrintedPage(printed_page)
                };

                entries.push(TocEntry {
                    title,
                    level: title_level,
                    target,
                });
                continue;
            }

            if line_is_dots_only(&text) {
                continue;
            }

            if let Some(number) = extract_standalone_number(&text) {
                if let Some((pending, pending_level, y1)) = pending_title.take() {
                    if (line.y0 - y1).abs() <= max_gap * 2.0 {
                        let mut title = pending;
                        if let Some((prefix, py)) = pending_prefix.take() {
                            if (line.y0 - py).abs() <= max_gap * 2.0 && !starts_with_digit(&title) {
                                title = format!("{prefix} {title}");
                            } else {
                                pending_prefix = Some((prefix, py));
                            }
                        }
                        entries.push(TocEntry {
                            title,
                            level: pending_level,
                            target: TocTarget::PrintedPage(number),
                        });
                        continue;
                    }
                    pending_title = Some((pending, pending_level, y1));
                }

                pending_prefix = Some((number.to_string(), line.y1));
                continue;
            }

            if line_has_letters(&text) {
                let cleaned = text
                    .trim()
                    .trim_matches(|c: char| c == '.' || c == '·')
                    .trim();
                if cleaned.is_empty() {
                    continue;
                }
                if let Some((pending, _, y1)) = pending_title.as_ref() {
                    if contains_digit(pending)
                        && !contains_digit(cleaned)
                        && (line.y0 - *y1).abs() <= max_gap * 2.0
                    {
                        continue;
                    }
                }
                pending_title = Some((cleaned.to_string(), level, line.y1));
            }
        } // end of segments loop
    }

    entries
}

// Helper functions for TOC extraction

fn line_text(line: &super::super::types::LineBounds) -> String {
    line.chars.iter().map(|c| c.c).collect()
}

fn is_toc_heading(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower == "contents" || lower == "table of contents" || lower == "table of content"
}

fn extract_trailing_page_number(text: &str) -> Option<(usize, usize)> {
    let trimmed = text.trim_end();
    if trimmed.len() < 2 {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut end = trimmed.len();

    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b']' {
        end -= 1;
    }
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    // Try Arabic numerals first
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start < end {
        let digits = &trimmed[start..end];
        if let Ok(number) = digits.parse::<usize>() {
            // Reject if:
            // - Number is 0
            // - Number looks like a year (1800-2100)
            // - Number is preceded by '(' (indicates year in citation)
            // - Number is too large for a page number (> 1500)
            let is_year = (1800..=2100).contains(&number);
            let preceded_by_paren =
                start > 0 && (bytes[start - 1] == b'(' || bytes[start - 1] == b'[');
            if number > 0 && number <= 1500 && !is_year && !preceded_by_paren {
                return Some((number, start));
            }
        }
    }

    // Try Roman numerals (i, v, x, l, c, d, m - lowercase only for page numbers)
    start = end;
    while start > 0
        && matches!(
            bytes[start - 1],
            b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm'
        )
    {
        start -= 1;
    }
    if start < end && end - start <= 10 {
        let roman = &trimmed[start..end];
        if let Some(number) = parse_roman_numeral(roman) {
            return Some((number, start));
        }
    }

    None
}

/// Parse a lowercase Roman numeral string to a number.
fn parse_roman_numeral(s: &str) -> Option<usize> {
    let mut total = 0usize;
    let mut prev = 0usize;

    for c in s.chars().rev() {
        let val = match c {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        };
        if val < prev {
            total = total.checked_sub(val)?;
        } else {
            total = total.checked_add(val)?;
        }
        prev = val;
    }

    if total > 0 { Some(total) } else { None }
}

fn strip_toc_title(text: &str, number_start: usize) -> Option<String> {
    let raw = text[..number_start].trim_end();
    let trimmed = raw
        .trim_end_matches(['.', '·', '-', '–', '—'])
        .trim_end_matches('[')
        .trim_end();
    let trimmed = trimmed.trim();
    if trimmed.len() < 2 {
        return None;
    }
    if !line_has_letters(trimmed) {
        return None;
    }
    // Reject entries that look like quotes or body text:
    // - Start with dash/hyphen (attribution like "-Prof. Dr. ...")
    // - Contain sentence patterns (lowercase word after space indicates body text)
    // - Are too long (> 60 chars) and mostly lowercase
    if trimmed.starts_with('-') || trimmed.starts_with('—') || trimmed.starts_with('–') {
        return None;
    }
    // Check for body text pattern: words after first that start lowercase
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() > 2 {
        let lowercase_starts = words[1..]
            .iter()
            .filter(|w| {
                w.chars()
                    .next()
                    .is_some_and(|c| c.is_lowercase() && c.is_alphabetic())
            })
            .count();
        // If more than half of words (after first) start lowercase, likely body text
        if lowercase_starts > words.len() / 2 {
            return None;
        }
    }
    Some(trimmed.to_string())
}

fn extract_standalone_number(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    if trimmed.len() > 4 {
        return None;
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return trimmed.parse::<usize>().ok();
    }
    None
}

fn line_has_letters(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic())
}

/// Split a line on bullet separators (■, •, ◆, etc.) that indicate multiple TOC entries.
/// Returns segments that each potentially contain a TOC entry.
fn split_on_bullet_separators(text: &str) -> Vec<String> {
    // Common bullet/separator characters used in PDF TOCs
    let separators = ['■', '•', '◆', '●', '▪', '◾'];

    // Check if any separator exists
    if !text.chars().any(|c| separators.contains(&c)) {
        return vec![text.to_string()];
    }

    // Split on separators
    let mut segments = Vec::new();
    let mut current = String::new();

    for c in text.chars() {
        if separators.contains(&c) {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current.clear();
        } else {
            current.push(c);
        }
    }

    // Don't forget the last segment
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        segments.push(trimmed);
    }

    if segments.is_empty() {
        vec![text.to_string()]
    } else {
        segments
    }
}

fn line_is_dots_only(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_whitespace() || c == '.' || c == '·')
}

fn line_is_roman_numeral_only(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 6 {
        return false;
    }
    trimmed
        .chars()
        .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
}

fn starts_with_digit(text: &str) -> bool {
    text.chars()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| c.is_ascii_digit())
}

fn contains_digit(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

fn link_for_line<'a>(
    line: &super::super::types::LineBounds,
    links: &'a [super::super::types::LinkRect],
) -> Option<&'a super::super::types::LinkRect> {
    links.iter().find(|link| {
        let topleft_x = link.x0 as f32;
        let topleft_y = link.y0 as f32;
        let bottomright_x = link.x1 as f32;
        let bottomright_y = link.y1 as f32;
        topleft_x < line.x1
            && bottomright_x > line.x0
            && topleft_y < line.y1
            && bottomright_y > line.y0
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test using exact TOC structure from "Dependency Injection" book.
    /// The test PDF contains only the outline metadata (no copyrighted content).
    /// This ensures our TOC extraction correctly handles real-world technical books.
    #[test]
    fn test_di_book_toc_extraction() {
        let path = "tests/testdata/di_book_toc_test.pdf";
        let doc = mupdf::Document::open(path).expect("Failed to open test PDF");
        let page_count = doc.page_count().expect("Failed to get page count") as usize;

        let entries = extract_toc(&doc, page_count);

        // Should not be empty - the outline should be accepted
        assert!(
            !entries.is_empty(),
            "TOC extraction returned empty - outline was likely rejected by looks_like_valid_toc"
        );

        // Should have exactly 229 entries (the DI book's full TOC)
        assert_eq!(
            entries.len(),
            229,
            "Expected 229 TOC entries from DI book, got {}",
            entries.len()
        );

        // Verify first entry (book title)
        assert_eq!(entries[0].title, "Dependency Injection");
        assert_eq!(entries[0].level, 0);
        assert!(matches!(entries[0].target, TocTarget::InternalPage(0)));

        // Verify front matter entries (level 0)
        assert_eq!(entries[1].title, "brief contents");
        assert_eq!(entries[1].level, 0);

        assert_eq!(entries[2].title, "contents");
        assert_eq!(entries[2].level, 0);

        // Verify Part 1 (level 0)
        let part1 = entries
            .iter()
            .find(|e| e.title.contains("Part 1:"))
            .expect("Part 1 not found");
        assert_eq!(part1.level, 0);
        assert_eq!(part1.title, "Part 1: Putting Dependency Injection");

        // Verify Chapter 1 (level 1)
        let chapter1 = entries
            .iter()
            .find(|e| e.title.starts_with("1 The basics"))
            .expect("Chapter 1 not found");
        assert_eq!(chapter1.level, 1);
        assert_eq!(
            chapter1.title,
            "1 The basics of Dependency Injection: What, why, and how"
        );

        // Verify Section 1.1 with tab character (level 2)
        let section_1_1 = entries
            .iter()
            .find(|e| e.title.contains("Writing maintainable code"))
            .expect("Section 1.1 not found");
        assert_eq!(section_1_1.level, 2);
        assert!(
            section_1_1.title.contains('\t'),
            "Section title should contain tab: {}",
            section_1_1.title
        );

        // Verify Subsection 1.1.1 (level 3)
        let subsection_1_1_1 = entries
            .iter()
            .find(|e| e.title.contains("Common myths about DI"))
            .expect("Subsection 1.1.1 not found");
        assert_eq!(subsection_1_1_1.level, 3);

        // Verify Part 4 exists (last part)
        let part4 = entries
            .iter()
            .find(|e| e.title.contains("Part 4:"))
            .expect("Part 4 not found");
        assert_eq!(part4.level, 0);
        assert_eq!(part4.title, "Part 4: DI Containers");

        // Verify last entries (back matter at level 1)
        let glossary = entries.iter().find(|e| e.title == "glossary");
        assert!(glossary.is_some(), "glossary entry not found");
        assert_eq!(glossary.unwrap().level, 1);

        let index = entries.iter().find(|e| e.title == "index");
        assert!(index.is_some(), "index entry not found");
        assert_eq!(index.unwrap().level, 1);

        // Verify page numbers are preserved correctly
        if let TocTarget::InternalPage(page) = chapter1.target {
            assert_eq!(page, 33, "Chapter 1 should be on page 33");
        } else {
            panic!("Expected InternalPage target for Chapter 1");
        }

        // Count entries by level to verify structure
        let level_counts: Vec<usize> = (0..=3)
            .map(|l| entries.iter().filter(|e| e.level == l).count())
            .collect();

        assert!(
            level_counts[0] > 10,
            "Should have front matter and parts at level 0"
        );
        assert!(level_counts[1] > 15, "Should have chapters at level 1");
        assert!(level_counts[2] > 40, "Should have sections at level 2");
        assert!(
            level_counts[3] > 100,
            "Should have many subsections at level 3"
        );
    }

    #[test]
    fn test_looks_like_valid_toc_accepts_di_book_titles() {
        // Sample of actual DI book titles - should all be accepted
        let entries = vec![
            TocEntry {
                title: "Dependency Injection".to_string(),
                level: 0,
                target: TocTarget::InternalPage(0),
            },
            TocEntry {
                title: "Part 1: Putting Dependency Injection".to_string(),
                level: 0,
                target: TocTarget::InternalPage(31),
            },
            TocEntry {
                title: "1 The basics of Dependency Injection: What, why, and how".to_string(),
                level: 1,
                target: TocTarget::InternalPage(33),
            },
            TocEntry {
                title: "1.1\tWriting maintainable code".to_string(),
                level: 2,
                target: TocTarget::InternalPage(35),
            },
            TocEntry {
                title: "1.1.1\tCommon myths about DI".to_string(),
                level: 3,
                target: TocTarget::InternalPage(35),
            },
        ];

        assert!(
            looks_like_valid_toc(&entries),
            "DI book titles should be accepted as valid TOC"
        );
    }

    #[test]
    fn test_looks_like_valid_toc_rejects_body_text() {
        let entries = vec![
            TocEntry {
                title: "This is a very long sentence that looks like body text from a paragraph in the document. It has multiple sentences and exceeds what you'd expect from a chapter title.".to_string(),
                level: 0,
                target: TocTarget::InternalPage(0),
            },
            TocEntry {
                title: "Another long body text entry. This one also has multiple sentences. And even more text here.".to_string(),
                level: 0,
                target: TocTarget::InternalPage(1),
            },
            TocEntry {
                title: "Yet more body text that should not be considered a valid TOC entry. Multiple periods indicate sentences.".to_string(),
                level: 0,
                target: TocTarget::InternalPage(2),
            },
        ];

        assert!(
            !looks_like_valid_toc(&entries),
            "Body text should be rejected"
        );
    }

    #[test]
    fn test_looks_like_valid_toc_requires_minimum_entries() {
        let entries = vec![
            TocEntry {
                title: "Chapter 1".to_string(),
                level: 0,
                target: TocTarget::InternalPage(0),
            },
            TocEntry {
                title: "Chapter 2".to_string(),
                level: 0,
                target: TocTarget::InternalPage(1),
            },
        ];

        assert!(
            !looks_like_valid_toc(&entries),
            "Should require at least 3 entries"
        );
    }
}

// ============================================================================
// TOC Hierarchy Inference
// ============================================================================

/// Parsed section number from a title (e.g., "1", "1.1", "2.3.4").
#[derive(Debug, Clone)]
struct SectionNumber {
    parts: Vec<u32>,
}

impl SectionNumber {
    /// Parse a section number from the beginning of a title.
    /// Returns the parsed number and the remaining title text.
    fn parse(title: &str) -> Option<(Self, String)> {
        let trimmed = title.trim_start();
        if trimmed.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        let mut chars = trimmed.char_indices().peekable();
        let mut last_end = 0;

        // Parse first number
        let mut num_start = None;
        while let Some(&(i, c)) = chars.peek() {
            if c.is_ascii_digit() {
                if num_start.is_none() {
                    num_start = Some(i);
                }
                last_end = i + c.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let start = num_start?;

        let first_num: u32 = trimmed[start..last_end].parse().ok()?;
        parts.push(first_num);

        // Parse subsequent ".N" parts
        while let Some(&(_, c)) = chars.peek() {
            if c == '.' {
                chars.next();
                let mut num_start = None;
                let mut num_end = last_end;
                while let Some(&(i, c)) = chars.peek() {
                    if c.is_ascii_digit() {
                        if num_start.is_none() {
                            num_start = Some(i);
                        }
                        num_end = i + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Some(start) = num_start {
                    if let Ok(num) = trimmed[start..num_end].parse::<u32>() {
                        parts.push(num);
                        last_end = num_end;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Must be followed by whitespace or end of string to be valid
        let after = &trimmed[last_end..];
        let first_after = after.chars().next();
        if first_after.is_some() && !first_after.unwrap().is_whitespace() {
            return None;
        }

        let remaining = after.trim_start().to_string();
        Some((Self { parts }, remaining))
    }

    fn depth(&self) -> usize {
        self.parts.len()
    }

    /// Check if this number is a child of another (e.g., 1.1 is child of 1).
    fn is_child_of(&self, parent: &SectionNumber) -> bool {
        if self.parts.len() != parent.parts.len() + 1 {
            return false;
        }
        self.parts[..parent.parts.len()] == parent.parts[..]
    }
}

/// Infer hierarchy levels from section numbering patterns.
/// Only adjusts levels when clear patterns are detected.
fn infer_toc_hierarchy(entries: &mut [TocEntry]) {
    if entries.len() < 3 {
        return;
    }

    // Parse section numbers for all entries
    let parsed: Vec<Option<SectionNumber>> = entries
        .iter()
        .map(|e| SectionNumber::parse(&e.title).map(|(num, _)| num))
        .collect();

    // Count how many entries have parseable section numbers
    let with_numbers = parsed.iter().filter(|p| p.is_some()).count();

    // Be conservative: only proceed if majority have numbers
    if with_numbers < entries.len() / 2 {
        return;
    }

    // Check for consistent hierarchical pattern
    let mut saw_hierarchy = false;
    for i in 1..parsed.len() {
        if let (Some(prev), Some(curr)) = (&parsed[i - 1], &parsed[i]) {
            if curr.is_child_of(prev) || prev.depth() != curr.depth() {
                saw_hierarchy = true;
                break;
            }
        }
    }

    if !saw_hierarchy {
        return;
    }

    // Apply levels based on section number depth
    for (i, entry) in entries.iter_mut().enumerate() {
        if let Some(ref num) = parsed[i] {
            // depth 1 (e.g., "1") -> level 0
            // depth 2 (e.g., "1.1") -> level 1
            // depth 3+ -> level 2+
            entry.level = num.depth().saturating_sub(1);
        }
    }
}
