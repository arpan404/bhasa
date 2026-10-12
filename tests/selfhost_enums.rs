#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;

use common::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn seed(workspace: &Workspace) -> (PathBuf, PathBuf) {
    let contents = ["lexer.bh", "emitter.bh", "driver.bh"]
        .into_iter()
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let source = workspace.write("compiler.bh", &contents);
    let executable = build(workspace, &source, true);
    (source, executable)
}

fn bounded(mut command: Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        err.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break (child.wait().unwrap(), true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: out_reader.join().unwrap(),
        stderr: err_reader.join().unwrap(),
    };
    assert!(
        !timed_out,
        "subprocess exceeded 60 seconds: {}",
        describe(&output)
    );
    output
}

fn emit(compiler: &Path, source: &Path, target: &str) -> Output {
    let mut command = Command::new(compiler);
    command
        .arg("emit-llvm")
        .arg(source)
        .args(["--target", target]);
    bounded(command)
}

fn host_target(clang: &std::ffi::OsStr, workspace: &Workspace) -> String {
    let output = Command::new(clang)
        .args(["-S", "-emit-llvm", "-x", "c"])
        .arg(workspace.write("empty.c", ""))
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

fn diagnostic(output: &Output, source: &str) {
    assert_eq!(
        output.status.code(),
        Some(1),
        "{source}\n{}",
        describe(output)
    );
    assert!(
        output.stdout.is_empty(),
        "invalid program emitted LLVM: {source}"
    );
    let errors = stderr(output);
    assert!(!errors.is_empty(), "missing diagnostic: {source}");
    for line in errors.lines() {
        let (code, span) = line
            .split_once(" at byte ")
            .unwrap_or_else(|| panic!("{line}"));
        let code: u32 = code.strip_prefix("error BH").unwrap().parse().unwrap();
        assert!(code > 0);
        let (start, end) = span.split_once("..").unwrap();
        let start: usize = start.parse().unwrap();
        let end: usize = end.parse().unwrap();
        assert!(start <= end && end <= source.len(), "{line}: {source}");
        assert!(
            source.is_char_boundary(start) && source.is_char_boundary(end),
            "{line}: {source}"
        );
    }
}

fn tracked_runtime(workspace: &Workspace) -> Vec<PathBuf> {
    let tracker = workspace.write(
        "tracker.c",
        r#"
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) {
    if (live != 0) { fprintf(stderr, "live Bhasa allocations: %ld\n", live); _Exit(99); }
}
void *bhasa_test_malloc(size_t n) {
    if (!registered) { registered = 1; atexit(check_live); }
    void *p = malloc(n); if (p) ++live; return p;
}
void *bhasa_test_realloc(void *p, size_t n) {
    int was_null = p == NULL;
    void *q = realloc(p, n); if (q && was_null) ++live; return q;
}
void bhasa_test_free(void *p) { if (p) --live; free(p); }
"#,
    );
    let mut sources = vec![tracker];
    for name in ["core", "net", "text"] {
        sources.push(workspace.write(
            &format!("tracked-{name}.c"),
            &format!(
                r#"
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <stdlib.h>
void *bhasa_test_malloc(size_t);
void *bhasa_test_realloc(void *, size_t);
void bhasa_test_free(void *);
#define malloc bhasa_test_malloc
#define realloc bhasa_test_realloc
#define free bhasa_test_free
#include "{}"
"#,
                fixture(&format!("runtime/{name}.c")).display()
            ),
        ));
    }
    sources
}

