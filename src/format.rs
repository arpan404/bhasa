//! Deterministic formatting that retains comments, string spelling and statement boundaries.
use crate::ast::Span;
use crate::diagnostic::Diagnostic;
use crate::lexer::{lex, TokenKind};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Output {
    text: String,
    line: String,
    depth: usize,
}

impl Output {
    fn word(&mut self, text: &str, separated: bool) {
        if self.line.is_empty() {
            self.line.push_str(&"    ".repeat(self.depth));
        } else if separated && !self.line.ends_with(' ') {
            self.line.push(' ');
        }
        self.line.push_str(text);
    }

    fn space(&mut self) {
        if !self.line.is_empty() && !self.line.ends_with(' ') {
            self.line.push(' ');
        }
    }

    fn newline(&mut self) {
        if !self.line.is_empty() {
            self.text.push_str(self.line.trim_end());
            self.text.push('\n');
            self.line.clear();
        }
    }

    fn blank(&mut self) {
        self.newline();
        if !self.text.is_empty() && !self.text.ends_with("\n\n") {
            self.text.push('\n');
        }
    }

    fn comments(&mut self, gap: &str) -> bool {
        let mut found = false;
        for line in gap.split(['\r', '\n']) {
            let comment = line.trim();
            if comment.starts_with('#') || comment.starts_with("//") {
                found = true;
                if !self.line.trim().is_empty() {
                    self.space();
                    self.line.push(' ');
                }
                self.word(comment, false);
                self.newline();
            }
        }
        found
    }
}

/// Format lexically valid source without requiring names or imports to resolve.
/// A token check prevents formatting from changing significant statement boundaries.
pub fn format_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let tokens = lex(source)?;
    let mut output = Output::default();
    let mut end = 0;
    let mut previous: Option<&TokenKind> = None;
    let mut newlines = 0;
    for token in &tokens {
        if output.comments(&source[end..token.span.start]) {
            newlines = 0;
        }
        end = token.span.end;
        let kind = &token.kind;
        let spelling = &source[token.span.start..token.span.end];
        if *kind == TokenKind::Newline {
            output.newline();
            newlines += 1;
            if newlines > 1 {
                output.blank();
            }
            continue;
        }
        newlines = 0;
        if matches!(previous, Some(TokenKind::RightBrace))
            && !matches!(
                kind,
                TokenKind::Else | TokenKind::Semicolon | TokenKind::Eof
            )
        {
            output.newline();
        }
        match kind {
            TokenKind::Eof => break,
            TokenKind::LeftBrace => {
                output.word(spelling, true);
                output.newline();
                output.depth = output
                    .depth
                    .checked_add(1)
                    .filter(|d| *d <= 128)
                    .ok_or_else(|| {
                        vec![Diagnostic::new(
                            "F001",
                            "formatting nesting exceeds 128 blocks",
                            token.span,
                        )]
                    })?;
            }
            TokenKind::RightBrace => {
                if output.depth == 0 {
                    return Err(vec![Diagnostic::new(
                        "F001",
                        "unmatched closing brace",
                        token.span,
                    )]);
                }
                output.newline();
                output.depth -= 1;
                output.word(spelling, false);
            }
            TokenKind::Comma | TokenKind::Colon => {
                output.line.truncate(output.line.trim_end().len());
                output.word(spelling, false);
                output.space();
            }
            TokenKind::Dot => {
                output.line.truncate(output.line.trim_end().len());
                output.word(spelling, false);
            }
            TokenKind::Semicolon => {
                output.line.truncate(output.line.trim_end().len());
                output.word(spelling, false);
                output.newline();
            }
            TokenKind::RightParen | TokenKind::RightBracket => {
                output.line.truncate(output.line.trim_end().len());
                output.word(spelling, false);
            }
            TokenKind::LeftBracket => {
                output.line.truncate(output.line.trim_end().len());
                output.word(spelling, false);
            }
            TokenKind::LeftParen => {
                let separated = !matches!(
                    previous,
                    Some(
                        TokenKind::Ident(_)
                            | TokenKind::IntType
                            | TokenKind::FloatType
                            | TokenKind::RightBracket
                            | TokenKind::LeftParen
                            | TokenKind::Not
                    )
                );
                output.word(spelling, separated);
            }
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Equal
            | TokenKind::EqualEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Arrow => {
                output.word(spelling, true);
                output.space();
            }
            TokenKind::Not => {
                output.word(spelling, !matches!(previous, Some(TokenKind::LeftParen)));
                if spelling == "not" {
                    output.space();
                }
            }
            _ => output.word(
                spelling,
                !matches!(
                    previous,
                    Some(TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::Dot)
                ) && !(previous == Some(&TokenKind::Not) && output.line.ends_with('!')),
            ),
        }
        previous = Some(kind);
    }
    if output.depth != 0 {
        return Err(vec![Diagnostic::new(
            "F001",
            "unclosed block",
            Span::new(source.len(), source.len()),
        )]);
    }
    output.newline();
    let result = format!("{}\n", output.text.trim_end());
    let result = if result == "\n" {
        String::new()
    } else {
        result
    };
    let formatted = lex(&result)?;
    if significant(&tokens) != significant(&formatted) {
        return Err(vec![Diagnostic::new(
            "F002",
            "formatting would change statement boundaries",
            Span::default(),
        )]);
    }
    Ok(result)
}

