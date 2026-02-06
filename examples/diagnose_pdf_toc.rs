//! Diagnostic tool for PDF TOC extraction.
//!
//! Usage: cargo run --example diagnose_pdf_toc --features pdf -- <pdf_path>
//!
//! This tool shows the complete TOC extraction flow:
//! - Whether metadata outlines exist and pass validation
//! - Heuristic page scanning details (if used)
//! - Hierarchy inference details
//! - The final extracted TOC

use bookokrat::pdf::{
    HeuristicsInfo, OutlineInfo, TocDiagnostics, TocSource, TocTarget, extract_toc_with_diagnostics,
};
use mupdf::Document;
use std::env;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: diagnose_pdf_toc <pdf_path>");
        std::process::exit(1);
    });

    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                        PDF TOC Diagnostic Tool                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝\n");
    println!("File: {}\n", path);

    let doc = match Document::open(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to open PDF: {:?}", e);
            std::process::exit(1);
        }
    };

    let page_count = doc.page_count().unwrap_or(0) as usize;
    println!("Page count: {}\n", page_count);

    let diagnostics = extract_toc_with_diagnostics(&doc, page_count);

    print_diagnostics(&diagnostics);
}

fn print_diagnostics(diag: &TocDiagnostics) {
    // Source summary
    println!("┌──────────────────────────────────────────────────────────────────────────────┐");
    println!(
        "│ RESULT: {}",
        match diag.source {
            TocSource::Metadata => "TOC from PDF metadata outlines ✓",
            TocSource::Heuristics => "TOC from heuristic page scanning ⚠",
            TocSource::None => "No TOC found ✗",
        }
    );
    println!("└──────────────────────────────────────────────────────────────────────────────┘\n");

    // Outline info
    if let Some(ref outline) = diag.outline_info {
        print_outline_info(outline);
    } else {
        println!("═══ STEP 1: PDF Metadata Outlines ═══\n");
        println!("  ⚠ No outlines found in PDF metadata\n");
    }

    // Heuristics info
    if let Some(ref heuristics) = diag.heuristics_info {
        print_heuristics_info(heuristics);
    }

    // Final entries
    print_final_entries(diag);
}

fn print_outline_info(info: &OutlineInfo) {
    println!("═══ STEP 1: PDF Metadata Outlines ═══\n");
    println!("  Top-level entries: {}", info.top_level_count);
    println!("  Total entries (flattened): {}", info.total_count);
    println!();

    println!("  ─── Validation ───");
    println!("  Total entries: {}", info.validation.total_entries);
    println!(
        "  Valid entries: {} ({}%)",
        info.validation.valid_entries, info.validation.valid_percent
    );
    println!("  Required: ≥70% and ≥3 entries");
    println!(
        "  Result: {}",
        if info.validation_passed {
            "PASSED ✓"
        } else {
            "FAILED ✗"
        }
    );

    if !info.validation.invalid_examples.is_empty() {
        println!();
        println!("  Invalid entry examples:");
        for (title, len, sentences) in &info.validation.invalid_examples {
            println!(
                "    • \"{}\" (len={}, sentences={})",
                truncate(title, 50),
                len,
                sentences
            );
        }
    }
    println!();
}

