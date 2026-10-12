mod common;

use common::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn seed_source() -> String {
    ["lexer.bh", "emitter.bh", "driver.bh"]
        .into_iter()
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed(workspace: &Workspace) -> (PathBuf, PathBuf) {
    let source = workspace.write("seed.bh", &seed_source());
    let executable = build(workspace, &source, true);
    (source, executable)
}

fn emit(seed: &Path, source: &Path, target: &str) -> Output {
    Command::new(seed)
        .arg("emit-llvm")
        .arg(source)
        .args(["--target", target])
        .output()
        .unwrap()
}

fn target(clang: &std::ffi::OsStr, workspace: &Workspace) -> String {
    let source = workspace.write("empty.c", "");
    let output = Command::new(clang)
        .args(["-S", "-emit-llvm", "-x", "c"])
        .arg(source)
        .args(["-o", "-"])
        .output()
        .unwrap();
    succeeds(&output);
    stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("target triple = \"")?.strip_suffix('"'))
        .unwrap()
        .to_owned()
}

#[test]
fn seed_rejects_invalid_programs_with_bounded_diagnostics() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("seed-diagnostics");
    let (_, executable) = seed(&workspace);
    let mut invalid: Vec<(String, &str)> = vec![
        ("def main() { print(missing) }".into(), "BH202"),
        ("def main() { var x: int = true }".into(), "BH201"),
        ("def main() { var x = 1; var x = 2 }".into(), "BH202"),
        ("def main() { print(9223372036854775808) }".into(), "BH206"),
        ("def main() { print(-9223372036854775809) }".into(), "BH206"),
        ("def main() -> int { if true { return 1 } }".into(), "BH205"),
        ("def main() { return; print(1) }".into(), "BH205"),
        (
            "def replace(value: buffer) { value = buffer() }".into(),
            "BH201",
        ),
        ("def main() { write_stdout(buffer()) }".into(), "BH207"),
        (
            "extern def printf(n: int) -> int\ndef main() { print(1) }".into(),
            "BH210",
        ),
        (
            "def main() { var value = gpu_available(1) }".into(),
            "BH201",
        ),
        (
            "def worker(n: int) -> int { return n }\ndef main() { var job = spawn(worker(1)) }"
                .into(),
            "BH208",
        ),
        ("def main() { var value = buffer(1) }".into(), "BH201"),
        ("def main() { print(1, 2) }".into(), "BH201"),
        ("def main() { var x = 1; x() }".into(), "BH202"),
        ("def main() { if true {".into(), "BH200"),
    ];
    invalid.push((
        format!(
            "def main() {{ print({}1{}) }}",
            "(".repeat(300),
            ")".repeat(300)
        ),
        "BH204",
    ));
    invalid.push((
        format!("def main() {{ print({}true) }}", "not ".repeat(300)),
        "BH204",
    ));
    invalid.push((
        format!(
            "def main() {{ {}print(1){} }}",
            "if true {".repeat(300),
            "}".repeat(300)
        ),
        "BH204",
    ));
    invalid.push((
        format!(
            "def main() {{ {}print({}1{}){} }}",
            "if true {".repeat(80),
            "(".repeat(80),
            ")".repeat(80),
            "}".repeat(80)
        ),
        "BH204",
    ));
    invalid.push((
        format!("def main() {{ print({}) }}", "9".repeat(10_000)),
        "BH206",
    ));
    for (source, error) in invalid {
        let input = workspace.write("invalid.bh", &source);
        let output = emit(&executable, &input, "x86_64-unknown-linux-gnu");
        assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
        assert!(stderr(&output).contains(error), "{}", describe(&output));
        assert!(output.stdout.is_empty(), "failed compilation emitted LLVM");
    }
    for source in [
        "def",
        "def f",
        "def f(",
        "def f(x",
        "def f(x:",
        "def f(x: int",
        "def f(x: int) ->",
        "def main() {",
        "def main() { print( }",
        "def main() { print(1 +) }",
        "def main() { if true }",
        "def main() { if }",
        "def main() { var x = }",
        "def main() { var x: }",
    ] {
        let input = workspace.write("truncated.bh", source);
        let output = emit(&executable, &input, "x86_64-unknown-linux-gnu");
        assert_eq!(
            output.status.code(),
            Some(1),
            "{source}\n{}",
            describe(&output)
        );
        assert!(
            stderr(&output).contains("error BH"),
            "{}",
            describe(&output)
        );
        assert!(output.stdout.is_empty());
    }
    let long = workspace.write(
        "long.bh",
        &format!("def main() {{ print(0{}) }}", " + 1".repeat(2_000)),
    );
    // Match the Rust frontend's expression-tree bound even for flat chains
    // that do not grow the direct emitter's parser call stack.
    let output = emit(&executable, &long, "x86_64-unknown-linux-gnu");
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(stderr(&output).contains("BH204"), "{}", describe(&output));
    assert!(output.stdout.is_empty());
    let output = emit(&executable, &long, "bad\"\ninjection");
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(output.stdout.is_empty());
}

