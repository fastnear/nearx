//! Structured JSON renderer with syntax highlighting for ratatui
//!
//! This module provides a Value-based approach to JSON rendering that ensures
//! proper formatting with all closing brackets on their own lines.

use crate::theme::Theme;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

/// Color palette for JSON syntax highlighting
pub struct JsonPalette {
    pub key: Color,
    pub string: Color,
    pub number: Color,
    pub boolean: Color,
    pub null: Color,
    pub structure: Color, // brackets, braces, colons, commas
}

impl JsonPalette {
    /// Create palette from theme colors
    pub fn from_theme(_theme: &Theme) -> Self {
        // TEMPORARY: Force ANSI colors since they work
        JsonPalette {
            key: Color::Cyan,        // Cyan for keys
            string: Color::Green,    // Green for strings
            number: Color::Yellow,   // Yellow/Orange for numbers
            boolean: Color::Magenta, // Magenta for booleans (more visible)
            null: Color::Blue,       // Blue for null
            structure: Color::White, // White for structure (more visible)
        }
    }
}

// Not used anymore - forcing ANSI colors
/*
/// Check if terminal supports true color
fn supports_true_color() -> bool {
    #[cfg(feature = "native")]
    {
        let colorterm = std::env::var("COLORTERM").ok();
        let term = std::env::var("TERM").ok();

        // Support both COLORTERM and modern TERM values
        colorterm
            .as_ref()
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or_else(|| {
                // Fallback: check if TERM indicates 256 color support
                // For now, we're being conservative and only using ANSI
                // unless COLORTERM explicitly says truecolor
                false
            })
    }

    #[cfg(not(feature = "native"))]
    false
}
*/

/// Render a JSON value with syntax highlighting
pub fn render_json(value: &Value, theme: &Theme) -> Vec<Line<'static>> {
    let palette = JsonPalette::from_theme(theme);

    let mut lines = Vec::new();
    render_value(value, 0, &mut lines, &palette, false);
    lines
}

/// Render a JSON value at the given indentation level
fn render_value(
    value: &Value,
    indent: usize,
    lines: &mut Vec<Line<'static>>,
    palette: &JsonPalette,
    inline: bool,
) {
    match value {
        Value::Object(map) => render_object(map, indent, lines, palette, inline),
        Value::Array(arr) => render_array(arr, indent, lines, palette, inline),
        Value::String(s) => {
            let span = Span::styled(
                format!("\"{}\"", escape_string(s)),
                Style::default().fg(palette.string),
            );
            if inline {
                // Return spans to caller
                if let Some(last_line) = lines.last_mut() {
                    last_line.spans.push(span);
                }
            } else {
                lines.push(Line::from(vec![indent_span(indent), span]));
            }
        }
        Value::Number(n) => {
            let span = Span::styled(n.to_string(), Style::default().fg(palette.number));
            if inline {
                if let Some(last_line) = lines.last_mut() {
                    last_line.spans.push(span);
                }
            } else {
                lines.push(Line::from(vec![indent_span(indent), span]));
            }
        }
        Value::Bool(b) => {
            let span = Span::styled(b.to_string(), Style::default().fg(palette.boolean));
            if inline {
                if let Some(last_line) = lines.last_mut() {
                    last_line.spans.push(span);
                }
            } else {
                lines.push(Line::from(vec![indent_span(indent), span]));
            }
        }
        Value::Null => {
            let span = Span::styled("null", Style::default().fg(palette.null));
            if inline {
                if let Some(last_line) = lines.last_mut() {
                    last_line.spans.push(span);
                }
            } else {
                lines.push(Line::from(vec![indent_span(indent), span]));
            }
        }
    }
}

/// Render a JSON object with proper formatting
fn render_object(
    map: &serde_json::Map<String, Value>,
    indent: usize,
    lines: &mut Vec<Line<'static>>,
    palette: &JsonPalette,
    inline: bool,
) {
    if map.is_empty() {
        // Empty object should render as a single "{}" token.
        let span = Span::styled("{}", Style::default().fg(palette.structure));
        if inline {
            if let Some(last_line) = lines.last_mut() {
                last_line.spans.push(span);
            }
        } else {
            lines.push(Line::from(vec![indent_span(indent), span]));
        }
        return;
    }

    // Opening brace
    if inline {
        if let Some(last_line) = lines.last_mut() {
            last_line
                .spans
                .push(Span::styled("{", Style::default().fg(palette.structure)));
        }
    } else {
        lines.push(Line::from(vec![
            indent_span(indent),
            Span::styled("{", Style::default().fg(palette.structure)),
        ]));
    }

    // Render entries
    let entries: Vec<_> = map.iter().collect();
    let len = entries.len();

    for (i, (key, value)) in entries.iter().enumerate() {
        // Key and colon
        let entry_spans = vec![
            indent_span(indent + 2),
            Span::styled(format!("\"{}\"", key), Style::default().fg(palette.key)),
            Span::styled(": ", Style::default().fg(palette.structure)),
        ];

        // Check if value should be inline
        if is_simple_value(value) {
            // Render value inline
            lines.push(Line::from(entry_spans));
            render_value(value, indent + 2, lines, palette, true);

            // Add comma if not last
            if i < len - 1 {
                if let Some(last_line) = lines.last_mut() {
                    last_line
                        .spans
                        .push(Span::styled(",", Style::default().fg(palette.structure)));
                }
            }
        } else {
            // Keep complex open delimiters inline with the key context.
            lines.push(Line::from(entry_spans));
            render_value(value, indent + 2, lines, palette, true);

            // Add comma if not last
            if i < len - 1 {
                if let Some(last_line) = lines.last_mut() {
                    last_line
                        .spans
                        .push(Span::styled(",", Style::default().fg(palette.structure)));
                }
            }
        }
    }

    // Closing brace on its own line
    lines.push(Line::from(vec![
        indent_span(indent),
        Span::styled("}", Style::default().fg(palette.structure)),
    ]));
}

