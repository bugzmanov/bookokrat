//! Unified terminal input handling with Kitty graphics protocol demultiplexing.
//!
//! This module provides a way to read both keyboard/mouse events AND Kitty graphics
//! protocol responses from stdin without them interfering with each other.
//!
//! The problem: stdin contains mixed data - keyboard input AND Kitty APC responses.
//! Crossterm's event reading doesn't understand Kitty responses and would corrupt them.
//!
//! The solution: Read raw bytes from stdin, demultiplex them:
//! - Kitty responses (starting with `\x1b_G`) go to a separate queue
//! - Everything else gets parsed as keyboard/mouse events

use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

/// Kitty graphics protocol response.
#[derive(Debug, Clone)]
pub struct KittyResponse {
    pub image_id: Option<u32>,
    pub message: String,
}

impl KittyResponse {
    /// Check if this response indicates an evicted image (ENOENT error).
    pub fn is_evicted(&self) -> bool {
        self.message.contains("ENOENT")
            || self.message.contains("No such")
            || self.message.contains("not found")
    }

    /// Check if this response is OK.
    pub fn is_ok(&self) -> bool {
        self.message == "OK"
    }

    /// Check if this response indicates an error.
    pub fn is_error(&self) -> bool {
        !self.is_ok()
    }

    /// Parse a Kitty response from raw bytes.
    fn parse(data: &[u8]) -> Option<Self> {
        // Format: \x1b_Gi=<id>[,p=<pid>];<message>\x1b\\
        if data.len() < 6 {
            return None;
        }

        // Find the start of the response
        let start = data.windows(3).position(|w| w == b"\x1b_G")?;
        let data = &data[start + 3..];

        // Find the semicolon separator
        let semi_pos = data.iter().position(|&b| b == b';')?;
        let params = &data[..semi_pos];
        let rest = &data[semi_pos + 1..];

        // Find the end of message (before \x1b\\)
        let msg_end = rest.windows(2).position(|w| w == b"\x1b\\")?;
        let message = String::from_utf8_lossy(&rest[..msg_end]).to_string();

        // Parse image_id from parameters
        let params_str = String::from_utf8_lossy(params);
        let mut image_id = None;
        for part in params_str.split(',') {
            if let Some(val) = part.strip_prefix("i=") {
                image_id = val.parse().ok();
            }
        }

        Some(Self { image_id, message })
    }
}

/// Unified terminal input buffer that demultiplexes Kitty responses from keyboard events.
pub struct TerminalInput {
    /// Raw bytes buffer from stdin
    buffer: VecDeque<u8>,
    /// Parsed events ready to be returned
    event_queue: VecDeque<Event>,
    /// Kitty responses ready to be returned
    kitty_queue: VecDeque<KittyResponse>,
    /// Currently held mouse button (for distinguishing drag from move)
    mouse_button_held: Option<MouseButton>,
    /// Whether we've already tried reading more data after seeing incomplete sequence
    read_attempted_for_incomplete: bool,
}

