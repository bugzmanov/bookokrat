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
    pub expires_at: Instant,
}

impl Notification {
    pub fn new(message: impl Into<String>, level: NotificationLevel, duration: Duration) -> Self {
        let now = Instant::now();
        Self {
            message: message.into(),
            level,
            created_at: now,
            expires_at: now + duration,
        }
    }

    pub fn info(message: impl Into<String>, duration: Duration) -> Self {
        Self::new(message, NotificationLevel::Info, duration)
    }

    pub fn warning(message: impl Into<String>, duration: Duration) -> Self {
        Self::new(message, NotificationLevel::Warning, duration)
    }

    pub fn error(message: impl Into<String>, duration: Duration) -> Self {
        Self::new(message, NotificationLevel::Error, duration)
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn time_remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    #[deprecated(note = "Use time_remaining() instead")]
    pub fn remaining_time(&self) -> Duration {
        self.time_remaining()
    }
}

#[derive(Debug, Default)]
pub struct NotificationManager {
    notifications: Vec<Notification>,
    default_duration: Duration,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self::with_default_duration(Duration::from_secs(5))
    }

    pub fn with_default_duration(default_duration: Duration) -> Self {
        Self {
            notifications: Vec::new(),
            default_duration,
        }
    }

    pub fn notify(&mut self, message: impl Into<String>, level: NotificationLevel) {
        self.notify_for(message, level, self.default_duration);
    }

    pub fn notify_for(
        &mut self,
        message: impl Into<String>,
        level: NotificationLevel,
        duration: Duration,
    ) {
        let notification = Notification::new(message, level, duration);
        self.notifications.insert(0, notification);
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationLevel::Info);
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationLevel::Warning);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationLevel::Error);
    }

    /// Backward compatible: wraps show() for existing code
    pub fn show(&mut self, notification: Notification) {
        self.notifications.insert(0, notification);
    }

    /// Backward compatible alias for info()
    pub fn show_info(&mut self, message: impl Into<String>) {
        self.info(message);
    }

    /// Backward compatible alias for warn()
    pub fn show_warning(&mut self, message: impl Into<String>) {
        self.warn(message);
    }

    /// Backward compatible alias for error()
    pub fn show_error(&mut self, message: impl Into<String>) {
        self.error(message);
    }

    /// Remove expired notifications, returns true if any were removed
    pub fn update(&mut self) -> bool {
        let initial_len = self.notifications.len();
        self.notifications.retain(|n| !n.is_expired());
        self.notifications.len() != initial_len
    }

    /// Get the most recent notification (backward compatible)
    pub fn get_current(&self) -> Option<&Notification> {
        self.notifications.first()
    }

    /// Alias for get_current()
    pub fn current(&self) -> Option<&Notification> {
        self.get_current()
    }

    pub fn all(&self) -> &[Notification] {
        &self.notifications
    }

    pub fn clear(&mut self) {
        self.notifications.clear();
    }

    /// Backward compatible: dismiss current notification
    pub fn dismiss(&mut self) {
        if !self.notifications.is_empty() {
            self.notifications.remove(0);
        }
    }

    pub fn dismiss_current(&mut self) -> bool {
        if self.notifications.is_empty() {
            false
        } else {
            self.notifications.remove(0);
            true
        }
    }

    pub fn has_notification(&self) -> bool {
        !self.notifications.is_empty()
    }

    pub fn has_notifications(&self) -> bool {
        self.has_notification()
    }

    pub fn count(&self) -> usize {
        self.notifications.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn notification_expiration() {
        let notification =
            Notification::new("test", NotificationLevel::Info, Duration::from_millis(50));
        assert!(!notification.is_expired());

        thread::sleep(Duration::from_millis(60));
        assert!(notification.is_expired());
    }

    #[test]
    fn manager_adds_and_retrieves() {
        let mut manager = NotificationManager::new();

        manager.info("First");
        manager.warn("Second");
        manager.error("Third");

        assert_eq!(manager.count(), 3);

        let current = manager.current().unwrap();
        assert_eq!(current.message, "Third");
        assert_eq!(current.level, NotificationLevel::Error);
    }

    #[test]
    fn manager_removes_expired() {
        let mut manager = NotificationManager::with_default_duration(Duration::from_millis(50));

        manager.info("Short-lived");
        assert_eq!(manager.count(), 1);

        thread::sleep(Duration::from_millis(60));
        let changed = manager.update();

        assert!(changed);
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn manager_dismiss_current() {
        let mut manager = NotificationManager::new();

        manager.info("First");
        manager.info("Second");

        assert_eq!(manager.count(), 2);
        assert!(manager.dismiss_current());
        assert_eq!(manager.count(), 1);
        assert_eq!(manager.current().unwrap().message, "First");
    }

    #[test]
    fn backward_compat_methods() {
        let mut manager = NotificationManager::new();

        manager.show_info("info");
        manager.show_warning("warning");
        manager.show_error("error");

        assert!(manager.has_notification());
        assert_eq!(manager.get_current().unwrap().message, "error");

        manager.dismiss();
        assert_eq!(manager.get_current().unwrap().message, "warning");
    }
}
