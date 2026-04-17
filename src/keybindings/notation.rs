use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use std::fmt;

/// A single key press with modifiers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyInput {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    pub fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
}

/// A complete key binding: one or more KeyInputs in sequence
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding(pub Vec<KeyInput>);

impl KeyBinding {
    pub fn single(input: KeyInput) -> Self {
        Self(vec![input])
    }

    pub fn keys(&self) -> &[KeyInput] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_key_binding(self))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyNotationError {
    #[error("empty key binding")]
    Empty,
    #[error("invalid key notation: {0}")]
    InvalidNotation(String),
    #[error("unknown key name: {0}")]
    UnknownKeyName(String),
    #[error("unclosed angle bracket in: {0}")]
    UnclosedBracket(String),
    #[error("empty angle bracket <>")]
    EmptyBracket,
    #[error("modifier without key name: {0}")]
    ModifierWithoutKey(String),
}

/// Parse a neovim-style key notation string into a KeyBinding.
///
/// Examples:
/// - `"j"` → single key j
/// - `"gg"` → sequence: g, g
/// - `"<C-d>"` → Ctrl+d
/// - `"<Space>f"` → Space then f
/// - `"<C-S-a>"` → Ctrl+Shift+a
/// - `"<lt>"` → literal <
pub fn parse_key_binding(s: &str) -> Result<KeyBinding, KeyNotationError> {
    if s.is_empty() {
        return Err(KeyNotationError::Empty);
    }

    let mut keys = Vec::new();
    let mut chars = s.chars().peekable();

    while chars.peek().is_some() {
        let input = parse_next_key(&mut chars, s)?;
        keys.push(input);
    }

    if keys.is_empty() {
        return Err(KeyNotationError::Empty);
    }

    Ok(KeyBinding(keys))
}

/// Format a KeyBinding back to neovim-style notation string (for config files).
pub fn format_key_binding(binding: &KeyBinding) -> String {
    let mut result = String::new();
    for input in &binding.0 {
        result.push_str(&format_key_input(input));
    }
    result
}

/// Format a KeyBinding for display in the UI (human-friendly).
/// Uses `Ctrl+d` instead of `<C-d>`, `Space+a` instead of `<Space>a`, etc.
pub fn format_key_binding_display(binding: &KeyBinding) -> String {
    let parts: Vec<String> = binding.0.iter().map(format_key_input_display).collect();
    if parts.len() == 1 {
        return parts[0].clone();
    }
    // Multi-key sequences: join with + if first is a modifier-like key (Space, Ctrl+x)
    // Otherwise concatenate (e.g., gg)
    let first_is_modifier =
        matches!(binding.0[0].code, KeyCode::Char(' ')) || !binding.0[0].modifiers.is_empty();
    if first_is_modifier {
        parts.join("+")
    } else {
        parts.concat()
    }
}

fn format_key_input_display(input: &KeyInput) -> String {
    let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = input.modifiers.contains(KeyModifiers::ALT);

    let key_name = match input.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "S-Tab".to_string(),
        KeyCode::Backspace => "BS".to_string(),
        KeyCode::Delete => "Del".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => format!("{:?}", input.code),
    };

    let mut prefix = String::new();
    if has_ctrl {
        prefix.push_str("Ctrl+");
    }
    if has_alt {
        prefix.push_str("Alt+");
    }
    format!("{prefix}{key_name}")
}

/// Convert a crossterm KeyEvent to a KeyInput for matching.
pub fn key_event_to_input(event: &KeyEvent) -> KeyInput {
    let mut modifiers = event.modifiers;
    let code = event.code;

    // crossterm reports uppercase chars as Shift+char; we normalize:
    // Shift+Char('A') stays as-is (the uppercase char IS the identity)
    // but we strip SHIFT from the modifiers since the char itself carries the case.
    if let KeyCode::Char(c) = code {
        if c.is_ascii_uppercase() {
            modifiers.remove(KeyModifiers::SHIFT);
        }
    }

    // BackTab is crossterm's way of saying Shift+Tab.
    // Normalize to KeyCode::BackTab with SHIFT removed from modifiers.
    if code == KeyCode::BackTab {
        modifiers.remove(KeyModifiers::SHIFT);
    }

    KeyInput { code, modifiers }
}

