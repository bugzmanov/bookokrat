//! PDF table of contents structures and extraction helpers.

use mupdf::Document;

// ============================================================================
// Diagnostic Types (for debugging TOC extraction)
// ============================================================================

/// Diagnostic information about TOC extraction process.
#[derive(Debug)]
pub struct TocDiagnostics {
    /// Source of the final TOC
    pub source: TocSource,
    /// Raw outline info (if outlines exist)
    pub outline_info: Option<OutlineInfo>,
    /// Heuristics info (if fallback was used)
    pub heuristics_info: Option<HeuristicsInfo>,
    /// Final extracted entries
    pub entries: Vec<TocEntry>,
}

/// Where the TOC came from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TocSource {
    /// From PDF metadata outlines
    Metadata,
    /// From heuristic page scanning
    Heuristics,
    /// No TOC found
    None,
}

/// Information about PDF outline metadata
#[derive(Debug)]
pub struct OutlineInfo {
    /// Number of top-level outline entries
    pub top_level_count: usize,
    /// Total flattened entry count
    pub total_count: usize,
    /// Whether validation passed
    pub validation_passed: bool,
    /// Validation details
    pub validation: ValidationInfo,
}

/// Validation statistics
#[derive(Debug)]
pub struct ValidationInfo {
    /// Total entries checked
    pub total_entries: usize,
    /// Entries that look like valid titles
    pub valid_entries: usize,
    /// Percentage valid (need >= 70%)
    pub valid_percent: usize,
    /// Examples of invalid entries (title, length, sentence_count)
    pub invalid_examples: Vec<(String, usize, usize)>,
}

/// Information about heuristic extraction
#[derive(Debug)]
pub struct HeuristicsInfo {
    /// Pages scanned for TOC heading
    pub pages_scanned: usize,
    /// Pages with "Contents" heading found
    pub heading_pages: Vec<PageScanInfo>,
    /// Selected TOC start page
    pub toc_start_page: Option<usize>,
    /// After backtracking
    pub toc_start_after_backtrack: Option<usize>,
    /// Pages extracted from
    pub extraction_pages: Vec<PageExtractionInfo>,
    /// Hierarchy inference details
    pub hierarchy_info: HierarchyInferenceInfo,
}

/// Info about scanning a page for TOC heading
#[derive(Debug)]
pub struct PageScanInfo {
    /// Page index (0-based)
    pub page_idx: usize,
    /// Whether "Contents" heading was found
    pub has_heading: bool,
    /// Score (lines with page numbers)
    pub score: usize,
    /// Total lines on page
    pub total_lines: usize,
    /// Sample of raw lines (first 10 with content)
    pub sample_lines: Vec<LineScanInfo>,
}

/// Info about a single line during TOC scanning
#[derive(Debug)]
pub struct LineScanInfo {
    /// Raw text of the line
    pub text: String,
    /// Whether trailing page number was detected
    pub has_page_number: bool,
    /// The detected page number (if any)
    pub page_number: Option<String>,
    /// Whether title extraction succeeded
    pub title_ok: bool,
    /// Reason for rejection (if any)
    pub reject_reason: Option<String>,
}

/// Info about extracting entries from a page
#[derive(Debug)]
pub struct PageExtractionInfo {
    /// Page index (0-based)
    pub page_idx: usize,
    /// Total lines on page
    pub total_lines: usize,
    /// Lines with trailing page numbers
    pub lines_with_page_numbers: usize,
    /// Entries extracted
    pub entries_extracted: usize,
    /// Sample of extracted entries (title, level, target)
    pub sample_entries: Vec<(String, usize, String)>,
}

/// Info about hierarchy inference
#[derive(Debug)]
pub struct HierarchyInferenceInfo {
    /// Whether inference was applied
    pub applied: bool,
    /// Reason if not applied
    pub skip_reason: Option<String>,
    /// Entries with section numbers
    pub entries_with_numbers: usize,
    /// Total entries
    pub total_entries: usize,
}

