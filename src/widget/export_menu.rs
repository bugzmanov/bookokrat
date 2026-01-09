use crate::inputs::key_seq::KeySeq;
use crate::main_app::VimNavMotions;
use crate::theme::current_theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    OrgMode,
    PlainText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportContent {
    AnnotationsOnly,
    AnnotationsWithContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportOrganization {
    SingleFile,
    ChapterBased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuStep {
    Format,
    Content,
    Organization,
}

pub enum ExportMenuAction {
    Close,
    Export {
        format: ExportFormat,
        content: ExportContent,
        organization: ExportOrganization,
    },
}

pub struct ExportMenu {
    current_step: MenuStep,
    state: ListState,
    last_popup_area: Option<Rect>,

    // Selected options (accumulated as user progresses)
    selected_format: Option<ExportFormat>,
    selected_content: Option<ExportContent>,
    selected_organization: Option<ExportOrganization>,
}

impl ExportMenu {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));

        Self {
            current_step: MenuStep::Format,
            state,
            last_popup_area: None,
            selected_format: None,
            selected_content: None,
            selected_organization: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(50, 40, area);
        self.last_popup_area = Some(popup_area);

        f.render_widget(Clear, popup_area);

        let palette = current_theme();

        let (title, items) = self.get_current_menu_items();

        let list_items: Vec<ListItem> = items
            .iter()
            .map(|item| {
                ListItem::new(Line::from(Span::styled(
                    item,
                    Style::default().fg(palette.base_05),
                )))
            })
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette.base_0c))
                    .style(Style::default().bg(palette.base_00)),
            )
            .highlight_style(
                Style::default()
                    .bg(palette.base_02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("» ");

        f.render_stateful_widget(list, popup_area, &mut self.state);
    }

    fn get_current_menu_items(&self) -> (String, Vec<String>) {
        match self.current_step {
            MenuStep::Format => (
                "Export Annotations - Select Format".to_string(),
                vec![
                    "Markdown (.md)".to_string(),
                    "Org-mode (.org)".to_string(),
                    "Plain Text (.txt)".to_string(),
                ],
            ),
            MenuStep::Content => (
                "Export Annotations - Content".to_string(),
                vec![
                    "Annotations only".to_string(),
                    "Annotations with context".to_string(),
                ],
            ),
            MenuStep::Organization => (
                "Export Annotations - Organization".to_string(),
                vec![
                    "All notes in single file".to_string(),
                    "Separate files per chapter".to_string(),
                ],
            ),
        }
    }

    fn select_current_item(&mut self) -> Option<ExportMenuAction> {
        let selected_idx = self.state.selected()?;

        match self.current_step {
            MenuStep::Format => {
                self.selected_format = Some(match selected_idx {
                    0 => ExportFormat::Markdown,
                    1 => ExportFormat::OrgMode,
                    2 => ExportFormat::PlainText,
                    _ => return None,
                });
                self.current_step = MenuStep::Content;
                self.state.select(Some(0));
                None
            }
            MenuStep::Content => {
                self.selected_content = Some(match selected_idx {
                    0 => ExportContent::AnnotationsOnly,
                    1 => ExportContent::AnnotationsWithContext,
                    _ => return None,
                });
                self.current_step = MenuStep::Organization;
                self.state.select(Some(0));
                None
            }
            MenuStep::Organization => {
                self.selected_organization = Some(match selected_idx {
                    0 => ExportOrganization::SingleFile,
                    1 => ExportOrganization::ChapterBased,
                    _ => return None,
                });

                // All selections complete - trigger export
                Some(ExportMenuAction::Export {
                    format: self.selected_format?,
                    content: self.selected_content?,
                    organization: self.selected_organization?,
                })
            }
        }
    }

    fn go_back(&mut self) -> bool {
        match self.current_step {
            MenuStep::Format => false, // Can't go back from first step
            MenuStep::Content => {
                self.current_step = MenuStep::Format;
                self.state.select(Some(0));
                true
            }
            MenuStep::Organization => {
                self.current_step = MenuStep::Content;
                self.state.select(Some(0));
                true
            }
        }
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        key_seq: &mut KeySeq,
    ) -> Option<ExportMenuAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('j') => {
                self.handle_j();
                None
            }
            KeyCode::Char('k') => {
                self.handle_k();
                None
            }
            KeyCode::Char('g') if key_seq.handle_key('g') == "gg" => {
                self.handle_gg();
                None
            }
            KeyCode::Char('G') => {
                self.handle_upper_g();
                None
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_d();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_ctrl_u();
                None
            }
            KeyCode::Esc => {
                // Try to go back first, close if at first step
                if !self.go_back() {
                    Some(ExportMenuAction::Close)
                } else {
                    None
                }
            }
            KeyCode::Char('h') => {
                // Vim-style 'h' to go back
                if !self.go_back() {
                    Some(ExportMenuAction::Close)
                } else {
                    None
                }
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                // Enter or vim 'l' to select/proceed
                self.select_current_item()
            }
            _ => None,
        }
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) -> bool {
        if let Some(popup_area) = self.last_popup_area {
            if x >= popup_area.x
                && x < popup_area.x + popup_area.width
                && y > popup_area.y
                && y < popup_area.y + popup_area.height - 1
            {
                let relative_y = y.saturating_sub(popup_area.y).saturating_sub(1);
                let offset = self.state.offset();
                let new_index = offset + relative_y as usize;

                let item_count = match self.current_step {
                    MenuStep::Format => 3,
                    MenuStep::Content => 2,
                    MenuStep::Organization => 2,
                };

                if new_index < item_count {
                    self.state.select(Some(new_index));
                    return true;
                }
            }
        }
        false
    }

    pub fn is_outside_popup_area(&self, x: u16, y: u16) -> bool {
        if let Some(popup_area) = self.last_popup_area {
            x < popup_area.x
                || x >= popup_area.x + popup_area.width
                || y < popup_area.y
                || y >= popup_area.y + popup_area.height
        } else {
            true
        }
    }
}

impl VimNavMotions for ExportMenu {
    fn handle_h(&mut self) {
        // Already handled in handle_key as go_back
    }

    fn handle_j(&mut self) {
        let item_count = match self.current_step {
            MenuStep::Format => 3,
            MenuStep::Content => 2,
            MenuStep::Organization => 2,
        };

        let i = match self.state.selected() {
            Some(i) => {
                if i >= item_count - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn handle_k(&mut self) {
        let item_count: usize = match self.current_step {
            MenuStep::Format => 3,
            MenuStep::Content => 2,
            MenuStep::Organization => 2,
        };

        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    item_count.saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn handle_l(&mut self) {
        // Select current (handled in handle_key)
    }

    fn handle_ctrl_d(&mut self) {
        // Half page down
        self.handle_j();
    }

    fn handle_ctrl_u(&mut self) {
        // Half page up
        self.handle_k();
    }

    fn handle_gg(&mut self) {
        self.state.select(Some(0));
    }

    fn handle_upper_g(&mut self) {
        let item_count: usize = match self.current_step {
            MenuStep::Format => 3,
            MenuStep::Content => 2,
            MenuStep::Organization => 2,
        };
        self.state.select(Some(item_count.saturating_sub(1)));
    }
}

// Helper function for centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
