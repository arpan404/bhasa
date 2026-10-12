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

const ALIASES: &str = r#"
struct Leaf { text: buffer; values: ints; count: int }
struct Pair { left: Leaf; right: Leaf }
struct Wrap { pair: Pair }
struct Envelope { wrap: Wrap }
def pair(first: Pair, second: Pair) {}
def child(parent: Pair, child: Leaf) {}
def parent(child: Leaf, parent: Pair) {}
def bytes(parent: Pair, text: buffer) {}
def numbers(parent: Pair, values: ints) {}
def leaves(first: Leaf, second: Leaf) {}
def scalar(parent: Pair, value: int) {}
def buffers(first: buffer, second: buffer) {}
def separate(first: buffer, second: ints) {}
def consume(first: buffer, second: int) {}
def condition(first: buffer, second: bool) {}
def wrapped(first: Pair, second: Wrap) {}
def nested_copy(first: Pair, second: Envelope) {}
def consume_wrap(first: buffer, second: Wrap) {}
def leaf_scalar(first: Leaf, second: int) {}
def replace(value: Pair) -> int { value.left = Leaf(buffer(), ints(), 0); return 1 }
def replace_leaf(value: Leaf) -> int { value.text = buffer("changed"); return 1 }
def replace_bytes(value: Pair) -> buffer {
    value.left = Leaf(buffer("changed"), ints(), 0)
    return value.left.text
}
def replace_pair(value: Pair) -> Pair {
    value.left = Leaf(buffer("changed"), ints(), 0)
    return value
}
def inspect(value: Pair) -> int { return len(value.left.text) }
def main() {
    var leaf = Leaf(buffer(), ints(), 1)
    var p = Pair(leaf, leaf)
    var q = p
"#;

#[test]
fn record_schemas_constructors_fields_and_borrows_match_rust() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-record-checks");
    let (_, compiler) = seed(&workspace);
    let mut cases: Vec<(String, bool)> = [
        ("struct Empty {}\ndef main() { var value: Empty = Empty(); value = value }", true),
        ("struct Outer { inner: Inner; flag: bool; label: str }\nstruct Inner { n: int }\ndef main() { var outer = Outer(Inner(4), true, \"hi\"); outer.inner.n = 5; print(outer.inner.n) }", true),
        ("struct A { b: B; c: B }\nstruct B { value: int }", true),
        ("struct Box { value: int }\ndef changed(value: Box) -> Box { value.value = 3; return value }\ndef main() { var value = changed(Box(1)); print(changed(value).value) }", true),
        ("struct Handles { listener: tcp_listener; socket: tcp_socket }\ndef copy(value: Handles) -> Handles { return value }", true),
        ("struct Point { x: int; y: int }\ndef main() { var p = Point(1, 2,); p.x = p.y }", true),
        ("struct Point { x: int; x: bool }", false),
        ("struct Point { x: int }\nstruct Point { y: int }", false),
        ("struct Point { x: int }\ndef Point(x: int) -> int { return x }", false),
        ("def Point(x: int) -> int { return x }\nstruct Point { x: int }", false),
        ("struct print { x: int }", false),
        ("struct tcp_listen { x: int }", false),
        ("struct bhasa_rt_record { x: int }", false),
        ("struct Box { value: Missing }", false),
        ("def consume(value: Missing) {}", false),
        ("def make() -> Missing { return 1 }", false),
        ("def main() { var value: Missing = 1 }", false),
        ("struct Node { next: Node }", false),
        ("struct A { b: B }\nstruct B { a: A }", false),
        ("struct A { b: B }\nstruct B { c: C }\nstruct C { a: A }", false),
        ("struct Box { value: void }", false),
        ("struct Box { value: task }", false),
        ("struct A { x: int }\nstruct B { x: int }\ndef main() { var a: A = B(1) }", false),
        ("struct A { x: int }\nstruct B { x: int }\ndef main() { var a = A(1); a = B(2) }", false),
        ("struct A { x: int }\nstruct B { x: int }\ndef take(a: A) {}\ndef main() { take(B(1)) }", false),
        ("struct A { x: int }\nstruct B { x: int }\ndef make() -> A { return B(1) }", false),
        ("struct Point { x: int }\ndef replace(p: Point) { p = Point(2) }", false),
        ("struct Point { x: int }\ndef main() { var p = Point(1); p.x = false }", false),
        ("struct Point { x: int }\ndef main() { var p = Point(1); print(p.missing) }", false),
        ("struct Point { x: int }\ndef main() { var p = Point(1); p.missing = 2 }", false),
        ("def main() { var p = 1; print(p.x) }", false),
        ("def main() { var p = 1; p.x = 2 }", false),
        ("struct Point { x: int }\ndef main() { Point(1).x = 2 }", false),
        ("struct Point { x: int }\ndef main() { var p = Point(1); (p).x = 2 }", false),
        ("struct Point { x: int }\nextern def native(p: Point)", false),
        ("struct Point { x: int }\nextern def native() -> Point", false),
        ("struct Point { x: int }\ndef main() { var Point = 1; var p = Point(1) }", false),
        ("struct Point { x: int }\ndef main() { print(Point(1)) }", false),
        ("struct Point { x: int }\ndef main() { var p = Point(1); print(p == p) }", false),
        ("extern def malloc(size: int) -> int\nstruct Point { x: int }\ndef main() {}", true),
        ("extern def free(value: int)\nstruct Point { x: int }\ndef read(p: Point) -> int { return p.x }", true),
        ("extern def malloc(size: int) -> int\nstruct Point { x: int }\ndef main() { var p = Point(1) }", false),
        ("extern def free(value: int)\nstruct Point { x: int }\ndef copy(p: Point) -> Point { return p }", false),
    ].into_iter().map(|(source, valid)| (source.to_owned(), valid)).collect();
    for expression in ["Point()", "Point(1)", "Point(1, 2, 3)", "Point(true, 2)"] {
        cases.push((
            format!("struct Point {{ x: int; y: int }}\ndef main() {{ var p = {expression} }}"),
            false,
        ));
    }
    for (call, valid) in [
        ("pair(p, p)", false),
        ("child(p, p.left)", false),
        ("parent(p.right, p)", false),
        ("bytes(p, p.left.text)", false),
        ("numbers(p, p.right.values)", false),
        ("leaves(p.left, p.left)", false),
        ("consume(p.left.text, replace(p))", false),
        ("consume(p.left.text, 1 + replace(p))", false),
        ("condition(p.left.text, false and replace(p) == 0)", false),
        ("get(p.left.text, replace(p))", false),
        ("set(p.left.text, 0, replace(p))", false),
        ("consume(p.left.text, len(replace_bytes(p)))", false),
        (
            "consume(p.left.text, len(slice(replace_bytes(p), 0, 1)))",
            false,
        ),
        (
            "consume(p.left.text, len(replace_pair(p).left.text))",
            false,
        ),
        (
            "consume(p.left.text, len(Wrap(replace_pair(p)).pair.left.text))",
            false,
        ),
        ("consume(p.left.text, get(to_buffer(replace(p)), 0))", false),
        ("consume_wrap(p.left.text, Wrap(replace_pair(p)))", false),
        ("leaf_scalar(p.left, replace(p))", false),
        ("leaves(p.left, replace_pair(p).left)", false),
        ("consume(p.left.text, replace_leaf(p.left))", false),
        ("consume(p.left.text, inspect(p))", false),
        ("pair(p, q)", true),
        ("leaves(p.left, p.right)", true),
        ("separate(p.left.text, p.left.values)", true),
        ("buffers(p.left.text, p.left.text)", true),
        ("scalar(p, len(p.left.text))", true),
        (
            "scalar(p, len(slice(p.left.text, 0, len(p.left.text))))",
            true,
        ),
        ("scalar(p, get(p.left.text, len(p.left.text) - 1))", true),
        ("scalar(p, len(Wrap(p).pair.left.text))", true),
        (
            "scalar(p, len(Envelope(Wrap(p)).wrap.pair.left.text))",
            true,
        ),
        ("scalar(p, replace(p))", true),
        ("scalar(p, -replace(p))", true),
        ("scalar(p, inspect(p))", true),
        ("scalar(p, replace_leaf(p.left))", true),
        ("leaf_scalar(p.left, replace_leaf(p.left))", true),
        ("consume(p.left.text, replace_leaf(p.right))", true),
        ("consume(p.left.text, len(Wrap(p).pair.left.text))", true),
        ("consume_wrap(p.left.text, Wrap(p))", true),
        ("pair(p, replace_pair(p))", true),
        ("pair(p, Wrap(p).pair)", true),
        ("leaves(p.left, Pair(p.left, p.right).left)", true),
        ("wrapped(p, Wrap(p))", true),
        ("wrapped(p, Envelope(Wrap(p)).wrap)", true),
        ("nested_copy(p, Envelope(Wrap(p)))", true),
        ("nested_copy(p, Envelope(Wrap(replace_pair(p))))", true),
        ("scalar(p, 1)", true),
        ("scalar(p, p.left.count)", true),
        ("var copied = Pair(p.left, p.left)", true),
    ] {
        cases.push((format!("{ALIASES}\n{call}\n}}"), valid));
    }
    for (index, (source, valid)) in cases.iter().enumerate() {
        let rust = bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu");
        assert_eq!(
            rust.is_ok(),
            *valid,
            "Rust baseline case {index}: {source}\n{rust:?}"
        );
        let output = emit(
            &compiler,
            &workspace.write("input.bh", source),
            "x86_64-unknown-linux-gnu",
        );
        if *valid {
            succeeds(&output);
        } else {
            diagnostic(&output, source);
        }
    }
    for (source, code) in [
        ("struct Box { value: Missing }", "BH211"),
        ("struct Box { value: int; value: bool }", "BH212"),
        ("struct Box { value: Box }", "BH213"),
        ("struct Box { value: buffer }\ndef use(box: Box, bytes: buffer) {}\ndef main() { var box = Box(buffer()); use(box, box.value) }", "BH214"),
        ("extern def malloc(size: int) -> int\nstruct Box { value: int }\ndef main() { var box = Box(1) }", "BH210"),
    ] {
        let output = emit(&compiler, &workspace.write("diagnostic.bh", source), "x86_64-unknown-linux-gnu");
        diagnostic(&output, source);
        assert!(stderr(&output).contains(code), "expected {code}: {}", describe(&output));
    }
}