/// Render a JSON array with proper formatting
fn render_array(
    arr: &[Value],
    indent: usize,
    lines: &mut Vec<Line<'static>>,
    palette: &JsonPalette,
    inline: bool,
) {
    if arr.is_empty() {
        // Empty array should render as a single "[]" token.
        let span = Span::styled("[]", Style::default().fg(palette.structure));
        if inline {
            if let Some(last_line) = lines.last_mut() {
                last_line.spans.push(span);
            }
        } else {
            lines.push(Line::from(vec![indent_span(indent), span]));
        }
        return;
    }

    // Check if we can render inline (all simple values and short)
    let can_inline = arr.len() <= 3 && arr.iter().all(is_simple_value);

    if can_inline && inline {
        // Render as [val1, val2, val3]
        if let Some(last_line) = lines.last_mut() {
            last_line
                .spans
                .push(Span::styled("[", Style::default().fg(palette.structure)));
        }

        for (i, value) in arr.iter().enumerate() {
            render_value(value, 0, lines, palette, true);
            if i < arr.len() - 1 {
                if let Some(last_line) = lines.last_mut() {
                    last_line
                        .spans
                        .push(Span::styled(", ", Style::default().fg(palette.structure)));
                }
            }
        }

        if let Some(last_line) = lines.last_mut() {
            last_line
                .spans
                .push(Span::styled("]", Style::default().fg(palette.structure)));
        }
    } else {
        // Multi-line array
        if inline {
            if let Some(last_line) = lines.last_mut() {
                last_line
                    .spans
                    .push(Span::styled("[", Style::default().fg(palette.structure)));
            }
        } else {
            lines.push(Line::from(vec![
                indent_span(indent),
                Span::styled("[", Style::default().fg(palette.structure)),
            ]));
        }

        // Render elements
        for (i, value) in arr.iter().enumerate() {
            if is_simple_value(value) {
                // Simple value on one line
                let line_spans = vec![indent_span(indent + 2)];
                lines.push(Line::from(line_spans));
                render_value(value, indent + 2, lines, palette, true);
            } else {
                // Complex value
                render_value(value, indent + 2, lines, palette, false);
            }

            // Add comma if not last
            if i < arr.len() - 1 {
                if let Some(last_line) = lines.last_mut() {
                    last_line
                        .spans
                        .push(Span::styled(",", Style::default().fg(palette.structure)));
                }
            }
        }

        // Closing bracket on its own line
        lines.push(Line::from(vec![
            indent_span(indent),
            Span::styled("]", Style::default().fg(palette.structure)),
        ]));
    }
}

/// Check if a value is simple (can be rendered inline)
fn is_simple_value(value: &Value) -> bool {
    match value {
        Value::String(s) => s.len() < 50, // Short strings only
        Value::Number(_) | Value::Bool(_) | Value::Null => true,
        Value::Object(map) => map.is_empty(), // Empty objects
        Value::Array(arr) => arr.is_empty(),  // Empty arrays
    }
}

/// Create an indentation span
fn indent_span(indent: usize) -> Span<'static> {
    Span::raw(" ".repeat(indent))
}

/// Escape special characters in JSON strings
fn escape_string(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_render_simple_object() {
        let theme = Theme::default();
        let json = json!({
            "name": "test",
            "count": 42,
            "active": true
        });

        let lines = render_json(&json, &theme);

        // Should have opening brace, 3 fields, closing brace = 5 lines
        assert_eq!(lines.len(), 5);

        // Check that closing brace is on its own line
        let last_line = &lines[4];
        assert_eq!(last_line.spans.len(), 2); // indent + "}"
    }

    #[test]
    fn test_render_nested_object() {
        let theme = Theme::default();
        let json = json!({
            "user": {
                "name": "alice",
                "id": 123
            }
        });

        let lines = render_json(&json, &theme);

        // Should properly indent nested structures
        // Line 0: {
        // Line 1:   "user": {
        // Line 2:     "name": "alice",
        // Line 3:     "id": 123
        // Line 4:   }
        // Line 5: }
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn test_empty_containers() {
        let theme = Theme::default();
        let json = json!({
            "empty_obj": {},
            "empty_arr": []
        });

        let lines = render_json(&json, &theme);

        // Empty containers should be inline: {}  and []
        assert!(lines
            .iter()
            .any(|line| { line.spans.iter().any(|span| span.content.contains("{}")) }));
    }
}
