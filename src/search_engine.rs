use log::debug;

/// Identifies whether a search result is from EPUB or PDF
#[derive(Debug, Clone)]
pub enum SearchResultTarget {
    /// EPUB result: chapter index and node index in Markdown AST
    Epub {
        chapter_index: usize,
        node_index: usize,
    },
    /// PDF result: page index and line bounds for selection
    Pdf {
        page_index: usize,
        line_index: usize,
        /// Y bounds of the line (y0, y1) for creating selection
        line_y_bounds: (f32, f32),
    },
}

#[derive(Debug, Clone)]
pub struct BookSearchResult {
    /// Target location (EPUB chapter or PDF page)
    pub target: SearchResultTarget,
    /// Display title (chapter title for EPUB, "Page N" for PDF)
    pub section_title: String,
    pub line_number: usize,
    pub snippet: String,
    pub context_before: String,
    pub context_after: String,
    pub match_score: f64,
    pub match_positions: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct SearchLine {
    pub text: String,
    pub node_index: usize,
    /// For PDF: Y bounds of this line (y0, y1). None for EPUB.
    pub y_bounds: Option<(f32, f32)>,
}

#[derive(Debug)]
struct ProcessedSection {
    index: usize,
    title: String,
    lines: Vec<SearchLine>,
    is_pdf: bool,
}

pub struct SearchEngine {
    sections: Vec<ProcessedSection>,
    is_pdf_mode: bool,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            is_pdf_mode: false,
        }
    }

    /// Process EPUB chapters for search indexing
    pub fn process_chapters(&mut self, chapters: Vec<(usize, String, Vec<SearchLine>)>) {
        self.is_pdf_mode = false;
        self.sections = chapters
            .into_iter()
            .map(|(index, title, lines)| ProcessedSection {
                index,
                title,
                lines,
                is_pdf: false,
            })
            .collect();
    }

    /// Process PDF pages for search indexing
    pub fn process_pdf_pages(&mut self, pages: Vec<(usize, Vec<SearchLine>)>) {
        self.is_pdf_mode = true;
        self.sections = pages
            .into_iter()
            .map(|(page_num, lines)| ProcessedSection {
                index: page_num,
                title: format!("Page {}", page_num + 1),
                lines,
                is_pdf: true,
            })
            .collect();
    }

    pub fn search_fuzzy(&self, query: &str) -> Vec<BookSearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        let trimmed = query.trim();
        if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
            let phrase = &trimmed[1..trimmed.len() - 1];
            return self.search_exact_phrase(phrase);
        }

        self.search_word_based(query)
    }

    fn search_word_based(&self, query: &str) -> Vec<BookSearchResult> {
        let mut results = Vec::new();

        let query_words: Vec<String> = query
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();

        if query_words.is_empty() {
            return Vec::new();
        }

        for section in &self.sections {
            for (line_idx, line) in section.lines.iter().enumerate() {
                let line_lower = line.text.to_lowercase();

                let line_words: Vec<&str> = line_lower.split_whitespace().collect();

                let mut matched_words = 0;
                let mut all_match_positions = Vec::new();

                for query_word in &query_words {
                    let mut word_found = false;

                    for line_word in &line_words {
                        // Match if:
                        // 1. Exact word match
                        // 2. Line word starts with query word (prefix match)
                        // 3. Line word contains query word (substring match for compound words)
                        if line_word == query_word
                            || line_word.starts_with(query_word.as_str())
                            || (query_word.len() >= 4 && line_word.contains(query_word.as_str()))
                        {
                            word_found = true;

                            if let Some(pos) = line_lower.find(query_word.as_str()) {
                                for (char_pos, (byte_idx, _ch)) in
                                    line.text.char_indices().enumerate()
                                {
                                    if byte_idx >= pos && byte_idx < pos + query_word.len() {
                                        all_match_positions.push(char_pos);
                                    }
                                }
                            }
                            break;
                        }
                    }

                    if word_found {
                        matched_words += 1;
                    }
                }

                let match_ratio = matched_words as f64 / query_words.len() as f64;

                // Include results where:
                // - All words match (perfect match)
                // - At least half the words match for multi-word queries
                // - Single word queries must match
                let include_result = if query_words.len() == 1 {
                    matched_words > 0
                } else {
                    match_ratio >= 0.5
                };

                if include_result {
                    let (context_before, context_after) = self.extract_context(section, line_idx);

                    // Truncate very long snippet lines to keep results readable
                    let max_snippet_chars = 300;
                    let snippet = if line.text.chars().count() > max_snippet_chars {
                        let truncated: String = line.text.chars().take(max_snippet_chars).collect();
                        format!("{truncated}...")
                    } else {
                        line.text.clone()
                    };

                    let target = if section.is_pdf {
                        SearchResultTarget::Pdf {
                            page_index: section.index,
                            line_index: line.node_index,
                            line_y_bounds: line.y_bounds.unwrap_or((0.0, 0.0)),
                        }
                    } else {
                        SearchResultTarget::Epub {
                            chapter_index: section.index,
                            node_index: line.node_index,
                        }
                    };

                    results.push(BookSearchResult {
                        target,
                        section_title: section.title.clone(),
                        line_number: line_idx,
                        snippet,
                        context_before,
                        context_after,
                        match_score: match_ratio,
                        match_positions: all_match_positions,
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(50);

        debug!(
            "Word-based search for '{}' found {} results",
            query,
            results.len()
        );
        results
    }

    fn search_exact_phrase(&self, phrase: &str) -> Vec<BookSearchResult> {
        if phrase.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();
        let phrase_lower = phrase.to_lowercase();

        for section in &self.sections {
            for (line_idx, line) in section.lines.iter().enumerate() {
                let line_lower = line.text.to_lowercase();

                let mut search_start = 0;
                let mut match_positions_in_line = Vec::new();

                while let Some(match_start) = line_lower[search_start..].find(&phrase_lower) {
                    let absolute_start = search_start + match_start;

                    let mut positions = Vec::new();
                    for (char_pos, (byte_idx, _ch)) in line.text.char_indices().enumerate() {
                        if byte_idx >= absolute_start && byte_idx < absolute_start + phrase.len() {
                            positions.push(char_pos);
                        }
                    }

                    match_positions_in_line.extend(positions);
                    search_start = absolute_start + phrase.len();
                }

                if !match_positions_in_line.is_empty() {
                    let (context_before, context_after) = self.extract_context(section, line_idx);

                    let max_snippet_chars = 300;
                    let snippet = if line.text.chars().count() > max_snippet_chars {
                        let truncated: String = line.text.chars().take(max_snippet_chars).collect();
                        format!("{truncated}...")
                    } else {
                        line.text.clone()
                    };

                    let target = if section.is_pdf {
                        SearchResultTarget::Pdf {
                            page_index: section.index,
                            line_index: line.node_index,
                            line_y_bounds: line.y_bounds.unwrap_or((0.0, 0.0)),
                        }
                    } else {
                        SearchResultTarget::Epub {
                            chapter_index: section.index,
                            node_index: line.node_index,
                        }
                    };

                    results.push(BookSearchResult {
                        target,
                        section_title: section.title.clone(),
                        line_number: line_idx,
                        snippet,
                        context_before,
                        context_after,
                        match_score: 1.0, // Exact match gets highest score
                        match_positions: match_positions_in_line,
                    });
                }
            }
        }

        results.truncate(50);

        debug!(
            "Phrase search for '{}' found {} results",
            phrase,
            results.len()
        );
        results
    }

    fn extract_context(&self, section: &ProcessedSection, line_idx: usize) -> (String, String) {
        // Limit context to 1 line before and 1 line after to keep results concise
        let context_lines = 1;
        let max_line_length = 200; // Truncate context lines longer than this

        let before_start = line_idx.saturating_sub(context_lines);
        let before_end = line_idx;
        let context_before = if before_start < before_end {
            section.lines[before_start..before_end]
                .iter()
                .filter(|line| !line.text.trim().is_empty())
                .take(1)
                .map(|line| {
                    if line.text.chars().count() > max_line_length {
                        let truncated: String = line.text.chars().take(max_line_length).collect();
                        format!("{truncated}...")
                    } else {
                        line.text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };

        let after_start = (line_idx + 1).min(section.lines.len());
        let after_end = (line_idx + 1 + context_lines).min(section.lines.len());
        let context_after = if after_start < after_end {
            section.lines[after_start..after_end]
                .iter()
                .filter(|line| !line.text.trim().is_empty())
                .take(1)
                .map(|line| {
                    if line.text.chars().count() > max_line_length {
                        let truncated: String = line.text.chars().take(max_line_length).collect();
                        format!("{truncated}...")
                    } else {
                        line.text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };

        (context_before, context_after)
    }

    pub fn clear(&mut self) {
        self.sections.clear();
        self.is_pdf_mode = false;
    }

    /// Returns true if the search engine is in PDF mode
    pub fn is_pdf_mode(&self) -> bool {
        self.is_pdf_mode
    }
}
