#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]
#![warn(clippy::dbg_macro, clippy::print_stdout)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[cfg(all(feature = "ratatui", feature = "tuirs"))]
compile_error!("ratatui support and tui-rs support are exclusive. only one of them can be enabled at the same time. see https://github.com/rhysd/tui-textarea#installation");

mod cursor;
mod highlight;
mod history;
mod input;
mod scroll;
#[cfg(feature = "search")]
mod search;
mod textarea;
mod util;
mod widget;
mod word;

#[cfg(feature = "ratatui")]
#[allow(clippy::single_component_path_imports)]
mod ratatui {
    // Best effort to reproduce ratatui 0.29 module layout to keep compatibility with tui module layout
    pub use ratatui_core::{buffer, layout, style, text};
    pub mod widgets {
        pub use ratatui_core::widgets::*;
        pub use ratatui_widgets::{block::Block, paragraph::Paragraph};
    }
}
#[cfg(feature = "tuirs")]
use tui as ratatui;

#[cfg(all(feature = "crossterm", not(feature = "crossterm_0_28")))]
#[allow(clippy::single_component_path_imports)]
use crossterm;
#[cfg(feature = "tuirs-crossterm")]
use crossterm_025 as crossterm;
#[cfg(feature = "crossterm_0_28")]
#[allow(clippy::single_component_path_imports)]
use crossterm_028 as crossterm;

#[cfg(feature = "termion")]
#[allow(clippy::single_component_path_imports)]
use termion;
#[cfg(feature = "tuirs-termion")]
use termion_15 as termion;

pub use cursor::CursorMove;
pub use input::{Input, Key};
pub use scroll::Scrolling;
pub use textarea::TextArea;
