use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span, Text},
};
use crate::theme::Base16Palette;

pub struct TextRenderer {
    pub words_per_minute: f32,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            words_per_minute: 250.0,
        }
    }
    
    pub fn calculate_reading_time(&self, text: &str) -> (u32, u32) {
        let word_count = text.split_whitespace().count() as f32;
        let total_minutes = word_count / self.words_per_minute;
        let hours = (total_minutes / 60.0) as u32;
        let minutes = (total_minutes % 60.0) as u32;
        (hours, minutes)
    }
    
    pub fn parse_styled_text<'a>(&self, text: &str, chapter_title: &Option<String>, palette: &Base16Palette) -> Text<'a> {
        let mut lines = Vec::new();
        
        if let Some(ref title) = chapter_title {
            lines.push(Line::from(vec![
                Span::styled(title.clone(), Style::default().fg(palette.base_0d).add_modifier(Modifier::BOLD))
            ]));
            lines.push(Line::from(""));
        }
        
        let text_lines: Vec<&str> = text.lines().collect();
        for line in text_lines.iter() {
            let mut spans = Vec::new();
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            let mut current_text = String::new();
            
            while i < chars.len() {
                if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), Style::default().fg(palette.base_07)));
                        current_text.clear();
                    }
                    
                    i += 2;
                    let mut bold_text = String::new();
                    let mut found_closing = false;
                    while i + 1 < chars.len() {
                        if chars[i] == '*' && chars[i + 1] == '*' {
                            found_closing = true;
                            i += 2;
                            break;
                        } else {
                            bold_text.push(chars[i]);
                            i += 1;
                        }
                    }
                    if found_closing {
                        spans.push(Span::styled(
                            bold_text,
                            Style::default().fg(palette.base_08).add_modifier(Modifier::BOLD)
                        ));
                    } else {
                        current_text.push_str("**");
                        current_text.push_str(&bold_text);
                    }
                }
                else if (chars[i] == '"' || chars[i] == '\u{201C}' || chars[i] == '\u{201D}') &&
                        chars[i] != '\'' && chars[i] != '\u{2018}' && chars[i] != '\u{2019}' {
                    let quote_char = chars[i];
                    let closing_quote = match quote_char {
                        '"' => '"',
                        '\u{201C}' => '\u{201D}',
                        '\u{201D}' => '\u{201D}',
                        _ => quote_char
                    };
                    
                    if !current_text.is_empty() {
                        spans.push(Span::styled(current_text.clone(), Style::default().fg(palette.base_07)));
                        current_text.clear();
                    }
                    
                    let start_pos = i;
                    i += 1;
                    let mut quoted_text = String::new();
                    let mut found_closing = false;
                    
                    let max_quote_length = 200;
                    let search_limit = (i + max_quote_length).min(chars.len());
                    
                    while i < search_limit {
                        if chars[i] == closing_quote || chars[i] == quote_char {
                            spans.push(Span::styled(
                                format!("{}{}{}", quote_char, quoted_text, chars[i]),
                                Style::default().fg(palette.base_0d).add_modifier(Modifier::BOLD)
                            ));
                            i += 1;
                            found_closing = true;
                            break;
                        } else {
                            quoted_text.push(chars[i]);
                            i += 1;
                        }
                    }
                    
                    if !found_closing {
                        current_text.push(chars[start_pos]);
                        i = start_pos + 1;
                    }
                }
                else {
                    current_text.push(chars[i]);
                    i += 1;
                }
            }
            
            if !current_text.is_empty() {
                spans.push(Span::styled(current_text, Style::default().fg(palette.base_07)));
            }
            
            if spans.is_empty() {
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(spans));
            }
        }
        
        Text::from(lines)
    }
    
    pub fn calculate_progress(&self, scroll_offset: usize, content: &str, visible_width: usize, visible_height: usize) -> u32 {
        let total_lines = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                (line.len() as f32 / visible_width as f32).ceil() as usize
            })
            .sum::<usize>();
        
        let max_scroll = if total_lines > visible_height {
            total_lines - visible_height
        } else {
            0
        };
        
        if max_scroll > 0 {
            ((scroll_offset as f32 / max_scroll as f32) * 100.0).min(100.0) as u32
        } else {
            100
        }
    }
}