#[test]
fn seed_rejects_malformed_token_tables_at_its_api_boundary() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("seed-token-tables");
    let scanner = fs::read_to_string(fixture("bootstrap/compiler/lexer.bh")).unwrap();
    let emitter = fs::read_to_string(fixture("bootstrap/compiler/emitter.bh")).unwrap();
    let source = workspace.write(
        "token-tables.bh",
        &format!(
            r#"{scanner}
{emitter}
def attempt(tokens: ints) {{
    var errors = ints()
    var output = bh_compile(buffer("x"), tokens, errors, buffer("x86_64-unknown-linux-gnu"))
    print(len(output)); print(len(errors)); print(get(errors, 0))
}}
def main() {{
    var tokens = ints(); attempt(tokens)
    push(tokens, 1); attempt(tokens)
    push(tokens, 0); push(tokens, 1); attempt(tokens)
    push(tokens, 0); push(tokens, 1); push(tokens, 2); attempt(tokens)
    set(tokens, 5, 1); set(tokens, 1, -1); attempt(tokens)
    set(tokens, 1, 0); set(tokens, 0, 0); attempt(tokens)
}}
"#
        ),
    );
    let executable = build(&workspace, &source, true);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "0\n3\n200\n".repeat(6));
    assert!(output.stderr.is_empty());
}

