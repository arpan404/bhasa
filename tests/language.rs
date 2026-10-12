use std::fs;

mod common;
use common::fixture;

fn accepted(source: &str) {
    if let Err(diagnostics) = bhasa::check(source) {
        panic!("valid source rejected:\n{source}\n{diagnostics:?}");
    }
}

fn rejected(source: &str, category: Option<char>) {
    let diagnostics = bhasa::check(source).expect_err(source);
    assert!(!diagnostics.is_empty(), "missing diagnostic for {source}");
    for diagnostic in diagnostics {
        assert!(!diagnostic.message.trim().is_empty());
        assert!(diagnostic.span.start <= diagnostic.span.end);
        assert!(diagnostic.span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.span.start));
        assert!(source.is_char_boundary(diagnostic.span.end));
        if let Some(prefix) = category {
            assert!(
                diagnostic.code.starts_with(prefix),
                "expected {prefix} diagnostic for {source}, got {diagnostic:?}"
            );
        }
    }
}

#[test]
fn every_basic_example_checks() {
    let mut count = 0;
    for entry in fs::read_dir(fixture("examples/basics")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) == Some("bh") {
            accepted(&fs::read_to_string(&path).unwrap());
            count += 1;
        }
    }
    assert!(count >= 7, "expected all basic examples to be present");
}

#[test]
fn invalid_fixtures_have_diagnostics() {
    for entry in fs::read_dir(fixture("tests/fixtures/invalid")).unwrap() {
        let path = entry.unwrap().path();
        rejected(&fs::read_to_string(path).unwrap(), None);
    }
}

#[test]
fn statement_terminators_comments_and_parenthesized_newlines() {
    accepted(
        "# comment\ndef add(a: int, b: int) -> int { return a + b }\n\
         def main() { var n = add(\n1,\n2\n); print(n) // comment\nreturn }",
    );
    accepted("def main() { print(1) }");
    accepted("");
    accepted("def library() -> int { return 42 }");
}

#[test]
fn lexical_errors_and_unsupported_features_fail_clearly() {
    for source in [
        "def main() { print(\"bad\\q\") }",
        "def main() { print(\"bad\0string\") }",
        "def main() { print(\"unterminated) }",
        "def main() { var café = 1 }",
        "def main() { print(@) }",
    ] {
        rejected(source, Some('L'));
    }
    for source in [
        "def main() { var n }",
        "def main() { print(1)",
        "def main() { var n = [1, 2] }",
        "import something",
        "def main() { for i in values {} }",
    ] {
        rejected(source, None);
    }
}

#[test]
fn oversized_literals_never_wrap() {
    for value in [
        "9223372036854775808",
        "18446744073709551616",
        "-9223372036854775809",
        "99999999999999999999999999999999999999999999",
    ] {
        rejected(&format!("def main() {{ print({value}) }}"), None);
    }
    let source = "def main() { print(-9223372036854775808) }";
    if let Err(diagnostics) = bhasa::check(source) {
        assert!(
            diagnostics.iter().any(|diagnostic| {
                let message = diagnostic.message.to_lowercase();
                ["range", "overflow", "64", "large"]
                    .iter()
                    .any(|word| message.contains(word))
            }),
            "minimum literal needs a clear range diagnostic: {diagnostics:?}"
        );
    }
}

#[test]
fn variable_scope_and_assignment_rules() {
    for source in [
        "def main() { print(missing) }",
        "def main() { missing = 1 }",
        "def main() { var n = n }",
        "def main() { var n = 1; var n = 2 }",
        "def main() { if true { var n = 1 }; print(n) }",
        "def main() { while false { var n = 1 }; print(n) }",
        "def main() { var n = 1; n = false }",
        "def main() { var n: int = false }",
        "def main() { var n: void = print(1) }",
        "def main() { var n = print(1) }",
    ] {
        rejected(source, Some('S'));
    }
    accepted("def main() { var n = 1; if true { var n = false; print(n) }; print(n) }");
}

#[test]
fn operator_types_and_conditions_are_strict() {
    for expression in [
        "true + false",
        "-true",
        "not 1",
        "1 and 2",
        "true or 1",
        "1 == true",
        "false < true",
        "\"a\" + \"b\"",
        "\"a\" == \"a\"",
        "\"a\" != \"b\"",
        "\"a\" < \"b\"",
    ] {
        rejected(&format!("def main() {{ print({expression}) }}"), Some('S'));
    }
    rejected("def main() { if 1 { print(1) } }", Some('S'));
    rejected("def main() { while 0 { print(1) } }", Some('S'));
}

#[test]
fn function_signatures_calls_and_returns_are_checked() {
    for source in [
        "def f() {}\ndef f() {}",
        "def f(n: int, n: int) {}",
        "def f(n: void) {}",
        "def f() -> int { return true }",
        "def f() -> int { return }",
        "def f() { return 1 }",
        "def main() { absent() }",
        "def f(n: int) {}\ndef main() { f() }",
        "def f(n: int) {}\ndef main() { f(1, 2) }",
        "def f(n: int) {}\ndef main() { f(true) }",
        "def f() {}\ndef main() { print(f()) }",
        "def main() { print() }",
        "def main() { print(1, 2) }",
        "def print(n: int) {}",
        "def main(n: int) {}",
        "def main() -> bool { return true }",
        "def main() -> str { return \"main\" }",
        "def main() {}\ndef main() {}",
    ] {
        rejected(source, Some('S'));
    }
    accepted("def main() -> int { return 7 }");
    accepted(
        "def even(n: int) -> bool { if n == 0 { return true } else { return odd(n - 1) } }\n\
              def odd(n: int) -> bool { if n == 0 { return false } else { return even(n - 1) } }",
    );
}

#[test]
fn missing_returns_and_unreachable_statements_are_rejected() {
    for source in [
        "def f() -> int {}",
        "def f(n: bool) -> int { if n { return 1 } }",
        "def f(n: bool) -> int { if n { return 1 } else { print(2) } }",
        "def main() { return; print(1) }",
        "def f(n: bool) -> int { if n { return 1 } else { return 2 }; print(3) }",
        "def f() -> int { return\n42 }",
    ] {
        rejected(source, Some('S'));
    }
    accepted("def f(n: bool) -> int { if n { return 1 } else { return 2 } }");
}

#[test]
fn extern_is_an_explicit_scalar_c_abi_boundary() {
    accepted(
        "extern def twice(n: int) -> int\nextern def consume(n: int)\n\
              def main() { consume(twice(21)) }",
    );
    for source in [
        "extern def main()",
        "extern def print(n: int)",
        "extern def f(n: bool) -> int",
        "extern def f(n: str) -> int",
        "extern def f() -> bool",
        "extern def f() -> str",
        "extern def f() -> int { return 1 }",
        "extern def f(n: int) -> int\nextern def f(n: int) -> int",
        "extern def f(n: int) -> int\ndef f(n: int) -> int { return n }",
    ] {
        rejected(source, None);
    }
}

#[test]
fn library_wrappers_emit_ir_and_propagate_errors() {
    let source = "def square(n: int) -> int { return n * n }";
    assert!(bhasa::parse(source).is_ok());
    let ir = bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu").unwrap();
    assert!(ir.contains("target triple = \"x86_64-unknown-linux-gnu\""));
    assert!(ir.contains("define "));
    assert!(
        bhasa::compile_to_llvm("def main() { print(missing) }", "x86_64-unknown-linux-gnu")
            .is_err()
    );
}
