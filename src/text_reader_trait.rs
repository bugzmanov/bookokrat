use crate::images::book_images::BookImages;
use crate::main_app::VimNavMotions;
use image::DynamicImage;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub link_type: crate::markdown::LinkType,
    pub target_chapter: Option<String>,
    pub target_anchor: Option<String>,
}

impl LinkInfo {
    pub fn from_url(url: String) -> Self {
        let (link_type, target_chapter, target_anchor) = crate::markdown::classify_link_href(&url);

        Self {
            text: url.clone(),
            url: url,
            line: 0, // Not needed for navigation
            start_col: 0,
            end_col: 0,
            link_type: link_type,
            target_chapter,
            target_anchor,
        }
    }
}

/// Trait defining the interface for text readers
/// This abstracts over the string-based and AST-based implementations
pub trait TextReaderTrait: VimNavMotions {
    // Content loading
    fn set_content_from_string(&mut self, content: &str, chapter_title: Option<String>);

    // Content updates
    fn clear_content(&mut self);

    // Scrolling
    fn scroll_up(&mut self);
    fn scroll_down(&mut self);
    fn scroll_half_screen_up(&mut self, screen_height: usize);
    fn scroll_half_screen_down(&mut self, screen_height: usize);
    fn get_scroll_offset(&self) -> usize;
    fn get_max_scroll_offset(&self) -> usize;
    fn get_current_node_index(&self) -> usize;
    fn restore_to_node_index(&mut self, node_index: usize);

    // Text selection
    fn handle_mouse_down(&mut self, x: u16, y: u16, area: Rect);
    fn handle_mouse_drag(&mut self, x: u16, y: u16, area: Rect);
    fn handle_mouse_up(&mut self, x: u16, y: u16, area: Rect) -> Option<String>;
    fn handle_double_click(&mut self, x: u16, y: u16, area: Rect);
    fn handle_triple_click(&mut self, x: u16, y: u16, area: Rect);
    fn clear_selection(&mut self);
    fn copy_selection_to_clipboard(&self) -> Result<(), String>;
    fn copy_chapter_to_clipboard(&self) -> Result<(), String>;
    fn has_text_selection(&self) -> bool;

    // Image handling
    fn preload_image_dimensions(&mut self, book_images: &BookImages);
    fn check_for_loaded_images(&mut self) -> bool;
    fn check_image_click(&self, x: u16, y: u16, area: Rect) -> Option<String>;
    fn get_image_picker(&self) -> Option<&Picker>;
    fn get_loaded_image(&self, image_src: &str) -> Option<Arc<DynamicImage>>;

    // Link handling
    fn get_link_at_position(&self, line: usize, column: usize) -> Option<&LinkInfo>;

    // Internal link navigation (only supported by AST-based reader)
    fn get_anchor_position(&self, anchor_id: &str) -> Option<usize>;
    fn scroll_to_line(&mut self, target_line: usize);
    fn highlight_line_temporarily(&mut self, line: usize, duration: std::time::Duration);
    fn set_current_chapter_file(&mut self, chapter_file: Option<String>);
    fn get_current_chapter_file(&self) -> &Option<String>;
    fn handle_pending_anchor_scroll(&mut self, pending_anchor: Option<String>);

    // Updates
    fn update_highlight(&mut self) -> bool;
    fn update_auto_scroll(&mut self) -> bool;

    // Raw HTML mode
    fn toggle_raw_html(&mut self);
    fn set_raw_html(&mut self, html: String);

    // Progress tracking
    fn calculate_progress(&self, content: &str, width: usize, height: usize) -> u32;

    // Terminal handling
    fn handle_terminal_resize(&mut self);

    // Rendering
    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        current_chapter: usize,
        total_chapters: usize,
        palette: &crate::theme::Base16Palette,
        is_focused: bool,
    );

    // State access (needed by main_app)
    fn get_last_content_area(&self) -> Option<Rect>;
}
