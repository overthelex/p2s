use thiserror::Error;

const MAX_LABEL_LEN: usize = 64;
const DEFAULT_MAX_DESCRIPTION_LEN: usize = 512;

#[derive(Debug, Error)]
pub enum HardenError {
    #[error("label exceeds {MAX_LABEL_LEN} characters (got {0})")]
    LabelTooLong(usize),

    #[error("label contains newlines")]
    LabelContainsNewlines,

    #[error("label is empty after normalization")]
    LabelEmpty,
}

/// Harden a card label for safe storage and display.
/// Rejects labels that are too long or contain newlines.
/// Strips control characters and trims whitespace.
pub fn harden_label(s: &str) -> Result<String, HardenError> {
    if s.contains('\n') || s.contains('\r') {
        return Err(HardenError::LabelContainsNewlines);
    }

    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect();

    let trimmed = cleaned.trim().to_string();

    if trimmed.is_empty() {
        return Err(HardenError::LabelEmpty);
    }

    if trimmed.chars().count() > MAX_LABEL_LEN {
        return Err(HardenError::LabelTooLong(trimmed.chars().count()));
    }

    Ok(trimmed)
}

/// Harden a free-text description field.
/// Strips control characters and truncates to the ceiling.
pub fn harden_description(s: &str, max_len: usize) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control() || matches!(*c, ' ' | '\n' | '\t'))
        .collect();

    let trimmed = cleaned.trim();

    if trimmed.chars().count() <= max_len {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_len).collect()
    }
}

/// Harden a tool description using the default ceiling.
pub fn harden_tool_description(s: &str) -> String {
    harden_description(s, DEFAULT_MAX_DESCRIPTION_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_label_passes() {
        assert_eq!(harden_label("My API Service").unwrap(), "My API Service");
    }

    #[test]
    fn label_trimmed() {
        assert_eq!(harden_label("  padded  ").unwrap(), "padded");
    }

    #[test]
    fn label_control_chars_stripped() {
        assert_eq!(harden_label("hello\x07world").unwrap(), "helloworld");
    }

    #[test]
    fn label_too_long_rejected() {
        let long = "a".repeat(65);
        assert!(matches!(harden_label(&long), Err(HardenError::LabelTooLong(65))));
    }

    #[test]
    fn label_exactly_max_passes() {
        let exact = "a".repeat(64);
        assert!(harden_label(&exact).is_ok());
    }

    #[test]
    fn label_newline_rejected() {
        assert!(matches!(
            harden_label("line1\nline2"),
            Err(HardenError::LabelContainsNewlines)
        ));
    }

    #[test]
    fn label_carriage_return_rejected() {
        assert!(matches!(
            harden_label("line1\rline2"),
            Err(HardenError::LabelContainsNewlines)
        ));
    }

    #[test]
    fn empty_label_rejected() {
        assert!(matches!(harden_label(""), Err(HardenError::LabelEmpty)));
    }

    #[test]
    fn only_control_chars_rejected() {
        assert!(matches!(harden_label("\x01\x02\x03"), Err(HardenError::LabelEmpty)));
    }

    #[test]
    fn description_truncated() {
        let long = "a".repeat(600);
        let result = harden_description(&long, 512);
        assert_eq!(result.len(), 512);
    }

    #[test]
    fn description_preserves_newlines() {
        let desc = "Line 1\nLine 2\nLine 3";
        assert_eq!(harden_description(desc, 512), desc);
    }

    #[test]
    fn description_strips_control_chars() {
        let desc = "hello\x00\x07world";
        assert_eq!(harden_description(desc, 512), "helloworld");
    }

    #[test]
    fn tool_description_uses_default_ceiling() {
        let long = "b".repeat(1000);
        let result = harden_tool_description(&long);
        assert_eq!(result.len(), DEFAULT_MAX_DESCRIPTION_LEN);
    }
}