#[test]
fn malformed_and_deep_record_schemas_fail_without_traps_or_unbounded_recursion() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-record-diagnostics");
    let (_, compiler) = seed(&workspace);
    let mut invalid: Vec<String> = [
        "struct",
        "struct Point",
        "struct Point {",
        "struct Point { x",
        "struct Point { x:",
        "struct Point { x: int",
        "struct Point { x: int;",
        "struct Point { x: }",
        "struct Point { x: int y: int }",
        "struct Point { x: int }\ndef main() { var p = Point(1); p.",
        "struct Point { x: int }\ndef main() { var p = Point(1); p. = 1 }",
        "# Unicode α\nstruct Point { x: Missing }",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut chain = (0..129)
        .map(|i| format!("struct R{i} {{ next: R{} }}\n", i + 1))
        .collect::<Vec<_>>();
    chain.push("struct R129 { value: int }\n".to_owned());
    invalid.push(chain.concat());
    chain.reverse();
    invalid.push(chain.concat());
    let mut cycle = (0..160)
        .map(|i| format!("struct C{i} {{ next: C{} }}\n", i + 1))
        .collect::<Vec<_>>();
    cycle.push("struct C160 { next: C0 }\n".to_owned());
    invalid.push(cycle.concat());
    // Layout depth and expression depth are separate limits. A valid schema
    // may still produce a field expression one level beyond the AST limit.
    let schema = (0..127)
        .map(|i| format!("struct D{i} {{ child: D{} }}\n", i + 1))
        .collect::<String>()
        + "struct D127 { value: int }\n";
    let path = format!("p{}", ".child".repeat(127));
    let valid = format!("{schema}def leaf(p: D0) -> D127 {{ return {path} }}");
    assert!(bhasa::compile_to_llvm(&valid, "x86_64-unknown-linux-gnu").is_ok());
    succeeds(&emit(
        &compiler,
        &workspace.write("depth-boundary.bh", &valid),
        "x86_64-unknown-linux-gnu",
    ));
    invalid.push(format!(
        "{schema}def leaf(p: D0) -> int {{ return {path}.value }}"
    ));
    for source in invalid {
        assert!(
            bhasa::compile_to_llvm(&source, "x86_64-unknown-linux-gnu").is_err(),
            "Rust accepted {source}"
        );
        let output = emit(
            &compiler,
            &workspace.write("invalid.bh", &source),
            "x86_64-unknown-linux-gnu",
        );
        diagnostic(&output, &source);
    }
}

