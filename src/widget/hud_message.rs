use std::time::{Duration, Instant};

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Base16Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudMode {
    Normal,
    Error,
}

#[derive(Debug, Clone)]
pub struct HudMessage {
    pub message: String,
    pub expires_at: Instant,
    pub mode: HudMode,
}

impl HudMessage {
    pub fn new(message: impl Into<String>, duration: Duration, mode: HudMode) -> Self {
        Self {
            message: message.into(),
            expires_at: Instant::now() + duration,
            mode,
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn styled_line(&self, palette: &Base16Palette) -> Line<'static> {
        let style = match self.mode {
            HudMode::Normal => Style::default()
                .fg(palette.base_06)
                .bg(palette.base_02)
                .add_modifier(Modifier::BOLD),
            HudMode::Error => Style::default()
                .fg(palette.base_07)
                .bg(palette.base_08)
                .add_modifier(Modifier::BOLD),
        };

        Line::from(vec![Span::styled(format!(" {} ", self.message), style)]).centered()
    }
}
