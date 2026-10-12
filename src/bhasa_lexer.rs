//! Typed boundary around the Bhasa scanner compiled by the Rust bootstrap.
//! No compiler phases are performed by this adapter; it translates token tables.

use crate::ast::Span;
use crate::diagnostic::Diagnostic;
use crate::lexer::{Token, TokenKind};
use std::ptr::NonNull;

#[repr(C)]
struct RawBuffer {
    len: i64,
    cap: i64,
    data: *mut u8,
}

#[repr(C)]
struct RawInts {
    len: i64,
    cap: i64,
    data: *mut i64,
}

extern "C" {
    #[link_name = "bhasa.fn.bh_lex"]
    fn scanner(source: *const RawBuffer, errors: *mut RawInts) -> *mut RawInts;
    #[link_name = "bhasa.fn.bh_string_value"]
    fn string_value(source: *const RawBuffer, tokens: *const RawInts, index: i64)
        -> *mut RawBuffer;
    fn bhasa_rt_ints_new() -> *mut RawInts;
    fn bhasa_rt_ints_free(value: *mut RawInts);
    fn bhasa_rt_buffer_free(value: *mut RawBuffer);
    #[cfg(feature = "bhasa-frontend")]
    #[link_name = "bhasa.fn.bh_compile"]
    fn native_compile(
        source: *const RawBuffer,
        tokens: *const RawInts,
        errors: *mut RawInts,
        target: *const RawBuffer,
    ) -> *mut RawBuffer;
}

struct Ints(NonNull<RawInts>);

impl Ints {
    fn own(pointer: *mut RawInts) -> Result<Self, String> {
        NonNull::new(pointer)
            .map(Self)
            .ok_or_else(|| "scanner returned a null integer table".into())
    }

    fn values(&self) -> Result<&[i64], String> {
        // The linked runtime owns this descriptor until this RAII wrapper drops.
        let value = unsafe { self.0.as_ref() };
        let length = checked_length(value.len, value.cap, std::mem::size_of::<i64>())?;
        if length == 0 {
            return Ok(&[]);
        }
        if value.data.is_null()
            || !(value.data as usize).is_multiple_of(std::mem::align_of::<i64>())
        {
            return Err("scanner returned invalid integer storage".into());
        }
        // The native runtime guarantees an allocation of cap elements. We check
        // dimensions before constructing a Rust view and never mutate this view.
        Ok(unsafe { std::slice::from_raw_parts(value.data, length) })
    }
}

impl Drop for Ints {
    fn drop(&mut self) {
        // Only descriptors returned by the runtime enter this owned wrapper.
        unsafe { bhasa_rt_ints_free(self.0.as_ptr()) }
    }
}

struct Buffer(NonNull<RawBuffer>);

impl Buffer {
    fn own(pointer: *mut RawBuffer) -> Result<Self, String> {
        NonNull::new(pointer)
            .map(Self)
            .ok_or_else(|| "scanner returned a null string buffer".into())
    }

    fn text(&self) -> Result<String, String> {
        let value = unsafe { self.0.as_ref() };
        let length = checked_length(value.len, value.cap, 1)?;
        let bytes = if length == 0 {
            &[]
        } else {
            if value.data.is_null() {
                return Err("scanner returned invalid string storage".into());
            }
            unsafe { std::slice::from_raw_parts(value.data, length) }
        };
        if bytes.contains(&0) {
            return Err("scanner returned NUL in a static string".into());
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|_| "scanner returned non-UTF-8 string text".into())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { bhasa_rt_buffer_free(self.0.as_ptr()) }
    }
}

fn checked_length(length: i64, capacity: i64, element_size: usize) -> Result<usize, String> {
    let length = usize::try_from(length).map_err(|_| "negative native length")?;
    let capacity = usize::try_from(capacity).map_err(|_| "negative native capacity")?;
    if capacity < length || capacity > isize::MAX as usize / element_size {
        return Err("invalid native capacity".into());
    }
    Ok(length)
}

