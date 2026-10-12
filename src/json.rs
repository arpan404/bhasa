//! Small, strict JSON codec for the dependency-free language server.
use std::collections::BTreeMap;
use std::fmt;

const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(values) => values.get(key),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => value.parse().ok(),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }
    pub fn object<const N: usize>(fields: [(&str, Value); N]) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self::Number(value.to_string())
    }
}
impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Number(value.to_string())
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) => f.write_str(value),
            Self::String(value) => f.write_str(&crate::diagnostic::json_string(value)),
            Self::Array(values) => {
                f.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{value}")?;
                }
                f.write_str("]")
            }
            Self::Object(values) => {
                f.write_str("{")?;
                for (index, (key, value)) in values.iter().enumerate() {
                    if index != 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{}:{value}", crate::diagnostic::json_string(key))?;
                }
                f.write_str("}")
            }
        }
    }
}

pub fn parse(source: &str) -> Result<Value, String> {
    let mut parser = Parser { source, offset: 0 };
    let value = parser.value(0)?;
    parser.space();
    if parser.offset != source.len() {
        return Err(parser.error("trailing data"));
    }
    Ok(value)
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
}
impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.offset)
    }
    fn byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }
    fn space(&mut self) {
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        if self.byte() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.take(byte) {
            Ok(())
        } else {
            Err(self.error("unexpected JSON token"))
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("JSON nesting exceeds 64 levels"));
        }
        self.space();
        match self.byte() {
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => {
                self.offset += 1;
                self.space();
                let mut values = Vec::new();
                if !self.take(b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        self.space();
                        if self.take(b']') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Value::Array(values))
            }
            Some(b'{') => {
                self.offset += 1;
                self.space();
                let mut values = BTreeMap::new();
                if !self.take(b'}') {
                    loop {
                        self.space();
                        let key = self.string()?;
                        self.space();
                        self.expect(b':')?;
                        let value = self.value(depth + 1)?;
                        if values.insert(key, value).is_some() {
                            return Err(self.error("duplicate object key"));
                        }
                        self.space();
                        if self.take(b'}') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Value::Object(values))
            }
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a JSON value")),
        }
    }
    fn literal(&mut self, literal: &str, value: Value) -> Result<Value, String> {
        if !self.source[self.offset..].starts_with(literal) {
            return Err(self.error("invalid literal"));
        }
        self.offset += literal.len();
        Ok(value)
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.offset;
        self.take(b'-');
        if !self.take(b'0') {
            if !matches!(self.byte(), Some(b'1'..=b'9')) {
                return Err(self.error("invalid number"));
            }
            self.digits();
        }
        if self.take(b'.') {
            if !matches!(self.byte(), Some(b'0'..=b'9')) {
                return Err(self.error("missing fraction"));
            }
            self.digits();
        }
        if self.take(b'e') || self.take(b'E') {
            if !self.take(b'+') {
                self.take(b'-');
            }
            if !matches!(self.byte(), Some(b'0'..=b'9')) {
                return Err(self.error("missing exponent"));
            }
            self.digits();
        }
        Ok(Value::Number(self.source[start..self.offset].to_owned()))
    }
    fn digits(&mut self) {
        while matches!(self.byte(), Some(b'0'..=b'9')) {
            self.offset += 1;
        }
    }
    fn hex(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = self
                .byte()
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or_else(|| self.error("invalid Unicode escape"))?;
            self.offset += 1;
            value = value * 16 + digit;
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut output = String::new();
        loop {
            match self.byte() {
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(output);
                }
                Some(b'\\') => {
                    self.offset += 1;
                    let byte = self
                        .byte()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.offset += 1;
                    output.push(match byte {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let first = self.hex()?;
                            let code = if (0xd800..=0xdbff).contains(&first) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let second = self.hex()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err(self.error("invalid surrogate pair"));
                                }
                                0x10000 + (first - 0xd800) * 0x400 + second - 0xdc00
                            } else {
                                first
                            };
                            char::from_u32(code)
                                .ok_or_else(|| self.error("unpaired Unicode surrogate"))?
                        }
                        _ => return Err(self.error("invalid string escape")),
                    });
                }
                Some(0..=31) => return Err(self.error("unescaped control character")),
                Some(_) => {
                    let ch = self.source[self.offset..].chars().next().unwrap();
                    self.offset += ch.len_utf8();
                    output.push(ch);
                }
                None => return Err(self.error("unterminated string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_numbers_and_escaping_round_trip() {
        let value =
            parse(r#"{"a":[null,true,false,-1.5e+12,0],"s":"\uD83C\uDF0D\n\t\u0000ह"}"#).unwrap();
        assert_eq!(value.get("s").and_then(Value::as_str), Some("🌍\n\t\0ह"));
        assert_eq!(parse(&value.to_string()).unwrap(), value);
    }
    #[test]
    fn malformed_and_deep_values_are_rejected() {
        for input in [
            "",
            "01",
            "-",
            "[1,]",
            "{\"x\":0,\"x\":1}",
            "1e",
            "true false",
            r#""\ud800""#,
            r#""\udc00""#,
            "\"\n\"",
            "{\"x\":}",
        ] {
            assert!(parse(input).is_err(), "accepted {input}");
        }
        assert!(parse(&format!("{}0{}", "[".repeat(65), "]".repeat(65))).is_err());
    }
}
