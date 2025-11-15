use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
};

#[derive(Debug, Clone)]
pub struct ProgressDialog {
    pub title: String,
    pub message: String,
    pub progress: u16, // 0-100
    pub visible: bool,
    dirty: bool,
}

impl ProgressDialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            progress: 0,
            visible: false,
            dirty: false,
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.dirty = true;
    }

    pub fn set_progress(&mut self, progress: u16) {
        self.progress = progress.min(100);
        self.dirty = true;
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.dirty = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.dirty = true;
    }

    pub fn take_dirty(&mut self) -> bool {
        if self.dirty {
            self.dirty = false;
            true
        } else {
            false
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate centered dialog area (60% width, 40% height)
        let dialog_area = self.centered_rect(60, 40, area);

        // Clear the area first to remove any background text
        f.render_widget(Clear, dialog_area);

        // Create the dialog block with title
        let block = Block::default()
            .title(self.title.as_str())
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .style(Style::default().bg(Color::Rgb(64, 64, 64)));

        // Inner area for content
        let inner = block.inner(dialog_area);

        // Create layout for message and progress bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);

        // Render message
        let message_paragraph = Paragraph::new(Line::from(Span::styled(
            self.message.as_str(),
            Style::default().fg(Color::White),
        )))
        .alignment(Alignment::Center);

        // Render progress bar
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(Color::Green).bg(Color::Rgb(64, 64, 64)))
            .ratio(self.progress as f64 / 100.0)
            .label(format!("{}%", self.progress));

        // Render all widgets
        f.render_widget(block, dialog_area);
        f.render_widget(message_paragraph, chunks[0]);
        f.render_widget(gauge, chunks[1]);
    }

    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}