// Separate translation units avoid colliding private core/net helper names.
// Replace allocations in LLVM too, so record storage joins the container and
// handle accounting on hosts without LeakSanitizer.
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
    for name in ["core", "net"] {
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
struct Pair { left: Leaf; right: Leaf; active: bool; label: str }
struct Leaf { bytes: buffer; numbers: ints }
struct Empty {}
struct Endpoint { listener: tcp_listener; socket: tcp_socket; data: Leaf }
def make(text: str) -> Pair {
    var numbers = ints(); push(numbers, 7)
    return Pair(Leaf(buffer(text), numbers), Leaf(buffer("right"), ints()), true, "label")
}
def mutate(value: Pair) -> Pair {
    append(value.left.bytes, "!")
    push(value.left.numbers, 8)
    return value
}
def copied_child(value: Pair) -> Leaf { return value.left }
def copied_bytes(value: Pair) -> buffer { return value.left.bytes }
def replace_parent(value: Pair) -> buffer {
    value.left = Leaf(buffer("replaced"), ints())
    return buffer("assigned")
}
def replace_child(value: Pair) -> Leaf {
    value.left = Leaf(buffer("changed"), ints())
    return value.left
}
def early(value: Pair, first: bool) -> Pair {
    var copied = value
    if first { var unused = make("branch"); return copied }
    var i = 0
    while i < 2 {
        var temporary = make("loop")
        if i == 1 { return temporary }
        i = i + 1
    }
    return make("fallback")
}
def disjoint(first: Leaf, second: Leaf) {
    first.bytes = buffer("first")
    second.bytes = buffer("second")
}
def endpoint() -> Endpoint {
    return Endpoint(tcp_listen(0), tcp_connect("", -1), Leaf(buffer("endpoint"), ints()))
}
def copied_endpoint(value: Endpoint) -> Endpoint { return value }
def main() -> int {
    var empty = Empty(); empty = empty
    var original = make("seed")
    var copied: Pair = original
    set(copied.left.bytes, 0, 67)
    set(copied.left.numbers, 0, 42)
    copied.active = false
    if not same(original.left.bytes, "seed") or get(original.left.numbers, 0) != 7 { return 10 }
    if not original.active or copied.active { return 11 }
    print(original.label)
    original = original
    original.left = original.left
    original.left.bytes = original.left.bytes
    original.left.numbers = original.left.numbers
    var returned = mutate(original)
    if not same(original.left.bytes, "seed!") or len(original.left.numbers) != 2 { return 12 }
    set(returned.left.bytes, 0, 82); set(returned.left.numbers, 0, 88)
    if not same(original.left.bytes, "seed!") or get(original.left.numbers, 0) != 7 { return 13 }
    var child = copied_child(original)
    set(child.bytes, 0, 83)
    var bytes = copied_bytes(original); push(bytes, 63)
    if not same(original.left.bytes, "seed!") { return 14 }
    original.right = original.left
    set(original.right.bytes, 0, 66)
    if not same(original.left.bytes, "seed!") { return 15 }
    original = Pair(original.right, original.left, true, "changed")
    if not same(original.left.bytes, "Beed!") or not same(original.right.bytes, "seed!") { return 16 }
    disjoint(original.left, original.right)
    if not same(original.left.bytes, "first") or not same(original.right.bytes, "second") { return 17 }
    original.left.bytes = replace_parent(original)
    if not same(original.left.bytes, "assigned") or len(original.left.numbers) != 0 { return 26 }
    var constructed = Pair(original.left, replace_child(original), true, "built")
    if not same(constructed.left.bytes, "assigned") or not same(constructed.right.bytes, "changed") { return 27 }
    var extracted = make("survivor").left
    var extracted_bytes = make("bytes survive").left.bytes
    var extracted_numbers = make("numbers survive").left.numbers
    if not same(extracted.bytes, "survivor") or not same(extracted_bytes, "bytes survive") { return 18 }
    if get(extracted_numbers, 0) != 7 or len(make("temporary").left.bytes) != 9 { return 19 }
    append(make("temporary borrow").left.bytes, "!")
    var i = 0
    while i < 32 {
        var local = make("iteration")
        var first = early(local, true)
        var second = early(local, false)
        if not same(first.left.bytes, "iteration") or not same(second.left.bytes, "loop") { return 20 }
        local.left = copied_child(first)
        local = second
        i = i + 1
    }
    var network = endpoint()
    if tcp_listener_status(network.listener) != 0 or tcp_socket_status(network.socket) == 0 { return 21 }
    var network_copy = copied_endpoint(network)
    var port = tcp_local_port(network.listener)
    network.listener = tcp_listen(-1)
    if tcp_local_port(network_copy.listener) != port { return 22 }
    network_copy.listener = network_copy.listener
    network_copy.socket = network_copy.socket
    network_copy = network_copy
    if tcp_local_port(network_copy.listener) != port { return 23 }
    set(network_copy.data.bytes, 0, 69)
    if not same(network.data.bytes, "endpoint") { return 24 }
    var retained_listener = endpoint().listener
    if tcp_listener_status(retained_listener) != 0 { return 25 }
    make("discarded record")
    make("discarded child").left
    make("discarded bytes").left.bytes
    endpoint()
    print("record ownership ok")
    return 0
}
"#;

#[test]
fn rebuilt_frontend_and_record_values_match_rust_with_no_sanitizer_or_allocation_errors() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("selfhost-record-ownership");
    let (source, a) = seed(&workspace);
    let target = host_target(&clang, &workspace);
    let runtime = tracked_runtime(&workspace);
    let b_ir = emit(&a, &source, &target);
    succeeds(&b_ir);
    let b = link(&clang, &workspace, &stdout(&b_ir), &runtime, "B", "-O2");
    let c_ir = emit(&b, &source, &target);
    succeeds(&c_ir);
    assert!(c_ir.stderr.is_empty(), "{}", describe(&c_ir));
    assert_eq!(
        b_ir.stdout, c_ir.stdout,
        "self-rebuild changed generated LLVM"
    );
    let c = link(&clang, &workspace, &stdout(&c_ir), &runtime, "C", "-O2");
    let input = workspace.write("ownership.bh", OWNERSHIP);
    let b_output = emit(&b, &input, &target);
    let c_output = emit(&c, &input, &target);
    succeeds(&b_output);
    succeeds(&c_output);
    assert!(b_output.stderr.is_empty() && c_output.stderr.is_empty());
    assert_eq!(b_output.stdout, c_output.stdout);
    let reference = bhasa::compile_to_llvm(OWNERSHIP, &target).unwrap();
    for (frontend, ir) in [("rust", reference), ("bhasa", stdout(&c_output))] {
        assert!(ir.contains("; bhasa runtime: core\n"));
        assert!(ir.contains("; bhasa runtime: net\n"));
        for optimization in ["-O0", "-O2"] {
            let executable = link(&clang, &workspace, &ir, &runtime, frontend, optimization);
            let mut command = Command::new(executable);
            command.current_dir(workspace.path());
            let output = bounded(command);
            succeeds(&output);
            assert_eq!(stdout(&output), "label\nrecord ownership ok\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
    for invalid in [
        "struct R { value: R }",
        "struct R { value: int }\ndef f(r: R) { r.value = buffer() }",
    ] {
        let output = emit(&c, &workspace.write("invalid.bh", invalid), &target);
        diagnostic(&output, invalid);
    }
}
