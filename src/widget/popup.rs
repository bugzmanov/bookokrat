use ::ratatui::layout::Rect;

pub trait Popup {
    /// Implementations should include field:
    /// last_popup_area: Option<Rect>
    fn get_last_popup_area(&self) -> Option<Rect>;

    /// Get the height of the popup for use in calculations
    fn popup_height(&self, default: u16) -> u16 {
        let popup_border_lines = 2;
        if let Some(popup_area) = self.get_last_popup_area() {
            popup_area.height - popup_border_lines
        } else {
            default
        }
    }

    /// Check if the given coordinates are outside the popup area
    fn is_outside_popup_area(&self, x: u16, y: u16) -> bool {
        if let Some(popup_area) = self.get_last_popup_area() {
            x < popup_area.x
                || x >= popup_area.x + popup_area.width
                || y < popup_area.y
                || y >= popup_area.y + popup_area.height
        } else {
            true
        }
    }
}