#[test]
fn host_file_boundaries_separate_complete_declarations_without_joining_syntax() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("seed-file-boundaries");
    let scanner = fs::read_to_string(fixture("bootstrap/compiler/lexer.bh")).unwrap();
    let emitter = fs::read_to_string(fixture("bootstrap/compiler/emitter.bh")).unwrap();
    let accepted = [
        ["", "", ""],
        ["", "def main() {}", ""],
        ["# comment-only file", "def main() {}", "# trailing comment"],
        [
            "def first() {} # comment at EOF",
            "def main() { first() } # another comment",
            "",
        ],
        ["def first() {}", "", "def main() { first() }"],
        [
            "def first() {}",
            "def second() {}",
            "def main() { first(); second() }",
        ],
        [
            "def main() { print(later()) }",
            "",
            "def later() -> int { return 42 }",
        ],
        [
            "def make() -> Point { return Point(4) }",
            "struct Point { x: int }",
            "def main() { print(make().x) }",
        ],
        [
            "struct Outer { point: Point }",
            "struct Point { x: int }",
            "def main() { var value = Outer(Point(3)); print(value.point.x) }",
        ],
        // A file EOF completes an extern even when it has no trailing newline.
        [
            "extern def foreign(value: int) -> int",
            "def main() { print(foreign(1)) }",
            "",
        ],
        [
            "extern def notify(value: int)",
            "",
            "def main() { notify(1) }",
        ],
        [
            "extern def first() -> int",
            "extern def second()",
            "def main() { second(); print(first()) }",
        ],
    ];
    let rejected = [
        ["def ", "split() {}", ""],
        ["def split", "() {}", ""],
        ["def split(", "value: int) {}", ""],
        ["def split(value", ": int) {}", ""],
        ["def split(value:", " int) {}", ""],
        ["def split(value: int", ") {}", ""],
        ["def split(value: int,", " flag: bool) {}", ""],
        ["def split()", " {}", ""],
        ["def split() ->", " int { return 1 }", ""],
        ["def split() -> int", " { return 1 }", ""],
        ["extern ", "def foreign(value: int) -> int", ""],
        ["extern def foreign(", "value: int) -> int", ""],
        ["extern def foreign(value:", " int) -> int", ""],
        ["extern def foreign() ->", " int", ""],
        ["struct ", "Point { x: int }", ""],
        ["struct Point", " { x: int }", ""],
        ["struct Point {", " x: int }", ""],
        ["struct Point { x", ": int }", ""],
        ["struct Point { x:", " int }", ""],
        ["struct Point { x: int", " }", ""],
        ["struct Point { x: int;", " y: int }", ""],
        ["def main() { print(1)", " }", ""],
        ["def main() { print(", "1) }", ""],
        ["def main() { var value =", " 1; print(value) }", ""],
        ["def main() { print(1 +", " 2) }", ""],
        ["def main() { if true { print(1)", " } }", ""],
        ["def main() { if true {}", " else { print(1) } }", ""],
        ["def main() { return", "; }", ""],
        [
            "struct Point { x: int } def read(p: Point) ->",
            " int { return p.x }",
            "",
        ],
        ["", "def main() {", " }"],
        ["def first() {}", "def main() { first()", " }"],
    ];
    let mut calls = String::new();
    let mut expected = Vec::new();
    for parts in accepted {
        let id = expected.len();
        calls.push_str(&format!(
            "    attempt({id}, {:?}, {:?}, {:?})\n",
            parts[0], parts[1], parts[2]
        ));
        expected.push((parts.concat(), None));
    }
    for parts in rejected {
        let flat = parts.concat();
        let reference = bhasa::compile_to_llvm(&flat, "x86_64-unknown-linux-gnu");
        assert!(
            reference.is_ok(),
            "boundary regression must have valid flat syntax: {flat}\n{reference:?}"
        );
        // The first two files are complete only in the final two cases.
        // Locate the incomplete file without mistaking an empty file for it.
        let offset = if parts[0].is_empty() || parts[0] == "def first() {}" {
            parts[0].len() + parts[1].len()
        } else {
            parts[0].len()
        };
        let id = expected.len();
        calls.push_str(&format!(
            "    attempt({id}, {:?}, {:?}, {:?})\n",
            parts[0], parts[1], parts[2]
        ));
        expected.push((flat, Some((offset, offset))));
    }
    for (kind, start, end) in [(0, 0, 1), (1, 1, 2), (2, 0, 0), (3, 0, 0)] {
        let id = expected.len();
        calls.push_str(&format!("    malformed({id}, {kind})\n"));
        expected.push((
            format!("malformed boundary table {kind}"),
            Some((start, end)),
        ));
    }
    let source = workspace.write("file-boundaries.bh", &format!(r#"{scanner}
{emitter}
def add_file(source: buffer, tokens: ints, text: str) -> int {{
    var part = buffer(text)
    var errors = ints()
    var chunk = bh_lex(part, errors)
    if len(errors) != 0 {{ return 1 }}
    var offset = len(source)
    append(source, part)
    var index = 0
    while index + 1 < len(chunk) / 3 {{
        bh_triple(tokens, bh_token_kind(chunk, index), offset + bh_token_start(chunk, index), offset + bh_token_end(chunk, index))
        index = index + 1
    }}
    bh_triple(tokens, 314, len(source), len(source))
    return 0
}}
def report(id: int, source: buffer, tokens: ints) {{
    var errors = ints()
    var output = bh_compile(source, tokens, errors, buffer("x86_64-unknown-linux-gnu"))
    print(id); print(len(output)); print(len(errors))
    if len(errors) == 0 {{ print(0); print(0); print(0) }} else {{
        print(get(errors, 0)); print(get(errors, 1)); print(get(errors, 2))
    }}
}}
def attempt(id: int, first: str, second: str, third: str) {{
    var source = buffer()
    var tokens = ints()
    var failures = add_file(source, tokens, first) + add_file(source, tokens, second) + add_file(source, tokens, third)
    if failures != 0 {{ print("unexpected scanner error"); return }}
    bh_triple(tokens, 0, len(source), len(source))
    report(id, source, tokens)
}}
def malformed(id: int, kind: int) {{
    var source = buffer("  ")
    var tokens = ints()
    if kind == 0 {{
        bh_triple(tokens, 314, 0, 1)
        bh_triple(tokens, 0, 2, 2)
    }}
    if kind == 1 {{
        bh_triple(tokens, 314, 0, 0)
        bh_triple(tokens, 314, 1, 2)
        bh_triple(tokens, 0, 2, 2)
    }}
    if kind == 2 {{
        bh_triple(tokens, 314, 0, 0)
        bh_triple(tokens, 0, 0, 0)
        bh_triple(tokens, 314, 2, 2)
        bh_triple(tokens, 0, 2, 2)
    }}
    if kind == 3 {{ bh_triple(tokens, 314, 2, 2) }}
    report(id, source, tokens)
}}
def main() {{
{calls}}}
"#));
    let executable = build(&workspace, &source, true);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    let values = stdout(&output)
        .lines()
        .map(|line| line.parse::<usize>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), expected.len() * 6, "{}", describe(&output));
    let mut span_failures = Vec::new();
    for (index, ((source, error_span), values)) in expected
        .into_iter()
        .zip(values.as_chunks::<6>().0.iter())
        .enumerate()
    {
        assert_eq!(values[0], index, "case order: {source}");
        if let Some((start, end)) = error_span {
            let expected = [0, 3, 200, start, end];
            if values[1..] != expected {
                span_failures.push(format!(
                    "case {index}: {source}\nexpected {expected:?}, got {:?}",
                    &values[1..]
                ));
            }
        } else {
            assert!(values[1] > 0, "complete files emitted no LLVM: {source}");
            assert_eq!(
                &values[2..],
                &[0, 0, 0, 0],
                "complete files rejected: {source}"
            );
        }
    }
    assert!(span_failures.is_empty(), "{}", span_failures.join("\n"));
}

