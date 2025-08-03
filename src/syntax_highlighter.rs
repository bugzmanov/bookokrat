use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        // Try different themes for better visibility
        // Monokai has bright, distinct colors similar to many code editors
        let theme = theme_set
            .themes
            .get("base16-monokai.dark")
            .or_else(|| theme_set.themes.get("Monokai Extended"))
            .or_else(|| theme_set.themes.get("Monokai"))
            .or_else(|| theme_set.themes.get("base16-tomorrow-night"))
            .unwrap_or(&theme_set.themes["base16-ocean.dark"])
            .clone();

        Self { syntax_set, theme }
    }

    /// Highlight a code block and return styled Lines for ratatui
    pub fn highlight_code(&self, code: &str, language: Option<&str>) -> Vec<Line<'static>> {
        // Try to detect language from hint or content
        let syntax = if let Some(lang) = language {
            self.syntax_set
                .find_syntax_by_token(lang)
                .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
        } else {
            // Try to detect C/C++ code patterns
            if code.contains("#include") || code.contains("int main") || code.contains("->") {
                self.syntax_set.find_syntax_by_extension("c")
            } else if code.contains("#!/bin/bash") || code.contains("echo") {
                self.syntax_set.find_syntax_by_extension("sh")
            } else if code.contains("def ") || code.contains("import ") {
                self.syntax_set.find_syntax_by_extension("py")
            } else {
                None
            }
        }
        .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(code) {
            match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let spans: Vec<Span> = ranges
                        .into_iter()
                        .map(|(style, text)| {
                            // Use the vibrant color function for better readability
                            let fg_color = get_vibrant_color_for_style(&style, text);
                            let mut ratatui_style = Style::default().fg(fg_color);

                            // Convert syntect font style to ratatui modifiers
                            if style
                                .font_style
                                .contains(syntect::highlighting::FontStyle::BOLD)
                            {
                                ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                            }
                            if style
                                .font_style
                                .contains(syntect::highlighting::FontStyle::ITALIC)
                            {
                                ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                            }
                            if style
                                .font_style
                                .contains(syntect::highlighting::FontStyle::UNDERLINE)
                            {
                                ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                            }

                            Span::styled(text.to_string(), ratatui_style)
                        })
                        .collect();

                    // Remove trailing newline from the line if present
                    let line_spans: Vec<Span> = spans
                        .into_iter()
                        .map(|span| {
                            if span.content.ends_with('\n') {
                                let mut content = span.content.to_string();
                                content.pop();
                                Span::styled(content, span.style)
                            } else {
                                span
                            }
                        })
                        .collect();

                    lines.push(Line::from(line_spans));
                }
                Err(_) => {
                    // Fallback to plain text on error
                    lines.push(Line::from(line.trim_end_matches('\n').to_string()));
                }
            }
        }

        lines
    }
}

/// Get vibrant color based on syntax highlighting scope
fn get_vibrant_color_for_style(_style: &SyntectStyle, text: &str) -> Color {
    let trimmed = text.trim();

    // Comments - medium gray (more visible than before)
    if text.starts_with("//") || text.starts_with("/*") || text.contains("*/") {
        return Color::Rgb(128, 128, 128); // Medium gray for better visibility
    }

    // Strings and characters - BRIGHT green (much more vibrant)
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
        || text.contains("\\n")
        || text.contains("\\t")
    {
        return Color::Rgb(0, 255, 0); // BRIGHT green
    }

    // Numbers (including hex) - BRIGHT orange
    if trimmed.parse::<i32>().is_ok()
        || trimmed.parse::<f64>().is_ok()
        || trimmed.starts_with("0x")
        || trimmed.starts_with("0X")
        || (trimmed.len() > 0 && trimmed.chars().all(|c| c.is_numeric() || c == '.'))
    {
        return Color::Rgb(255, 165, 0); // BRIGHT orange
    }

    // C/C++ keywords and types
    match trimmed {
        // Control flow - BRIGHT magenta/purple
        "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "break" | "continue"
        | "return" | "goto" => {
            Color::Rgb(255, 0, 255) // BRIGHT magenta
        }
        // Types - BRIGHT cyan
        "void" | "int" | "char" | "float" | "double" | "long" | "short" | "unsigned" | "signed"
        | "bool" | "struct" | "union" | "enum" | "typedef" | "const" | "static" | "extern"
        | "volatile" | "register" | "auto" => {
            Color::Rgb(0, 255, 255) // BRIGHT cyan
        }
        // Booleans - Orange (like in the screenshot)
        "true" | "false" => {
            Color::Rgb(255, 165, 0) // Orange for true/false
        }
        // Preprocessor - BRIGHT yellow
        "#include" | "#define" | "#ifdef" | "#ifndef" | "#endif" | "#if" | "#else" | "#elif" => {
            Color::Rgb(255, 255, 0) // BRIGHT yellow
        }
        // NULL and special values - BRIGHT red
        "NULL" | "nullptr" => {
            Color::Rgb(255, 0, 0) // BRIGHT red
        }
        _ => {
            // Operators and punctuation
            match trimmed {
                "==" | "!=" | "<=" | ">=" | "&&" | "||" | "++" | "--" | "->" | "::" | "<<"
                | ">>" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" => {
                    Color::Rgb(255, 255, 255) // WHITE for operators
                }
                "(" | ")" | "{" | "}" | "[" | "]" | ";" | "," | "." | ":" => {
                    Color::Rgb(200, 200, 200) // Light gray for punctuation
                }
                "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "~" | "!" | "=" | "<" | ">"
                | "?" => {
                    Color::Rgb(255, 255, 255) // WHITE for operators
                }
                _ => {
                    // Function names and identifiers
                    if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !trimmed.chars().next().map_or(false, |c| c.is_numeric())
                    {
                        // Function calls get a bright yellow tint
                        if text.chars().any(|c| c.is_lowercase()) {
                            Color::Rgb(255, 220, 0) // Bright yellow for functions
                        } else {
                            // Constants/macros in all caps
                            Color::Rgb(255, 165, 0) // Orange for constants
                        }
                    } else {
                        // Default - white
                        Color::Rgb(255, 255, 255) // White default
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighting() {
        let highlighter = SyntaxHighlighter::new();

        let c_code = r#"#include <stdio.h>

int main() {
    printf("Hello, World!\n");
    return 0;
}"#;

        let highlighted = highlighter.highlight_code(c_code, Some("c"));
        assert!(!highlighted.is_empty());

        // Test auto-detection
        let auto_highlighted = highlighter.highlight_code(c_code, None);
        assert!(!auto_highlighted.is_empty());
    }
}