impl TerminalInput {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(1024),
            event_queue: VecDeque::new(),
            kitty_queue: VecDeque::new(),
            mouse_button_held: None,
            read_attempted_for_incomplete: false,
        }
    }

    /// Poll for available input with timeout.
    /// Returns true if there are events available.
    pub fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        // First check if we already have events queued
        if !self.event_queue.is_empty() {
            return Ok(true);
        }

        let buffer_len_before = self.buffer.len();

        // Read from stdin with timeout
        self.read_stdin(timeout)?;

        // Track whether we got new data - used for lone ESC detection
        let got_new_data = self.buffer.len() > buffer_len_before;
        self.read_attempted_for_incomplete = !got_new_data && !self.buffer.is_empty();

        // Process buffer to extract events and Kitty responses
        self.process_buffer();

        Ok(!self.event_queue.is_empty())
    }

    /// Read the next keyboard/mouse event.
    pub fn read_event(&mut self) -> io::Result<Event> {
        // If no events queued, try to read more
        if self.event_queue.is_empty() {
            let buffer_len_before = self.buffer.len();
            self.read_stdin(Duration::from_millis(100))?;
            let got_new_data = self.buffer.len() > buffer_len_before;
            self.read_attempted_for_incomplete = !got_new_data && !self.buffer.is_empty();
            self.process_buffer();
        }

        self.event_queue
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "No events available"))
    }

    /// Get any pending Kitty responses.
    pub fn take_kitty_responses(&mut self) -> Vec<KittyResponse> {
        self.kitty_queue.drain(..).collect()
    }

    /// Check if there are pending Kitty responses.
    pub fn has_kitty_responses(&self) -> bool {
        !self.kitty_queue.is_empty()
    }

    /// Read available bytes from stdin with timeout.
    fn read_stdin(&mut self, timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(now);

        // Check if data is available using poll
        if !Self::poll_stdin(remaining)? {
            return Ok(());
        }

        // Read available bytes
        let mut buf = [0u8; 256];
        match Self::read_stdin_nonblocking(&mut buf) {
            Ok(0) => Ok(()),
            Ok(n) => {
                self.buffer.extend(&buf[..n]);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Poll stdin for available data using libc.
    #[cfg(unix)]
    fn poll_stdin(timeout: Duration) -> io::Result<bool> {
        let timeout_ms =
            i32::try_from(timeout.as_millis().min(i32::MAX as u128)).unwrap_or(i32::MAX);

        let mut pfd = libc::pollfd {
            fd: 0, // stdin
            events: libc::POLLIN,
            revents: 0,
        };

        let result = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };

        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(result > 0 && (pfd.revents & libc::POLLIN) != 0)
    }

    #[cfg(not(unix))]
    fn poll_stdin(timeout: Duration) -> io::Result<bool> {
        // On non-Unix, fall back to crossterm's poll
        crossterm::event::poll(timeout).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    /// Read from stdin without blocking.
    #[cfg(unix)]
    fn read_stdin_nonblocking(buf: &mut [u8]) -> io::Result<usize> {
        let result = unsafe { libc::read(0, buf.as_mut_ptr() as *mut _, buf.len()) };

        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(result as usize)
    }

    #[cfg(not(unix))]
    fn read_stdin_nonblocking(buf: &mut [u8]) -> io::Result<usize> {
        std::io::stdin().read(buf)
    }

    /// Process the buffer to extract events and Kitty responses.
    fn process_buffer(&mut self) {
        while !self.buffer.is_empty() {
            // Check for Kitty response (starts with \x1b_G)
            if self.buffer.len() >= 3
                && self.buffer[0] == 0x1b
                && self.buffer[1] == b'_'
                && self.buffer[2] == b'G'
            {
                if let Some(response) = self.try_extract_kitty_response() {
                    if let Some(parsed) = KittyResponse::parse(&response) {
                        log::trace!("Received Kitty response: {parsed:?}");
                        self.kitty_queue.push_back(parsed);
                    }
                    continue;
                } else {
                    // Incomplete Kitty response, wait for more data
                    break;
                }
            }

            // Try to parse as keyboard/mouse event
            if let Some(event) = self.try_parse_event() {
                self.event_queue.push_back(event);
                self.read_attempted_for_incomplete = false; // Reset on successful parse
            } else {
                // Can't parse - might be incomplete, wait for more data
                break;
            }
        }
    }

    /// Try to extract a complete Kitty response from the buffer.
    /// Returns the raw bytes if successful, or None if incomplete.
    fn try_extract_kitty_response(&mut self) -> Option<Vec<u8>> {
        // Look for the terminator \x1b\\
        let buf: Vec<u8> = self.buffer.iter().copied().collect();

        for i in 0..buf.len().saturating_sub(1) {
            if buf[i] == 0x1b && buf[i + 1] == b'\\' {
                // Found terminator - extract the response
                let response: Vec<u8> = self.buffer.drain(..=i + 1).collect();
                return Some(response);
            }
        }

        None
    }

    /// Try to parse an event from the buffer.
    /// Returns the event and consumes the bytes if successful.
    fn try_parse_event(&mut self) -> Option<Event> {
        if self.buffer.is_empty() {
            return None;
        }

        let first = self.buffer[0];

        // Handle escape sequences
        if first == 0x1b {
            return self.try_parse_escape_sequence();
        }

        // Handle regular characters
        self.try_parse_character()
    }

    /// Try to parse an escape sequence.
    fn try_parse_escape_sequence(&mut self) -> Option<Event> {
        if self.buffer.len() < 2 {
            // Only ESC in buffer - might be a lone ESC or start of escape sequence
            // If we tried reading more and got nothing, it's a lone ESC
            if self.buffer.len() == 1
                && self.buffer[0] == 0x1b
                && self.read_attempted_for_incomplete
            {
                self.buffer.pop_front();
                self.read_attempted_for_incomplete = false;
                return Some(Event::Key(KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: KeyModifiers::empty(),
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                }));
            }
            return None;
        }

        let second = self.buffer[1];

        match second {
            // CSI sequence: \x1b[...
            b'[' => self.try_parse_csi_sequence(),
            // SS3 sequence: \x1bO...
            b'O' => self.try_parse_ss3_sequence(),
            // Alt+key: \x1b<char>
            _ if second.is_ascii() => {
                self.buffer.drain(..2);
                Some(Event::Key(KeyEvent {
                    code: KeyCode::Char(second as char),
                    modifiers: KeyModifiers::ALT,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                }))
            }
            _ => {
                // Unknown escape sequence - consume the escape
                self.buffer.pop_front();
                Some(Event::Key(KeyEvent {
                    code: KeyCode::Esc,
                    modifiers: KeyModifiers::empty(),
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                }))
            }
        }
    }

    /// Try to parse a CSI sequence (\x1b[...).
    fn try_parse_csi_sequence(&mut self) -> Option<Event> {
        if self.buffer.len() < 3 {
            return None;
        }

        // Check for mouse event (SGR format): \x1b[<...M or \x1b[<...m
        if self.buffer[2] == b'<' {
            return self.try_parse_mouse_sgr();
        }

        // Find the final byte (letter or ~)
        let mut end_idx = 2;
        while end_idx < self.buffer.len() {
            let b = self.buffer[end_idx];
            if b.is_ascii_alphabetic() || b == b'~' {
                break;
            }
            end_idx += 1;
        }

        if end_idx >= self.buffer.len() {
            return None; // Incomplete
        }

        let final_byte = self.buffer[end_idx];
        let params: Vec<u8> = self
            .buffer
            .iter()
            .skip(2)
            .take(end_idx - 2)
            .copied()
            .collect();
        let params_str = String::from_utf8_lossy(&params);

        // Consume the sequence
        self.buffer.drain(..=end_idx);

        // Parse based on final byte
        let event = match final_byte {
            b'A' => key_event(KeyCode::Up, parse_modifiers(&params_str)),
            b'B' => key_event(KeyCode::Down, parse_modifiers(&params_str)),
            b'C' => key_event(KeyCode::Right, parse_modifiers(&params_str)),
            b'D' => key_event(KeyCode::Left, parse_modifiers(&params_str)),
            b'H' => key_event(KeyCode::Home, parse_modifiers(&params_str)),
            b'F' => key_event(KeyCode::End, parse_modifiers(&params_str)),
            b'Z' => key_event(KeyCode::BackTab, KeyModifiers::SHIFT),
            b'~' => self.parse_tilde_sequence(&params_str),
            b'u' => self.parse_kitty_keyboard(&params_str),
            _ => return None,
        };

        Some(event)
    }

    /// Parse tilde-terminated CSI sequences (function keys, etc.)
    fn parse_tilde_sequence(&self, params: &str) -> Event {
        let parts: Vec<&str> = params.split(';').collect();
        let key_num: u8 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let modifiers = parts
            .get(1)
            .map(|s| parse_modifier_num(s))
            .unwrap_or(KeyModifiers::empty());

        let code = match key_num {
            1 => KeyCode::Home,
            2 => KeyCode::Insert,
            3 => KeyCode::Delete,
            4 => KeyCode::End,
            5 => KeyCode::PageUp,
            6 => KeyCode::PageDown,
            11 => KeyCode::F(1),
            12 => KeyCode::F(2),
            13 => KeyCode::F(3),
            14 => KeyCode::F(4),
            15 => KeyCode::F(5),
            17 => KeyCode::F(6),
            18 => KeyCode::F(7),
            19 => KeyCode::F(8),
            20 => KeyCode::F(9),
            21 => KeyCode::F(10),
            23 => KeyCode::F(11),
            24 => KeyCode::F(12),
            _ => KeyCode::Null,
        };

        key_event(code, modifiers)
    }

    /// Parse Kitty keyboard protocol sequences.
    fn parse_kitty_keyboard(&self, params: &str) -> Event {
        let parts: Vec<&str> = params.split(';').collect();
        let key_num: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let modifiers = parts
            .get(1)
            .map(|s| parse_modifier_num(s))
            .unwrap_or(KeyModifiers::empty());

        let code = if key_num < 128 {
            KeyCode::Char(char::from_u32(key_num).unwrap_or('\0'))
        } else {
            KeyCode::Null
        };

        key_event(code, modifiers)
    }

    /// Try to parse SS3 sequence (\x1bO...).
    fn try_parse_ss3_sequence(&mut self) -> Option<Event> {
        if self.buffer.len() < 3 {
            return None;
        }

        let third = self.buffer[2];
        self.buffer.drain(..3);

        let code = match third {
            b'A' => KeyCode::Up,
            b'B' => KeyCode::Down,
            b'C' => KeyCode::Right,
            b'D' => KeyCode::Left,
            b'H' => KeyCode::Home,
            b'F' => KeyCode::End,
            b'P' => KeyCode::F(1),
            b'Q' => KeyCode::F(2),
            b'R' => KeyCode::F(3),
            b'S' => KeyCode::F(4),
            _ => return None,
        };

        Some(key_event(code, KeyModifiers::empty()))
    }

    /// Try to parse SGR mouse format: \x1b[<Cb;Cx;CyM or \x1b[<Cb;Cx;Cym
    fn try_parse_mouse_sgr(&mut self) -> Option<Event> {
        // Find the terminating M or m
        let mut end_idx = 3;
        while end_idx < self.buffer.len() {
            let b = self.buffer[end_idx];
            if b == b'M' || b == b'm' {
                break;
            }
            end_idx += 1;
        }

        if end_idx >= self.buffer.len() {
            return None; // Incomplete
        }

        let is_release = self.buffer[end_idx] == b'm';
        let params: Vec<u8> = self
            .buffer
            .iter()
            .skip(3)
            .take(end_idx - 3)
            .copied()
            .collect();
        let params_str = String::from_utf8_lossy(&params);

        self.buffer.drain(..=end_idx);

        let parts: Vec<&str> = params_str.split(';').collect();
        if parts.len() < 3 {
            return None;
        }

        let cb: u16 = parts[0].parse().ok()?;
        let cx: u16 = parts[1].parse().ok()?;
        let cy: u16 = parts[2].parse().ok()?;

        let column = cx.saturating_sub(1);
        let row = cy.saturating_sub(1);

        let modifiers = KeyModifiers::from_bits_truncate(((cb >> 2) & 0x07) as u8);

        let button = match cb & 0x43 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::Left,
        };

        let kind = if cb & 64 != 0 {
            // Scroll event - bits 0-1 encode the scroll button:
            // 0 = button 4 (scroll up), 1 = button 5 (scroll down)
            // 2 = button 6 (scroll left), 3 = button 7 (scroll right)
            match cb & 3 {
                0 => MouseEventKind::ScrollUp,
                1 => MouseEventKind::ScrollDown,
                2 => MouseEventKind::ScrollLeft,
                3 => MouseEventKind::ScrollRight,
                _ => unreachable!(),
            }
        } else if cb & 32 != 0 {
            // Motion event - use tracked button state since terminal may send
            // button code 3 for all motion events regardless of button held
            if let Some(held_button) = self.mouse_button_held {
                MouseEventKind::Drag(held_button)
            } else {
                MouseEventKind::Moved
            }
        } else if is_release {
            self.mouse_button_held = None;
            MouseEventKind::Up(button)
        } else {
            self.mouse_button_held = Some(button);
            MouseEventKind::Down(button)
        };

        Some(Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers,
        }))
    }

    /// Try to parse a regular character (including UTF-8).
    fn try_parse_character(&mut self) -> Option<Event> {
        let first = self.buffer[0];

        // Control characters
        if first < 32 || first == 127 {
            self.buffer.pop_front();
            let (code, mods) = match first {
                0 => (KeyCode::Char(' '), KeyModifiers::CONTROL),
                8 | 127 => (KeyCode::Backspace, KeyModifiers::empty()),
                9 => (KeyCode::Tab, KeyModifiers::empty()),
                10 | 13 => (KeyCode::Enter, KeyModifiers::empty()),
                27 => (KeyCode::Esc, KeyModifiers::empty()),
                1..=26 => (
                    KeyCode::Char((b'a' + first - 1) as char),
                    KeyModifiers::CONTROL,
                ),
                _ => (KeyCode::Null, KeyModifiers::empty()),
            };
            return Some(key_event(code, mods));
        }

        // ASCII character
        if first < 128 {
            self.buffer.pop_front();
            return Some(key_event(
                KeyCode::Char(first as char),
                KeyModifiers::empty(),
            ));
        }

        // UTF-8 multi-byte character
        let len = if first & 0xE0 == 0xC0 {
            2
        } else if first & 0xF0 == 0xE0 {
            3
        } else if first & 0xF8 == 0xF0 {
            4
        } else {
            // Invalid UTF-8 start byte
            self.buffer.pop_front();
            return None;
        };

        if self.buffer.len() < len {
            return None; // Incomplete UTF-8
        }

        let bytes: Vec<u8> = self.buffer.drain(..len).collect();
        if let Ok(s) = std::str::from_utf8(&bytes) {
            if let Some(c) = s.chars().next() {
                return Some(key_event(KeyCode::Char(c), KeyModifiers::empty()));
            }
        }

        None
    }
}