/// Extract TOC with full diagnostic information.
pub fn extract_toc_with_diagnostics(doc: &Document, page_count: usize) -> TocDiagnostics {
    // Try metadata outlines first
    if let Ok(outlines) = doc.outlines() {
        if !outlines.is_empty() {
            let mut entries = Vec::new();
            flatten_outlines(&outlines, 0, &mut entries);

            let validation = compute_validation_info(&entries);
            let validation_passed = validation.valid_percent >= 70 && entries.len() >= 3;

            let outline_info = OutlineInfo {
                top_level_count: outlines.len(),
                total_count: entries.len(),
                validation_passed,
                validation,
            };

            if validation_passed {
                return TocDiagnostics {
                    source: TocSource::Metadata,
                    outline_info: Some(outline_info),
                    heuristics_info: None,
                    entries,
                };
            }

            // Outlines exist but failed validation - try heuristics
            let (heuristics_info, heuristic_entries) =
                extract_toc_from_pages_with_diagnostics(doc, page_count);

            if !heuristic_entries.is_empty() {
                return TocDiagnostics {
                    source: TocSource::Heuristics,
                    outline_info: Some(outline_info),
                    heuristics_info: Some(heuristics_info),
                    entries: heuristic_entries,
                };
            }

            // Heuristics also failed, return empty
            return TocDiagnostics {
                source: TocSource::None,
                outline_info: Some(outline_info),
                heuristics_info: Some(heuristics_info),
                entries: Vec::new(),
            };
        }
    }

    // No outlines - use heuristics
    let (heuristics_info, entries) = extract_toc_from_pages_with_diagnostics(doc, page_count);

    let source = if entries.is_empty() {
        TocSource::None
    } else {
        TocSource::Heuristics
    };

    TocDiagnostics {
        source,
        outline_info: None,
        heuristics_info: Some(heuristics_info),
        entries,
    }
}

fn compute_validation_info(entries: &[TocEntry]) -> ValidationInfo {
    let mut valid_count = 0;
    let mut invalid_examples = Vec::new();

    for entry in entries {
        let title_len = entry.title.chars().count();
        let sentence_count = entry
            .title
            .matches(". ")
            .count()
            .saturating_add(entry.title.matches("? ").count())
            .saturating_add(entry.title.matches("! ").count());

        if title_len <= 100 && sentence_count <= 1 {
            valid_count += 1;
        } else if invalid_examples.len() < 5 {
            invalid_examples.push((entry.title.clone(), title_len, sentence_count));
        }
    }

    let valid_percent = if entries.is_empty() {
        0
    } else {
        valid_count * 100 / entries.len()
    };

    ValidationInfo {
        total_entries: entries.len(),
        valid_entries: valid_count,
        valid_percent,
        invalid_examples,
    }
}

fn extract_toc_from_pages_with_diagnostics(
    doc: &Document,
    n_pages: usize,
) -> (HeuristicsInfo, Vec<TocEntry>) {
    let max_scan = n_pages.min(30);

    // Step 1: Find TOC start page
    let (heading_pages, toc_start_page) = find_toc_start_with_diagnostics(doc, max_scan);

    let Some(start_idx) = toc_start_page else {
        return (
            HeuristicsInfo {
                pages_scanned: max_scan,
                heading_pages,
                toc_start_page: None,
                toc_start_after_backtrack: None,
                extraction_pages: Vec::new(),
                hierarchy_info: HierarchyInferenceInfo {
                    applied: false,
                    skip_reason: Some("No TOC start page found".to_string()),
                    entries_with_numbers: 0,
                    total_entries: 0,
                },
            },
            Vec::new(),
        );
    };

    // Step 2: Backtrack
    let start_idx_after_backtrack = backtrack_toc_start(doc, start_idx);

    // Step 3: Extract entries from pages
    let mut entries = Vec::new();
    let mut extraction_pages = Vec::new();
    let mut saw_entries = false;
    let end = (start_idx_after_backtrack + 5).min(n_pages);

    for page_idx in start_idx_after_backtrack..end {
        let Ok(page) = doc.load_page(page_idx as i32) else {
            continue;
        };

        let (page_info, page_entries) =
            extract_toc_entries_from_page_with_diagnostics(&page, page_idx);
        extraction_pages.push(page_info);

        if !page_entries.is_empty() {
            entries.extend(page_entries);
            saw_entries = true;
        } else if saw_entries {
            break;
        }
    }

    // Step 4: Infer hierarchy
    let hierarchy_info = infer_toc_hierarchy_with_diagnostics(&mut entries);

    (
        HeuristicsInfo {
            pages_scanned: max_scan,
            heading_pages,
            toc_start_page: Some(start_idx),
            toc_start_after_backtrack: Some(start_idx_after_backtrack),
            extraction_pages,
            hierarchy_info,
        },
        entries,
    )
}

