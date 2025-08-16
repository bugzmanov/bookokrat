use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Configuration for image placeholder rendering
pub struct ImagePlaceholderConfig {
    /// Number of spaces between border and content
    pub internal_padding: usize,
    /// Total height of the placeholder in lines
    pub total_height: usize,
    /// Border color
    pub border_color: Color,
}

impl Default for ImagePlaceholderConfig {
    fn default() -> Self {
        Self {
            internal_padding: 4,
            total_height: 15,
            border_color: Color::Rgb(101, 115, 126), // base_03
        }
    }
}

/// Represents a rendered image placeholder
pub struct ImagePlaceholder {
    /// The raw text lines (for text selection and other purposes)
    pub raw_lines: Vec<String>,
    /// The styled lines for rendering
    pub styled_lines: Vec<Line<'static>>,
    /// Whether the placeholder should be visible (false = invisible but still occupies space)
    pub visible: bool,
}

impl ImagePlaceholder {
    /// Creates a new image placeholder with the given source text and configuration
    pub fn new(
        image_src: &str,
        terminal_width: usize,
        config: &ImagePlaceholderConfig,
        visible: bool,
    ) -> Self {
        let mut raw_lines = Vec::new();
        let mut styled_lines = Vec::new();

        // Calculate frame width based on content + internal padding
        let content_width = image_src.len();
        // Frame width = content + 2 borders + 2 * internal padding
        let frame_width = (content_width + 2 + (2 * config.internal_padding))
            .min(terminal_width)
            .max(20);
        let padding = (terminal_width.saturating_sub(frame_width)) / 2;
        let padding_str = " ".repeat(padding);

        // Top border
        let top_border = if visible {
            format!("{}┌{}┐", padding_str, "─".repeat(frame_width - 2))
        } else {
            " ".repeat(terminal_width)
        };
        raw_lines.push(top_border.clone());
        styled_lines.push(if visible {
            Line::from(Span::styled(
                top_border,
                Style::default().fg(config.border_color),
            ))
        } else {
            Line::from(top_border)
        });

        // Middle lines (total_height - 2 for top/bottom borders)
        let middle_lines = config.total_height - 2;
        let center_line = middle_lines / 2;
        let size_info_line = middle_lines - 1; // Show size info on the last line before bottom border

        for i in 0..middle_lines {
            let middle_line = if visible {
                if i == center_line {
                    // Center line with [image src=...] text
                    let text_len = image_src.len();
                    let available_width = frame_width - 2 - (2 * config.internal_padding);

                    if text_len <= available_width {
                        // Center the text within the available space
                        let text_padding = (available_width - text_len) / 2;
                        let left_spaces = config.internal_padding + text_padding;
                        let right_spaces = frame_width - 2 - left_spaces - text_len;
                        format!(
                            "{}│{}{}{}│",
                            padding_str,
                            " ".repeat(left_spaces),
                            image_src,
                            " ".repeat(right_spaces)
                        )
                    } else {
                        // Truncate if too long
                        let max_len = available_width.saturating_sub(3); // Leave room for "..."
                        let truncated = format!(
                            "{}...",
                            &image_src.chars().take(max_len).collect::<String>()
                        );
                        let text_padding = (available_width - truncated.len()) / 2;
                        let left_spaces = config.internal_padding + text_padding;
                        let right_spaces = frame_width - 2 - left_spaces - truncated.len();
                        format!(
                            "{}│{}{}{}│",
                            padding_str,
                            " ".repeat(left_spaces),
                            truncated,
                            " ".repeat(right_spaces)
                        )
                    }
                } else if i == size_info_line {
                    // Show size info on the last line
                    let size_text = format!("{}L", config.total_height);
                    let available_width = frame_width - 2 - (2 * config.internal_padding);

                    if size_text.len() <= available_width {
                        let text_padding = (available_width - size_text.len()) / 2;
                        let left_spaces = config.internal_padding + text_padding;
                        let right_spaces = frame_width - 2 - left_spaces - size_text.len();
                        format!(
                            "{}│{}{}{}│",
                            padding_str,
                            " ".repeat(left_spaces),
                            size_text,
                            " ".repeat(right_spaces)
                        )
                    } else {
                        format!("{}│{}│", padding_str, " ".repeat(frame_width - 2))
                    }
                } else {
                    format!("{}│{}│", padding_str, " ".repeat(frame_width - 2))
                }
            } else {
                // When not visible, create empty lines that maintain spacing
                " ".repeat(terminal_width)
            };

            raw_lines.push(middle_line.clone());
            styled_lines.push(if visible {
                Line::from(Span::styled(
                    middle_line,
                    Style::default().fg(config.border_color),
                ))
            } else {
                Line::from(middle_line)
            });
        }

        // Bottom border
        let bottom_border = if visible {
            format!("{}└{}┘", padding_str, "─".repeat(frame_width - 2))
        } else {
            " ".repeat(terminal_width)
        };
        raw_lines.push(bottom_border.clone());
        styled_lines.push(if visible {
            Line::from(Span::styled(
                bottom_border,
                Style::default().fg(config.border_color),
            ))
        } else {
            Line::from(bottom_border)
        });

        Self {
            raw_lines,
            styled_lines,
            visible,
        }
    }

