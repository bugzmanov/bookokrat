use std::path::Path;
use ratatui::{
    widgets::{ListState, List, ListItem, Block, Borders},
    style::Style,
    text::{Line, Span},
    Frame,
    layout::Rect,
};
use log::error;
use crate::bookmark::Bookmarks;
use crate::theme::Base16Palette;

pub struct BookList {
    pub epub_files: Vec<String>,
    pub selected: usize,
    pub list_state: ListState,
}

impl BookList {
    pub fn new() -> Self {
        let epub_files = Self::scan_epub_files();
        let has_files = !epub_files.is_empty();
        
        let mut list_state = ListState::default();
        if has_files {
            list_state.select(Some(0));
        }
        
        Self {
            epub_files,
            selected: 0,
            list_state,
        }
    }
    
    fn scan_epub_files() -> Vec<String> {
        std::fs::read_dir(".")
            .unwrap_or_else(|e| {
                error!("Failed to read directory: {}", e);
                panic!("Failed to read directory: {}", e);
            })
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "epub" {
                    Some(path.to_str()?.to_string())
                } else {
                    None
                }
            })
            .collect()
    }
    
    
    pub fn move_selection_down(&mut self) {
        if self.selected < self.epub_files.len().saturating_sub(1) {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }
    
    pub fn move_selection_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }
    
    pub fn get_selected_file(&self) -> Option<&str> {
        self.epub_files.get(self.selected).map(|s| s.as_str())
    }
    
    pub fn set_selection_to_file(&mut self, file_path: &str) {
        if let Some(pos) = self.epub_files.iter().position(|f| f == file_path) {
            self.selected = pos;
            self.list_state.select(Some(pos));
        }
    }
    
    pub fn get_display_name(file_path: &str) -> String {
        Path::new(file_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
    
    pub fn render(&mut self, f: &mut Frame, area: Rect, _is_active: bool, palette: &Base16Palette, bookmarks: &Bookmarks) {
        let (interface_color, _, border_color, highlight_bg, highlight_fg) = 
            palette.get_interface_colors(false);
        
        // Create list items with last read timestamps
        let items: Vec<ListItem> = self
            .epub_files
            .iter()
            .map(|file| {
                let bookmark = bookmarks.get_bookmark(file);
                let last_read = bookmark
                    .map(|b| b.last_read.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Never".to_string());
                
                let display_name = Self::get_display_name(file);
                
                let content = Line::from(vec![
                    Span::styled(
                        display_name,
                        Style::default().fg(interface_color),
                    ),
                    Span::styled(
                        format!(" ({})", last_read),
                        Style::default().fg(palette.base_03),
                    ),
                ]);
                ListItem::new(content)
            })
            .collect();

        let files = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .title("Books")
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(palette.base_00)))
            .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg))
            .style(Style::default().bg(palette.base_00));

        f.render_stateful_widget(files, area, &mut self.list_state);
    }
}