fn span(source: &str, start: i64, end: i64) -> Result<Span, String> {
    let start = usize::try_from(start).map_err(|_| "negative token start")?;
    let end = usize::try_from(end).map_err(|_| "negative token end")?;
    if start > end
        || end > source.len()
        || !source.is_char_boundary(start)
        || !source.is_char_boundary(end)
    {
        return Err("scanner returned an invalid source span".into());
    }
    Ok(Span::new(start, end))
}

fn table<'a>(values: &'a Ints, source: &str) -> Result<&'a [i64], String> {
    let values = values.values()?;
    if !values.len().is_multiple_of(3) {
        return Err("scanner returned an incomplete table row".into());
    }
    for row in values.as_chunks::<3>().0 {
        span(source, row[1], row[2])?;
    }
    Ok(values)
}

fn diagnostic(source: &str, row: &[i64], tokens: &[i64]) -> Result<Diagnostic, String> {
    let location = span(source, row[1], row[2])?;
    let text = &source[location.start..location.end];
    let first = text.chars().next();
    let message = match row[0] {
        1 => match first {
            Some(character @ ('&' | '|')) => format!(
                "unsupported operator '{character}'; use '{character}{character}' for a boolean operation"
            ),
            Some(character) => format!("unsupported character {character:?}"),
            None => return Err("empty unsupported-character diagnostic".into()),
        },
        2 => format!("invalid integer literal {text:?}; only decimal digits are allowed"),
        3 => "unterminated string literal".into(),
        8 => format!("invalid float literal {text:?}; expected decimal digits and an optional signed exponent"),
        4 => {
            let escaped = text.chars().nth(1).ok_or("invalid escape diagnostic span")?;
            format!("unsupported string escape \\{escaped}")
        }
        5 => "string literals cannot contain raw line breaks; use \\n or \\r".into(),
        6 => {
            let in_string = tokens.as_chunks::<3>().0.iter().any(|token| {
                token[0] == 3 && token[1] <= row[1] && token[2] >= row[2]
            });
            if in_string {
                "NUL bytes are not allowed in strings".into()
            } else {
                "NUL bytes are not allowed in source".into()
            }
        }
        _ => return Err("scanner returned an unknown diagnostic code".into()),
    };
    Ok(Diagnostic::new(
        format!("L{:03}", row[0]),
        message,
        location,
    ))
}

fn fixed_token(code: i64, text: &str) -> Option<TokenKind> {
    use TokenKind::*;
    match (code, text) {
        (300, "def") => Some(Def),
        (301, "extern") => Some(Extern),
        (302, "var") => Some(Var),
        (303, "if") => Some(If),
        (304, "else") => Some(Else),
        (305, "while") => Some(While),
        (306, "return") => Some(Return),
        (307, "true") => Some(True),
        (308, "false") => Some(False),
        (309, "int") => Some(IntType),
        (310, "bool") => Some(BoolType),
        (311, "str") => Some(StrType),
        (312, "void") => Some(VoidType),
        (313, "struct") => Some(Struct),
        (315, "enum") => Some(Enum),
        (316, "match") => Some(Match),
        (317, "case") => Some(Case),
        (318, "float") => Some(FloatType),
        (319, "for") => Some(For),
        (320, "in") => Some(In),
        (321, "break") => Some(Break),
        (322, "continue") => Some(Continue),
        (46, ".") => Some(Dot),
        (260, "and" | "&&") => Some(And),
        (261, "or" | "||") => Some(Or),
        (33, "not" | "!") => Some(Not),
        (40, "(") => Some(LeftParen),
        (41, ")") => Some(RightParen),
        (91, "[") => Some(LeftBracket),
        (93, "]") => Some(RightBracket),
        (123, "{") => Some(LeftBrace),
        (125, "}") => Some(RightBrace),
        (44, ",") => Some(Comma),
        (58, ":") => Some(Colon),
        (59, ";") => Some(Semicolon),
        (10, "\n" | "\r" | "\r\n") => Some(Newline),
        (262, "->") => Some(Arrow),
        (43, "+") => Some(Plus),
        (45, "-") => Some(Minus),
        (42, "*") => Some(Star),
        (47, "/") => Some(Slash),
        (37, "%") => Some(Percent),
        (61, "=") => Some(Equal),
        (256, "==") => Some(EqualEqual),
        (257, "!=") => Some(NotEqual),
        (60, "<") => Some(Less),
        (258, "<=") => Some(LessEqual),
        (62, ">") => Some(Greater),
        (259, ">=") => Some(GreaterEqual),
        _ => None,
    }
}