fn parse_next_key(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    full_str: &str,
) -> Result<KeyInput, KeyNotationError> {
    match chars.peek() {
        Some('<') => {
            chars.next(); // consume '<'
            let mut token = String::new();
            let mut found_close = false;
            for c in chars.by_ref() {
                if c == '>' {
                    found_close = true;
                    break;
                }
                token.push(c);
            }
            if !found_close {
                return Err(KeyNotationError::UnclosedBracket(full_str.to_string()));
            }
            if token.is_empty() {
                return Err(KeyNotationError::EmptyBracket);
            }
            parse_angle_bracket_token(&token)
        }
        Some(&c) => {
            chars.next();
            Ok(KeyInput::char(c))
        }
        None => Err(KeyNotationError::Empty),
    }
}

fn parse_angle_bracket_token(token: &str) -> Result<KeyInput, KeyNotationError> {
    let mut modifiers = KeyModifiers::NONE;
    let mut remaining = token;

    // Parse modifier prefixes: C-, A-/M-, S- (case-insensitive)
    loop {
        let lower = remaining.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("c-") {
            modifiers |= KeyModifiers::CONTROL;
            remaining = &remaining[2..];
            if rest.is_empty() {
                return Err(KeyNotationError::ModifierWithoutKey(token.to_string()));
            }
        } else if let Some(rest) = lower
            .strip_prefix("a-")
            .or_else(|| lower.strip_prefix("m-"))
        {
            modifiers |= KeyModifiers::ALT;
            remaining = &remaining[2..];
            if rest.is_empty() {
                return Err(KeyNotationError::ModifierWithoutKey(token.to_string()));
            }
        } else if let Some(rest) = lower.strip_prefix("s-") {
            modifiers |= KeyModifiers::SHIFT;
            remaining = &remaining[2..];
            if rest.is_empty() {
                return Err(KeyNotationError::ModifierWithoutKey(token.to_string()));
            }
        } else {
            break;
        }
    }

    // Parse the key name (case-insensitive)
    let key_lower = remaining.to_ascii_lowercase();
    let code = match key_lower.as_str() {
        "cr" | "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "tab" => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                modifiers.remove(KeyModifiers::SHIFT);
                KeyCode::BackTab
            } else {
                KeyCode::Tab
            }
        }
        "backtab" | "s-tab" => KeyCode::BackTab,
        "bs" | "backspace" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "pageup" | "page_up" => KeyCode::PageUp,
        "pagedown" | "page_down" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" | "ins" => KeyCode::Insert,
        "lt" => KeyCode::Char('<'),
        "gt" => KeyCode::Char('>'),
        "bar" => KeyCode::Char('|'),
        "bslash" => KeyCode::Char('\\'),
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        _ => {
            // Single character inside brackets (e.g., <C-a>)
            let chars: Vec<char> = remaining.chars().collect();
            if chars.len() == 1 {
                KeyCode::Char(chars[0])
            } else {
                return Err(KeyNotationError::UnknownKeyName(remaining.to_string()));
            }
        }
    };

    Ok(KeyInput::new(code, modifiers))
}

