//! Independent methods contract tests, sharing the process-bounded API harness.
use super::*;

const RECEIVERS: &str = r#"
struct Leaf {
    text: buffer
    def size(self, extra: int) -> int { return len(self.text) + extra }
    def replace_text(self) -> int { self.text = buffer("new"); return 1 }
    def take(self, other: Leaf) -> int { return len(other.text) }
}
struct Box {
    leaf: Leaf
    other: Leaf
    def reset(self) -> int { self.leaf = Leaf(buffer("reset")); return 1 }
    def count(self, extra: int) -> int { return len(self.leaf.text) + extra }
    def descendant(self, child: Leaf) -> int { return len(child.text) }
    def copied(self, child: Leaf) -> int { return len(child.text) }
}
def make() -> Box { return Box(Leaf(buffer("left")), Leaf(buffer("right"))) }
def reset(box: Box) -> int { return box.reset() }
"#;

#[test]
fn methods_accept_valid_calls_and_reject_invalid_contracts() {
    let mut cases = Vec::new();
    for (name, source, accepted) in [
        (
            "same-names-different-types",
            "struct A { def value(self) -> int { return 1 } }\nstruct B { def value(self) -> bool { return true } }\ndef value() -> int { return 3 }\ndef main() { print(A().value()); print(B().value()); print(value()) }",
            true,
        ),
        (
            "builtin-method-names",
            "struct A { def print(self) -> int { return 7 }; def buffer(self) -> buffer { return buffer(\"ok\") } }\ndef main() { print(A().print()); print(A().buffer()) }",
            true,
        ),
        (
            "recursive-method",
            "struct A { def count(self, n: int) -> int { if n == 0 { return 0 }; return self.count(n - 1) + 1 } }\ndef main() { print(A().count(4)) }",
            true,
        ),
        (
            "method-returned-borrow",
            "struct A { data: buffer; def copy(self) -> A { return self }; def text(self) -> buffer { return self.data } }\ndef main() { var a = A(buffer(\"ok\")); var b = a.copy(); print(b.text()) }",
            true,
        ),
        ("missing-self", "struct A { def f() {} }", false),
        ("receiver-wrong-name", "struct A { def f(this) {} }", false),
        (
            "receiver-not-first",
            "struct A { def f(n: int, self) {} }",
            false,
        ),
        (
            "receiver-explicit-type",
            "struct A { def f(self: A) {} }",
            false,
        ),
        (
            "duplicate-self",
            "struct A { def f(self, self: int) {} }",
            false,
        ),
        (
            "untyped-extra-parameter",
            "struct A { def f(self, n) {} }",
            false,
        ),
        (
            "duplicate-method",
            "struct A { def f(self) {}; def f(self) {} }",
            false,
        ),
        (
            "overloaded-method",
            "struct A { def f(self) {}; def f(self, n: int) {} }",
            false,
        ),
        (
            "field-then-method",
            "struct A { value: int; def value(self) -> int { return 1 } }",
            false,
        ),
        (
            "method-then-field",
            "struct A { def value(self) -> int { return 1 }; value: int }",
            false,
        ),
        (
            "extern-method",
            "struct A { extern def f(self) -> int }",
            false,
        ),
        ("bodyless-method", "struct A { def f(self) -> int }", false),
        (
            "receiver-rebind",
            "struct A { n: int; def f(self) { self = A(2) } }",
            false,
        ),
        (
            "implicit-field",
            "struct A { n: int; def f(self) -> int { return n } }",
            false,
        ),
        (
            "implicit-method",
            "struct A { def a(self) -> int { return 1 }; def b(self) -> int { return a() } }",
            false,
        ),
        (
            "missing-return",
            "struct A { def f(self, yes: bool) -> int { if yes { return 1 } } }",
            false,
        ),
        (
            "unreachable-after-return",
            "struct A { def f(self) -> int { return 1; print(2) } }",
            false,
        ),
        (
            "wrong-return-type",
            "struct A { def f(self) -> int { return true } }",
            false,
        ),
        (
            "unknown-method",
            "struct A {}\ndef main() { A().missing() }",
            false,
        ),
        (
            "field-is-not-method",
            "struct A { n: int }\ndef main() { A(1).n() }",
            false,
        ),
        (
            "method-is-not-value",
            "struct A { def f(self) -> int { return 1 } }\ndef main() { var a = A(); var method = a.f }",
            false,
        ),
        ("scalar-receiver", "def main() { var n = 1; n.f() }", false),
        ("buffer-receiver", "def main() { buffer().f() }", false),
        (
            "static-call",
            "struct A { def f(self) -> int { return 1 } }\ndef main() { A.f() }",
            false,
        ),
        (
            "missing-argument",
            "struct A { def f(self, n: int) {} }\ndef main() { A().f() }",
            false,
        ),
        (
            "extra-argument",
            "struct A { def f(self) {} }\ndef main() { A().f(1) }",
            false,
        ),
        (
            "wrong-argument-type",
            "struct A { def f(self, n: int) {} }\ndef main() { A().f(true) }",
            false,
        ),
        (
            "explicit-receiver-argument",
            "struct A { def f(self) {} }\ndef main() { var a = A(); a.f(a) }",
            false,
        ),
        (
            "direct-method-spawn",
            "struct A { def f(self, n: int) -> int { return n } }\ndef main() { var a = A(); var job = spawn(a.f(1)); join(job) }",
            false,
        ),
    ] {
        cases.push(case(name, source, accepted));
    }
    for (name, call, accepted) in [
        ("nested-receiver", "print(b.leaf.size(1))", true),
        ("temporary-receiver", "print(make().leaf.size(1))", true),
        (
            "disjoint-record-arguments",
            "print(b.leaf.take(b.other))",
            true,
        ),
        (
            "same-receiver-later-field-change",
            "print(b.leaf.size(b.leaf.replace_text()))",
            true,
        ),
        (
            "root-receiver-later-descendant-change",
            "print(b.count(b.reset()))",
            true,
        ),
        (
            "read-during-later-argument",
            "print(b.count(len(b.leaf.text)))",
            true,
        ),
        (
            "copy-during-later-argument",
            "print(b.copied(Leaf(b.leaf.text)))",
            true,
        ),
        (
            "captured-receiver-invalidated",
            "print(b.leaf.size(b.reset()))",
            false,
        ),
        (
            "nested-invalidation-expression",
            "print(b.leaf.size(1 + reset(b)))",
            false,
        ),
        (
            "captured-field-invalidated",
            "print(get(b.leaf.text, b.reset()))",
            false,
        ),
        (
            "captured-field-invalidated-by-own-method",
            "print(get(b.leaf.text, b.leaf.replace_text()))",
            false,
        ),
        (
            "invalidation-buried-in-temporary-call",
            "print(b.leaf.size(make().leaf.size(b.reset())))",
            false,
        ),
        (
            "same-receiver-and-argument",
            "print(b.leaf.take(b.leaf))",
            false,
        ),
        (
            "parent-and-descendant",
            "print(b.descendant(b.leaf))",
            false,
        ),
    ] {
        cases.push(case(
            name,
            format!("{RECEIVERS}\ndef main() {{ var b = make(); {call} }}"),
            accepted,
        ));
    }
    both(
        &Workspace::new("method-contracts"),
        &cases,
        "x86_64-unknown-linux-gnu",
    );
}

