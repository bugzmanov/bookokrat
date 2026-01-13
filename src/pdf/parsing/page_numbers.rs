//! Page number detection and mapping

use std::collections::HashMap;

use super::super::types::LineBounds;
use mupdf::Document;

/// Tracks printed page numbers to enable printed-to-PDF mapping
#[derive(Default)]
pub struct PageNumberTracker {
    targets: Vec<usize>,
    samples: Vec<PageNumberSample>,
    offset: Option<i32>,
}

#[derive(Clone, Copy)]
struct PageNumberSample {
    page_idx: usize,
    #[expect(dead_code)]
    printed: i32,
    offset: i32,
}

impl PageNumberTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_targets(&mut self, n_pages: usize) {
        self.targets = sample_targets(n_pages);
        self.samples.clear();
        self.offset = None;
    }

    #[must_use]
    pub fn has_offset(&self) -> bool {
        self.offset.is_some()
    }

    pub fn observe(&mut self, page_idx: usize, line_bounds: &[LineBounds], page_height_px: f32) {
        if self.offset.is_some() || !self.targets.contains(&page_idx) {
            return;
        }

        if self
            .samples
            .iter()
            .any(|sample| sample.page_idx == page_idx)
        {
            return;
        }

        let Some(printed) = detect_page_number(line_bounds, page_height_px) else {
            return;
        };

        let offset = printed - (page_idx as i32 + 1);
        self.samples.push(PageNumberSample {
            page_idx,
            printed,
            offset,
        });

        self.update_offset();
    }

    pub fn observe_sample(&mut self, page_idx: usize, printed: i32) {
        if self.offset.is_some() || !self.targets.contains(&page_idx) {
            return;
        }

        if self
            .samples
            .iter()
            .any(|sample| sample.page_idx == page_idx)
            || printed <= 0
        {
            return;
        }

        let offset = printed - (page_idx as i32 + 1);
        self.samples.push(PageNumberSample {
            page_idx,
            printed,
            offset,
        });

        self.update_offset();
    }

    #[must_use]
    pub fn map_printed_to_pdf(&self, printed_page: usize, n_pages: usize) -> Option<usize> {
        let offset = self.offset?;

        let target = printed_page as i32 - 1 - offset;
        if target < 0 {
            return None;
        }

        let target = target as usize;
        if target >= n_pages {
            return None;
        }

        Some(target)
    }

    #[must_use]
    pub fn content_page_range(&self, n_pages: usize) -> Option<(usize, usize)> {
        let offset = self.offset?;
        if n_pages == 0 {
            return None;
        }

        let min_printed = 1 + offset;
        let max_printed = n_pages as i32 + offset;
        if max_printed < 1 {
            return None;
        }

        let start = min_printed.max(1) as usize;
        let end = max_printed.max(1) as usize;
        Some((start, end))
    }

    fn update_offset(&mut self) {
        if self.samples.len() < 3 {
            return;
        }

        let mut counts: HashMap<i32, usize> = HashMap::new();
        for sample in &self.samples {
            *counts.entry(sample.offset).or_insert(0) += 1;
        }

        let mut best = None;
        for (offset, count) in counts {
            if best.is_none_or(|(_, best_count)| count > best_count) {
                best = Some((offset, count));
            }
        }

        if let Some((offset, count)) = best {
            if count >= 3 {
                self.offset = Some(offset);
            }
        }
    }
}

/// Generate sample page indices for page number detection.
/// Samples 20 pages starting from page 10 (adjusted for small PDFs).
#[must_use]
pub fn sample_targets(n_pages: usize) -> Vec<usize> {
    if n_pages == 0 {
        return Vec::new();
    }

    const SAMPLE_COUNT: usize = 20;
    const PREFERRED_START: usize = 9; // 0-indexed, so page 10

    // For small PDFs, start earlier and sample what we can
    let start = if n_pages <= PREFERRED_START + SAMPLE_COUNT {
        // Small PDF: start from beginning or adjust to fit
        n_pages.saturating_sub(SAMPLE_COUNT).min(PREFERRED_START)
    } else {
        PREFERRED_START
    };

    let end = (start + SAMPLE_COUNT).min(n_pages);
    (start..end).collect()
}

pub fn collect_page_number_samples(doc: &Document, n_pages: usize) -> Vec<(usize, i32)> {
    let targets = sample_targets(n_pages);
    let mut samples = Vec::new();

    for page_num in targets {
        let Ok(page) = doc.load_page(page_num as i32) else {
            continue;
        };

        let Ok(bounds) = page.bounds() else {
            continue;
        };

        let page_height = bounds.y1 - bounds.y0;
        let line_bounds = super::super::worker::extract_line_bounds(&page, 1.0);

        if let Some(printed) = detect_page_number(&line_bounds, page_height) {
            samples.push((page_num, printed));
        }
    }

    samples
}

/// Detect printed page number from line bounds
#[must_use]
pub fn detect_page_number(line_bounds: &[LineBounds], page_height_px: f32) -> Option<i32> {
    if line_bounds.is_empty() || page_height_px <= 0.0 {
        return None;
    }

    let max_edge_distance = page_height_px * 0.2;
    let mut best = None;

    for line in line_bounds {
        let line_text: String = line.chars.iter().map(|c| c.c).collect();
        let Some(num) = parse_page_number(&line_text) else {
            continue;
        };

        let top_dist = line.y0;
        let bottom_dist = (page_height_px - line.y1).max(0.0);
        let edge_dist = top_dist.min(bottom_dist);
        if edge_dist > max_edge_distance {
            continue;
        }

        let score = edge_dist;
        if best.is_none_or(|(_, best_score)| score < best_score) {
            best = Some((num, score));
        }
    }

    best.map(|(num, _)| num)
}

fn parse_page_number(text: &str) -> Option<i32> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 12 {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("page") {
        let rest = lower.trim_start_matches("page");
        return extract_single_number(rest);
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation() || c.is_whitespace())
    {
        return None;
    }

    extract_single_number(trimmed)
}

fn extract_single_number(text: &str) -> Option<i32> {
    let mut current = String::new();
    let mut found: Option<i32> = None;

    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            if found.is_some() {
                return None;
            }
            found = current.parse::<i32>().ok();
            current.clear();
        }
    }

    if !current.is_empty() {
        if found.is_some() {
            return None;
        }
        found = current.parse::<i32>().ok();
    }

    found.filter(|num| *num > 0)
}
