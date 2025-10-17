use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Convert terminal to SVG
pub fn terminal_to_svg(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut ansi_output = String::new();

    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).unwrap();

            // Add ANSI escape codes for styling
            let mut styled_char = String::new();

            // Reset first
            styled_char.push_str("\u{1b}[0m");

            // Add colors
            if cell.fg != ratatui::style::Color::Reset {
                styled_char.push_str(&format_color(cell.fg, true));
            }
            if cell.bg != ratatui::style::Color::Reset {
                styled_char.push_str(&format_color(cell.bg, false));
            }

            // Add modifiers
            if cell.modifier.contains(ratatui::style::Modifier::BOLD) {
                styled_char.push_str("\u{1b}[1m");
            }
            if cell.modifier.contains(ratatui::style::Modifier::ITALIC) {
                styled_char.push_str("\u{1b}[3m");
            }
            if cell.modifier.contains(ratatui::style::Modifier::UNDERLINED) {
                styled_char.push_str("\u{1b}[4m");
            }

            // Add the character
            styled_char.push_str(cell.symbol());

            ansi_output.push_str(&styled_char);
        }

        // Add newline and reset at end of line
        if y < buffer.area.height - 1 {
            ansi_output.push_str("\u{1b}[0m\n");
        }
    }

    // Final reset
    ansi_output.push_str("\u{1b}[0m");

    // Convert ANSI to SVG
    let term = anstyle_svg::Term::new();
    term.render_svg(&ansi_output)
}

pub fn format_color(color: ratatui::style::Color, is_foreground: bool) -> String {
    use ratatui::style::Color;

    let base = if is_foreground { 30 } else { 40 };

    match color {
        Color::Reset => "\u{1b}[0m".to_string(),
        Color::Black => format!("\u{1b}[{base}m"),
        Color::Red => format!("\u{1b}[{}m", base + 1),
        Color::Green => format!("\u{1b}[{}m", base + 2),
        Color::Yellow => format!("\u{1b}[{}m", base + 3),
        Color::Blue => format!("\u{1b}[{}m", base + 4),
        Color::Magenta => format!("\u{1b}[{}m", base + 5),
        Color::Cyan => format!("\u{1b}[{}m", base + 6),
        Color::Gray => format!("\u{1b}[{}m", base + 7),
        Color::DarkGray => format!("\u{1b}[{}m", base + 60),
        Color::LightRed => format!("\u{1b}[{}m", base + 61),
        Color::LightGreen => format!("\u{1b}[{}m", base + 62),
        Color::LightYellow => format!("\u{1b}[{}m", base + 63),
        Color::LightBlue => format!("\u{1b}[{}m", base + 64),
        Color::LightMagenta => format!("\u{1b}[{}m", base + 65),
        Color::LightCyan => format!("\u{1b}[{}m", base + 66),
        Color::White => format!("\u{1b}[{}m", base + 67),
        Color::Rgb(r, g, b) => {
            if is_foreground {
                format!("\u{1b}[38;2;{r};{g};{b}m")
            } else {
                format!("\u{1b}[48;2;{r};{g};{b}m")
            }
        }
        Color::Indexed(idx) => {
            if is_foreground {
                format!("\u{1b}[38;5;{idx}m")
            } else {
                format!("\u{1b}[48;5;{idx}m")
            }
        }
    }
}
