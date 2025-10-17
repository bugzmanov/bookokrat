use log::debug;

#[derive(Debug, Clone)]
pub struct BookSearchResult {
    pub chapter_index: usize,
    pub chapter_title: String,
    pub line_number: usize,
    pub snippet: String,
    pub context_before: String,
    pub context_after: String,
    pub match_score: f64,
    pub match_positions: Vec<usize>,
}

#[derive(Debug)]
struct ProcessedChapter {
    index: usize,
    title: String,
    lines: Vec<String>,
    #[allow(dead_code)]
    raw_text: String,
}

pub struct SearchEngine {
    chapters: Vec<ProcessedChapter>,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine {
    pub fn new() -> Self {
        Self {
            chapters: Vec::new(),
        }
    }

    pub fn process_chapters(&mut self, chapters: Vec<(usize, String, String)>) {
        self.chapters = chapters
            .into_iter()
            .map(|(index, title, content)| {
                let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                ProcessedChapter {
                    index,
                    title,
                    lines,
                    raw_text: content,
                }
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

        for chapter in &self.chapters {
            for (line_idx, line) in chapter.lines.iter().enumerate() {
                let line_lower = line.to_lowercase();

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
                                for (char_pos, (byte_idx, _ch)) in line.char_indices().enumerate() {
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
                    let (context_before, context_after) = self.extract_context(chapter, line_idx);

                    // Truncate very long snippet lines to keep results readable
                    let max_snippet_chars = 300;
                    let snippet = if line.chars().count() > max_snippet_chars {
                        let truncated: String = line.chars().take(max_snippet_chars).collect();
                        format!("{truncated}...")
                    } else {
                        line.clone()
                    };

                    results.push(BookSearchResult {
                        chapter_index: chapter.index,
                        chapter_title: chapter.title.clone(),
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

        for chapter in &self.chapters {
            for (line_idx, line) in chapter.lines.iter().enumerate() {
                let line_lower = line.to_lowercase();

                let mut search_start = 0;
                let mut match_positions_in_line = Vec::new();

                while let Some(match_start) = line_lower[search_start..].find(&phrase_lower) {
                    let absolute_start = search_start + match_start;

                    let mut positions = Vec::new();
                    for (char_pos, (byte_idx, _ch)) in line.char_indices().enumerate() {
                        if byte_idx >= absolute_start && byte_idx < absolute_start + phrase.len() {
                            positions.push(char_pos);
                        }
                    }

                    match_positions_in_line.extend(positions);
                    search_start = absolute_start + phrase.len();
                }

                if !match_positions_in_line.is_empty() {
                    let (context_before, context_after) = self.extract_context(chapter, line_idx);

                    let max_snippet_chars = 300;
                    let snippet = if line.chars().count() > max_snippet_chars {
                        let truncated: String = line.chars().take(max_snippet_chars).collect();
                        format!("{truncated}...")
                    } else {
                        line.clone()
                    };

                    results.push(BookSearchResult {
                        chapter_index: chapter.index,
                        chapter_title: chapter.title.clone(),
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

    fn extract_context(&self, chapter: &ProcessedChapter, line_idx: usize) -> (String, String) {
        // Limit context to 1 line before and 1 line after to keep results concise
        let context_lines = 1;
        let max_line_length = 200; // Truncate context lines longer than this

        let before_start = line_idx.saturating_sub(context_lines);
        let before_end = line_idx;
        let context_before = if before_start < before_end {
            chapter.lines[before_start..before_end]
                .iter()
                .filter(|line| !line.trim().is_empty())
                .take(1)
                .map(|line| {
                    if line.chars().count() > max_line_length {
                        let truncated: String = line.chars().take(max_line_length).collect();
                        format!("{truncated}...")
                    } else {
                        line.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };

        let after_start = (line_idx + 1).min(chapter.lines.len());
        let after_end = (line_idx + 1 + context_lines).min(chapter.lines.len());
        let context_after = if after_start < after_end {
            chapter.lines[after_start..after_end]
                .iter()
                .filter(|line| !line.trim().is_empty())
                .take(1)
                .map(|line| {
                    if line.chars().count() > max_line_length {
                        let truncated: String = line.chars().take(max_line_length).collect();
                        format!("{truncated}...")
                    } else {
                        line.clone()
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
        self.chapters.clear();
    }
}
