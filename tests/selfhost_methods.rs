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
    // Include generated record helpers.
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

const METHOD_ALIASES: &str = r#"
struct Leaf {
    text: buffer
    count: int
    def copied(self) -> Leaf { return self }
    def bytes(self) -> buffer { return self.text }
    def add(self, amount: int) -> int { self.count = self.count + amount; return self.count }
    def accept(self, other: Leaf) {}
    def replace_text(self, text: buffer) { self.text = text }
}
struct Pair {
    left: Leaf
    right: Leaf
    def copied(self) -> Pair { return self }
    def scalar(self, amount: int) -> int { return self.left.count + amount }
    def child(self, value: Leaf) {}
    def replace(self) -> int { self.left = Leaf(buffer("changed"), 2); return 1 }
    def changed(self) -> Pair { self.left = Leaf(buffer("changed"), 2); return self }
}
struct Wrap { value: Pair }
def consume(first: buffer, second: int) {}
def wrapped(first: buffer, second: Wrap) {}
def main() {
    var p = Pair(Leaf(buffer("left"), 1), Leaf(buffer("right"), 2))
"#;

#[test]
fn method_receivers_calls_aliases_and_task_safety_match_rust() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-method-checks");
    let (_, compiler) = seed(&workspace);
    let mut cases: Vec<(String, bool)> = [
        ("struct Point { x: int; y: int; def total(self) -> int { return self.x + self.y }; def shift(self, amount: int) { self.x = self.x + amount } }\ndef main() { var p = Point(20, 22); p.shift(1); print(p.total()) }", true),
        ("struct Empty { def answer(self) -> int { return 42 } }\ndef main() { print(Empty().answer()) }", true),
        ("struct Point { def first(self) -> int { return self.later() }; def later(self) -> int { return forward() } }\ndef forward() -> int { return 42 }\ndef main() { print(Point().first()) }", true),
        ("struct Point { x: int; def down(self, n: int) -> int { if n == 0 { return self.x }; return self.down(n - 1) } }\ndef main() { print(Point(42).down(4)) }", true),
        ("struct First { def total(self) -> int { return 1 } }\nstruct Second { def total(self) -> int { return 2 } }\ndef total(value: int) -> int { return value }\ndef First_total() -> int { return 3 }\ndef main() { print(total(First().total() + Second().total() + First_total())) }", true),
        ("struct Names { def print(self) -> int { return 1 }; def len(self) -> int { return 2 }; def tcp_listen_on(self) -> int { return 3 }; def main(self) -> int { return 4 }; def Names(self) -> int { return 5 }; def spawn(self) -> int { return 6 }; def join(self) -> int { return 7 }; def gpu_dispatch(self) -> int { return 8 }; def bhasa_rt_foo(self) -> int { return 9 } }\ndef main() { var n = Names(); print(n.print() + n.len() + n.tcp_listen_on() + n.main() + n.Names() + n.spawn() + n.join() + n.gpu_dispatch() + n.bhasa_rt_foo()) }", true),
        ("struct Holder { value: Later; def number(self) -> int { return self.value.number() } }\nstruct Later { value: int; def number(self) -> int { return self.value } }\ndef main() { print(Holder(Later(42)).number()) }", true),
        ("struct Point { x: int; def copied(self) -> Point { return self } }\ndef main() { var p = Point(1); var copy = p.copied(); copy.x = 2; print(p.x) }", true),
        ("struct Point { x: int; def value(self) -> int { return self.x } }\ndef worker(n: int) -> int { return Point(n).value() }\ndef main() { var job = spawn(worker(3)); var result = join(job); print(result) }", true),
        ("struct Point { def total() -> int { return 1 } }", false),
        ("struct Point { def total(other) -> int { return 1 } }", false),
        ("struct Point { def total(other: Point) -> int { return 1 } }", false),
        ("struct Point { def total(self: Point) -> int { return 1 } }", false),
        ("struct Point { def total(self, self: int) -> int { return self } }", false),
        ("struct Point { def total(self, amount) -> int { return 1 } }", false),
        ("struct Point { extern def total(self) -> int }", false),
        ("struct Point { x: int; def replace(self) { self = Point(2) } }", false),
        ("struct Point { x: int; def total(self) -> int { return x } }", false),
        ("struct Point { total: int; def total(self) -> int { return 1 } }", false),
        ("struct Point { def total(self) -> int { return 1 }; total: int }", false),
        ("struct Point { def total(self) -> int { return 1 }; def total(self, amount: int) -> int { return amount } }", false),
        ("struct Point { def missing(self) -> int { if true { return 1 } } }", false),
        ("struct Point { def total(self) -> int { return true } }", false),
        ("struct Point { def copied(self) -> Point { return 1 } }", false),
        ("struct Point { def total(self) -> int { return 1 } }\ndef main() { print(Point.total()) }", false),
        ("struct Point { def total(self) -> int { return 1 } }\ndef main() { print(1.total()) }", false),
        ("struct Point { x: int }\ndef main() { print(Point(1).missing()) }", false),
        ("struct Point { x: int }\ndef main() { print(Point(1).x()) }", false),
        ("struct Point { def total(self) -> int { return 1 } }\ndef main() { var method = Point().total }", false),
        ("struct Point { def total(self) -> int { return 1 } }\ndef main() { var p = Point(); p.total = 2 }", false),
        ("struct Point { def shift(self, amount: int) {} }\ndef main() { Point().shift() }", false),
        ("struct Point { def shift(self, amount: int) {} }\ndef main() { Point().shift(true) }", false),
        ("struct Point { def shift(self, amount: int) {} }\ndef main() { Point().shift(1, 2) }", false),
        ("struct Point { def work(self, n: int) -> int { return n } }\ndef main() { var p = Point(); var job = spawn(p.work(1)); join(job) }", false),
        ("struct Point { def work(self, n: int) -> int { return n } }\ndef main() { var job = spawn(Point().work(1)); join(job) }", false),
    ].into_iter().map(|(source, valid)| (source.to_owned(), valid)).collect();
    for (call, valid) in [
        ("p.left.accept(p.left)", false),
        ("p.left.accept(p.right)", true),
        ("p.child(p.left)", false),
        ("p.left.replace_text(p.left.text)", false),
        ("p.left.replace_text(p.right.text)", true),
        ("var copy = p.left.text; p.left.replace_text(copy)", true),
        ("p.left.add(p.replace())", false),
        ("p.scalar(p.replace())", true),
        ("p.scalar(p.left.add(1))", true),
        ("p.left.add(p.left.add(1))", true),
        ("p.left.add(p.right.add(1))", true),
        ("consume(p.left.text, p.replace())", false),
        ("consume(p.left.text, p.left.add(1))", false),
        ("consume(p.left.text, p.right.add(1))", true),
        ("consume(p.left.text, len(p.left.bytes()))", false),
        ("p.scalar(len(p.left.bytes()))", true),
        ("p.left.accept(p.left.copied())", true),
        ("p.left.accept(p.copied().left)", false),
        ("p.left.accept(Pair(p.left, p.right).left)", true),
        ("wrapped(p.left.text, Wrap(p.changed()))", false),
        ("wrapped(p.left.text, Wrap(p))", true),
        ("var copied = Pair(p.left, p.changed().left)", true),
    ] {
        cases.push((format!("{METHOD_ALIASES}\n{call}\n}}"), valid));
    }
    let unsafe_worker = r#"
extern def foreign(n: int) -> int
struct Worker {
    value: int
    def call(self, n: int) -> int {
        if n == 0 { return foreign(self.value) }
        return recursive(self, n - 1)
    }
}
def recursive(worker: Worker, n: int) -> int { return worker.call(n) }
def worker(n: int) -> int { return Worker(n).call(2) }
"#;
    cases.push((
        format!("{unsafe_worker}\ndef main() {{ print(worker(1)) }}"),
        true,
    ));
    cases.push((
        format!("{unsafe_worker}\ndef main() {{ var job = spawn(worker(1)); join(job) }}"),
        false,
    ));
    let depth_cases_start = cases.len();
    for (depth, valid) in [(126, true), (127, true), (128, true), (129, false)] {
        cases.push((
            format!(
                "struct Deep {{ def run(self) {{ {}{} }} }}\ndef main() {{ Deep().run() }}",
                "if true {".repeat(depth),
                "}".repeat(depth)
            ),
            valid,
        ));
    }
    cases.push((
        format!(
            "struct Deep {{ def run(self) {{ {}var value = 1{} }} }}",
            "if true {".repeat(128),
            "}".repeat(128)
        ),
        false,
    ));
    cases.push((
        format!(
            "def global() {{ {}{} }}",
            "if true {".repeat(128),
            "}".repeat(128)
        ),
        true,
    ));
    for (depth, valid) in [(127, true), (128, false)] {
        cases.push((format!("struct Copy {{ def copied(self) -> Copy {{ return self }} }}\ndef main() {{ var value = Copy(){} }}", ".copied()".repeat(depth)), valid));
    }
    for (index, (source, valid)) in cases.into_iter().enumerate() {
        let input = workspace.write("case.bh", &source);
        if index >= depth_cases_start {
            // A test thread has a smaller stack than the compiler executable.
            // Exercise boundary inputs with the CLI and isolate any crash.
            let mut command = workspace.command();
            command
                .args(["emit-llvm", "--frontend", "rust"])
                .arg(&input)
                .args(["--target", "x86_64-unknown-linux-gnu"]);
            let rust = bounded(command);
            assert_eq!(
                rust.status.code(),
                Some(if valid { 0 } else { 1 }),
                "Rust depth case {index}: {source}\n{}",
                stderr(&rust)
            );
            if !valid {
                assert!(rust.stdout.is_empty());
            }
        } else {
            let rust = bhasa::compile_to_llvm(&source, "x86_64-unknown-linux-gnu");
            assert_eq!(
                rust.is_ok(),
                valid,
                "Rust baseline case {index}: {source}\n{:?}",
                rust.as_ref().map(|_| ())
            );
        }
        let output = emit(&compiler, &input, "x86_64-unknown-linux-gnu");
        if valid {
            succeeds(&output);
        } else {
            diagnostic(&output, &source);
        }
    }
}

