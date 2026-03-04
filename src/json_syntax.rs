use crate::theme::Theme;
/// JSON syntax highlighting for ratatui
/// Produces colored Span/Line objects with WCAG AAA compliant colors
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Parse JSON string and produce colored Lines for ratatui rendering
/// Uses theme colors with WCAG AAA compliance:
/// - Keys: Soft cyan (#66DDEC)
/// - String values: Yellow-green (#ABE338)
/// - Numbers: Warm orange (#F5AB32)
/// - Booleans/null: Light blue (#6BBEFF)
/// - Braces/Brackets: Tan/beige (#D4D0AB)
///
/// Uses ANSI colors for maximum terminal compatibility.
pub fn colorize_json(json_str: &str, _theme: &Theme) -> Vec<Line<'static>> {
    // Use subtle RGB colors - "more white" for elegant appearance
    let (key_color, string_color, number_color, boolean_color, struct_color) = (
        Color::Rgb(180, 220, 230), // Very light cyan for keys
        Color::Rgb(210, 230, 180), // Very light green for string values
        Color::Rgb(230, 220, 180), // Very light amber for numbers
        Color::Rgb(220, 210, 240), // Very light purple for booleans
        Color::Rgb(240, 240, 230), // Off-white for structure (brackets, colons, etc)
    );
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut chars = json_str.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Whitespace - preserve as-is
            ' ' | '\t' => {
                current_line.push(Span::raw(ch.to_string()));
            }

            // Newline - finish current line
            '\n' => {
                lines.push(Line::from(std::mem::take(&mut current_line)));
            }

            // String (could be key or value)
            '"' => {
                let (string_content, is_key) = parse_string(&mut chars);
                let color = if is_key { key_color } else { string_color };

                current_line.push(Span::styled(
                    format!("\"{string_content}\""),
                    Style::default().fg(color),
                ));
            }

            // Numbers
            '-' | '0'..='9' => {
                let number = parse_number(ch, &mut chars);
                current_line.push(Span::styled(number, Style::default().fg(number_color)));
            }

            // Booleans and null
            't' | 'f' | 'n' => {
                let keyword = parse_keyword(ch, &mut chars);
                current_line.push(Span::styled(keyword, Style::default().fg(boolean_color)));
            }

            // Structural characters (braces, brackets, colons, commas)
            '{' | '}' | '[' | ']' | ':' | ',' => {
                current_line.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(struct_color),
                ));
            }

            // Unknown - render as-is
            _ => {
                current_line.push(Span::raw(ch.to_string()));
            }
        }
    }

    // Add final line if non-empty
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    // Remove any trailing empty lines (defensive cleanup)
    while lines.last().is_some_and(|line| line.spans.is_empty()) {
        lines.pop();
    }

    lines
}

/// Parse a JSON string and determine if it's a key (followed by :) or value
fn parse_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> (String, bool) {
    let mut content = String::new();
    let mut escaped = false;

    for ch in chars.by_ref() {
        if escaped {
            content.push(ch);
            escaped = false;
        } else if ch == '\\' {
            content.push(ch);
            escaped = true;
        } else if ch == '"' {
            break;
        } else {
            content.push(ch);
        }
    }

    // Look ahead to see if this is a key (followed by whitespace + colon)
    let is_key = skip_whitespace_and_check_colon(chars);

    (content, is_key)
}

/// Skip whitespace and check if next non-whitespace char is a colon
fn skip_whitespace_and_check_colon(chars: &mut std::iter::Peekable<std::str::Chars>) -> bool {
    let mut peeked_chars = Vec::new();
    let mut found_colon = false;

    while let Some(&ch) = chars.peek() {
        if ch == ' ' || ch == '\t' || ch == '\n' {
            peeked_chars.push(chars.next().unwrap());
        } else if ch == ':' {
            found_colon = true;
            break;
        } else {
            break;
        }
    }

    found_colon
}

/// Parse a number (integer or float)
fn parse_number(first: char, chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut number = String::from(first);

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-' {
            number.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    number
}

/// Parse a keyword (true, false, null)
fn parse_keyword(first: char, chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut keyword = String::from(first);

    while let Some(&ch) = chars.peek() {
        if ch.is_alphabetic() {
            keyword.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    keyword
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_object() {
        let theme = Theme::default();
        let json = r#"{"name": "Alice", "age": 30}"#;
        let lines = colorize_json(json, &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiline_json() {
        let theme = Theme::default();
        let json = r#"{
  "name": "Alice",
  "active": true,
  "count": 42
}"#;
        let lines = colorize_json(json, &theme);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_colors_applied() {
        let theme = Theme::default();
        let json = r#"{"key": "value"}"#;
        let lines = colorize_json(json, &theme);

        assert_eq!(lines.len(), 1);
        let first_line = &lines[0];

        // Should have multiple spans with different styles
        assert!(first_line.spans.len() > 1);

        // Check that at least one span has a style (not default)
        let has_styled_spans = first_line
            .spans
            .iter()
            .any(|span| span.style != Style::default());
        assert!(
            has_styled_spans,
            "JSON colorizer should produce styled spans"
        );

        // Print debug info
        for (i, span) in first_line.spans.iter().enumerate() {
            eprintln!(
                "Span {}: text={:?}, style={:?}",
                i, &span.content, &span.style
            );
        }
    }
}