fn find_toc_start_with_diagnostics(
    doc: &Document,
    max_pages: usize,
) -> (Vec<PageScanInfo>, Option<usize>) {
    let mut page_infos = Vec::new();
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
        let line_bounds = super::super::worker::extract_line_bounds_merged(&page, 1.0);

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

        let mut score = 0;
        let mut sample_lines = Vec::new();

        if has_heading {
            headings.push(page_idx);

            // Track pending title for two-line pattern scoring
            let mut pending_title_for_score: Option<f32> = None; // stores y1 of title line
            let line_height_estimate = line_bounds
                .iter()
                .map(|l| l.y1 - l.y0)
                .fold(12.0_f32, f32::max);
            let max_gap = line_height_estimate * 2.5;

            for line in &line_bounds {
                let text = line_text(line);
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let mut line_info = LineScanInfo {
                    text: truncate_for_diag(&text, 60),
                    has_page_number: false,
                    page_number: None,
                    title_ok: false,
                    reject_reason: None,
                };

                if let Some((page_num, number_start)) = extract_trailing_page_number(&text) {
                    line_info.has_page_number = true;
                    line_info.page_number = Some(format!("{}", page_num));

                    if let Some(_title) = strip_toc_title(&text, number_start) {
                        // Single line with both title and page number
                        line_info.title_ok = true;
                        score += 1;
                        pending_title_for_score = None;
                    } else {
                        // Page number only - check if we have a pending title
                        if let Some(title_y1) = pending_title_for_score.take() {
                            if (line.y0 - title_y1).abs() <= max_gap {
                                // Title on previous line, page number on this line
                                line_info.title_ok = true;
                                line_info.reject_reason =
                                    Some("matched with pending title".to_string());
                                score += 1;
                            } else {
                                line_info.reject_reason =
                                    Some(diagnose_title_rejection(&text, number_start));
                            }
                        } else {
                            line_info.reject_reason =
                                Some(diagnose_title_rejection(&text, number_start));
                        }
                    }
                } else {
                    // No page number - could be a title line
                    if looks_like_toc_title_line(trimmed) {
                        pending_title_for_score = Some(line.y1);
                        line_info.reject_reason =
                            Some("potential title (waiting for page number)".to_string());
                    } else {
                        line_info.reject_reason = Some(diagnose_page_number_rejection(&text));
                        // Clear pending if gap too large
                        if let Some(title_y1) = pending_title_for_score {
                            if (line.y0 - title_y1).abs() > max_gap {
                                pending_title_for_score = None;
                            }
                        }
                    }
                }

                if sample_lines.len() < 15 {
                    sample_lines.push(line_info);
                }
            }

            if score >= 3 && earliest.is_none_or(|(idx, _)| page_idx < idx) {
                earliest = Some((page_idx, score));
            }
            if best.is_none_or(|(_, best_score)| score > best_score) {
                best = Some((page_idx, score));
            }
        }

        if has_heading || score > 0 {
            page_infos.push(PageScanInfo {
                page_idx,
                has_heading,
                score,
                total_lines: line_bounds.len(),
                sample_lines,
            });
        }
    }

    let best_idx = best.map(|(idx, _)| idx);
    let result = if let Some(best_idx) = best_idx {
        let earliest_near = headings
            .into_iter()
            .filter(|idx| *idx <= best_idx && best_idx.saturating_sub(*idx) <= 2)
            .min();
        earliest_near.or(Some(best_idx))
    } else {
        earliest.or(best).map(|(idx, _)| idx)
    };

    (page_infos, result)
}