#[test]
fn methods_preserve_host_file_boundary_tokens() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-method-files");
    let scanner = fs::read_to_string(fixture("bootstrap/compiler/lexer.bh")).unwrap();
    let emitter = fs::read_to_string(fixture("bootstrap/compiler/emitter.bh")).unwrap();
    let cases = [
        ("struct Holder { value: Point; def total(self) -> int { return self.value.total() } }", "struct Point { x: int; y: int; def total(self) -> int { return self.x + self.y } }\ndef main() { print(Holder(Point(20, 22)).total()) }", true),
        ("struct P { def value(self) -> int { return later() } }", "def later() -> int { return 42 }\ndef main() { print(P().value()) }", true),
        ("struct P { def value(self", ") -> int { return 42 } }", false),
        ("struct P { def value(self) ->", " int { return 42 } }", false),
        ("struct P { def value(self) -> int { return", " 42 } }", false),
        ("struct P { def value(self) -> int { return 42 }", " }", false),
        ("struct P { def value(self) -> int { return 42 } }\ndef f(p: P) -> int { return p.", "value() }", false),
        ("struct P { def value(self) -> int { return 42 } }\ndef f(p: P) -> int { return p.value(", ") }", false),
    ];
    let mut calls = String::new();
    for (left, right, _) in cases {
        let source = format!("{left}{right}");
        let reference = bhasa::compile_to_llvm(&source, "x86_64-unknown-linux-gnu");
        assert!(
            reference.is_ok(),
            "flat method source must be valid: {source}\n{reference:?}"
        );
        calls.push_str(&format!("    attempt({left:?}, {right:?})\n"));
    }
    let source = workspace.write("method-files.bh", &format!(r#"{scanner}
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
def attempt(left: str, right: str) {{
    var source = buffer()
    var tokens = ints()
    var failed = add_file(source, tokens, left) + add_file(source, tokens, right)
    if failed != 0 {{ print("unexpected lexical error"); return }}
    bh_triple(tokens, 0, len(source), len(source))
    var errors = ints()
    var output = bh_compile(source, tokens, errors, buffer("x86_64-unknown-linux-gnu"))
    print(len(output)); print(len(errors))
    if len(errors) == 0 {{ print(0); print(0); print(0) }} else {{
        print(get(errors, 0)); print(get(errors, 1)); print(get(errors, 2))
    }}
}}
def main() {{
{calls}}}
"#));
    let executable = build(&workspace, &source, true);
    let mut command = Command::new(executable);
    command.current_dir(workspace.path());
    let output = bounded(command);
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    let values = stdout(&output)
        .lines()
        .map(|line| line.parse::<usize>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), cases.len() * 5, "{}", describe(&output));
    let (rows, remainder) = values.as_chunks::<5>();
    assert!(remainder.is_empty());
    for ((left, right, valid), actual) in cases.into_iter().zip(rows) {
        if valid {
            assert!(actual[0] > 0, "{left} | {right}");
            assert_eq!(&actual[1..], &[0, 0, 0, 0], "{left} | {right}");
        } else {
            assert_eq!(
                actual,
                &[0, 3, 200, left.len(), left.len()],
                "{left} | {right}"
            );
        }
    }
}