fn significant(tokens: &[crate::lexer::Token]) -> Vec<&TokenKind> {
    let mut result: Vec<&TokenKind> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        index += 1;
        if token.kind == TokenKind::Newline {
            while tokens
                .get(index)
                .is_some_and(|token| token.kind == TokenKind::Newline)
            {
                index += 1;
            }
            let previous = result.last().copied();
            let next = tokens.get(index).map(|token| &token.kind);
            if previous.is_none()
                || matches!(
                    previous,
                    Some(
                        TokenKind::LeftBrace
                            | TokenKind::RightBrace
                            | TokenKind::Semicolon
                            | TokenKind::Newline
                    )
                )
                || matches!(
                    next,
                    Some(TokenKind::LeftBrace | TokenKind::RightBrace | TokenKind::Eof)
                )
            {
                continue;
            }
        }
        result.push(&token.kind);
    }
    result
}

pub fn run(path: &Path, check: bool) -> i32 {
    match format_file(path, check) {
        Ok(true) => 0,
        Ok(false) => {
            eprintln!("{} needs formatting", path.display());
            1
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn format_file(path: &Path, check: bool) -> Result<bool, String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
    let formatted = format_source(&source).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|d| d.render(&source, &path.display().to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    if source == formatted {
        return Ok(true);
    }
    if check {
        return Ok(false);
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or("source has no parent directory")?;
    let temporary = parent.join(format!(
        ".bhasa-format-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        created = true;
        file.set_permissions(metadata.permissions())
            .map_err(|error| error.to_string())?;
        file.write_all(formatted.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        if fs::read_to_string(&path).map_err(|error| error.to_string())? != source {
            return Err("source changed while formatting; rerun the formatter".into());
        }
        fs::rename(&temporary, &path).map_err(|error| error.to_string())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_strings_imports_and_statement_newlines_survive() {
        let source = "# leading\r\nimport \"math.bh\"\r\ndef main(){var x=1; // inner\nif x>0{print(\"// not # comment 🌍\")}else{print(\"no\")}\n}\n";
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("import \"math.bh\"\n"));
        assert!(formatted.contains("    var x = 1;\n    // inner\n"));
        assert!(formatted.contains("    } else {\n"));
        assert!(formatted.contains("print(\"// not # comment 🌍\")"));
        assert_eq!(format_source(&formatted).unwrap(), formatted);
        assert!(format_source("def f(){ return\n42\n}")
            .unwrap()
            .contains("return\n    42"));
    }

    #[test]
    fn malformed_tokens_and_unbalanced_braces_are_rejected() {
        for source in ["def x() {", "}", "def x(){print(\"bad\\q\")}"] {
            assert!(format_source(source).is_err());
        }
    }

    #[test]
    fn formatted_program_has_identical_llvm_and_is_idempotent() {
        for source in [
            "def main(){var n= -1; if not(n ==0){print(n+2)}else{print(0)}}",
            "def main() {\n var x = (1 + // comment\n 2)\n print(x) # end\n}\n",
            "struct Point{x:int;def move(self,n:int)->Point{self.x=self.x+n;return self} def value(self)->int{return self.x}}\ndef main(){var p=Point(40);print(p.move(2).value())}",
            "enum Maybe{none;some(value:buffer)}\ndef main(){match Maybe.some(buffer(\"ok\")){case Maybe.none{print(0)}case Maybe.some(value){print(value)}}}",
            include_str!("../bootstrap/compiler/lexer.bh"),
            include_str!("../examples/concurrency/tasks.bh"),
        ] {
            let formatted = format_source(source).unwrap();
            assert_eq!(format_source(&formatted).unwrap(), formatted);
            assert_eq!(
                crate::compile_to_llvm(source, "x86_64-unknown-linux-gnu"),
                crate::compile_to_llvm(&formatted, "x86_64-unknown-linux-gnu")
            );
        }
    }
}
