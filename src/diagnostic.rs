use crate::ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            span,
        }
    }

    fn position(&self, source: &str) -> (usize, usize, usize) {
        let mut offset = self.span.start.min(source.len());
        while !source.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &source[..offset];
        let mut line = 1;
        let mut line_start = 0;
        for (index, ch) in prefix.char_indices() {
            match ch {
                '\r' => {
                    line += 1;
                    line_start = index + 1;
                }
                '\n' => {
                    if index == 0 || prefix.as_bytes()[index - 1] != b'\r' {
                        line += 1;
                    }
                    line_start = index + 1;
                }
                _ => {}
            }
        }
        let column = source[line_start..offset].chars().count() + 1;
        (line, column, line_start)
    }

    pub fn render(&self, source: &str, path: &str) -> String {
        let (line, column, start) = self.position(source);
        let text = source[start..].split(['\r', '\n']).next().unwrap_or("");
        // Limit context to avoid dumping a huge generated source line.
        let skip = column.saturating_sub(61);
        let context: String = text.chars().skip(skip).take(160).collect();
        let indent = " ".repeat(column.saturating_sub(1).saturating_sub(skip));
        format!(
            "{path}:{line}:{column}: error[{}]: {}\n  {context}\n  {indent}^",
            self.code, self.message
        )
    }

    pub fn to_json(&self, source: &str, path: &str) -> String {
        let (line, column, _) = self.position(source);
        format!(
            "{{\"schema_version\":1,\"severity\":\"error\",\"code\":{},\"message\":{},\"file\":{},\"span\":{{\"start\":{},\"end\":{}}},\"line\":{},\"column\":{}}}",
            json_string(&self.code), json_string(&self.message), json_string(path),
            self.span.start, self.span.end, line, column
        )
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Diagnostic {}

/// Quote JSON without adding a serialization dependency to the bootstrap.
pub fn json_string(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c <= '\u{1f}' => {
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_unicode_with_character_columns_and_byte_spans() {
        let source = "first\nα = wrong\n";
        let error = Diagnostic::new("S001", "unknown name", Span::new(11, 16));
        assert!(error.render(source, "test.bh").starts_with("test.bh:2:5:"));
        assert!(error.to_json(source, "test.bh").contains("\"start\":11"));
    }
    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(
            json_string("\"\\\n\t\u{0000}अ"),
            "\"\\\"\\\\\\n\\t\\u0000अ\""
        );
    }
    #[test]
    fn invalid_spans_do_not_panic_during_rendering() {
        let source = "α";
        for start in [0, 1, 2, usize::MAX] {
            let diagnostic = Diagnostic::new("T001", "test", Span::new(start, start));
            assert!(diagnostic.render(source, "test.bh").contains("error[T001]"));
        }
    }

    #[test]
    fn line_positions_follow_all_supported_newline_styles() {
        for source in ["first\rnext", "first\r\nnext", "first\nnext"] {
            let offset = source.find("next").unwrap();
            let diagnostic = Diagnostic::new("T002", "test", Span::new(offset, offset + 4));
            assert!(diagnostic
                .render(source, "test.bh")
                .starts_with("test.bh:2:1:"));
        }
    }
}