#[test]
fn explicit_listen_address_overloads_and_types_match_rust() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("selfhost-listen-on-checks");
    let (_, compiler) = seed(&workspace);
    for (source, valid) in [
        ("def main() { var listener: tcp_listener = tcp_listen_on(\"127.0.0.1\", 0); print(tcp_listener_status(listener)) }", true),
        ("def open(host: buffer) -> tcp_listener { return tcp_listen_on(host, 0) }\ndef main() { var listener = open(buffer(\"::1\")); print(tcp_local_port(listener)) }", true),
        ("def main() { tcp_listen_on(\"127.0.0.1\", 0) }", true),
        ("def main() { var listener = tcp_listen_on() }", false),
        ("def main() { var listener = tcp_listen_on(0) }", false),
        ("def main() { var listener = tcp_listen_on(1, 0) }", false),
        ("def main() { var listener = tcp_listen_on(ints(), 0) }", false),
        ("def main() { var listener = tcp_listen_on(\"127.0.0.1\", false) }", false),
        ("def main() { var listener = tcp_listen_on(\"127.0.0.1\", 0, 1) }", false),
        ("def main() { var socket: tcp_socket = tcp_listen_on(\"127.0.0.1\", 0) }", false),
        ("def tcp_listen_on(host: str, port: int) -> int { return port }", false),
        ("struct tcp_listen_on { value: int }", false),
    ] {
        let rust = bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu");
        assert_eq!(rust.is_ok(), valid, "Rust baseline: {source}\n{rust:?}");
        let output = emit(&compiler, &workspace.write("case.bh", source), "x86_64-unknown-linux-gnu");
        if valid { succeeds(&output); } else { diagnostic(&output, source); }
    }
}

