#![cfg(feature = "bhasa-lexer")]

use bhasa::lexer::{lex, lex_rust};
use std::fs;
use std::path::Path;

fn compare(source: &str) {
    assert_eq!(lex(source), lex_rust(source), "source: {source:?}");
}

#[test]
fn hybrid_scanner_preserves_the_repository_corpus_and_diagnostics() {
    fn visit(path: &Path) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "build" {
                    visit(&path);
                }
            } else if path.extension().is_some_and(|extension| extension == "bh") {
                compare(&fs::read_to_string(path).unwrap());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    visit(&root.join("examples"));
    visit(&root.join("bootstrap"));
    visit(&root.join("tests/fixtures"));
    for source in [
        "\0#\0comment\n\"\0\\\0\"",
        "\"unterminated\\",
        "\"bad\\q\\é\" & | 😀",
        "0xFF 12abc 12_3",
        "\"raw\nline\r\nbreak\"",
        "\"escaped\\\nline\\\r\nbreak\"",
        "(\r\n1\n+\r2)\r# comment\r\n// comment\n)\n3\t\u{b}\u{c}",
    ] {
        compare(source);
    }
}

#[test]
fn hybrid_scanner_matches_rust_on_generated_malformed_inputs() {
    let alphabet = [
        'a', 'Z', '_', '0', '1', '9', ' ', '\t', '\r', '\n', '\u{b}', '\u{c}', '#', '/', '"', '\\',
        'n', 'r', 't', '&', '|', '!', '=', '<', '>', '-', '+', '*', '%', '(', ')', '{', '}', ',',
        ':', ';', '\0', 'é', 'भ', '😀',
    ];
    let mut state = 0x324fc738ab21965du64;
    for length in 0..160 {
        for _ in 0..8 {
            let mut source = String::new();
            for _ in 0..length {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                source.push(alphabet[(state >> 32) as usize % alphabet.len()]);
            }
            compare(&source);
        }
    }
}

#[test]
fn hybrid_scanner_handles_parallel_compiler_requests() {
    let threads = (0..8)
        .map(|index| {
            std::thread::spawn(move || {
                for value in 0..50 {
                    compare(&format!(
                        "def f_{index}(n: int) -> int {{ return n + {value} }}\n# भाषा"
                    ));
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}