#[test]
fn methods_worker_graphs_use_qualified_calls_through_recursion() {
    let cases = [
        case(
            "method-cycle-reaches-extern",
            r#"
extern def foreign(n: int) -> int
def walk(n: int) -> int { return n }
struct Worker {
    def walk(self, n: int) -> int {
        if n == 0 { return foreign(1) }
        return self.again(n - 1)
    }
    def again(self, n: int) -> int { return self.walk(n) }
}
def worker(n: int) -> int { return Worker().walk(n) }
def main() { var job = spawn(worker(2)); join(job) }
"#,
            false,
        ),
        case(
            "unused-global-and-method-extern-do-not-taint-worker",
            r#"
extern def foreign(n: int) -> int
def walk(n: int) -> int { return foreign(n) }
struct Worker {
    def walk(self, n: int) -> int {
        if n == 0 { return 1 }
        return self.again(n - 1)
    }
    def again(self, n: int) -> int { return worker(n) }
    def unused(self) -> int { return foreign(1) }
}
def worker(n: int) -> int { return Worker().walk(n) }
def main() { var job = spawn(worker(2)); var answer = join(job); print(answer) }
"#,
            true,
        ),
        case(
            "method-extern-evaluated-before-spawn",
            r#"
extern def foreign(n: int) -> int
struct Input { def value(self) -> int { return foreign(1) } }
def worker(n: int) -> int { return n }
def main() { var job = spawn(worker(Input().value())); join(job) }
"#,
            true,
        ),
    ];
    both(
        &Workspace::new("method-worker-graphs"),
        &cases,
        "x86_64-unknown-linux-gnu",
    );
}