fn link(
    clang: &std::ffi::OsStr,
    workspace: &Workspace,
    ir: &str,
    runtime: &[PathBuf],
    name: &str,
    optimization: &str,
) -> PathBuf {
    let ir = ir
        .replace("@malloc(", "@bhasa_test_malloc(")
        .replace("@realloc(", "@bhasa_test_realloc(")
        .replace("@free(", "@bhasa_test_free(");
    // The sanitizer flag does not instrument loads/stores in existing IR.
    // Include generated record helpers and rebuilt compiler functions.
    let ir = ir
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                assert!(line.ends_with(" {"), "unexpected LLVM definition: {line}");
                format!("{} sanitize_address {{\n", &line[..line.len() - 2])
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    let ir_path = workspace.write(&format!("{name}.ll"), &ir);
    let executable = workspace.file(name);
    let output = Command::new(clang)
        .args([
            optimization,
            "-pthread",
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-fno-omit-frame-pointer",
        ])
        .arg(ir_path)
        .args(runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    succeeds(&output);
    executable
}

const OWNERSHIP: &str = r#"
enum OptionInt { none; some(value: int) }
enum Data { empty; bytes(value: buffer); row(record: Row); values(items: ints); nested(value: OptionInt); handles(listener: tcp_listener, socket: tcp_socket) }
struct Row { text: buffer; number: int; def changed(self) -> Row { self.text = buffer("changed"); return self } }
struct Holder { value: Data; def copy(self) -> Holder { return self } }
enum Pair { values(first: Row, second: Row); absent }
enum Details { value(number: int, valid: bool, name: str); empty }
def get_number(value: OptionInt) -> int {
    match value { case OptionInt.none { return 0 } case OptionInt.some(number) { return number } }
}
def make() -> Data { print("once"); return Data.bytes(buffer("original")) }
def copy(value: Data) -> Data { return value }
def extract(value: Data) -> buffer {
    match value {
        case Data.empty { return buffer() }
        case Data.bytes(text) { append(text, "!"); return text }
        case Data.row(row) { return row.text }
        case Data.values(values) { return to_buffer(len(values)) }
        case Data.nested(value) { return to_buffer(get_number(value)) }
        case Data.handles(listener, socket) { return to_buffer(tcp_listener_status(listener)) }
    }
}
def main() -> int {
    if get_number(OptionInt.none()) != 0 or get_number(OptionInt.some(42)) != 42 { return 1 }
    var item: Data = make()
    var copied = copy(item)
    copied = copied
    match copied {
        case Data.empty { return 2 }
        case Data.bytes(text) { copied = Data.empty(); append(text, " mutated"); print(text) }
        case Data.row(row) { return 3 }
        case Data.values(values) { return 4 }
        case Data.nested(value) { return 5 }
        case Data.handles(listener, socket) { return 6 }
    }
    if not same(extract(item), "original!") { return 7 }
    var holder = Holder(item)
    holder.value = holder.value
    holder = holder.copy()
    if not same(extract(holder.value), "original!") { return 8 }
    var row = Row(buffer("before"), 1)
    var pair = Pair.values(row, row.changed())
    match pair {
        case Pair.absent { return 9 }
        case Pair.values(first, second) {
            if not same(first.text, "before") or not same(second.text, "changed") { return 10 }
            first.text = buffer("local"); second.number = 2
        }
    }
    var numbers = ints(); push(numbers, 7)
    if not same(extract(Data.values(numbers)), "1") { return 11 }
    if not same(extract(Data.nested(OptionInt.some(99))), "99") { return 12 }
    if not same(extract(Data.row(Row(buffer("row"), 0))), "row") { return 13 }
    var endpoint = Data.handles(tcp_listen_on("invalid", 0), tcp_connect("", -1))
    endpoint = copy(endpoint)
    match endpoint {
        case Data.empty { return 14 }
        case Data.bytes(text) { return 15 }
        case Data.row(row) { return 16 }
        case Data.values(values) { return 17 }
        case Data.nested(value) { return 18 }
        case Data.handles(listener, socket) {
            if tcp_listener_status(listener) <= 0 or tcp_socket_status(socket) <= 0 { return 19 }
        }
    }
    match Details.value(7, true, "static") {
        case Details.empty { return 20 }
        case Details.value(number, valid, name) { if number != 7 or not valid { return 21 }; print(name) }
    }
    var i = 0
    while i < 20 {
        var current = copy(item)
        if not same(extract(current), "original!") { return 22 }
        i = i + 1
    }
    Data.bytes(buffer("discarded"))
    var text = buffer("Aé🌍")
    if not utf8_valid(text) or utf8_count(text) != 3 or utf8_get(text, 2) != 127757 { return 20 }
    if not same(utf8_slice(text, 1, 2), "é🌍") { return 21 }
    if utf8_append(text, 33) != 0 or not same(text, "Aé🌍!") { return 22 }
    if utf8_append(text, 55296) == 0 or not same(text, "Aé🌍!") { return 23 }
    var invalid = buffer(); push(invalid, 255)
    if utf8_valid(invalid) or utf8_count(invalid) != -1 or utf8_get(invalid, 0) != -1 { return 24 }
    var listener = tcp_listen(-1)
    var socket = tcp_accept_timeout(listener, 0)
    if tcp_socket_status(socket) == 0 { return 25 }
    var destination = buffer("unchanged")
    if tcp_read_timeout(socket, destination, 1, 0) == 0 or not same(destination, "unchanged") { return 26 }
    if tcp_write_timeout(socket, destination, 0) == 0 { return 27 }
    if tcp_socket_status(tcp_connect_timeout("invalid host", -1, 0)) == 0 { return 28 }
    if tcp_socket_status(tcp_connect_timeout(buffer("invalid host"), -1, 0)) == 0 { return 29 }
    var before = monotonic_millis()
    if before < 0 or monotonic_millis() < before { return 30 }
    print("enum ownership ok")
    return 0
}
"#;

#[test]
fn enum_schemas_and_exhaustive_matches_accept_and_reject_bounded_inputs() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-enum-checks");
    let (_, compiler) = seed(&workspace);
    let mut cases: Vec<(String, bool)> = [
        ("enum E { none; value(x: int) }\ndef main() { var e: E = E.none(); match e { case E.none { } case E.value(x) { print(x) } } }", true),
        ("enum E { none(); value(x: int,) }\ndef main() { match E.value(1,) { case E.value(x,) { print(x) } case E.none() {} } }", true),
        ("enum E { value(r: R) }\nstruct R { e: Other }\nenum Other { empty }\ndef main() { var e = E.value(R(Other.empty())) }", true),
        ("enum E { yes; no }\ndef f(e: E) -> int { match e { case E.yes { return 1 } case E.no { return 0 } } }", true),
        ("enum E { only }\ndef f(e: E) -> int { match e { case E.only { return 7 } } }", true),
        ("enum E {}", false),
        ("enum E { same; same }", false),
        ("enum E { value(a: int, a: bool) }", false),
        ("enum E { value(v: void) }", false),
        ("enum E { value(v: task) }", false),
        ("enum E { value(v: Missing) }", false),
        ("enum E { value(v: E) }", false),
        ("enum E { value(v: R) }\nstruct R { next: E }", false),
        ("enum E { value(v: F) }\nenum F { value(v: E) }", false),
        ("enum E { x }\nstruct E {}", false),
        ("enum E { x }\ndef E() {}", false),
        ("def E() {}\nenum E { x }", false),
        ("enum tcp_listener { x }", false),
        ("enum bhasa_rt_fake { x }", false),
        ("enum E { x }\ndef main() { var e = E() }", false),
        ("enum E { x }\ndef main() { var e = E.x }", false),
        ("enum E { x }\ndef main() { var e = E.no() }", false),
        ("enum E { x }\ndef main() { var e = E.x(1) }", false),
        ("enum E { x(v: int) }\ndef main() { var e = E.x(true) }", false),
        ("enum E { x(v: int) }\ndef main() { var e = E.x() }", false),
        ("enum E { x(v: int) }\ndef main() { var e = E.x(1); print(e.v) }", false),
        ("enum E { x }\ndef main() { var e = E.x(); e.tag = 0 }", false),
        ("enum E { x }\ndef f(e: E) { e = E.x() }", false),
        ("enum E { x }\ndef main() { match 1 { case E.x {} } }", false),
        ("enum E { x; y }\ndef main() { match E.x() { case E.x {} } }", false),
        ("enum E { x }\ndef main() { match E.x() { case E.x {} case E.x {} } }", false),
        ("enum E { x }\ndef main() { match E.x() { } }", false),
        ("enum E { x }\nenum F { x }\ndef main() { match E.x() { case F.x {} } }", false),
        ("enum E { x(a: int, b: int) }\ndef main() { match E.x(1, 2) { case E.x(a, a) {} } }", false),
        ("enum E { x(a: int) }\ndef main() { match E.x(1) { case E.x {} } }", false),
        ("enum E { x }\ndef main() { match E.x() { case E.x(v) {} } }", false),
        ("enum E { x(a: int) }\ndef main() { match E.x(1) { case E.x(v) {} }; print(v) }", false),
        ("enum E { x; y }\ndef f(e: E) -> int { match e { case E.x { return 1 } case E.y { print(0) } } }", false),
        ("enum E { x }\ndef f(e: E) -> int { match e { case E.x { return 1 } }; return 2 }", false),
        ("def main() { var b = buffer(); utf8_append(b, 65) }", false),
        ("def main() { var b = buffer(); var socket = tcp_connect_timeout(\"invalid\", -1, 0); tcp_read_timeout(socket, b, 1, 0) }", false),
        ("def main() { var b = buffer(); var socket = tcp_connect_timeout(\"invalid\", -1, 0); tcp_write_timeout(socket, b, 0) }", false),
        ("def main() { utf8_valid(\"text\") }", false),
    ].into_iter().map(|(source, valid)| (source.to_owned(), valid)).collect();
    let worker = "enum E { value(n:int) } def worker(n:int) -> int { return n } ";
    for (body, valid) in [
        ("match E.value(1) { case E.value(n) { var job = spawn(worker(n)); var result = join(job); print(result) } }", true),
        ("match E.value(1) { case E.value(n) { var job = spawn(worker(n)) } }", false),
        ("var job = spawn(worker(1)); match E.value(1) { case E.value(n) { join(job) } }", false),
        ("var job = spawn(worker(1)); match E.value(1) { case E.value(job) {} } join(job)", false),
    ] {
        cases.push((format!("{worker} def main() {{ {body} }}"), valid));
    }
    cases.push(("enum E { value(n:int) } extern def foreign(n:int) -> int\ndef worker(n:int) -> int { match E.value(n) { case E.value(value) { return foreign(value) } } } def main() { var job = spawn(worker(1)); var result = join(job); print(result) }".to_owned(), false));
    for prefix in [
        "enum",
        "enum E",
        "enum E {",
        "enum E { value(",
        "enum E { value(x:",
        "enum E { value(x: int",
        "enum E { value(x: int)",
        "enum E { value(x: int) }",
    ] {
        // The final complete declaration is valid even without an entrypoint.
        cases.push((prefix.to_owned(), prefix.ends_with('}')));
    }
    let deep = (0..129)
        .map(|i| format!("enum E{i} {{ value(v: E{}) }}\n", i + 1))
        .collect::<String>()
        + "enum E129 { empty }\n";
    cases.push((deep, false));
    for depth in [80, 128, 129] {
        let source = format!(
            "enum E {{ only }}\ndef f(e: E) {{ {} {} }}",
            "match e { case E.only { ".repeat(depth),
            "} } ".repeat(depth)
        );
        cases.push((source, depth <= 128));
    }
    let mixed = format!(
        "enum E {{ only }}\ndef f(e: E) {{ {}{} {}{} }}",
        "if true { ".repeat(80),
        "match e { case E.only { ".repeat(60),
        "} } ".repeat(60),
        "} ".repeat(80)
    );
    cases.push((mixed, false));
    for (index, (source, valid)) in cases.iter().enumerate() {
        let output = emit(
            &compiler,
            &workspace.write("case.bh", source),
            "x86_64-unknown-linux-gnu",
        );
        if *valid {
            succeeds(&output);
        } else {
            diagnostic(&output, source);
        }
        // Exercise the supported CLI stack for the documented 128-level
        // nesting boundary; a test worker's smaller stack is not that boundary.
        let mut command = workspace.command();
        command.args([
            "emit-llvm",
            as_str(&workspace.file("case.bh")),
            "--target",
            "linux-x64",
        ]);
        let reference = bounded(command);
        assert_eq!(
            reference.status.success(),
            *valid,
            "Rust parity case {index}: {source}\n{}",
            describe(&reference)
        );
    }
}