fn tracked_runtime(workspace: &Workspace) -> PathBuf {
    fs::copy(fixture("runtime/core.h"), workspace.file("core.h")).unwrap();
    workspace.write(
        "tracked.c",
        &format!(
            r#"
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) {{
    if (live != 0) {{ fprintf(stderr, "live Bhasa allocations: %ld\n", live); _Exit(99); }}
}}
static void *tracked_malloc(size_t n) {{
    if (!registered) {{ registered = 1; atexit(check_live); }}
    void *p = malloc(n); if (p) ++live; return p;
}}
static void *tracked_realloc(void *p, size_t n) {{
    int was_null = p == NULL;
    void *q = realloc(p, n); if (q && was_null) ++live; return q;
}}
static void tracked_free(void *p) {{ if (p) --live; free(p); }}
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
{}
"#,
            include_str!("../runtime/core.c")
        ),
    )
}

fn link(
    clang: &std::ffi::OsStr,
    workspace: &Workspace,
    ir: &[u8],
    runtime: &Path,
    name: &str,
    optimization: &str,
) -> PathBuf {
    let ir_path = workspace.file(&format!("{name}.ll"));
    fs::write(&ir_path, ir).unwrap();
    let executable = workspace.file(name);
    let output = Command::new(clang)
        .args([
            optimization,
            "-pthread",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
        ])
        .arg(ir_path)
        .arg(runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    succeeds(&output);
    executable
}

#[test]
fn seed_rebuild_and_owned_values_are_clean_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("seed-ownership");
    let (source, a) = seed(&workspace);
    let triple = target(&clang, &workspace);
    let b_ir = emit(&a, &source, &triple);
    succeeds(&b_ir);
    let runtime = tracked_runtime(&workspace);
    let b = link(&clang, &workspace, &b_ir.stdout, &runtime, "B", "-O2");
    let c_ir = emit(&b, &source, &triple);
    succeeds(&c_ir);
    assert_eq!(b_ir.stdout, c_ir.stdout);
    assert!(c_ir.stderr.is_empty(), "{}", describe(&c_ir));
    let invalid = workspace.write(
        "invalid.bh",
        "def main() { var data = buffer(); if true { return 1 } }",
    );
    let failed = emit(&b, &invalid, &triple);
    assert_eq!(failed.status.code(), Some(1), "{}", describe(&failed));
    assert!(!stderr(&failed).contains("live Bhasa allocations"));
    let ownership = workspace.write("ownership.bh", r#"
def changed(value: buffer) -> buffer { append(value, "!"); return value }
def early(value: buffer, choose: bool) -> buffer {
    var local = slice(value, 0, len(value))
    if choose { var nested = buffer("temporary"); return local }
    return buffer("fallback")
}
def main() {
    var original = buffer("hello")
    var copied = original
    set(copied, 0, 72)
    print(original); print(copied)
    var returned = changed(original)
    push(returned, 63)
    print(original); print(returned)
    original = original
    print(early(original, true))
    var values = ints(); push(values, -9223372036854775808); push(values, 42)
    var independent = values; set(independent, 1, 7)
    print(get(values, 1)); print(get(independent, 1))
    var i = 0
    while i < 50 { var transient = slice(original, 0, 3); transient = early(transient, false); i = i + 1 }
    print(same(to_buffer(get(values, 0)), "-9223372036854775808"))
    print(false and (1 / 0 == 1)); print(true or (9223372036854775807 + 1 == 1))
    buffer("discarded temporary")
}
"#);
    let ir = emit(&b, &ownership, &triple);
    succeeds(&ir);
    for optimization in ["-O0", "-O2"] {
        let executable = link(
            &clang,
            &workspace,
            &ir.stdout,
            &runtime,
            "ownership",
            optimization,
        );
        let output = execute(&executable, &workspace);
        succeeds(&output);
        assert_eq!(
            stdout(&output),
            "hello\nHello\nhello!\nhello!?\nhello!\n42\n7\ntrue\nfalse\ntrue\n"
        );
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
    for expression in [
        "9223372036854775807 + 1",
        "-(-9223372036854775808)",
        "1 / 0",
        "-9223372036854775808 % -1",
    ] {
        let source = workspace.write(
            "trap.bh",
            &format!("def main() {{ print({expression}); print(\"survived\") }}"),
        );
        let ir = emit(&b, &source, &triple);
        succeeds(&ir);
        let executable = link(&clang, &workspace, &ir.stdout, &runtime, "trap", "-O2");
        let output = execute(&executable, &workspace);
        assert!(!output.status.success());
        assert!(!stdout(&output).contains("survived"));
    }
}
