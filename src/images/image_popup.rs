use crate::ratatui_image::{Image, Resize, ViewportOptions, picker::Picker, protocol::Protocol};
use image::{DynamicImage, GenericImageView};
use log::debug;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::sync::Arc;
use std::time::Instant;

pub struct ImagePopup {
    pub image: Arc<DynamicImage>,
    pub protocol: Option<Protocol>,
    pub src_path: String,
    pub picker: Picker,
    pub is_loading: bool,
    pub load_start: Option<Instant>,
    pub popup_area: Option<Rect>, // Check if this completes a key sequence (Space+d for stats)
}

impl ImagePopup {
    pub fn new(image: Arc<DynamicImage>, picker: &Picker, src_path: String) -> Self {
        Self {
            image,
            protocol: None,
            src_path,
            picker: picker.clone(),
            is_loading: true,
            load_start: Some(Instant::now()),
            popup_area: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, terminal_size: Rect) {
        let render_start = Instant::now();
        self.load_start = Some(render_start);
        let popup_area = self.calculate_optimal_popup_area(terminal_size);
        let calc_duration = render_start.elapsed();

        let clear_start = Instant::now();
        f.render_widget(Clear, popup_area);
        let clear_duration = clear_start.elapsed();

        let (width, height) = self.image.dimensions();
        let title = format!(" {} [{}x{} px] ", self.src_path, width, height);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().bg(Color::Black));

        let inner_area = block.inner(popup_area);

        let block_start = Instant::now();
        f.render_widget(block, popup_area);
        let block_duration = block_start.elapsed();

        debug!(
            "Pre-render timings: calc_area: {}ms, clear: {}ms, block: {}ms",
            calc_duration.as_millis(),
            clear_duration.as_millis(),
            block_duration.as_millis()
        );

        let size_text = format!("{width}x{height} pixels");

        let loading_text = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(
                "⏳ Loading image...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(size_text, Style::default().fg(Color::Gray))),
            Line::from(""),
            Line::from(Span::styled(
                "Processing image data, please wait",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let loading_paragraph = Paragraph::new(loading_text)
            .alignment(Alignment::Center)
            .style(Style::default().bg(Color::Black));

        let loading_start = Instant::now();
        f.render_widget(loading_paragraph, inner_area);
        let loading_duration = loading_start.elapsed();

        debug!(
            "Loading screen timings: paragraph: {}ms",
            loading_duration.as_millis()
        );

        // Time the protocol creation (which includes resize)
        let start = Instant::now();
        let protocol = self
            .picker
            .new_protocol(
                self.image.as_ref().clone(),
                self.calculate_optimal_popup_area(terminal_size),
                Resize::Viewport(ViewportOptions {
                    y_offset: 0,
                    x_offset: 0,
                }),
            )
            .unwrap();
        let duration = start.elapsed();

        self.protocol = Some(protocol);
        self.is_loading = false;

        // Log the timing information
        let total_time = self.load_start.map(|s| s.elapsed()).unwrap_or(duration);
        debug!(
            "--Image popup stats for '{}': protocol creation: {}ms, total time: {}ms",
            self.src_path,
            duration.as_millis(),
            total_time.as_millis()
        );

        let image_area = inner_area;
        let image_widget = Image::new(self.protocol.as_ref().unwrap());

        let total_time = self.load_start.map(|s| s.elapsed()).unwrap_or(duration);
        let duration = start.elapsed();
        debug!(
            "--Image creation stats for '{}': protocol creation: {}ms, total time: {}ms",
            self.src_path,
            duration.as_millis(),
            total_time.as_millis()
        );

        let render_start = Instant::now();
        f.render_widget(image_widget, image_area);
        let render_duration = render_start.elapsed();

        debug!(
            "--Image widget render time for '{}': {}ms",
            self.src_path,
            render_duration.as_millis()
        );

        let total_render_time = render_start.elapsed();
        debug!(
            "TOTAL render() time for '{}': {}ms",
            self.src_path,
            total_render_time.as_millis()
        );

        // Return the popup area so the main app knows where the image is displayed
        self.popup_area = Some(popup_area)
    }

    /// Calculate the optimal popup area based on image dimensions and terminal size
    fn calculate_optimal_popup_area(&self, terminal_size: Rect) -> Rect {
        let (img_width, img_height) = self.image.dimensions();

        // Get font size from picker for accurate cell estimation
        let font_size = self.picker.font_size();
        let cell_width_pixels = font_size.0 as f32;
        let cell_height_pixels = font_size.1 as f32;

        // Calculate image size in terminal cells (the image is already pre-scaled)
        let image_width_cells = (img_width as f32 / cell_width_pixels).ceil() as u16;
        let image_height_cells = (img_height as f32 / cell_height_pixels).ceil() as u16;

        // Reserve minimal space for borders (2) only - no help text
        let max_width = terminal_size.width.saturating_sub(4);
        let max_height = terminal_size.height.saturating_sub(2);

        // Since image is pre-scaled, just ensure it fits on screen
        let content_width = image_width_cells.min(max_width);
        let content_height = image_height_cells.min(max_height);

        // Add space for borders (1 on each side)
        let popup_width = content_width.saturating_add(2);
        let popup_height = content_height.saturating_add(2);

        // Center the popup in the terminal
        let x_offset = (terminal_size.width.saturating_sub(popup_width)) / 2;
        let y_offset = (terminal_size.height.saturating_sub(popup_height)) / 2;

        Rect {
            x: terminal_size.x + x_offset,
            y: terminal_size.y + y_offset,
            width: popup_width,
            height: popup_height,
        }
    }

    pub(crate) fn is_outside_popup_area(&self, click_x: u16, click_y: u16) -> bool {
        if let Some(popup_area) = self.popup_area {
            click_x < popup_area.x
                || click_x >= popup_area.x + popup_area.width
                || click_y < popup_area.y
                || click_y >= popup_area.y + popup_area.height
        } else {
            false
        }
    }
}
