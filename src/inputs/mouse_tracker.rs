use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClickType {
    Single,
    Double,
    Triple,
}

pub struct MouseTracker {
    last_click_time: Option<Instant>,
    last_click_position: Option<(u16, u16)>,
    click_count: u32,
}

impl MouseTracker {
    pub fn new() -> Self {
        Self {
            last_click_time: None,
            last_click_position: None,
            click_count: 0,
        }
    }
    pub fn detect_click_type(&mut self, column: u16, row: u16) -> ClickType {
        const DOUBLE_CLICK_TIME_MS: u64 = 500; // Maximum time between clicks for double-click
        const CLICK_DISTANCE_THRESHOLD: u16 = 3; // Maximum distance between clicks

        let now = Instant::now();
        let position = (column, row);

        let is_within_time = if let Some(last_time) = self.last_click_time {
            now.duration_since(last_time).as_millis() <= DOUBLE_CLICK_TIME_MS as u128
        } else {
            false
        };

        let is_within_distance = if let Some(last_pos) = self.last_click_position {
            let distance_x = column.abs_diff(last_pos.0);
            let distance_y = row.abs_diff(last_pos.1);
            distance_x <= CLICK_DISTANCE_THRESHOLD && distance_y <= CLICK_DISTANCE_THRESHOLD
        } else {
            false
        };

        if is_within_time && is_within_distance {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }

        self.last_click_time = Some(now);
        self.last_click_position = Some(position);

        match self.click_count {
            2 => ClickType::Double,
            3 => ClickType::Triple,
            _ => ClickType::Single,
        }
    }
}

impl Default for MouseTracker {
    fn default() -> Self {
        Self::new()
    }
}
