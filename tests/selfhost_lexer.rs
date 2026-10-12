#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;

use bhasa::lexer::{lex_rust as lex, TokenKind};
use common::*;
use std::fs;
use std::process::Command;

fn token_code(kind: &TokenKind) -> i64 {
    use TokenKind::*;
    match kind {
        Eof => 0,
        Ident(_) => 1,
        Int(_) => 2,
        Float(_) => 4,
        Str(_) => 3,
        Def => 300,
        Extern => 301,
        Var => 302,
        If => 303,
        Else => 304,
        While => 305,
        For => 319,
        In => 320,
        Break => 321,
        Continue => 322,
        Return => 306,
        True => 307,
        False => 308,
        IntType => 309,
        FloatType => 318,
        BoolType => 310,
        StrType => 311,
        VoidType => 312,
        And => 260,
        Or => 261,
        Not => 33,
        LeftParen => 40,
        RightParen => 41,
        LeftBracket => 91,
        RightBracket => 93,
        LeftBrace => 123,
        RightBrace => 125,
        Comma => 44,
        Colon => 58,
        Semicolon => 59,
        Struct => 313,
        // 314 is reserved for host-inserted source-file boundaries.
        Enum => 315,
        Match => 316,
        Case => 317,
        Dot => 46,
        Newline => 10,
        Arrow => 262,
        Plus => 43,
        Minus => 45,
        Star => 42,
        Slash => 47,
        Percent => 37,
        Equal => 61,
        EqualEqual => 256,
        NotEqual => 257,
        Less => 60,
        LessEqual => 258,
        Greater => 62,
        GreaterEqual => 259,
    }
}

const DRIVER: &str = r#"
def main() -> int {
    if arg_count() != 2 { return 2 }
    var source = buffer()
    var status = read_file(arg(1), source)
    if status != 0 { return 3 }
    var errors = ints()
    var tokens = bh_lex(source, errors)
    if len(errors) != 0 {
        var index = 0
        while index < len(errors) { print(get(errors, index)); index = index + 1 }
        return 1
    }
    var index = 0
    while index < len(tokens) / 3 {
        print(bh_token_kind(tokens, index))
        print(bh_token_start(tokens, index))
        print(bh_token_end(tokens, index))
        if bh_token_kind(tokens, index) == 3 {
            var decoded = bh_string_value(source, tokens, index)
            print(len(decoded))
            var byte = 0
            while byte < len(decoded) { print(get(decoded, byte)); byte = byte + 1 }
        }
        index = index + 1
    }
    return 0
}
"#;

#[test]
fn bhasa_scanner_matches_bootstrap_tokens_spans_and_decoded_strings() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-lexer");
    let scanner = fs::read_to_string(fixture("bootstrap/compiler/lexer.bh")).unwrap();
    let program = workspace.write("scanner.bh", &format!("{scanner}\n{DRIVER}"));
    let mut valid = vec![
        "".to_owned(),
        "def extern var if else while for in break continue return true false int float bool str void struct enum match case buffer ints and or not def2 _if enum2 match_value case2".into(),
        "()[]{} ,:; -> + - * / % = == != ! < <= > >= && ||".into(),
        "(\r\n1\n+\r2)\r# comment\r\n// comment\n 3\t\u{b}\u{c}".into(),
        "\"भाषा 😀 café\" \"\\n\\r\\t\\\\\\\"\"".into(),
        "0 9223372036854775808 9999999999999999999999999999999999999".into(),
        "# unicode: भाषा 😀\n// comment\r\n)\n12".into(),
        "0.1 1e3 1E+3 25e-2 0002.500 1.0e0 1.member".into(),
        scanner.clone(),
    ];
    for name in [
        "examples/basics/hello.bh",
        "examples/basics/loops.bh",
        "examples/basics/strings.bh",
        "examples/basics/recursion.bh",
        "examples/rust-interop/main.bh",
        "bootstrap/lexer.bh",
        "bootstrap/lexer-tests.bh",
    ] {
        valid.push(fs::read_to_string(fixture(name)).unwrap());
    }
    let invalid = [
        "0xFF 12suffix 12_3",
        "1e 1e+ 1e- 1.0e 1e3f 1.0_1 1e+2_3",
        "\"bad\\q\"",
        "\"unterminated",
        "\"raw\nline\r\nbreak\"",
        "\"escaped\\\nline\\\r\nbreak\"",
        "& | é 😀",
        "\0#\0comment\n\"\0\\\0\"",
    ];
    for release in [false, true] {
        let executable = build(&workspace, &program, release);
        for source in &valid {
            let input = workspace.write("input with spaces.bh", source);
            let output = Command::new(&executable).arg(input).output().unwrap();
            succeeds(&output);
            assert!(output.stderr.is_empty(), "{}", describe(&output));
            let actual: Vec<i64> = stdout(&output)
                .lines()
                .map(|line| line.parse().unwrap())
                .collect();
            let mut expected = Vec::new();
            for token in lex(source).unwrap() {
                expected.extend([
                    token_code(&token.kind),
                    token.span.start as i64,
                    token.span.end as i64,
                ]);
                if let TokenKind::Str(text) = token.kind {
                    expected.push(text.len() as i64);
                    expected.extend(text.bytes().map(i64::from));
                }
            }
            assert_eq!(actual, expected, "source: {source}");
        }
        for source in invalid {
            let input = workspace.write("invalid.bh", source);
            let output = Command::new(&executable).arg(input).output().unwrap();
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            let actual: Vec<i64> = stdout(&output)
                .lines()
                .map(|line| line.parse().unwrap())
                .collect();
            let mut expected = Vec::new();
            for error in lex(source).unwrap_err() {
                expected.extend([
                    error.code.trim_start_matches('L').parse::<i64>().unwrap(),
                    error.span.start as i64,
                    error.span.end as i64,
                ]);
            }
            assert_eq!(actual, expected, "source: {source:?}");
        }
        for bytes in [
            vec![0xc0, 0x80],
            vec![0xed, 0xa0, 0x80],
            vec![0xf4, 0x90, 0x80, 0x80],
            vec![0xe2, 0x82],
        ] {
            let input = workspace.file("invalid-utf8.bh");
            fs::write(&input, bytes).unwrap();
            let output = Command::new(&executable).arg(input).output().unwrap();
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            let values: Vec<i64> = stdout(&output)
                .lines()
                .map(|line| line.parse().unwrap())
                .collect();
            assert!(!values.is_empty());
            assert!(values.as_chunks::<3>().0.iter().all(|error| error[0] == 7));
        }
    }
}