#[test]
fn enums_rebuild_and_match_rust_with_zero_live_allocations() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("selfhost-enum-ownership");
    let (source, a) = seed(&workspace);
    let target = host_target(&clang, &workspace);
    let runtime = tracked_runtime(&workspace);
    let b_ir = emit(&a, &source, &target);
    succeeds(&b_ir);
    let b = link(&clang, &workspace, &stdout(&b_ir), &runtime, "B", "-O2");
    let c_ir = emit(&b, &source, &target);
    succeeds(&c_ir);
    assert!(c_ir.stderr.is_empty(), "{}", describe(&c_ir));
    assert_eq!(b_ir.stdout, c_ir.stdout);
    let c = link(&clang, &workspace, &stdout(&c_ir), &runtime, "C", "-O2");
    let output = emit(&c, &workspace.write("ownership.bh", OWNERSHIP), &target);
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    assert!(stdout(&output).contains("; bhasa runtime: text"));
    assert!(stdout(&output).contains("; bhasa runtime: net"));
    let reference = bhasa::compile_to_llvm(OWNERSHIP, &target).unwrap();
    for (frontend, ir) in [("rust", reference), ("bhasa", stdout(&output))] {
        for optimization in ["-O0", "-O2"] {
            let executable = link(&clang, &workspace, &ir, &runtime, frontend, optimization);
            let output = bounded(Command::new(executable));
            succeeds(&output);
            assert_eq!(
                stdout(&output),
                "once\noriginal mutated\nstatic\nenum ownership ok\n"
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
    for invalid in [
        "enum R { value(v: R) }",
        "enum E { x; y }\ndef main() { match E.x() { case E.x {} } }",
    ] {
        let output = emit(&c, &workspace.write("invalid.bh", invalid), &target);
        diagnostic(&output, invalid);
    }
}

#[test]
fn enum_declarations_and_match_arms_cannot_cross_source_file_boundaries() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-enum-file-boundaries");
    let mut compiler = ["lexer.bh", "emitter.bh"]
        .into_iter()
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    compiler.push_str(r#"
def rejected(source: buffer, boundary: int) -> bool {
    var errors = ints(); var tokens = bh_lex(source, errors); var merged = ints()
    var i = 0; var inserted = false
    while i < len(tokens) / 3 {
        if not inserted and bh_token_start(tokens, i) >= boundary {
            bh_triple(merged, 314, boundary, boundary); inserted = true
        }
        bh_triple(merged, bh_token_kind(tokens, i), bh_token_start(tokens, i), bh_token_end(tokens, i))
        i = i + 1
    }
    var output = bh_compile(source, merged, errors, buffer("x86_64-unknown-linux-gnu"))
    if len(errors) != 0 { return len(output) == 0 }
    return false
}
def main() -> int {
"#);
    for (index, (source, marker, rejected)) in [
        ("enum E { none; some(value: int) }", "some", true),
        ("enum E { none; some(value: int) }", "int", true),
        (
            "enum E { none }\ndef main() { match E.none() { case E.none {} } }",
            "case",
            true,
        ),
        (
            "enum E { none }\ndef main() { match E.none() { case E.none { print(1) } } }",
            "print",
            true,
        ),
        (
            "enum E { none }\ndef main() { match E.none() { case E.none {} } }",
            "def",
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let boundary = source.find(marker).unwrap();
        let quoted = source
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        compiler.push_str(&format!(
            "if rejected(buffer(\"{quoted}\"), {boundary}) != {rejected} {{ return {} }}\n",
            index + 1
        ));
    }
    compiler.push_str("return 0\n}\n");
    let input = workspace.write("boundaries.bh", &compiler);
    let executable = build(&workspace, &input, true);
    succeeds(&bounded(Command::new(executable)));
}
