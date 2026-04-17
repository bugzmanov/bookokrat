pub mod action;
pub mod config;
pub mod context;
pub mod defaults;
pub mod keymap;
pub mod notation;

use std::sync::{LazyLock, RwLock, RwLockReadGuard};

use keymap::Keymap;

static KEYMAP: LazyLock<RwLock<Keymap>> = LazyLock::new(|| RwLock::new(config::load_keymap()));

/// Get a read guard to the global keymap.
pub fn keymap() -> RwLockReadGuard<'static, Keymap> {
    KEYMAP.read().unwrap()
}

/// Reload the keymap from config (defaults + user overrides).
pub fn reload_keymap() {
    if let Ok(mut km) = KEYMAP.write() {
        *km = config::load_keymap();
    }
}
