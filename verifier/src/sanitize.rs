/// Pre-sanitization filter for untrusted web content (§3.6 requirement 3).
///
/// Strips obvious prompt injection carriers BEFORE the LLM reads the content.
/// This is the first line of defense, not the only one.
pub fn sanitize_content(raw_html: &str) -> String {
    let mut result = raw_html.to_string();

    result = strip_html_comments(&result);
    result = strip_style_hidden_elements(&result);
    result = strip_script_tags(&result);
    result = strip_suspicious_meta_tags(&result);
    result = strip_invisible_text(&result);
    result = collapse_whitespace(&result);
    result = truncate_if_excessive(&result, 50_000);

    result
}

fn strip_html_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;

    while let Some(start) = remaining.find("<!--") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find("-->") {
            remaining = &remaining[start + end + 3..];
        } else {
            remaining = "";
            break;
        }
    }
    result.push_str(remaining);
    result
}

fn strip_script_tags(s: &str) -> String {
    strip_tag_pair(s, "<script", "</script>")
}

fn strip_style_hidden_elements(s: &str) -> String {
    let mut result = s.to_string();
    // Remove style tags with display:none or visibility:hidden
    result = strip_tag_pair(&result, "<style", "</style>");
    // Remove elements with inline style hiding
    for pattern in &["display:none", "display: none", "visibility:hidden", "visibility: hidden", "font-size:0", "font-size: 0", "opacity:0", "opacity: 0"] {
        result = strip_elements_with_style(&result, pattern);
    }
    result
}

fn strip_tag_pair(s: &str, open: &str, close: &str) -> String {
    let lower = s.to_lowercase();
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    let mut lower_remaining = lower.as_str();

    while let Some(start) = lower_remaining.find(open) {
        result.push_str(&remaining[..start]);
        if let Some(end) = lower_remaining[start..].find(close) {
            let skip = end + close.len();
            remaining = &remaining[start + skip..];
            lower_remaining = &lower_remaining[start + skip..];
        } else {
            remaining = "";
            lower_remaining = "";
            break;
        }
    }
    result.push_str(remaining);
    result
}

fn strip_elements_with_style(s: &str, pattern: &str) -> String {
    let lower = s.to_lowercase();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < s.len() {
        if lower[i..].starts_with("<") {
            if let Some(tag_end) = s[i..].find('>') {
                let tag = &lower[i..i + tag_end + 1];
                if tag.contains("style=") && tag.contains(pattern) {
                    // Find the tag name to locate its closing tag
                    let tag_name_start = 1; // skip '<'
                    let tag_name_end = lower[i + tag_name_start..]
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(0);
                    let tag_name = &lower[i + tag_name_start..i + tag_name_start + tag_name_end];
                    let close_tag = format!("</{tag_name}>");

                    // Skip to after the closing tag
                    if let Some(close_pos) = lower[i + tag_end + 1..].find(&close_tag) {
                        i = i + tag_end + 1 + close_pos + close_tag.len();
                    } else {
                        // No closing tag — skip just the opening tag
                        i += tag_end + 1;
                    }
                    continue;
                }
            }
        }
        if let Some(c) = s[i..].chars().next() {
            result.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }
    result
}

fn strip_suspicious_meta_tags(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    let mut lower_remaining = lower.as_str();

    while let Some(start) = lower_remaining.find("<meta") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find('>') {
            let meta_tag = &lower_remaining[start..start + end + 1];
            // Only strip meta tags with suspiciously long content attributes
            if meta_tag.contains("content=") && meta_tag.len() > 200 {
                // Skip it
            } else {
                result.push_str(&remaining[start..start + end + 1]);
            }
            remaining = &remaining[start + end + 1..];
            lower_remaining = &lower_remaining[start + end + 1..];
        } else {
            remaining = &remaining[start..];
            lower_remaining = &lower_remaining[start..];
            break;
        }
    }
    result.push_str(remaining);
    result
}

fn strip_invisible_text(s: &str) -> String {
    s.chars().filter(|c| {
        // Keep normal text, common whitespace, and printable chars
        // Strip zero-width chars and other invisible Unicode
        !matches!(*c,
            '\u{200B}' | // zero-width space
            '\u{200C}' | // zero-width non-joiner
            '\u{200D}' | // zero-width joiner
            '\u{200E}' | // left-to-right mark
            '\u{200F}' | // right-to-left mark
            '\u{2060}' | // word joiner
            '\u{2061}' | // function application
            '\u{2062}' | // invisible times
            '\u{2063}' | // invisible separator
            '\u{2064}' | // invisible plus
            '\u{FEFF}'   // byte order mark / zero-width no-break space
        )
    }).collect()
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_was_space = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(c);
            prev_was_space = false;
        }
    }
    result
}

fn truncate_if_excessive(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect::<String>() + "\n[CONTENT TRUNCATED]"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_comments() {
        let input = "Hello <!-- injection attempt --> World";
        assert_eq!(sanitize_content(input), "Hello World");
    }

    #[test]
    fn strips_script_tags() {
        let input = "Before <script>alert('xss')</script> After";
        assert_eq!(sanitize_content(input), "Before After");
    }

    #[test]
    fn strips_style_tags() {
        let input = "Before <style>.hidden{display:none}</style> After";
        assert_eq!(sanitize_content(input), "Before After");
    }

    #[test]
    fn strips_zero_width_chars() {
        let input = "Hello\u{200B}World\u{FEFF}!";
        assert_eq!(sanitize_content(input), "HelloWorld!");
    }

    #[test]
    fn collapses_whitespace() {
        let input = "Hello    World\n\n\nFoo";
        assert_eq!(sanitize_content(input), "Hello World Foo");
    }

    #[test]
    fn truncates_excessive_content() {
        let input = "A".repeat(60_000);
        let result = sanitize_content(&input);
        assert!(result.len() < 60_000);
        assert!(result.ends_with("[CONTENT TRUNCATED]"));
    }

    #[test]
    fn preserves_normal_content() {
        let input = "This is a normal service description with API endpoints.";
        assert_eq!(sanitize_content(input), input);
    }

    #[test]
    fn strips_hidden_elements() {
        let input = r#"Visible <div style="display:none">HIDDEN INJECTION</div> Also visible"#;
        let result = sanitize_content(input);
        assert!(!result.contains("HIDDEN INJECTION"));
    }

    #[test]
    fn adversarial_nested_comments() {
        let input = "A <!-- outer <!-- inner --> B --> C";
        let result = sanitize_content(input);
        // Should strip from first <!-- to first -->
        assert!(!result.contains("inner"));
    }
}
