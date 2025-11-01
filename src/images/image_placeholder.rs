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

/// Loading status for image placeholders
#[derive(Debug, Clone, PartialEq)]
pub enum LoadingStatus {
    Loading,
    Failed,
    Loaded,
    Unsupported,
}

impl LoadingStatus {
    fn as_str(&self) -> &'static str {
        match self {
            LoadingStatus::Loading => "loading...",
            LoadingStatus::Failed => "loading failed",
            LoadingStatus::Loaded => "loaded",
            LoadingStatus::Unsupported => "images not supported in this terminal",
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
        status: LoadingStatus,
    ) -> Self {
        let mut raw_lines = Vec::new();
        let mut styled_lines = Vec::new();

        // Calculate frame width based on status message length (not image src)
        // This ensures the status message always fits
        let status_text = status.as_str();
        let min_content_width = status_text.len().max(image_src.len());
        // Frame width = content + 2 borders + 2 * internal padding
        let frame_width = (min_content_width + 2 + (2 * config.internal_padding))
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
        let _center_line = middle_lines / 2;
        let status_info_line = middle_lines - 1; // Show status info on the last line before bottom border

        for i in 0..middle_lines {
            let middle_line = if visible {
                if i == status_info_line {
                    // Show loading status on the last line
                    let status_text = status.as_str();
                    let available_width = frame_width - 2 - (2 * config.internal_padding);

                    // Truncate status text if too long
                    let display_text = if status_text.len() <= available_width {
                        status_text.to_string()
                    } else {
                        let max_len = available_width.saturating_sub(3); // Leave room for "..."
                        format!(
                            "{}...",
                            &status_text.chars().take(max_len).collect::<String>()
                        )
                    };

                    let text_padding = (available_width - display_text.len()) / 2;
                    let left_spaces = config.internal_padding + text_padding;
                    let right_spaces = frame_width - 2 - left_spaces - display_text.len();
                    format!(
                        "{}│{}{}{}│",
                        padding_str,
                        " ".repeat(left_spaces),
                        display_text,
                        " ".repeat(right_spaces)
                    )
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_placeholder_creation() {
        let config = ImagePlaceholderConfig::default();
        let placeholder = ImagePlaceholder::new(
            "[image src=\"../images/test.png\"]",
            80,
            &config,
            true,
            LoadingStatus::Loading,
        );

        let expected_lines = vec![
            "                   ┌────────────────────────────────────────┐",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │                                        │",
            "                   │               loading...               │",
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
                "Line {i} doesn't match.\nExpected: '{expected}'\nActual:   '{actual}'"
            );
        }

        let narrow_placeholder = ImagePlaceholder::new(
            "[image src=\"../images/test.png\"]",
            40,
            &config,
            true,
            LoadingStatus::Failed,
        );

        let expected_narrow_lines = vec![
            "┌──────────────────────────────────────┐",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│                                      │",
            "│            loading failed            │",
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
                "Narrow display line {i} doesn't match.\nExpected: '{expected}'\nActual:   '{actual}'"
            );
        }
    }

    #[test]
    fn test_image_placeholder_truncation() {
        let config = ImagePlaceholderConfig::default();
        let long_src = "[image src=\"../very/long/path/to/image/that/exceeds/width/limit.png\"]";
        let placeholder =
            ImagePlaceholder::new(long_src, 40, &config, true, LoadingStatus::Loading);

        // Placeholder should not render the raw image src on the canvas
        let middle_line = &placeholder.raw_lines[7];
        assert!(
            !middle_line.contains("[image"),
            "Placeholder should not display the raw image source"
        );
        assert_eq!(
            middle_line.chars().count(),
            placeholder.raw_lines[0].chars().count(),
            "Inner lines should match the border width"
        );

        // Status indicator remains visible on the last content line
        let status_line = &placeholder.raw_lines[13];
        assert!(status_line.contains("loading..."));
    }

    #[test]
    fn test_7_line_placeholder() {
        let config = ImagePlaceholderConfig {
            internal_padding: 4,
            total_height: 7,
            border_color: Color::Rgb(101, 115, 126),
        };
        let placeholder = ImagePlaceholder::new(
            "[image src=\"../images/wide.jpg\"]",
            80,
            &config,
            true,
            LoadingStatus::Loaded,
        );

        assert_eq!(
            placeholder.raw_lines.len(),
            7,
            "7-line placeholder should have exactly 7 lines"
        );

        // Check that the status indicator shows "loaded"
        let status_line = &placeholder.raw_lines[5]; // Second to last line
        assert!(
            status_line.contains("loaded"),
            "Should show 'loaded' for loaded status"
        );
    }
}
