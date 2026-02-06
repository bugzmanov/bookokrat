use std::env;

#[cfg(feature = "pdf")]
use log::warn;

use crate::vendored::ratatui_image::picker::{Picker, ProtocolType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKind {
    Kitty,
    Ghostty,
    Konsole,
    WezTerm,
    ITerm,
    AppleTerminal,
    VsCode,
    Tmux,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsProtocol {
    Kitty,
    Iterm2,
    Sixel,
    Halfblocks,
}

#[derive(Clone, Debug)]
pub struct TerminalEnv {
    pub term_program: String,
    pub term_program_version: String,
    pub term: String,
    pub colorterm: String,
    pub iterm_session: bool,
    pub wezterm_executable: bool,
    pub kitty_window: bool,
    pub kitty_pid: bool,
    pub tmux: bool,
}

impl TerminalEnv {
    pub fn read() -> Self {
        let term_program = env::var("TERM_PROGRAM")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();
        let term_program_version = env::var("TERM_PROGRAM_VERSION").unwrap_or_default();
        let term = env::var("TERM")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();
        let colorterm = env::var("COLORTERM")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();

        Self {
            term_program,
            term_program_version,
            term,
            colorterm,
            iterm_session: env::var("ITERM_SESSION_ID").is_ok(),
            wezterm_executable: env::var("WEZTERM_EXECUTABLE").is_ok(),
            kitty_window: env::var("KITTY_WINDOW_ID").is_ok(),
            kitty_pid: env::var("KITTY_PID").is_ok(),
            tmux: env::var("TMUX").is_ok(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PdfCapabilities {
    pub supported: bool,
    pub blocked_reason: Option<String>,
    pub supports_comments: bool,
    pub supports_scroll_mode: bool,
    pub supports_normal_mode: bool,
    pub supports_kitty_shm: Option<bool>,
    pub supports_kitty_delete_range: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct TerminalCapabilities {
    pub env: TerminalEnv,
    pub kind: TerminalKind,
    pub protocol: Option<GraphicsProtocol>,
    pub supports_graphics: bool,
    pub supports_true_color: bool,
    pub pdf: PdfCapabilities,
}

pub fn detect_terminal() -> TerminalCapabilities {
    let env = TerminalEnv::read();
    detect_terminal_from_env(env)
}

pub fn detect_terminal_with_picker(picker: &mut Picker) -> TerminalCapabilities {
    let env = TerminalEnv::read();
    let mut caps = detect_terminal_from_env(env);

    apply_protocol_overrides(&mut caps, Some(picker));
    caps.supports_graphics = protocol_supports_graphics(caps.protocol);
    caps.pdf = derive_pdf_capabilities(&caps);

    caps
}

pub fn supports_true_color() -> bool {
    let env = TerminalEnv::read();
    supports_true_color_env(&env)
}

pub fn detect_terminal_from_env(env: TerminalEnv) -> TerminalCapabilities {
    let kind = detect_kind(&env);
    let supports_true_color = supports_true_color_env(&env);
    let protocol = guess_protocol_from_env(&env, kind);
    let supports_graphics = supports_graphics_from_env(&env);
    let mut caps = TerminalCapabilities {
        env,
        kind,
        protocol,
        supports_graphics,
        supports_true_color,
        pdf: PdfCapabilities {
            supported: false,
            blocked_reason: None,
            supports_comments: false,
            supports_scroll_mode: false,
            supports_normal_mode: false,
            supports_kitty_shm: None,
            supports_kitty_delete_range: None,
        },
    };

    apply_protocol_overrides(&mut caps, None);
    caps.supports_graphics = protocol_supports_graphics(caps.protocol) || supports_graphics;
    caps.pdf = derive_pdf_capabilities(&caps);

    caps
}

fn detect_kind(env: &TerminalEnv) -> TerminalKind {
    if env.term_program == "kitty" {
        return TerminalKind::Kitty;
    }
    if env.term_program == "ghostty" {
        return TerminalKind::Ghostty;
    }
    if env.term_program == "konsole" || env::var("KONSOLE_VERSION").is_ok() {
        return TerminalKind::Konsole;
    }
    if env.term_program == "wezterm" {
        return TerminalKind::WezTerm;
    }
    if env.term_program == "iterm.app" || env.iterm_session {
        return TerminalKind::ITerm;
    }
    if env.term_program == "apple_terminal" {
        return TerminalKind::AppleTerminal;
    }
    if env.term_program == "vscode" {
        return TerminalKind::VsCode;
    }
    if env.tmux {
        return TerminalKind::Tmux;
    }
    if env.kitty_window || env.kitty_pid {
        return TerminalKind::Kitty;
    }
    if env.term.contains("kitty") {
        return TerminalKind::Kitty;
    }
    if env.term.contains("ghostty") {
        return TerminalKind::Ghostty;
    }
    TerminalKind::Unknown
}

fn supports_true_color_env(env: &TerminalEnv) -> bool {
    env.colorterm == "truecolor"
        || env.colorterm == "24bit"
        || env.term.contains("truecolor")
        || env.term.contains("24bit")
}

fn supports_graphics_from_env(env: &TerminalEnv) -> bool {
    if env.kitty_window || env.kitty_pid || env.iterm_session || env.wezterm_executable {
        return true;
    }

    let graphics_terminals = [
        "kitty",
        "ghostty",
        "konsole",
        "iterm.app",
        "wezterm",
        "mintty",
        "vscode",
        "tabby",
        "hyper",
        "rio",
        "bobcat",
        "warpterminal",
    ];

    for terminal in graphics_terminals {
        if env.term_program.contains(terminal) {
            return true;
        }
    }

    env.term.contains("kitty") || env.term.contains("ghostty")
}

fn guess_protocol_from_env(env: &TerminalEnv, kind: TerminalKind) -> Option<GraphicsProtocol> {
    match kind {
        TerminalKind::Kitty | TerminalKind::Ghostty => Some(GraphicsProtocol::Kitty),
        TerminalKind::Konsole => Some(GraphicsProtocol::Iterm2),
        TerminalKind::WezTerm => Some(GraphicsProtocol::Iterm2),
        TerminalKind::ITerm => {
            if iterm_supports_kitty(env) {
                Some(GraphicsProtocol::Kitty)
            } else {
                None
            }
        }
        _ => {
            if env.kitty_window || env.kitty_pid || env.term.contains("kitty") {
                Some(GraphicsProtocol::Kitty)
            } else {
                None
            }
        }
    }
}

fn protocol_supports_graphics(protocol: Option<GraphicsProtocol>) -> bool {
    matches!(
        protocol,
        Some(GraphicsProtocol::Kitty)
            | Some(GraphicsProtocol::Iterm2)
            | Some(GraphicsProtocol::Sixel)
    )
}

fn iterm_supports_kitty(env: &TerminalEnv) -> bool {
    if env.term_program != "iterm.app" && !env.iterm_session {
        return false;
    }
    let (major, minor) = parse_version_major_minor(&env.term_program_version);
    major > 3 || (major == 3 && minor >= 6)
}

fn parse_version_major_minor(version: &str) -> (u32, u32) {
    let version_parts: Vec<u32> = version.split('.').filter_map(|s| s.parse().ok()).collect();
    (
        version_parts.first().copied().unwrap_or(0),
        version_parts.get(1).copied().unwrap_or(0),
    )
}

fn apply_protocol_overrides(caps: &mut TerminalCapabilities, picker: Option<&mut Picker>) {
    let mut protocol = caps.protocol;

    if caps.kind == TerminalKind::ITerm {
        if iterm_supports_kitty(&caps.env) {
            protocol = Some(GraphicsProtocol::Kitty);
        }
    } else if caps.kind == TerminalKind::Konsole {
        protocol = Some(GraphicsProtocol::Iterm2);
    } else if caps.kind == TerminalKind::WezTerm {
        protocol = Some(GraphicsProtocol::Iterm2);
    }

    if let Some(picker) = picker {
        match caps.kind {
            TerminalKind::ITerm if iterm_supports_kitty(&caps.env) => {
                picker.set_protocol_type(ProtocolType::Kitty);
            }
            TerminalKind::WezTerm => {
                picker.set_protocol_type(ProtocolType::Iterm2);
            }
            TerminalKind::Konsole => {
                picker.set_protocol_type(ProtocolType::Iterm2);
            }
            _ => {}
        }

        let picker_protocol = match picker.protocol_type() {
            ProtocolType::Kitty => Some(GraphicsProtocol::Kitty),
            ProtocolType::Iterm2 => Some(GraphicsProtocol::Iterm2),
            ProtocolType::Sixel => Some(GraphicsProtocol::Sixel),
            ProtocolType::Halfblocks => Some(GraphicsProtocol::Halfblocks),
        };
        protocol = picker_protocol.or(protocol);
    }

    caps.protocol = protocol;
}

fn derive_pdf_capabilities(caps: &TerminalCapabilities) -> PdfCapabilities {
    let mut supported = caps.supports_graphics;
    let mut blocked_reason = None;

    if caps.kind == TerminalKind::ITerm && !iterm_supports_kitty(&caps.env) {
        supported = false;
        let iterm_version = &caps.env.term_program_version;
        blocked_reason = Some(format!(
            "PDF requires iTerm 3.6+. Current version: {}",
            if iterm_version.is_empty() {
                "unknown".to_string()
            } else {
                iterm_version.clone()
            }
        ));
    }

    let supports_comments = matches!(
        caps.protocol,
        Some(GraphicsProtocol::Kitty) | Some(GraphicsProtocol::Iterm2)
    );
    let supports_scroll_mode = matches!(caps.kind, TerminalKind::Kitty | TerminalKind::Ghostty)
        && matches!(caps.protocol, Some(GraphicsProtocol::Kitty));
    let supports_normal_mode = caps.kind != TerminalKind::ITerm;

    PdfCapabilities {
        supported,
        blocked_reason,
        supports_comments,
        supports_scroll_mode,
        supports_normal_mode,
        supports_kitty_shm: None,
        supports_kitty_delete_range: None,
    }
}

#[cfg(feature = "pdf")]
pub fn probe_kitty_shm_support(caps: &TerminalCapabilities) -> Option<bool> {
    if env::var("BOOKOKRAT_DISABLE_KITTY_SHM").is_ok() {
        return Some(false);
    }
    if !matches!(caps.protocol, Some(GraphicsProtocol::Kitty)) {
        return None;
    }

    let mode = crate::pdf::kittyv2::probe_capabilities();
    match mode {
        crate::pdf::kittyv2::TransferMode::SharedMemory => Some(true),
        crate::pdf::kittyv2::TransferMode::Chunked => {
            warn!("Kitty SHM probe failed; will use chunked transfer.");
            Some(false)
        }
    }
}

#[cfg(feature = "pdf")]
pub fn probe_kitty_delete_range_support(caps: &TerminalCapabilities) -> Option<bool> {
    if !matches!(caps.protocol, Some(GraphicsProtocol::Kitty)) {
        return None;
    }
    Some(crate::pdf::kittyv2::probe_delete_range_support())
}

pub fn protocol_override_from_env() -> Option<ProtocolType> {
    let value = env::var("BOOKOKRAT_PROTOCOL").ok()?;
    match value.to_ascii_lowercase().as_str() {
        "halfblocks" | "half" | "blocks" => Some(ProtocolType::Halfblocks),
        "sixel" => Some(ProtocolType::Sixel),
        "kitty" => Some(ProtocolType::Kitty),
        "iterm" | "iterm2" => Some(ProtocolType::Iterm2),
        _ => None,
    }
}