impl Default for TerminalInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a key event.
fn key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    })
}

/// Parse modifiers from CSI parameter string (e.g., "1;5" means Ctrl).
fn parse_modifiers(params: &str) -> KeyModifiers {
    if let Some(pos) = params.find(';') {
        parse_modifier_num(&params[pos + 1..])
    } else {
        KeyModifiers::empty()
    }
}

/// Parse modifier number (1=none, 2=shift, 3=alt, 5=ctrl, etc.)
fn parse_modifier_num(s: &str) -> KeyModifiers {
    let n: u8 = s.parse().unwrap_or(1);
    let n = n.saturating_sub(1);

    let mut mods = KeyModifiers::empty();
    if n & 1 != 0 {
        mods |= KeyModifiers::SHIFT;
    }
    if n & 2 != 0 {
        mods |= KeyModifiers::ALT;
    }
    if n & 4 != 0 {
        mods |= KeyModifiers::CONTROL;
    }
    mods
}

/// Event source that uses the unified terminal input.
pub struct UnifiedEventSource {
    input: TerminalInput,
}

impl UnifiedEventSource {
    pub fn new() -> Self {
        Self {
            input: TerminalInput::new(),
        }
    }

    /// Get any pending Kitty responses.
    pub fn take_kitty_responses(&mut self) -> Vec<KittyResponse> {
        self.input.take_kitty_responses()
    }