fn print_heuristics_info(info: &HeuristicsInfo) {
    println!("═══ STEP 2: Heuristic Page Scanning ═══\n");

    println!("  ─── TOC Start Detection ───");
    println!("  Pages scanned: {}", info.pages_scanned);

    if info.heading_pages.is_empty() {
        println!("  Pages with 'Contents' heading: none found");
    } else {
        println!("  Pages with 'Contents' heading:");
        for page in &info.heading_pages {
            let status = if page.has_heading {
                "✓ heading"
            } else {
                "  no heading"
            };
            println!(
                "    Page {}: {} | score={} | {} lines total",
                page.page_idx + 1,
                status,
                page.score,
                page.total_lines
            );

            // Show raw line analysis for pages with heading but low score
            if page.has_heading && page.score < 3 && !page.sample_lines.is_empty() {
                println!("      Raw line analysis:");
                for line in &page.sample_lines {
                    let status_icon = if line.title_ok {
                        "✓"
                    } else if line.has_page_number {
                        "△"
                    } else {
                        "✗"
                    };

                    let page_str = line
                        .page_number
                        .as_ref()
                        .map(|p| format!(" →{}", p))
                        .unwrap_or_default();

                    let reason = line
                        .reject_reason
                        .as_ref()
                        .map(|r| format!(" [{}]", r))
                        .unwrap_or_default();

                    println!(
                        "        {} \"{}\"{}{}",
                        status_icon, line.text, page_str, reason
                    );
                }
                println!();
            }
        }
    }

    match (info.toc_start_page, info.toc_start_after_backtrack) {
        (Some(start), Some(after)) if start != after => {
            println!(
                "  Selected start page: {} (backtracked from {})",
                after + 1,
                start + 1
            );
        }
        (Some(start), _) => {
            println!("  Selected start page: {}", start + 1);
        }
        (None, _) => {
            println!("  Selected start page: none (no suitable page found)");
        }
    }
    println!();

    // Page extraction details
    if !info.extraction_pages.is_empty() {
        println!("  ─── Entry Extraction ───");
        for page in &info.extraction_pages {
            println!(
                "  Page {} ({} lines total):",
                page.page_idx + 1,
                page.total_lines
            );
            println!(
                "    Lines with page numbers: {}",
                page.lines_with_page_numbers
            );
            println!("    Entries extracted: {}", page.entries_extracted);

            if !page.sample_entries.is_empty() {
                println!("    Sample entries:");
                for (title, level, target) in &page.sample_entries {
                    let indent = "  ".repeat(*level);
                    println!("      {}\"{}\" → {}", indent, truncate(title, 40), target);
                }
            }
            println!();
        }
    }

    // Hierarchy inference
    println!("  ─── Hierarchy Inference ───");
    let h = &info.hierarchy_info;
    println!(
        "  Entries with section numbers: {}/{}",
        h.entries_with_numbers, h.total_entries
    );
    if h.applied {
        println!("  Inference: APPLIED ✓");
        println!(
            "    (Levels adjusted based on section number depth: 1→L0, 1.1→L1, 1.1.1→L2, etc.)"
        );
    } else if let Some(ref reason) = h.skip_reason {
        println!("  Inference: SKIPPED");
        println!("    Reason: {}", reason);
    }
    println!();
}

fn print_final_entries(diag: &TocDiagnostics) {
    println!("═══ FINAL EXTRACTED TOC ═══\n");

    if diag.entries.is_empty() {
        println!("  ⚠ No TOC entries extracted!\n");
        return;
    }

    println!("  Total entries: {}\n", diag.entries.len());

    // Level distribution
    let max_level = diag.entries.iter().map(|e| e.level).max().unwrap_or(0);
    let level_counts: Vec<usize> = (0..=max_level)
        .map(|l| diag.entries.iter().filter(|e| e.level == l).count())
        .collect();

    println!("  Level distribution:");
    for (level, count) in level_counts.iter().enumerate() {
        if *count > 0 {
            let bar = "█".repeat((*count).min(40));
            println!("    L{}: {:4} {}", level, count, bar);
        }
    }
    println!();

    // Print entries
    println!("  Entries:");
    println!("  {}", "─".repeat(76));

    for (i, entry) in diag.entries.iter().enumerate() {
        let indent = "  ".repeat(entry.level);
        let page = match &entry.target {
            TocTarget::InternalPage(p) => format!("p.{}", p + 1),
            TocTarget::External(uri) => format!("→{}", truncate(uri, 15)),
            TocTarget::PrintedPage(p) => format!("#{}", p),
        };
        let title = truncate(&entry.title, 55 - entry.level * 2);
        println!("  [{:3}] {}{}  ({})", i, indent, title, page);
    }
    println!("  {}", "─".repeat(76));
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    }
}
