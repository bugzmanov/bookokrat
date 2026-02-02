//! Text selection state for PDF pages

/// A point in the selection with both terminal and PDF coordinates
#[derive(Clone, Copy, Debug, Default)]
pub struct SelectionPoint {
    /// Terminal column
    pub term_col: u16,
    /// Terminal row
    pub term_row: u16,
    /// Page index
    pub page: usize,
    /// X coordinate in PDF space
    pub pdf_x: f32,
    /// Y coordinate in PDF space
    pub pdf_y: f32,
}

/// Text selection state
#[derive(Clone, Debug, Default)]
pub struct TextSelection {
    /// Start point of selection
    pub start: Option<SelectionPoint>,
    /// End point of selection
    pub end: Option<SelectionPoint>,
    /// Whether selection is in progress
    pub is_selecting: bool,
}

impl TextSelection {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start selection at a point
    pub fn start_at(&mut self, point: SelectionPoint) {
        self.start = Some(point);
        self.end = Some(point);
        self.is_selecting = true;
    }

    /// Update the end point during selection
    pub fn update_end(&mut self, point: SelectionPoint) {
        if self.is_selecting {
            self.end = Some(point);
        }
    }

    /// Finish selection
    pub fn finish(&mut self) {
        self.is_selecting = false;
    }

    /// Clear selection
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.is_selecting = false;
    }

    /// Check if there is an active selection
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.start.is_some() && self.end.is_some()
    }

    /// Get ordered selection bounds (start before end)
    #[must_use]
    pub fn get_ordered_bounds(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        match (self.start, self.end) {
            (Some(start), Some(end)) => {
                let start_before = (start.page, start.term_row, start.term_col)
                    <= (end.page, end.term_row, end.term_col);
                if start_before {
                    Some((start, end))
                } else {
                    Some((end, start))
                }
            }
            _ => None,
        }
    }
}

/// Rectangle representing a selection area on a page
#[derive(Clone, Debug)]
pub struct SelectionRect {
    pub page: usize,
    pub topleft_x: u32,
    pub topleft_y: u32,
    pub bottomright_x: u32,
    pub bottomright_y: u32,
}

/// Request to extract text from selected regions
#[derive(Clone, Debug)]
pub struct ExtractionRequest {
    /// Bounds for each page in the selection
    pub bounds: Vec<super::request::PageSelectionBounds>,
}