#[test]
fn methods_generated_mutations_preserve_acceptance_and_valid_diagnostics() {
    let mut cases = Vec::new();
    for (seed_index, seed) in [
        "struct A { n: int; def add(self, x: int) -> int { return self.n + x } }\ndef main() { print(A(3).add(4)) }",
        "struct A { data: buffer; def text(self) -> buffer { return self.data } }\ndef main() { print(A(buffer(\"α\")).text()) }",
    ].into_iter().enumerate() {
        cases.push(case(format!("seed-{seed_index}"), seed, true));
        for (offset, character) in seed.char_indices() {
            let mut source = seed.to_owned();
            source.replace_range(offset..offset + character.len_utf8(), "");
            cases.push(Case { name: format!("method-delete-{seed_index}-{offset}"), source, accepts: None, entry: None });
        }
    }
    both(
        &Workspace::new("method-mutations"),
        &cases,
        "x86_64-unknown-linux-gnu",
    );
}

const METHOD_VALUES: &str = r#"
struct Cell {
    text: buffer
    values: ints
    def bytes(self) -> buffer { return self.text }
    def numbers(self) -> ints { return self.values }
    def copy(self) -> Cell { return self }
    def add(self, suffix: str) -> Cell {
        append(self.text, suffix)
        push(self.values, len(self.text))
        return self
    }
    def measure(self, extra: int) -> int { return len(self.text) + extra }
    def replace_text(self) -> int { self.text = buffer("new"); return 1 }
    def order(self, first: int, second: int) -> int {
        print(self.text)
        return first * 100 + second
    }
}
struct Pair {
    left: Cell
    right: Cell
    def first(self) -> Cell { return self.left }
    def pick(self, left: bool) -> Cell {
        var temporary = Cell(buffer("scratch"), ints())
        if left { return self.left }
        return self.right
    }
    def replace(self) -> buffer { self.left = Cell(buffer("new"), ints()); return buffer("rhs") }
    def reset(self) -> int { self.left = Cell(buffer("z"), ints()); return 2 }
    def measure(self, extra: int) -> int { return len(self.left.text) + extra }
}
struct A_B { def c(self) -> int { return 1 } }
struct A { def B_c(self) -> int { return 2 } }
struct BuiltinNames {
    def print(self) -> int { return 13 }
    def buffer(self) -> buffer { return buffer("method") }
}
def A_B_c() -> int { return 3 }
def c() -> int { return 4 }
def ordered_receiver() -> Cell { print(10); return Cell(buffer("once"), ints()) }
def ordered_argument(value: int) -> int { print(value); return value }
def make(text: str) -> Pair {
    var values = ints(); push(values, 7)
    var cell = Cell(buffer(text), values)
    return Pair(cell, cell)
}
def main() {
    var pair = make("seed")
    var returned = pair.left.add("!")
    append(returned.text, "?")
    print(pair.left.bytes()); print(returned.bytes())
    var copied = pair.first(); copied.text = buffer("copy")
    print(pair.left.bytes()); print(copied.bytes())
    print(make("temporary").first().bytes())
    print(make("temporary").left.add("!").bytes())
    var numbers = pair.left.numbers(); set(numbers, 0, 99)
    print(get(pair.left.values, 0)); print(get(numbers, 0))
    print(pair.first().measure(pair.reset()))
    pair.left.text = pair.replace(); print(pair.left.bytes())
    print(pair.measure(pair.reset()))
    print(pair.left.measure(pair.left.replace_text()))
    print(A_B().c()); print(A().B_c()); print(A_B_c()); print(c())
    print(BuiltinNames().print()); print(BuiltinNames().buffer())
    print(ordered_receiver().order(ordered_argument(20), ordered_argument(30)))
    var i = 0
    while i < 40 {
        var transient = make("loop")
        var first = transient.pick(true); var second = transient.pick(false)
        var text = transient.left.add("!").bytes()
        first = second.copy()
        transient.left = first.copy()
        transient.left.text = transient.replace()
        i = i + 1
    }
}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn tracked_link(
    clang: &std::ffi::OsStr,
    workspace: &Workspace,
    ir: &Path,
    optimization: &str,
) -> PathBuf {
    // Count direct LLVM record allocations and core container allocations.
    // These fixtures are single-threaded; libc's own buffers are not counted.
    let tracked = format!(
        r#"
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live;
static long allocations;
static int registered;
static void check_live(void) {{
    if (live != 0 || allocations == 0) {{
        fprintf(stderr, "method allocation ledger: live=%ld allocations=%ld\n", live, allocations);
        _Exit(99);
    }}
}}
void *method_tracked_malloc(size_t size) {{
    if (!registered) {{ registered = 1; atexit(check_live); }}
    void *result = malloc(size);
    if (result) {{ ++live; ++allocations; }}
    return result;
}}
void *method_tracked_realloc(void *pointer, size_t size) {{
    int was_null = pointer == NULL;
    void *result = realloc(pointer, size);
    if (result && was_null) {{ ++live; ++allocations; }}
    return result;
}}
void method_tracked_free(void *pointer) {{ if (pointer) --live; free(pointer); }}
#define malloc method_tracked_malloc
#define realloc method_tracked_realloc
#define free method_tracked_free
{}
"#,
        include_str!("../../runtime/core.c")
    );
    let runtime = workspace.write("tracked-method-runtime.c", &tracked);
    fs::copy(fixture("runtime/core.h"), workspace.file("core.h")).unwrap();
    let source = fs::read_to_string(ir)
        .unwrap()
        .replace("@malloc(", "@method_tracked_malloc(")
        .replace("@realloc(", "@method_tracked_realloc(")
        .replace("@free(", "@method_tracked_free(");
    let instrumented: String = source
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                format!(
                    "{} sanitize_address {{\n",
                    line.strip_suffix(" {").expect("LLVM function header")
                )
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    let input = workspace.write("tracked-methods.ll", &instrumented);
    let binary = workspace.file("method-program");
    let output = bounded(
        Command::new(clang)
            .args([
                optimization,
                "-fsanitize=address,undefined",
                "-fno-sanitize-recover=all",
                "-fno-omit-frame-pointer",
                "-pthread",
            ])
            .arg(input)
            .arg(runtime)
            .arg("-o")
            .arg(&binary),
        workspace,
        "method-link",
        30,
    );
    succeeds(&output);
    binary
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn methods_own_temporaries_and_returned_borrows_with_zero_live_allocations() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("method-owned-values");
    let target = native_target(&clang, &workspace);
    both(
        &workspace,
        &[case("method-values", METHOD_VALUES, true)],
        &target,
    );
    for frontend in frontends() {
        for optimization in ["-O0", "-O2"] {
            let executable = tracked_link(
                &clang,
                &workspace,
                &workspace.file(&format!("{frontend}-0.ll")),
                optimization,
            );
            let output = bounded(
                &mut Command::new(executable),
                &workspace,
                &format!("method-run-{frontend}-{optimization}"),
                30,
            );
            succeeds(&output);
            assert_eq!(
                stdout(&output),
                "seed!\nseed!?\nseed!\ncopy\ntemporary\ntemporary!\n7\n99\n7\nrhs\n3\n4\n1\n2\n3\n4\n13\nmethod\n10\n20\n30\nonce\n2030\n",
                "{frontend}, {optimization}"
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

fn imported_case(workspace: &Workspace, name: &str, leaf_method: &str, accepted: bool) -> Case {
    for directory in ["app/src", "library/src"] {
        fs::create_dir_all(workspace.file(&format!("{name}/{directory}"))).unwrap();
    }
    workspace.write(&format!("{name}/app/bhasa.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n[dependencies]\ndata = \"../library\"\n");
    workspace.write(
        &format!("{name}/library/bhasa.toml"),
        "[package]\nname = \"data\"\nversion = \"0.1.0\"\nentry = \"src/lib.bh\"\n",
    );
    workspace.write(
        &format!("{name}/app/src/wrap.bh"),
        r#"
struct Marker { ok: bool; def value(self) -> int { return 2 } }
struct Wrap {
    marker: Marker
    pair: Pair
    def value(self) -> int { return self.pair.sum() + self.marker.value() }
    def first(self) -> Leaf { return self.pair.left.copy() }
}
"#,
    );
    workspace.write(
        &format!("{name}/library/src/lib.bh"),
        "import \"types.bh\"\nimport \"make.bh\"\n",
    );
    workspace.write(
        &format!("{name}/library/src/types.bh"),
        &format!(
            r#"
struct Pair {{
    left: Leaf
    right: Leaf
    def sum(self) -> int {{ return self.left.value() + self.right.value() }}
}}
struct Leaf {{
    number: int
    text: buffer
    {leaf_method}
    def copy(self) -> Leaf {{ return self }}
}}
"#
        ),
    );
    workspace.write(&format!("{name}/library/src/make.bh"), "import \"types.bh\"\ndef make() -> Pair { return Pair(Leaf(19, buffer(\"left\")), Leaf(21, buffer(\"right\"))) }\n");
    let entry = workspace.write(
        &format!("{name}/app/src/main.bh"),
        r#"import "wrap.bh"
import "@data"
def main() {
    var wrapped = Wrap(Marker(true), make())
    print(wrapped.value())
    var leaf = wrapped.first(); leaf.text = buffer("copy")
    print(wrapped.pair.left.text)
    print(Wrap(Marker(false), make()).first().value())
}
"#,
    );
    let loaded = bhasa::project::load_unparsed(Some(&entry)).unwrap();
    let mut input = case(
        name,
        bhasa::project::flattened_source(&loaded.files),
        accepted,
    );
    input.entry = Some(entry);
    input
}

#[test]
fn methods_imported_type_ids_and_duplicate_imports_preserve_acceptance() {
    let workspace = Workspace::new("method-import-contracts");
    let cases = [
        imported_case(
            &workspace,
            "valid",
            "def value(self) -> int { return self.number }",
            true,
        ),
        imported_case(
            &workspace,
            "wrong-type",
            "def value(self) -> bool { return true }",
            false,
        ),
    ];
    both(&workspace, &cases, "x86_64-unknown-linux-gnu");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn methods_imported_receivers_execute_with_independent_owned_returns() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("method-import-values");
    let target = native_target(&clang, &workspace);
    let input = imported_case(
        &workspace,
        "valid",
        "def value(self) -> int { return self.number }",
        true,
    );
    both(&workspace, &[input], &target);
    for frontend in frontends() {
        for optimization in ["-O0", "-O2"] {
            let executable = tracked_link(
                &clang,
                &workspace,
                &workspace.file(&format!("{frontend}-0.ll")),
                optimization,
            );
            let output = bounded(
                &mut Command::new(executable),
                &workspace,
                &format!("method-import-{frontend}-{optimization}"),
                30,
            );
            succeeds(&output);
            assert_eq!(
                stdout(&output),
                "42\nleft\n19\n",
                "{frontend}, {optimization}"
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn methods_explicit_bind_typing_matches_without_opening_sockets() {
    let cases = [
        case(
            "bind-literal-host",
            "def main() { var listener: tcp_listener = tcp_listen_on(\"127.0.0.1\", 0) }",
            true,
        ),
        case(
            "bind-owned-host",
            "def open(host: buffer, port: int) -> tcp_listener { return tcp_listen_on(host, port) }",
            true,
        ),
        case(
            "bind-method-temporary",
            "struct Address { host: buffer; def listener(self, port: int) -> tcp_listener { return tcp_listen_on(self.host, port) } }\ndef main() { var listener = Address(buffer(\"::1\")).listener(0) }",
            true,
        ),
        case(
            "bind-runtime-validation",
            "def main() { var listener = tcp_listen_on(\"\", -1) }",
            true,
        ),
        case(
            "bind-integer-host",
            "def main() { var listener = tcp_listen_on(1, 0) }",
            false,
        ),
        case(
            "bind-integer-vector-host",
            "def main() { var listener = tcp_listen_on(ints(), 0) }",
            false,
        ),
        case(
            "bind-boolean-port",
            "def main() { var listener = tcp_listen_on(\"127.0.0.1\", false) }",
            false,
        ),
        case(
            "bind-missing-port",
            "def main() { var listener = tcp_listen_on(\"127.0.0.1\") }",
            false,
        ),
        case(
            "bind-extra-argument",
            "def main() { var listener = tcp_listen_on(\"127.0.0.1\", 0, 1) }",
            false,
        ),
        case(
            "bind-wrong-handle-type",
            "def main() { var listener: tcp_socket = tcp_listen_on(\"127.0.0.1\", 0) }",
            false,
        ),
    ];
    both(
        &Workspace::new("method-bind-types"),
        &cases,
        "x86_64-unknown-linux-gnu",
    );
}
