pub mod action;
pub mod config;
pub mod context;
pub mod defaults;
pub mod keymap;
pub mod notation;

use std::sync::{LazyLock, RwLock, RwLockReadGuard};

use keymap::Keymap;

/// The global keymap starts as pure defaults. Production callers (main.rs)
/// invoke `reload_keymap()` at startup to layer user overrides on top.
///
/// This means tests — which never call `reload_keymap()` — see only defaults
/// and are hermetic with respect to the developer's `~/.config/bookokrat/`.
static KEYMAP: LazyLock<RwLock<Keymap>> = LazyLock::new(|| RwLock::new(defaults::default_keymap()));

/// Get a read guard to the global keymap.
pub fn keymap() -> RwLockReadGuard<'static, Keymap> {
    KEYMAP.read().unwrap()
}

/// Reload the keymap from config (defaults + user overrides). Returns any
/// issues discovered while parsing the user's file so callers can surface
/// them in the UI.
pub fn reload_keymap() -> Vec<config::LoadError> {
    let (new_km, errors) = config::load_keymap();
    if let Ok(mut km) = KEYMAP.write() {
        *km = new_km;
    }
    errors
}
