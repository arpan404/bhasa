//! Tokenization for the scalar bootstrap language.

use crate::ast::Span;
use crate::diagnostic::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Int(String),
    Float(String),
    Str(String),
    Def,
    Struct,
    Enum,
    Match,
    Case,
    Extern,
    Var,
    If,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Return,
    True,
    False,
    IntType,
    FloatType,
    BoolType,
    StrType,
    VoidType,
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    Colon,
    Semicolon,
    Newline,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equal,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Eof,
}

/// Keep decimal spelling intact so the parser can recognize the signed minimum.
pub fn lex(source: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    #[cfg(feature = "bhasa-lexer")]
    {
        crate::bhasa_lexer::lex(source)
    }
    #[cfg(not(feature = "bhasa-lexer"))]
    {
        lex_rust(source)
    }
}

/// Reference implementation retained for differential testing and bootstrapping.
pub fn lex_rust(source: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    Lexer {
        source,
        offset: 0,
        paren_depth: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    }
    .run()
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    paren_depth: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn run(mut self) -> Result<Vec<Token>, Vec<Diagnostic>> {
        while self.offset < self.source.len() {
            let start = self.offset;
            let byte = self.source.as_bytes()[self.offset];
            let kind = match byte {
                b' ' | b'\t' | 0x0b | 0x0c => {
                    self.offset += 1;
                    continue;
                }
                b'\r' | b'\n' => {
                    self.consume_newline();
                    if self.paren_depth > 0 {
                        continue;
                    }
                    TokenKind::Newline
                }
                b'#' => {
                    self.skip_comment();
                    continue;
                }
                b'/' if self.source.as_bytes().get(self.offset + 1) == Some(&b'/') => {
                    self.skip_comment();
                    continue;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(),
                b'0'..=b'9' => {
                    if let Some(kind) = self.integer() {
                        kind
                    } else {
                        continue;
                    }
                }
                b'"' => {
                    if let Some(kind) = self.string() {
                        kind
                    } else {
                        continue;
                    }
                }
                b'(' => {
                    self.offset += 1;
                    self.paren_depth += 1;
                    TokenKind::LeftParen
                }
                b')' => {
                    self.offset += 1;
                    // Delimiter errors belong to the parser. A stray close must
                    // not suppress all subsequent statement separators.
                    self.paren_depth = self.paren_depth.saturating_sub(1);
                    TokenKind::RightParen
                }
                b'[' => {
                    self.offset += 1;
                    self.paren_depth += 1;
                    TokenKind::LeftBracket
                }
                b']' => {
                    self.offset += 1;
                    self.paren_depth = self.paren_depth.saturating_sub(1);
                    TokenKind::RightBracket
                }
                b'{' => self.single(TokenKind::LeftBrace),
                b'}' => self.single(TokenKind::RightBrace),
                b',' => self.single(TokenKind::Comma),
                b'.' => self.single(TokenKind::Dot),
                b':' => self.single(TokenKind::Colon),
                b';' => self.single(TokenKind::Semicolon),
                b'+' => self.single(TokenKind::Plus),
                b'-' => self.pair(b'>', TokenKind::Arrow, TokenKind::Minus),
                b'*' => self.single(TokenKind::Star),
                b'/' => self.single(TokenKind::Slash),
                b'%' => self.single(TokenKind::Percent),
                b'=' => self.pair(b'=', TokenKind::EqualEqual, TokenKind::Equal),
                b'!' => self.pair(b'=', TokenKind::NotEqual, TokenKind::Not),
                b'<' => self.pair(b'=', TokenKind::LessEqual, TokenKind::Less),
                b'>' => self.pair(b'=', TokenKind::GreaterEqual, TokenKind::Greater),
                b'&' | b'|' => {
                    self.offset += 1;
                    if self.source.as_bytes().get(self.offset) == Some(&byte) {
                        self.offset += 1;
                        if byte == b'&' {
                            TokenKind::And
                        } else {
                            TokenKind::Or
                        }
                    } else {
                        self.error(
                            "L001",
                            format!(
                                "unsupported operator '{}'; use '{}' for a boolean operation",
                                char::from(byte),
                                if byte == b'&' { "&&" } else { "||" }
                            ),
                            start,
                        );
                        continue;
                    }
                }
                0 => {
                    self.offset += 1;
                    self.error("L006", "NUL bytes are not allowed in source", start);
                    continue;
                }
                _ => {
                    let character = self.advance_char();
                    self.error(
                        "L001",
                        format!("unsupported character {character:?}"),
                        start,
                    );
                    continue;
                }
            };
            self.tokens.push(Token {
                kind,
                span: Span::new(start, self.offset),
            });
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.source.len(), self.source.len()),
        });
        if self.diagnostics.is_empty() {
            Ok(self.tokens)
        } else {
            Err(self.diagnostics)
        }
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.offset += 1;
        kind
    }

    fn pair(&mut self, second: u8, paired: TokenKind, single: TokenKind) -> TokenKind {
        self.offset += 1;
        if self.source.as_bytes().get(self.offset) == Some(&second) {
            self.offset += 1;
            paired
        } else {
            single
        }
    }

    fn consume_newline(&mut self) {
        if self.source.as_bytes()[self.offset] == b'\r' {
            self.offset += 1;
            if self.source.as_bytes().get(self.offset) == Some(&b'\n') {
                self.offset += 1;
            }
        } else {
            self.offset += 1;
        }
    }

    fn skip_comment(&mut self) {
        while self.offset < self.source.len() {
            match self.source.as_bytes()[self.offset] {
                b'\r' | b'\n' => break,
                0 => {
                    let start = self.offset;
                    self.offset += 1;
                    self.error("L006", "NUL bytes are not allowed in source", start);
                }
                _ => {
                    self.advance_char();
                }
            }
        }
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.offset;
        self.offset += 1;
        while let Some(byte) = self.source.as_bytes().get(self.offset) {
            if byte.is_ascii_alphanumeric() || *byte == b'_' {
                self.offset += 1;
            } else {
                break;
            }
        }
        match &self.source[start..self.offset] {
            "def" => TokenKind::Def,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "match" => TokenKind::Match,
            "case" => TokenKind::Case,
            "extern" => TokenKind::Extern,
            "var" => TokenKind::Var,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "int" => TokenKind::IntType,
            "float" => TokenKind::FloatType,
            "bool" => TokenKind::BoolType,
            "str" => TokenKind::StrType,
            "void" => TokenKind::VoidType,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            name => TokenKind::Ident(name.to_owned()),
        }
    }

    fn integer(&mut self) -> Option<TokenKind> {
        let start = self.offset;
        let bytes = self.source.as_bytes();
        while bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
        }
        let mut floating = false;
        if bytes.get(self.offset) == Some(&b'.')
            && bytes.get(self.offset + 1).is_some_and(u8::is_ascii_digit)
        {
            floating = true;
            self.offset += 1;
            while bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                self.offset += 1;
            }
        }
        let mut invalid = false;
        if matches!(bytes.get(self.offset), Some(b'e' | b'E')) {
            floating = true;
            self.offset += 1;
            if matches!(bytes.get(self.offset), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let exponent = self.offset;
            while bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                self.offset += 1;
            }
            invalid = self.offset == exponent;
        }
        if bytes
            .get(self.offset)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            invalid = true;
            while bytes
                .get(self.offset)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                self.offset += 1;
            }
        }
        if invalid {
            let spelling = &self.source[start..self.offset];
            let (code, message) = if floating {
                ("L008", format!("invalid float literal {spelling:?}; expected decimal digits and an optional signed exponent"))
            } else {
                (
                    "L002",
                    format!(
                        "invalid integer literal {spelling:?}; only decimal digits are allowed"
                    ),
                )
            };
            self.error(code, message, start);
            None
        } else if floating {
            Some(TokenKind::Float(self.source[start..self.offset].to_owned()))
        } else {
            Some(TokenKind::Int(self.source[start..self.offset].to_owned()))
        }
    }

    fn string(&mut self) -> Option<TokenKind> {
        let start = self.offset;
        self.offset += 1;
        let mut value = String::new();
        while self.offset < self.source.len() {
            let character_start = self.offset;
            let character = self.advance_char();
            match character {
                '"' => return Some(TokenKind::Str(value)),
                '\\' => {
                    if self.offset == self.source.len() {
                        break;
                    }
                    let escaped = self.advance_char();
                    match escaped {
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        '\\' => value.push('\\'),
                        '"' => value.push('"'),
                        '\0' => {
                            self.error(
                                "L006",
                                "NUL bytes are not allowed in strings",
                                character_start,
                            );
                        }
                        '\r' | '\n' => self.string_newline(escaped, character_start),
                        _ => self.error(
                            "L004",
                            format!("unsupported string escape \\{escaped}"),
                            character_start,
                        ),
                    }
                }
                '\0' => {
                    self.error(
                        "L006",
                        "NUL bytes are not allowed in strings",
                        character_start,
                    );
                }
                '\r' | '\n' => self.string_newline(character, character_start),
                _ => value.push(character),
            }
        }
        self.error("L003", "unterminated string literal", start);
        None
    }

    fn string_newline(&mut self, character: char, start: usize) {
        if character == '\r' && self.source.as_bytes().get(self.offset) == Some(&b'\n') {
            self.offset += 1;
        }
        self.error(
            "L005",
            "string literals cannot contain raw line breaks; use \\n or \\r",
            start,
        );
    }

    fn advance_char(&mut self) -> char {
        // Every caller checks for EOF, and offsets always remain UTF-8 boundaries.
        let character = self.source[self.offset..].chars().next().unwrap();
        self.offset += character.len_utf8();
        character
    }

    fn error(&mut self, code: &str, message: impl Into<String>, start: usize) {
        self.diagnostics.push(Diagnostic::new(
            code,
            message,
            Span::new(start, self.offset),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .expect("source should lex")
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn recognizes_keywords_and_preserves_identifier_boundaries() {
        use TokenKind::*;
        assert_eq!(
            kinds("def extern var if else while return true false int bool str void and or not def2 _if"),
            vec![Def, Extern, Var, If, Else, While, Return, True, False, IntType, BoolType, StrType, VoidType, And, Or, Not, Ident("def2".into()), Ident("_if".into()), Eof]
        );
    }

    #[test]
    fn recognizes_delimiters_and_longest_operators() {
        use TokenKind::*;
        assert_eq!(
            kinds("()[]{} ,:; -> + - * / % = == != ! < <= > >= && ||"),
            vec![
                LeftParen,
                RightParen,
                LeftBracket,
                RightBracket,
                LeftBrace,
                RightBrace,
                Comma,
                Colon,
                Semicolon,
                Arrow,
                Plus,
                Minus,
                Star,
                Slash,
                Percent,
                Equal,
                EqualEqual,
                NotEqual,
                Not,
                Less,
                LessEqual,
                Greater,
                GreaterEqual,
                And,
                Or,
                Eof
            ]
        );
    }

    #[test]
    fn recognizes_struct_and_field_access_without_stealing_identifiers() {
        use TokenKind::*;
        assert_eq!(
            kinds("struct Point { x: int } p.x structure struct2"),
            vec![
                Struct,
                Ident("Point".into()),
                LeftBrace,
                Ident("x".into()),
                Colon,
                IntType,
                RightBrace,
                Ident("p".into()),
                Dot,
                Ident("x".into()),
                Ident("structure".into()),
                Ident("struct2".into()),
                Eof,
            ]
        );
    }

    #[test]
    fn leaves_signed_literal_range_checking_to_parser() {
        use TokenKind::*;
        assert_eq!(
            kinds("-9223372036854775808 18446744073709551616 001"),
            vec![
                Minus,
                Int("9223372036854775808".into()),
                Int("18446744073709551616".into()),
                Int("001".into()),
                Eof
            ]
        );
    }

    #[test]
    fn suppresses_parenthesized_and_generic_argument_newlines() {
        use TokenKind::*;
        assert_eq!(
            kinds("{\ncall[\nint\n](\n(1\r\n),\r2\n)\n}\n"),
            vec![
                LeftBrace,
                Newline,
                Ident("call".into()),
                LeftBracket,
                IntType,
                RightBracket,
                LeftParen,
                LeftParen,
                Int("1".into()),
                RightParen,
                Comma,
                Int("2".into()),
                RightParen,
                Newline,
                RightBrace,
                Newline,
                Eof
            ]
        );
    }

    #[test]
    fn unmatched_close_does_not_hide_later_newlines() {
        use TokenKind::*;
        assert_eq!(
            kinds(")\n(\n)\n"),
            vec![RightParen, Newline, LeftParen, RightParen, Newline, Eof]
        );
    }

    #[test]
    fn comments_preserve_statement_boundaries() {
        use TokenKind::*;
        assert_eq!(
            kinds("return # ( \" comment\r\n// नेपाली )\n1 / 2 // tail"),
            vec![
                Return,
                Newline,
                Newline,
                Int("1".into()),
                Slash,
                Int("2".into()),
                Eof
            ]
        );
        assert_eq!(
            kinds("(1 // comment\n + 2)"),
            vec![
                LeftParen,
                Int("1".into()),
                Plus,
                Int("2".into()),
                RightParen,
                Eof
            ]
        );
    }

    #[test]
    fn decodes_supported_escapes_and_utf8_without_comment_parsing() {
        assert_eq!(
            kinds(r#""नमस्ते // # \n\r\t\\\"""#),
            vec![
                TokenKind::Str("नमस्ते // # \n\r\t\\\"".into()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn spans_use_utf8_bytes_and_crlf_is_one_token() {
        let source = "\"é🌍\"\r\nx";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 8));
        assert_eq!(tokens[1].span, Span::new(8, 10));
        assert_eq!(tokens[2].span, Span::new(10, 11));
        assert_eq!(tokens[3].span, Span::new(11, 11));
    }

    #[test]
    fn empty_source_has_zero_width_eof() {
        assert_eq!(
            lex("").unwrap(),
            vec![Token {
                kind: TokenKind::Eof,
                span: Span::new(0, 0)
            }]
        );
    }

    #[test]
    fn reports_whole_malformed_integer_literals() {
        let source = "123abc 1_000 0xff";
        let errors = lex(source).unwrap_err();
        assert_eq!(errors.len(), 3);
        let bad_literals: Vec<_> = errors
            .iter()
            .map(|error| &source[error.span.start..error.span.end])
            .collect();
        assert_eq!(bad_literals, vec!["123abc", "1_000", "0xff"]);
        assert!(errors
            .iter()
            .all(|error| error.message.contains("integer literal")));
    }

    #[test]
    fn reports_unsupported_characters_at_character_boundaries() {
        let source = "é @ & |";
        let errors = lex(source).unwrap_err();
        assert_eq!(errors.len(), 4);
        assert_eq!(errors[0].span, Span::new(0, 2));
        assert!(errors.iter().all(|error| error.code.starts_with('L')));
    }

    #[test]
    fn reports_unknown_escape_and_continues_after_string() {
        let source = r#""bad\q" @ "also\x""#;
        let errors = lex(source).unwrap_err();
        assert_eq!(errors.len(), 3);
        assert_eq!(&source[errors[0].span.start..errors[0].span.end], "\\q");
        assert_eq!(&source[errors[1].span.start..errors[1].span.end], "@");
        assert_eq!(&source[errors[2].span.start..errors[2].span.end], "\\x");
    }

    #[test]
    fn rejects_raw_line_breaks_including_escaped_physical_line_breaks() {
        for source in ["\"a\nb\"", "\"a\r\nb\"", "\"a\\\nb\"", "\"a\\\r\nb\""] {
            let errors = lex(source).unwrap_err();
            assert_eq!(errors.len(), 1, "{source:?}");
            assert!(errors[0].message.contains("raw line breaks"));
        }
    }

    #[test]
    fn rejects_nul_in_strings_source_and_comments() {
        for source in ["\"a\0b\"", "\"a\\\0b\"", "\0", "# a\0", "// a\0"] {
            let errors = lex(source).unwrap_err();
            assert_eq!(errors.len(), 1, "{source:?}");
            assert!(errors[0].message.contains("NUL"));
        }
        assert!(lex(r#""\0""#).unwrap_err()[0]
            .message
            .contains("unsupported string escape"));
    }

    #[test]
    fn rejects_unterminated_strings_and_trailing_backslash() {
        for source in ["\"unfinished", "\"unfinished\\"] {
            let errors = lex(source).unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].span, Span::new(0, source.len()));
            assert!(errors[0].message.contains("unterminated string"));
        }
    }
}
