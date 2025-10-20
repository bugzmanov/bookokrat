use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub created_at: Instant,
    pub timeout: Duration,
}

impl Notification {
    pub fn new(message: impl Into<String>, level: NotificationLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created_at: Instant::now(),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, NotificationLevel::Info)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, NotificationLevel::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, NotificationLevel::Error)
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.timeout
    }

    pub fn remaining_time(&self) -> Duration {
        self.timeout.saturating_sub(self.created_at.elapsed())
    }
}

#[derive(Debug, Default)]
pub struct NotificationManager {
    current: Option<Notification>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn show(&mut self, notification: Notification) {
        self.current = Some(notification);
    }

    pub fn show_info(&mut self, message: impl Into<String>) {
        self.show(Notification::info(message));
    }

    pub fn show_warning(&mut self, message: impl Into<String>) {
        self.show(Notification::warning(message));
    }

    pub fn show_error(&mut self, message: impl Into<String>) {
        self.show(Notification::error(message));
    }

    pub fn dismiss(&mut self) {
        self.current = None;
    }

    pub fn get_current(&self) -> Option<&Notification> {
        self.current.as_ref()
    }

    pub fn update(&mut self) -> bool {
        if let Some(ref notification) = self.current {
            if notification.is_expired() {
                self.current = None;
                return true;
            }
        }
        false
    }

    pub fn has_notification(&self) -> bool {
        self.current.is_some()
    }
}