    /// Returns the number of lines this placeholder occupies
    pub fn line_count(&self) -> usize {
        self.raw_lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_placeholder_creation() {
        let config = ImagePlaceholderConfig::default();
        let placeholder =
            ImagePlaceholder::new("[image src=\"../images/test.png\"]", 80, &config, true);

        let expected_lines = vec![
            "                   ┌────────────────────────────────────────┐",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │    [image src=\"../images/test.png\"]    │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                     15L                    │",
            "                   └────────────────────────────────────────┘",
        ];

        assert_eq!(
            placeholder.raw_lines.len(),
            expected_lines.len(),
            "Expected {} lines but got {}",
            expected_lines.len(),
            placeholder.raw_lines.len()
        );

        for (i, (actual, expected)) in placeholder
            .raw_lines
            .iter()
            .zip(expected_lines.iter())
            .enumerate()
        {
            assert_eq!(
                actual, expected,
                "Line {} doesn't match.\nExpected: '{}'\nActual:   '{}'",
                i, expected, actual
            );
        }

        let narrow_placeholder =
            ImagePlaceholder::new("[image src=\"../images/test.png\"]", 40, &config, true);

        let expected_narrow_lines = vec![
            "┌──────────────────────────────────────┐",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│    [image src=\"../images/test....    │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                  15L                  │",
            "└──────────────────────────────────────┘",
        ];

        for (i, (actual, expected)) in narrow_placeholder
            .raw_lines
            .iter()
            .zip(expected_narrow_lines.iter())
            .enumerate()
        {
            assert_eq!(
                actual, expected,
                "Narrow display line {} doesn't match.\nExpected: '{}'\nActual:   '{}'",
                i, expected, actual
            );
        }
    }

    #[test]
    fn test_image_placeholder_truncation() {
        let config = ImagePlaceholderConfig::default();
        let long_src = "[image src=\"../very/long/path/to/image/that/exceeds/width/limit.png\"]";
        let placeholder = ImagePlaceholder::new(long_src, 40, &config, true);

        // Check that the text is truncated
        let middle_line = &placeholder.raw_lines[7];
        assert!(middle_line.contains("..."));
        assert!(!middle_line.contains("limit.png"));
    }

    #[test]
    fn test_7_line_placeholder() {
        let config = ImagePlaceholderConfig {
            internal_padding: 4,
            total_height: 7,
            border_color: Color::Rgb(101, 115, 126),
        };
        let placeholder =
            ImagePlaceholder::new("[image src=\"../images/wide.jpg\"]", 80, &config, true);

        assert_eq!(
            placeholder.raw_lines.len(),
            7,
            "7-line placeholder should have exactly 7 lines"
        );

        // Check that the size indicator shows 7L
        let size_line = &placeholder.raw_lines[5]; // Second to last line
        assert!(
            size_line.contains("7L"),
            "Should show 7L for 7-line placeholder"
        );
    }
}