/// Check if a line looks like a TOC title (no page number, but could be a chapter/section name).
fn looks_like_toc_title_line(text: &str) -> bool {
    let trimmed = text.trim();

    // Must have some letters
    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return false;
    }

    // Should be reasonably short (TOC titles are typically under 80 chars)
    if trimmed.chars().count() > 80 {
        return false;
    }

    // Should not look like body text (multiple sentences)
    let sentence_count = trimmed
        .matches(". ")
        .count()
        .saturating_add(trimmed.matches("? ").count())
        .saturating_add(trimmed.matches("! ").count());
    if sentence_count > 1 {
        return false;
    }

    // Reject if starts with dash (attribution)
    if trimmed.starts_with('-') || trimmed.starts_with('—') || trimmed.starts_with('–') {
        return false;
    }

    true
}

fn extract_toc_entries_from_page_with_diagnostics(
    page: &mupdf::Page,
    page_idx: usize,
) -> (PageExtractionInfo, Vec<TocEntry>) {
    let Ok(bounds) = page.bounds() else {
        return (
            PageExtractionInfo {
                page_idx,
                total_lines: 0,
                lines_with_page_numbers: 0,
                entries_extracted: 0,
                sample_entries: Vec::new(),
            },
            Vec::new(),
        );
    };

    let page_height = bounds.y1 - bounds.y0;
    let page_width = bounds.x1 - bounds.x0;
    let line_bounds = super::super::worker::extract_line_bounds_merged(page, 1.0);
    let link_rects = super::super::worker::extract_link_rects(page, 1.0);

    let total_lines = line_bounds.len();

    let mut ordered_lines: Vec<_> = line_bounds.iter().collect();
    ordered_lines.sort_by(|a, b| {
        a.y0.partial_cmp(&b.y0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal))
    });

    let min_x = ordered_lines
        .iter()
        .filter(|l| l.y0 >= page_height * 0.08 && l.y1 <= page_height * 0.92)
        .map(|l| l.x0)
        .fold(f32::MAX, f32::min);
    let indent_step = page_width * 0.025;

    let mut entries = Vec::new();
    let mut lines_with_page_numbers = 0;
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

        let segments = split_on_bullet_separators(&raw_text);

        for text in segments {
            let level = base_level;

            if let Some((printed_page, number_start)) = extract_trailing_page_number(&text) {
                lines_with_page_numbers += 1;

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
        }
    }

    let sample_entries: Vec<_> = entries
        .iter()
        .take(5)
        .map(|e| {
            let target_str = match &e.target {
                TocTarget::InternalPage(p) => format!("page:{}", p),
                TocTarget::PrintedPage(p) => format!("printed:{}", p),
                TocTarget::External(u) => format!("ext:{}", u),
            };
            (e.title.clone(), e.level, target_str)
        })
        .collect();

    (
        PageExtractionInfo {
            page_idx,
            total_lines,
            lines_with_page_numbers,
            entries_extracted: entries.len(),
            sample_entries,
        },
        entries,
    )
}