fn format_key_input(input: &KeyInput) -> String {
    let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = input.modifiers.contains(KeyModifiers::ALT);
    let has_shift = input.modifiers.contains(KeyModifiers::SHIFT);
    let has_modifiers = has_ctrl || has_alt || has_shift;

    match input.code {
        KeyCode::Char(c) => {
            if c == ' ' {
                if has_modifiers {
                    format!("<{}>", modifier_prefix(input.modifiers, "Space"))
                } else {
                    "<Space>".to_string()
                }
            } else if c == '<' {
                if has_modifiers {
                    format!("<{}>", modifier_prefix(input.modifiers, "lt"))
                } else {
                    "<lt>".to_string()
                }
            } else if c == '>' {
                if has_modifiers {
                    format!("<{}>", modifier_prefix(input.modifiers, "gt"))
                } else {
                    "<gt>".to_string()
                }
            } else if c == '|' {
                if has_modifiers {
                    format!("<{}>", modifier_prefix(input.modifiers, "Bar"))
                } else {
                    "<Bar>".to_string()
                }
            } else if c == '\\' {
                if has_modifiers {
                    format!("<{}>", modifier_prefix(input.modifiers, "Bslash"))
                } else {
                    "<Bslash>".to_string()
                }
            } else if has_modifiers {
                format!("<{}>", modifier_prefix(input.modifiers, &c.to_string()))
            } else {
                c.to_string()
            }
        }
        KeyCode::Enter => format!("<{}>", modifier_prefix(input.modifiers, "CR")),
        KeyCode::Esc => format!("<{}>", modifier_prefix(input.modifiers, "Esc")),
        KeyCode::Tab => format!("<{}>", modifier_prefix(input.modifiers, "Tab")),
        KeyCode::BackTab => "<S-Tab>".to_string(),
        KeyCode::Backspace => format!("<{}>", modifier_prefix(input.modifiers, "BS")),
        KeyCode::Delete => format!("<{}>", modifier_prefix(input.modifiers, "Del")),
        KeyCode::Up => format!("<{}>", modifier_prefix(input.modifiers, "Up")),
        KeyCode::Down => format!("<{}>", modifier_prefix(input.modifiers, "Down")),
        KeyCode::Left => format!("<{}>", modifier_prefix(input.modifiers, "Left")),
        KeyCode::Right => format!("<{}>", modifier_prefix(input.modifiers, "Right")),
        KeyCode::PageUp => format!("<{}>", modifier_prefix(input.modifiers, "PageUp")),
        KeyCode::PageDown => format!("<{}>", modifier_prefix(input.modifiers, "PageDown")),
        KeyCode::Home => format!("<{}>", modifier_prefix(input.modifiers, "Home")),
        KeyCode::End => format!("<{}>", modifier_prefix(input.modifiers, "End")),
        KeyCode::Insert => format!("<{}>", modifier_prefix(input.modifiers, "Insert")),
        KeyCode::F(n) => format!("<{}>", modifier_prefix(input.modifiers, &format!("F{n}"))),
        _ => format!("<{:?}>", input.code),
    }
}