fn translate(source: &str) -> Result<Result<Vec<Token>, Vec<Diagnostic>>, String> {
    let length = i64::try_from(source.len()).map_err(|_| "source length exceeds scanner limit")?;
    // This descriptor borrows Rust's UTF-8 storage. Bhasa buffer parameters allow
    // mutation, so this boundary specifically trusts bh_lex/bh_string_value to
    // read source without mutation, retention or freeing. Their checked-in
    // implementations only use source with get/len/slice operations.
    let input = RawBuffer {
        len: length,
        cap: length,
        data: source.as_ptr().cast_mut(),
    };
    let errors = Ints::own(unsafe { bhasa_rt_ints_new() })?;
    let tokens = Ints::own(unsafe { scanner(&input, errors.0.as_ptr()) })?;
    let rows = table(&tokens, source)?;
    let error_rows = table(&errors, source)?;
    if !error_rows.is_empty() {
        return Ok(Err(error_rows
            .as_chunks::<3>()
            .0
            .iter()
            .map(|row| diagnostic(source, row, rows))
            .collect::<Result<_, _>>()?));
    }
    if rows.is_empty() {
        return Err("scanner omitted EOF".into());
    }
    let mut result = Vec::with_capacity(rows.len() / 3);
    let mut previous_end = 0;
    for (index, row) in rows.as_chunks::<3>().0.iter().enumerate() {
        let location = span(source, row[1], row[2])?;
        if location.start < previous_end {
            return Err("scanner returned overlapping tokens".into());
        }
        previous_end = location.end;
        let text = &source[location.start..location.end];
        let kind = match row[0] {
            0 if index + 1 == rows.len() / 3
                && location.start == source.len()
                && location.end == source.len() =>
            {
                TokenKind::Eof
            }
            1 if text
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
            {
                TokenKind::Ident(text.into())
            }
            2 if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) => {
                TokenKind::Int(text.into())
            }
            4 if matches!(
                crate::lexer::lex_rust(text).as_deref(),
                Ok([
                    Token {
                        kind: TokenKind::Float(_),
                        ..
                    },
                    Token {
                        kind: TokenKind::Eof,
                        ..
                    }
                ])
            ) =>
            {
                TokenKind::Float(text.into())
            }
            3 if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') => {
                let value =
                    Buffer::own(unsafe { string_value(&input, tokens.0.as_ptr(), index as i64) })?;
                TokenKind::Str(value.text()?)
            }
            code => {
                fixed_token(code, text).ok_or("scanner returned an invalid token kind/spelling")?
            }
        };
        result.push(Token {
            kind,
            span: location,
        });
    }
    if !matches!(result.last().map(|token| &token.kind), Some(TokenKind::Eof)) {
        return Err("scanner omitted final EOF".into());
    }
    Ok(Ok(result))
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    translate(source).unwrap_or_else(|message| {
        Err(vec![Diagnostic::new(
            "H001",
            format!("Bhasa scanner integration failed: {message}"),
            Span::default(),
        )])
    })
}

#[cfg(feature = "bhasa-frontend")]
pub(crate) fn compile(
    source: &str,
    target: &str,
    boundaries: &[usize],
) -> Result<String, Vec<Diagnostic>> {
    compile_inner(source, target, boundaries).unwrap_or_else(|message| {
        Err(vec![Diagnostic::new(
            "H001",
            format!("Bhasa frontend integration failed: {message}"),
            Span::default(),
        )])
    })
}