fn infer_toc_hierarchy_with_diagnostics(entries: &mut [TocEntry]) -> HierarchyInferenceInfo {
    if entries.len() < 3 {
        return HierarchyInferenceInfo {
            applied: false,
            skip_reason: Some(format!("Too few entries: {}", entries.len())),
            entries_with_numbers: 0,
            total_entries: entries.len(),
        };
    }

    let parsed: Vec<Option<SectionNumber>> = entries
        .iter()
        .map(|e| SectionNumber::parse(&e.title).map(|(num, _)| num))
        .collect();

    let with_numbers = parsed.iter().filter(|p| p.is_some()).count();

    if with_numbers < entries.len() / 2 {
        return HierarchyInferenceInfo {
            applied: false,
            skip_reason: Some(format!(
                "Not enough entries with section numbers: {}/{}",
                with_numbers,
                entries.len()
            )),
            entries_with_numbers: with_numbers,
            total_entries: entries.len(),
        };
    }

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
        return HierarchyInferenceInfo {
            applied: false,
            skip_reason: Some("No hierarchical pattern detected in section numbers".to_string()),
            entries_with_numbers: with_numbers,
            total_entries: entries.len(),
        };
    }

    // Apply hierarchy
    for (i, entry) in entries.iter_mut().enumerate() {
        if let Some(ref num) = parsed[i] {
            entry.level = num.depth().saturating_sub(1);
        }
    }

    HierarchyInferenceInfo {
        applied: true,
        skip_reason: None,
        entries_with_numbers: with_numbers,
        total_entries: entries.len(),
    }
}

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
        let line_bounds = super::super::worker::extract_line_bounds_merged(&page, 1.0);

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

        // Track pending title for two-line pattern scoring
        let mut pending_title_for_score: Option<f32> = None;
        let line_height_estimate = line_bounds
            .iter()
            .map(|l| l.y1 - l.y0)
            .fold(12.0_f32, f32::max);
        let max_gap = line_height_estimate * 2.5;

        let mut score = 0;
        for line in &line_bounds {
            let text = line_text(line);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((_, number_start)) = extract_trailing_page_number(&text) {
                if strip_toc_title(&text, number_start).is_some() {
                    // Single line with both title and page number
                    score += 1;
                    pending_title_for_score = None;
                } else if let Some(title_y1) = pending_title_for_score.take() {
                    // Page number only - check if we have a pending title
                    if (line.y0 - title_y1).abs() <= max_gap {
                        score += 1;
                    }
                }
            } else if looks_like_toc_title_line(trimmed) {
                pending_title_for_score = Some(line.y1);
            } else if let Some(title_y1) = pending_title_for_score {
                if (line.y0 - title_y1).abs() > max_gap {
                    pending_title_for_score = None;
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
    let line_bounds = super::super::worker::extract_line_bounds_merged(page, 1.0);
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

fn truncate_for_diag(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    }
}

fn diagnose_page_number_rejection(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.len() < 2 {
        return "line too short".to_string();
    }

    let bytes = trimmed.as_bytes();
    let mut end = trimmed.len();

    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    if end == 0 {
        return "only whitespace".to_string();
    }

    // Check what's at the end
    let last_chars: String = trimmed
        .chars()
        .rev()
        .take(10)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    // Check for Arabic numerals
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }

    if start < end {
        let digits = &trimmed[start..end];
        if let Ok(number) = digits.parse::<usize>() {
            if number == 0 {
                return format!("number is 0: '{}'", digits);
            }
            if (1800..=2100).contains(&number) {
                return format!("looks like year: {}", number);
            }
            if number > 1500 {
                return format!("number too large: {}", number);
            }
            if start > 0 && (bytes[start - 1] == b'(' || bytes[start - 1] == b'[') {
                return format!("preceded by paren: {}", number);
            }
        }
    }

    // Check for Roman numerals
    let mut roman_start = end;
    while roman_start > 0
        && matches!(
            bytes[roman_start - 1],
            b'i' | b'v'
                | b'x'
                | b'l'
                | b'c'
                | b'd'
                | b'm'
                | b'I'
                | b'V'
                | b'X'
                | b'L'
                | b'C'
                | b'D'
                | b'M'
        )
    {
        roman_start -= 1;
    }

    if roman_start < end {
        let potential_roman = &trimmed[roman_start..end];
        // Check if it's uppercase (we only accept lowercase for page numbers)
        if potential_roman.chars().any(|c| c.is_uppercase()) {
            return format!("uppercase roman '{}' (need lowercase)", potential_roman);
        }
        return format!("no trailing number found, ends with: '{}'", last_chars);
    }

    format!("no trailing number, ends with: '{}'", last_chars)
}

fn diagnose_title_rejection(text: &str, number_start: usize) -> String {
    let raw = text[..number_start].trim_end();
    let trimmed = raw
        .trim_end_matches(['.', '·', '-', '–', '—'])
        .trim_end_matches('[')
        .trim_end()
        .trim();

    if trimmed.len() < 2 {
        return format!("title too short after stripping: '{}'", trimmed);
    }

    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return format!("no letters in title: '{}'", trimmed);
    }

    if trimmed.starts_with('-') || trimmed.starts_with('—') || trimmed.starts_with('–') {
        return format!("starts with dash: '{}'", truncate_for_diag(trimmed, 30));
    }

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
        if lowercase_starts > words.len() / 2 {
            return format!(
                "looks like body text ({}/{} words start lowercase)",
                lowercase_starts,
                words.len() - 1
            );
        }
    }

    "unknown reason".to_string()
}

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
