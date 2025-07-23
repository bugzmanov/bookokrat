use std::path::Path;
use ratatui::widgets::ListState;
use log::{info, error};
use crate::bookmark::Bookmarks;

pub struct BookList {
    pub epub_files: Vec<String>,
    pub selected: usize,
    pub list_state: ListState,
    pub bookmarks: Bookmarks,
}

impl BookList {
    pub fn new() -> Self {
        let epub_files = Self::scan_epub_files();
        let has_files = !epub_files.is_empty();
        
        let mut list_state = ListState::default();
        if has_files {
            list_state.select(Some(0));
        }
        
        let bookmarks = Bookmarks::load().unwrap_or_else(|e| {
            error!("Failed to load bookmarks: {}", e);
            Bookmarks::new()
        });
        
        Self {
            epub_files,
            selected: 0,
            list_state,
            bookmarks,
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
    
    pub fn get_most_recent_book(&self) -> Option<String> {
        if let Some((recent_path, _)) = self.bookmarks.get_most_recent() {
            if self.epub_files.contains(&recent_path) {
                info!("Found most recent book: {}", recent_path);
                return Some(recent_path);
            }
        }
        None
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
}