const METHOD_OWNERSHIP: &str = r#"
struct Namespace {
    def spawn(self) -> int { return 1 }
    def join(self) -> int { return 2 }
    def gpu_dispatch(self) -> int { return 3 }
    def bhasa_rt_foo(self) -> int { return 4 }
}
struct Action {
    value: int
    def evaluate(self, first: int, second: int) -> int { return self.value + first * 10 + second }
}
def receiver(trace: buffer) -> Action { append(trace, "R"); return Action(100) }
def argument(trace: buffer, label: str, value: int) -> int { append(trace, label); return value }
struct Point {
    x: int
    y: int
    def total(self) -> int { return self.x + self.y }
    def shift(self, amount: int) { self.x = self.x + amount }
}
struct Leaf {
    text: buffer
    values: ints
    def copied(self) -> Leaf { return self }
    def bytes(self) -> buffer { return self.text }
    def numbers(self) -> ints { return self.values }
    def change(self, suffix: str) -> Leaf {
        append(self.text, suffix)
        push(self.values, 8)
        return self
    }
    def early(self, first: bool) -> Leaf {
        var local = self
        if first { var unused = make("branch"); return local }
        var count = 0
        while count < 2 {
            var temporary = make("loop")
            if count == 1 { return temporary }
            count = count + 1
        }
        return make("fallback")
    }
}
struct Pair {
    left: Leaf
    right: Leaf
    def child(self) -> Leaf { return self.left }
    def replace(self) -> buffer {
        self.left = make("replaced")
        return buffer("assigned")
    }
    def changed(self) -> Leaf { self.left = make("changed"); return self.left }
}
def make(text: str) -> Leaf {
    var values = ints(); push(values, 7)
    return Leaf(buffer(text), values)
}
def main() -> int {
    var trace = buffer()
    var evaluated = receiver(trace).evaluate(argument(trace, "A", 1), argument(trace, "B", 2))
    if not same(trace, "RAB") or evaluated != 112 { return 22 }
    print(trace); print(evaluated)
    if Namespace().spawn() + Namespace().join() + Namespace().gpu_dispatch() + Namespace().bhasa_rt_foo() != 10 { return 23 }
    var point = Point(20, 22)
    point.shift(1)
    if point.total() != 43 { return 10 }
    var original = make("seed")
    var copied = original.copied()
    set(copied.text, 0, 67); set(copied.values, 0, 42)
    if not same(original.text, "seed") or get(original.values, 0) != 7 { return 11 }
    var changed = original.change("!")
    if not same(original.text, "seed!") or len(original.values) != 2 { return 12 }
    set(changed.text, 0, 82); set(changed.values, 0, 99)
    if not same(original.text, "seed!") or get(original.values, 0) != 7 { return 13 }
    var returned_bytes = original.bytes(); push(returned_bytes, 63)
    var returned_numbers = original.numbers(); push(returned_numbers, 9)
    if not same(original.text, "seed!") or len(original.values) != 2 { return 14 }
    var temporary_bytes = make("survivor").bytes()
    var temporary_numbers = make("numbers").numbers()
    var temporary_copy = make("record").copied()
    if not same(temporary_bytes, "survivor") or get(temporary_numbers, 0) != 7 { return 15 }
    if not same(temporary_copy.text, "record") { return 16 }
    if not same(make("chained").copied().change("!").bytes(), "chained!") { return 17 }
    var pair = Pair(make("left"), make("right"))
    var child = pair.child(); set(child.text, 0, 76)
    if not same(pair.left.text, "left") { return 18 }
    pair.left.text = pair.replace()
    if not same(pair.left.text, "assigned") { return 19 }
    var constructed = Pair(pair.left, pair.changed())
    if not same(constructed.left.text, "assigned") or not same(constructed.right.text, "changed") { return 20 }
    var count = 0
    while count < 32 {
        var local = make("iteration")
        var first = local.early(true)
        var second = local.early(false)
        if not same(first.text, "iteration") or not same(second.text, "loop") { return 21 }
        local = first.copied()
        count = count + 1
    }
    make("discarded").change("!")
    make("discarded bytes").bytes()
    make("discarded values").numbers()
    print("method ownership ok")
    return 0
}
"#;