fn modifier_prefix(modifiers: KeyModifiers, key_name: &str) -> String {
    let mut prefix = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        prefix.push_str("C-");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        prefix.push_str("A-");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        prefix.push_str("S-");
    }
    prefix.push_str(key_name);
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ki(code: KeyCode, modifiers: KeyModifiers) -> KeyInput {
        KeyInput::new(code, modifiers)
    }

    fn parse(s: &str) -> KeyBinding {
        parse_key_binding(s).unwrap_or_else(|e| panic!("failed to parse '{s}': {e}"))
    }

    // === Basic single keys ===

    #[test]
    fn parse_lowercase_char() {
        assert_eq!(parse("j"), KeyBinding(vec![KeyInput::char('j')]));
    }

    #[test]
    fn parse_uppercase_char() {
        assert_eq!(parse("G"), KeyBinding(vec![KeyInput::char('G')]));
    }

    #[test]
    fn parse_question_mark() {
        assert_eq!(parse("?"), KeyBinding(vec![KeyInput::char('?')]));
    }

    #[test]
    fn parse_slash() {
        assert_eq!(parse("/"), KeyBinding(vec![KeyInput::char('/')]));
    }

    #[test]
    fn parse_curly_braces() {
        assert_eq!(parse("{"), KeyBinding(vec![KeyInput::char('{')]));
        assert_eq!(parse("}"), KeyBinding(vec![KeyInput::char('}')]));
    }

    #[test]
    fn parse_dash() {
        assert_eq!(parse("-"), KeyBinding(vec![KeyInput::char('-')]));
    }

    #[test]
    fn parse_equals() {
        assert_eq!(parse("="), KeyBinding(vec![KeyInput::char('=')]));
    }

    #[test]
    fn parse_plus() {
        assert_eq!(parse("+"), KeyBinding(vec![KeyInput::char('+')]));
    }

    #[test]
    fn parse_dollar() {
        assert_eq!(parse("$"), KeyBinding(vec![KeyInput::char('$')]));
    }

    #[test]
    fn parse_caret() {
        assert_eq!(parse("^"), KeyBinding(vec![KeyInput::char('^')]));
    }

    #[test]
    fn parse_semicolon_comma() {
        assert_eq!(parse(";"), KeyBinding(vec![KeyInput::char(';')]));
        assert_eq!(parse(","), KeyBinding(vec![KeyInput::char(',')]));
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse("0"), KeyBinding(vec![KeyInput::char('0')]));
    }

    // === Modifier keys ===

    #[test]
    fn parse_ctrl_d() {
        let binding = parse("<C-d>");
        assert_eq!(binding, KeyBinding(vec![KeyInput::ctrl('d')]));
    }

    #[test]
    fn parse_ctrl_uppercase() {
        let binding = parse("<C-D>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(KeyCode::Char('D'), KeyModifiers::CONTROL)])
        );
    }

    #[test]
    fn parse_alt_x() {
        let binding = parse("<A-x>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(KeyCode::Char('x'), KeyModifiers::ALT)])
        );
    }

    #[test]
    fn parse_meta_x_alias() {
        let binding = parse("<M-x>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(KeyCode::Char('x'), KeyModifiers::ALT)])
        );
    }

    #[test]
    fn parse_shift_tab() {
        let binding = parse("<S-Tab>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(KeyCode::BackTab, KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_ctrl_shift_a() {
        let binding = parse("<C-S-a>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )])
        );
    }

    #[test]
    fn parse_modifier_order_independence() {
        let a = parse("<C-A-x>");
        let b = parse("<A-C-x>");
        assert_eq!(a, b);
    }

    #[test]
    fn parse_case_insensitive_modifiers() {
        let a = parse("<c-d>");
        let b = parse("<C-d>");
        assert_eq!(a, b);
    }

    #[test]
    fn parse_all_three_modifiers() {
        let binding = parse("<C-A-S-x>");
        assert_eq!(
            binding,
            KeyBinding(vec![ki(
                KeyCode::Char('x'),
                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
            )])
        );
    }

    // === Special key names ===

    #[test]
    fn parse_enter_cr() {
        assert_eq!(
            parse("<CR>"),
            KeyBinding(vec![ki(KeyCode::Enter, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<Enter>"), parse("<CR>"));
        assert_eq!(parse("<Return>"), parse("<CR>"));
    }

    #[test]
    fn parse_escape() {
        assert_eq!(
            parse("<Esc>"),
            KeyBinding(vec![ki(KeyCode::Esc, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<Escape>"), parse("<Esc>"));
    }

    #[test]
    fn parse_space() {
        assert_eq!(parse("<Space>"), KeyBinding(vec![KeyInput::char(' ')]));
    }

    #[test]
    fn parse_tab() {
        assert_eq!(
            parse("<Tab>"),
            KeyBinding(vec![ki(KeyCode::Tab, KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_backtab_alias() {
        assert_eq!(
            parse("<BackTab>"),
            KeyBinding(vec![ki(KeyCode::BackTab, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<S-Tab>"), parse("<BackTab>"));
    }

    #[test]
    fn parse_backspace() {
        assert_eq!(
            parse("<BS>"),
            KeyBinding(vec![ki(KeyCode::Backspace, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<Backspace>"), parse("<BS>"));
    }

    #[test]
    fn parse_delete() {
        assert_eq!(
            parse("<Del>"),
            KeyBinding(vec![ki(KeyCode::Delete, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<Delete>"), parse("<Del>"));
    }

    #[test]
    fn parse_arrow_keys() {
        assert_eq!(
            parse("<Up>"),
            KeyBinding(vec![ki(KeyCode::Up, KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<Down>"),
            KeyBinding(vec![ki(KeyCode::Down, KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<Left>"),
            KeyBinding(vec![ki(KeyCode::Left, KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<Right>"),
            KeyBinding(vec![ki(KeyCode::Right, KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_page_keys() {
        assert_eq!(
            parse("<PageUp>"),
            KeyBinding(vec![ki(KeyCode::PageUp, KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<PageDown>"),
            KeyBinding(vec![ki(KeyCode::PageDown, KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_home_end() {
        assert_eq!(
            parse("<Home>"),
            KeyBinding(vec![ki(KeyCode::Home, KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<End>"),
            KeyBinding(vec![ki(KeyCode::End, KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_function_keys() {
        assert_eq!(
            parse("<F1>"),
            KeyBinding(vec![ki(KeyCode::F(1), KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<F5>"),
            KeyBinding(vec![ki(KeyCode::F(5), KeyModifiers::NONE)])
        );
        assert_eq!(
            parse("<F12>"),
            KeyBinding(vec![ki(KeyCode::F(12), KeyModifiers::NONE)])
        );
    }

    #[test]
    fn parse_lt_gt() {
        assert_eq!(parse("<lt>"), KeyBinding(vec![KeyInput::char('<')]));
        assert_eq!(parse("<gt>"), KeyBinding(vec![KeyInput::char('>')]));
    }

    #[test]
    fn parse_case_insensitive_key_names() {
        assert_eq!(parse("<cr>"), parse("<CR>"));
        assert_eq!(parse("<esc>"), parse("<Esc>"));
        assert_eq!(parse("<space>"), parse("<Space>"));
        assert_eq!(parse("<pageup>"), parse("<PageUp>"));
    }

    #[test]
    fn parse_insert() {
        assert_eq!(
            parse("<Insert>"),
            KeyBinding(vec![ki(KeyCode::Insert, KeyModifiers::NONE)])
        );
        assert_eq!(parse("<Ins>"), parse("<Insert>"));
    }

    #[test]
    fn parse_bar_bslash() {
        assert_eq!(parse("<Bar>"), KeyBinding(vec![KeyInput::char('|')]));
        assert_eq!(parse("<Bslash>"), KeyBinding(vec![KeyInput::char('\\')]));
    }

    // === Sequences ===

    #[test]
    fn parse_gg_sequence() {
        assert_eq!(
            parse("gg"),
            KeyBinding(vec![KeyInput::char('g'), KeyInput::char('g')])
        );
    }

    #[test]
    fn parse_dd_sequence() {
        assert_eq!(
            parse("dd"),
            KeyBinding(vec![KeyInput::char('d'), KeyInput::char('d')])
        );
    }

    #[test]
    fn parse_space_f_sequence() {
        assert_eq!(
            parse("<Space>f"),
            KeyBinding(vec![KeyInput::char(' '), KeyInput::char('f')])
        );
    }

    #[test]
    fn parse_space_gt_sequence() {
        assert_eq!(
            parse("<Space><gt>"),
            KeyBinding(vec![KeyInput::char(' '), KeyInput::char('>')])
        );
    }

    #[test]
    fn parse_ctrl_w_j_sequence() {
        assert_eq!(
            parse("<C-w>j"),
            KeyBinding(vec![KeyInput::ctrl('w'), KeyInput::char('j')])
        );
    }

    #[test]
    fn parse_single_key_is_length_one() {
        assert_eq!(parse("j").len(), 1);
        assert_eq!(parse("<CR>").len(), 1);
        assert_eq!(parse("<C-d>").len(), 1);
    }

    #[test]
    fn parse_gd_sequence() {
        assert_eq!(
            parse("gd"),
            KeyBinding(vec![KeyInput::char('g'), KeyInput::char('d')])
        );
    }

    #[test]
    fn parse_three_key_sequence() {
        assert_eq!(
            parse("abc"),
            KeyBinding(vec![
                KeyInput::char('a'),
                KeyInput::char('b'),
                KeyInput::char('c')
            ])
        );
    }

    // === Error cases ===

    #[test]
    fn parse_empty_string() {
        assert!(parse_key_binding("").is_err());
    }

    #[test]
    fn parse_unclosed_bracket() {
        assert!(matches!(
            parse_key_binding("<C-d"),
            Err(KeyNotationError::UnclosedBracket(_))
        ));
    }

    #[test]
    fn parse_empty_bracket() {
        assert!(matches!(
            parse_key_binding("<>"),
            Err(KeyNotationError::EmptyBracket)
        ));
    }

    #[test]
    fn parse_unknown_key_name() {
        assert!(matches!(
            parse_key_binding("<Foo>"),
            Err(KeyNotationError::UnknownKeyName(_))
        ));
    }

    #[test]
    fn parse_modifier_without_key() {
        assert!(matches!(
            parse_key_binding("<C->"),
            Err(KeyNotationError::ModifierWithoutKey(_))
        ));
    }

    #[test]
    fn parse_unknown_multi_char() {
        assert!(matches!(
            parse_key_binding("<abc>"),
            Err(KeyNotationError::UnknownKeyName(_))
        ));
    }

    #[test]
    fn parse_invalid_modifier() {
        // 'X-' is not a valid modifier; 'X' is not a known key name either
        // with length > 1, it should be unknown
        assert!(parse_key_binding("<X-a>").is_err());
    }

    // === Round-trip tests ===

    #[test]
    fn roundtrip_simple_chars() {
        for c in ['j', 'k', 'G', '?', '/', '0', 'a', 'Z'] {
            let s = c.to_string();
            let binding = parse(&s);
            let formatted = format_key_binding(&binding);
            assert_eq!(parse(&formatted), binding, "roundtrip failed for '{s}'");
        }
    }

    #[test]
    fn roundtrip_special_keys() {
        for notation in [
            "<CR>",
            "<Esc>",
            "<Space>",
            "<Tab>",
            "<S-Tab>",
            "<BS>",
            "<Del>",
            "<Up>",
            "<Down>",
            "<Left>",
            "<Right>",
            "<PageUp>",
            "<PageDown>",
            "<Home>",
            "<End>",
            "<F1>",
            "<F12>",
            "<lt>",
            "<gt>",
        ] {
            let binding = parse(notation);
            let formatted = format_key_binding(&binding);
            let reparsed = parse(&formatted);
            assert_eq!(
                binding, reparsed,
                "roundtrip failed for '{notation}' -> '{formatted}'"
            );
        }
    }

    #[test]
    fn roundtrip_modifiers() {
        for notation in ["<C-d>", "<A-x>", "<C-S-a>", "<C-A-S-x>"] {
            let binding = parse(notation);
            let formatted = format_key_binding(&binding);
            let reparsed = parse(&formatted);
            assert_eq!(
                binding, reparsed,
                "roundtrip failed for '{notation}' -> '{formatted}'"
            );
        }
    }

    #[test]
    fn roundtrip_sequences() {
        for notation in ["gg", "<Space>f", "<C-w>j", "<Space><gt>"] {
            let binding = parse(notation);
            let formatted = format_key_binding(&binding);
            let reparsed = parse(&formatted);
            assert_eq!(
                binding, reparsed,
                "roundtrip failed for '{notation}' -> '{formatted}'"
            );
        }
    }

    // === Conversion from crossterm KeyEvent ===

    #[test]
    fn key_event_to_input_simple_char() {
        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(key_event_to_input(&event), KeyInput::char('j'));
    }

    #[test]
    fn key_event_to_input_ctrl() {
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let expected = parse("<C-d>");
        assert_eq!(key_event_to_input(&event), expected.0[0]);
    }

    #[test]
    fn key_event_to_input_uppercase_strips_shift() {
        // crossterm sends Shift+'G' for uppercase G
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        let expected = KeyInput::char('G');
        assert_eq!(key_event_to_input(&event), expected);
    }

    #[test]
    fn key_event_to_input_space() {
        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(key_event_to_input(&event), KeyInput::char(' '));
    }

    #[test]
    fn key_event_to_input_backtab() {
        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let expected = ki(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(key_event_to_input(&event), expected);
    }

    #[test]
    fn key_event_to_input_enter() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            key_event_to_input(&event),
            ki(KeyCode::Enter, KeyModifiers::NONE)
        );
    }

    // === Format tests ===

    #[test]
    fn format_simple_char() {
        assert_eq!(format_key_input(&KeyInput::char('j')), "j");
        assert_eq!(format_key_input(&KeyInput::char('G')), "G");
    }

    #[test]
    fn format_ctrl() {
        assert_eq!(format_key_input(&KeyInput::ctrl('d')), "<C-d>");
    }

    #[test]
    fn format_special_keys() {
        assert_eq!(
            format_key_input(&ki(KeyCode::Enter, KeyModifiers::NONE)),
            "<CR>"
        );
        assert_eq!(
            format_key_input(&ki(KeyCode::Esc, KeyModifiers::NONE)),
            "<Esc>"
        );
        assert_eq!(
            format_key_input(&ki(KeyCode::BackTab, KeyModifiers::NONE)),
            "<S-Tab>"
        );
    }

    #[test]
    fn format_space() {
        assert_eq!(format_key_input(&KeyInput::char(' ')), "<Space>");
    }

    #[test]
    fn format_angle_brackets() {
        assert_eq!(format_key_input(&KeyInput::char('<')), "<lt>");
        assert_eq!(format_key_input(&KeyInput::char('>')), "<gt>");
    }

    #[test]
    fn format_sequence() {
        let binding = KeyBinding(vec![KeyInput::char('g'), KeyInput::char('g')]);
        assert_eq!(format_key_binding(&binding), "gg");
    }

    #[test]
    fn format_space_sequence() {
        let binding = KeyBinding(vec![KeyInput::char(' '), KeyInput::char('f')]);
        assert_eq!(format_key_binding(&binding), "<Space>f");
    }

    #[test]
    fn format_modified_special() {
        let input = ki(KeyCode::Up, KeyModifiers::CONTROL);
        assert_eq!(format_key_input(&input), "<C-Up>");
    }
}
