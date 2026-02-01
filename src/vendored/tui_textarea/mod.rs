#![allow(clippy::needless_range_loop)]

mod cursor;
mod highlight;
mod history;
mod input;
mod scroll;
mod search;
mod textarea;
mod util;
mod widget;
mod word;

mod ratatui {
    pub use ::ratatui::{buffer, layout, style, text, widgets};
}

pub use cursor::CursorMove;
pub use input::{Input, Key};
pub use scroll::Scrolling;
pub use textarea::TextArea;