fn verify_native_pair(
    workspace: &Workspace,
    compiler: &Path,
    clang: &std::ffi::OsStr,
    source: &str,
    expected: &str,
) {
    let target = host_target(clang, workspace);
    let runtime = tracked_runtime(workspace);
    let reference = bhasa::compile_to_llvm(source, &target).unwrap();
    let output = emit(compiler, &workspace.write("native.bh", source), &target);
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    for (frontend, ir) in [("rust", reference), ("bhasa", stdout(&output))] {
        for optimization in ["-O0", "-O2"] {
            let executable = link(clang, workspace, &ir, &runtime, frontend, optimization);
            let mut command = Command::new(executable);
            command.current_dir(workspace.path());
            let output = bounded(command);
            succeeds(&output);
            assert_eq!(stdout(&output), expected, "{frontend} {optimization}");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn native_method_receivers_and_returned_values_pass_instrumented_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("selfhost-method-ownership");
    let (_, compiler) = seed(&workspace);
    verify_native_pair(
        &workspace,
        &compiler,
        &clang,
        METHOD_OWNERSHIP,
        "RAB\n112\nmethod ownership ok\n",
    );
}

const LISTEN_ON: &str = r#"
struct Socket {
    handle: tcp_socket
    def copied(self) -> tcp_socket { return self.handle }
    def status(self) -> int { return tcp_socket_status(self.handle) }
}
struct Listener {
    handle: tcp_listener
    def status(self) -> int { return tcp_listener_status(self.handle) }
    def port(self) -> int { return tcp_local_port(self.handle) }
    def copied(self) -> tcp_listener { return self.handle }
    def accept(self) -> tcp_socket { return tcp_accept(self.handle) }
}
def receive(socket: tcp_socket, bytes: buffer, count: int) -> int {
    var chunk = buffer()
    while len(bytes) < count {
        var status = tcp_read(socket, chunk, count - len(bytes))
        if status != 0 { return status }
        if len(chunk) == 0 { return 90 }
        append(bytes, chunk)
    }
    return 0
}
def invalid(host: buffer, port: int) -> bool {
    var listener = tcp_listen_on(host, port)
    return tcp_listener_status(listener) > 0 and tcp_local_port(listener) == 0
}
def main() -> int {
    var first = Listener(tcp_listen_on("TEST_LOOPBACK", 0))
    if first.status() != 0 or first.port() == 0 { return 10 }
    var host = buffer("TEST_LOOPBACK")
    var listener = Listener(tcp_listen_on(host, 0))
    if listener.status() != 0 or listener.port() == 0 { return 11 }
    var copy = listener.copied()
    var port = listener.port()
    listener.handle = tcp_listen_on("", 0)
    if tcp_local_port(copy) != port { return 12 }
    var retained = Listener(copy)
    retained.handle = retained.handle
    var client = Socket(tcp_connect("TEST_LOOPBACK", retained.port())).copied()
    if tcp_socket_status(client) != 0 { return 13 }
    var accepted = Socket(retained.accept())
    if accepted.status() != 0 { return 14 }
    var server = accepted.copied()
    accepted.handle = tcp_connect("", -1)
    var request = buffer("ping"); push(request, 0); push(request, 255)
    var status = tcp_write(client, request)
    if status != 0 { return status }
    var bytes = buffer()
    status = receive(server, bytes, 6)
    if status != 0 { return status }
    if not same(bytes, request) { return 15 }
    status = tcp_write(server, bytes)
    if status != 0 { return status }
    var response = buffer()
    status = receive(client, response, 6)
    if status != 0 { return status }
    if not same(response, request) { return 16 }
    if not invalid(buffer(""), 0) { return 17 }
    if not invalid(buffer("localhost"), 0) { return 18 }
    if not invalid(buffer("[::1]"), 0) { return 19 }
    if not invalid(buffer("TEST_LOOPBACK"), -1) { return 20 }
    if not invalid(buffer("TEST_LOOPBACK"), 65536) { return 21 }
    var nul = buffer("TEST_LOOPBACK"); push(nul, 0)
    if not invalid(nul, 0) { return 22 }
    var binary = buffer(); push(binary, 255)
    if not invalid(binary, 0) { return 23 }
    tcp_listen_on("TEST_LOOPBACK", 0)
    print("listen-on methods ok")
    return 0
}
"#;

fn ipv6_available() -> bool {
    match std::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, 0)) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(error) => {
            // EPROTONOSUPPORT, EAFNOSUPPORT, and EADDRNOTAVAIL. Permission
            // failures must fail the test rather than becoming IPv6 skips.
            let unavailable = if cfg!(target_os = "macos") {
                [43, 47, 49]
            } else {
                [93, 97, 99]
            };
            assert!(
                error
                    .raw_os_error()
                    .is_some_and(|code| unavailable.contains(&code)),
                "IPv6 loopback probe failed: {error}"
            );
            eprintln!("SKIP IPv6 listen-on fixture: loopback is unavailable ({error})");
            false
        }
    }
}

#[test]
fn native_explicit_loopback_listeners_and_method_owned_handles_match_rust() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("selfhost-listen-on-native");
    let (_, compiler) = seed(&workspace);
    verify_native_pair(
        &workspace,
        &compiler,
        &clang,
        &LISTEN_ON.replace("TEST_LOOPBACK", "127.0.0.1"),
        "listen-on methods ok\n",
    );
    if ipv6_available() {
        verify_native_pair(
            &workspace,
            &compiler,
            &clang,
            &LISTEN_ON.replace("TEST_LOOPBACK", "::1"),
            "listen-on methods ok\n",
        );
    }
}