#[cfg(feature = "bhasa-frontend")]
fn compile_inner(
    source: &str,
    target: &str,
    boundaries: &[usize],
) -> Result<Result<String, Vec<Diagnostic>>, String> {
    if target.is_empty()
        || !target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte))
    {
        return Err("invalid target triple".into());
    }
    // The reviewed Bhasa scanner and frontend borrow these descriptors read-only.
    // They do not retain, mutate or free source/target storage. Other buffers and
    // returned tables are runtime-owned and always destroyed by the RAII wrappers.
    let input = RawBuffer {
        len: i64::try_from(source.len()).map_err(|_| "source too large")?,
        cap: source.len() as i64,
        data: source.as_ptr().cast_mut(),
    };
    let target = RawBuffer {
        len: i64::try_from(target.len()).map_err(|_| "target too large")?,
        cap: target.len() as i64,
        data: target.as_ptr().cast_mut(),
    };
    let errors = Ints::own(unsafe { bhasa_rt_ints_new() })?;
    let tokens = Ints::own(unsafe { scanner(&input, errors.0.as_ptr()) })?;
    let rows = table(&tokens, source)?;
    let error_rows = table(&errors, source)?;
    if !error_rows.is_empty() {
        return Ok(Err(error_rows
            .as_chunks::<3>()
            .0
            .iter()
            .map(|row| diagnostic(source, row, rows))
            .collect::<Result<_, _>>()?));
    }
    // A boundary is accepted only between complete top-level declarations.
    // The native frontend borrows the token table read-only, so Rust retains
    // ownership of this merged storage and descriptor through native_compile.
    let mut merged = Vec::with_capacity(rows.len() + boundaries.len() * 3);
    let mut next = 0;
    for row in rows.as_chunks::<3>().0 {
        while next < boundaries.len() && boundaries[next] <= row[1] as usize {
            let offset = boundaries[next];
            if offset > source.len()
                || !source.is_char_boundary(offset)
                || (next > 0 && offset <= boundaries[next - 1])
            {
                return Err("invalid module boundary".into());
            }
            merged.extend_from_slice(&[314, offset as i64, offset as i64]);
            next += 1;
        }
        if next < boundaries.len() && boundaries[next] < row[2] as usize {
            return Err("module boundary splits a token".into());
        }
        merged.extend_from_slice(row);
    }
    if next != boundaries.len() {
        return Err("module boundary follows EOF".into());
    }
    let merged_tokens = RawInts {
        len: i64::try_from(merged.len()).map_err(|_| "token table too large")?,
        cap: merged.capacity() as i64,
        data: merged.as_mut_ptr(),
    };
    let ir =
        Buffer::own(unsafe { native_compile(&input, &merged_tokens, errors.0.as_ptr(), &target) })?;
    let errors = table(&errors, source)?;
    if !errors.is_empty() {
        let diagnostics = errors
            .as_chunks::<3>()
            .0
            .iter()
            .map(|row| {
                let message = match row[0] {
                    200 => "invalid syntax",
                    201 => "type or function signature mismatch",
                    202 => "unknown, duplicate or reserved name",
                    203 => "unsupported language feature",
                    204 => "compiler nesting limit exceeded",
                    205 => "invalid return path or unreachable statement",
                    206 => "integer literal is outside the signed 64-bit range",
                    207 => "fallible operation requires its status to be used",
                    208 => "task must be joined exactly once in its declaring block",
                    209 => "worker must be a global one-argument Bhasa function with supported non-task types and no reachable foreign calls or pooled scheduling/awaiting",
                    210 => "foreign printf declaration conflicts with runtime printing",
                    211 => "unknown record type",
                    212 => "invalid or duplicate record schema or name",
                    213 => "recursive record layout or record nesting limit exceeded",
                    214 => "overlapping record and field argument borrows",
                    215 => "invalid enum variant constructor or match pattern",
                    216 => "match must cover every enum variant exactly once",
                    217 => "float literal cannot be represented as finite binary64",
                    _ => return Err("frontend returned an unknown diagnostic code".to_owned()),
                };
                Ok(Diagnostic::new(
                    format!("BH{}", row[0]),
                    message,
                    span(source, row[1], row[2])?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(Err(diagnostics));
    }
    Ok(Ok(ir.text()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_native_dimensions_and_spans() {
        assert!(checked_length(-1, 0, 1).is_err());
        assert!(checked_length(2, 1, 1).is_err());
        assert!(checked_length(0, i64::MAX, 8).is_err());
        assert!(span("é", 1, 2).is_err());
        assert!(span("x", 0, 2).is_err());
        assert!(span("x", -1, 0).is_err());
        assert!(fixed_token(300, "var").is_none());
        assert!(fixed_token(10, " ").is_none());
    }
}
