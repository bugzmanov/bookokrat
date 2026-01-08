use regex::Regex;

/// Sanitize a filename for cross-platform compatibility
/// Removes/replaces characters that are invalid on Windows, macOS, or Linux
pub fn sanitize_filename(name: &str) -> String {
    // Invalid characters for Windows: < > : " / \ | ? *
    // Also remove control characters (0-31)
    let invalid_chars = Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).unwrap();
    let sanitized = invalid_chars.replace_all(name, "_");

    // Trim leading/trailing spaces and dots (problematic on Windows)
    let sanitized = sanitized.trim_matches(|c| c == ' ' || c == '.');

    // Handle reserved Windows names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    let reserved = Regex::new(r"(?i)^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$").unwrap();
    if reserved.is_match(sanitized) {
        return format!("_{}", sanitized);
    }

    // Limit length to 200 characters (leave room for extensions and numbering)
    let sanitized = if sanitized.len() > 200 {
        &sanitized[..200]
    } else {
        &sanitized
    };

    // If empty after sanitization, use a default
    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_filename("Normal Book"), "Normal Book");
        assert_eq!(sanitize_filename("Book: A Tale"), "Book_ A Tale");
        assert_eq!(sanitize_filename("Book/Chapter"), "Book_Chapter");
        assert_eq!(sanitize_filename("Book\\Chapter"), "Book_Chapter");
        assert_eq!(sanitize_filename("Book|Chapter"), "Book_Chapter");
    }

    #[test]
    fn test_sanitize_special_chars() {
        assert_eq!(sanitize_filename("Book<>Test"), "Book__Test");
        assert_eq!(sanitize_filename("Book?*Test"), "Book__Test");
        assert_eq!(sanitize_filename("Book\"Test"), "Book_Test");
    }

    #[test]
    fn test_sanitize_reserved() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("con"), "_con");
        assert_eq!(sanitize_filename("COM1"), "_COM1");
        assert_eq!(sanitize_filename("LPT9"), "_LPT9");
        assert_eq!(sanitize_filename("AUX"), "_AUX");
        assert_eq!(sanitize_filename("PRN"), "_PRN");
        assert_eq!(sanitize_filename("NUL"), "_NUL");
    }

    #[test]
    fn test_sanitize_empty() {
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("..."), "untitled");
        assert_eq!(sanitize_filename("   "), "untitled");
        assert_eq!(sanitize_filename(" . "), "untitled");
    }

    #[test]
    fn test_sanitize_trim() {
        assert_eq!(sanitize_filename("  Book  "), "Book");
        assert_eq!(sanitize_filename("..Book.."), "Book");
        assert_eq!(sanitize_filename(" . Book . "), "Book");
    }

    #[test]
    fn test_sanitize_long_name() {
        let long_name = "a".repeat(250);
        let result = sanitize_filename(&long_name);
        assert_eq!(result.len(), 200);
    }

    #[test]
    fn test_sanitize_unicode() {
        assert_eq!(sanitize_filename("Book 📖 Test"), "Book 📖 Test");
        assert_eq!(sanitize_filename("日本語"), "日本語");
    }

    #[test]
    fn test_sanitize_control_chars() {
        assert_eq!(sanitize_filename("Book\x00Test"), "Book_Test");
        assert_eq!(sanitize_filename("Book\x1FTest"), "Book_Test");
    }
}