    /// Check if there are pending Kitty responses.
    pub fn has_kitty_responses(&self) -> bool {
        self.input.has_kitty_responses()
    }
}

impl Default for UnifiedEventSource {
    fn default() -> Self {
        Self::new()
    }
}

impl super::event_source::EventSource for UnifiedEventSource {
    fn poll(&mut self, timeout: Duration) -> anyhow::Result<bool> {
        Ok(self.input.poll(timeout)?)
    }

    fn read(&mut self) -> anyhow::Result<Event> {
        Ok(self.input.read_event()?)
    }

    fn take_kitty_responses(&mut self) -> Vec<KittyResponse> {
        self.input.take_kitty_responses()
    }

    fn has_kitty_responses(&self) -> bool {
        self.input.has_kitty_responses()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kitty_response_parse() {
        let data = b"\x1b_Gi=42;OK\x1b\\";
        let response = KittyResponse::parse(data).unwrap();
        assert_eq!(response.image_id, Some(42));
        assert_eq!(response.message, "OK");
        assert!(!response.is_evicted());
    }

    #[test]
    fn test_kitty_response_enoent() {
        let data = b"\x1b_Gi=123;ENOENT:Image not found\x1b\\";
        let response = KittyResponse::parse(data).unwrap();
        assert_eq!(response.image_id, Some(123));
        assert!(response.is_evicted());
    }

    #[test]
    fn test_modifier_parsing() {
        assert_eq!(parse_modifier_num("1"), KeyModifiers::empty());
        assert_eq!(parse_modifier_num("2"), KeyModifiers::SHIFT);
        assert_eq!(parse_modifier_num("3"), KeyModifiers::ALT);
        assert_eq!(parse_modifier_num("5"), KeyModifiers::CONTROL);
        assert_eq!(
            parse_modifier_num("6"),
            KeyModifiers::SHIFT | KeyModifiers::CONTROL
        );
    }
}
