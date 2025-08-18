use image::{DynamicImage, GenericImageView};
use log::debug;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_image::{Image, Resize, picker::Picker, protocol::Protocol};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct ImagePopup {
    pub image: Arc<DynamicImage>,
    pub protocol: Option<Protocol>,
    pub src_path: String,
    pub picker: Picker,
    pub is_loading: bool,
    pub load_start: Option<Instant>,
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
        }
    }

    pub fn render(&mut self, f: &mut Frame, terminal_size: Rect) {
        let render_start = Instant::now();
        self.load_start = Some(render_start.clone());
        let popup_area = self.calculate_optimal_popup_area(terminal_size);
        let calc_duration = render_start.elapsed();

        let clear_start = Instant::now();
        f.render_widget(Clear, popup_area);
        let clear_duration = clear_start.elapsed();

        let title = format!(" {} ", self.src_path);
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

        let (width, height) = self.image.dimensions();
        let size_text = format!("{}x{} pixels", width, height);

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

        let help_start = Instant::now();
        self.render_help_text(f, popup_area);
        let help_duration = help_start.elapsed();

        debug!(
            "Loading screen timings: paragraph: {}ms, help: {}ms",
            loading_duration.as_millis(),
            help_duration.as_millis()
        );

        // Time the protocol creation (which includes resize)
        let start = Instant::now();
        let protocol = self
            .picker
            .new_protocol(
                self.image.as_ref().clone(),
                self.calculate_optimal_popup_area(terminal_size),
                Resize::Viewport(ratatui_image::ViewportOptions {
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
        let image_widget = ratatui_image::Image::new(self.protocol.as_ref().unwrap());

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

        self.render_help_text(f, popup_area);

        let total_render_time = render_start.elapsed();
        debug!(
            "TOTAL render() time for '{}': {}ms",
            self.src_path,
            total_render_time.as_millis()
        );
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

        // Reserve minimal space for borders (2) and help text below (2)
        let max_width = terminal_size.width.saturating_sub(4);
        let max_height = terminal_size.height.saturating_sub(4);

        // Since image is pre-scaled, just ensure it fits on screen
        let content_width = image_width_cells.min(max_width);
        let content_height = image_height_cells.min(max_height);

        // Add space for borders (1 on each side)
        let popup_width = content_width.saturating_add(2);
        let popup_height = content_height.saturating_add(2);

        // Center the popup in the terminal
        let x_offset = (terminal_size.width.saturating_sub(popup_width)) / 2;
        let y_offset = (terminal_size.height.saturating_sub(popup_height + 2)) / 2; // +2 for help text below

        Rect {
            x: terminal_size.x + x_offset,
            y: terminal_size.y + y_offset,
            width: popup_width,
            height: popup_height,
        }
    }

    /// Calculate centered image area for original size mode
    fn calculate_centered_image_area(&self, inner_area: Rect) -> Rect {
        let (img_width, img_height) = self.image.dimensions();

        // Get font size from picker for accurate cell estimation
        let font_size = self.picker.font_size();
        let cell_width_pixels = font_size.0 as f32;
        let cell_height_pixels = font_size.1 as f32;

        // Calculate image size in terminal cells
        let estimated_width_cells = (img_width as f32 / cell_width_pixels).ceil() as u16;
        let estimated_height_cells = (img_height as f32 / cell_height_pixels).ceil() as u16;

        // Constrain to available space
        let width = estimated_width_cells.min(inner_area.width);
        let height = estimated_height_cells.min(inner_area.height);

        // Center within inner area
        let x_offset = (inner_area.width.saturating_sub(width)) / 2;
        let y_offset = (inner_area.height.saturating_sub(height)) / 2;

        Rect {
            x: inner_area.x + x_offset,
            y: inner_area.y + y_offset,
            width,
            height,
        }
    }

    fn render_help_text(&self, f: &mut Frame, popup_area: Rect) {
        let terminal_area = f.area();

        let help_y = popup_area.y + popup_area.height + 1; // +1 for spacing

        if help_y + 1 < terminal_area.height {
            let help_area = Rect {
                x: popup_area.x,
                y: help_y,
                width: popup_area.width,
                height: 1,
            };

            let (width, height) = self.image.dimensions();
            let help_text = format!(" ESC: close | {}x{} px ", width, height);

            let help_paragraph = Paragraph::new(Line::from(vec![Span::styled(
                help_text,
                Style::default().fg(Color::Yellow),
            )]))
            .alignment(Alignment::Center)
            .style(Style::default().bg(Color::Black));

            f.render_widget(help_paragraph, help_area);
        }
    }
}
