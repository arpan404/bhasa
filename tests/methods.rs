use std::fs;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

mod common;
use common::*;

fn accepted(source: &str) -> bhasa::ast::CheckedProgram {
    bhasa::check(source)
        .unwrap_or_else(|errors| panic!("valid methods rejected:\n{source}\n{errors:?}"))
}

fn rejected(source: &str, message: &str) {
    let errors = bhasa::check(source).expect_err(source);
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "expected {message:?}: {errors:?}\n{source}"
    );
}

#[test]
fn methods_resolve_forward_calls_and_keep_namespaces_independent() {
    let source = r#"
struct Point {
    x: int
    def total(self) -> int { return self.next() }
    def next(self) -> int { return self.x }
    def print(self) -> int { return self.x }
    def main(self) -> int { return self.x }
    def Point(self) -> int { return self.x }
    def bhasa_rt_name(self) -> int { return self.x }
    def spawn(self) -> int { return self.x }
    def join(self) -> int { return self.x }
    def buffer(self) -> int { return self.x }
    def copy(self) -> Point { return self }
}
struct Other { def total(self) -> int { return 9 } }
def total() -> int { return 1 }
def main() {
    var p = Point(42)
    print(p.total())
    print(p.copy().print())
    print(p.main())
    print(p.Point())
    print(p.bhasa_rt_name())
    print(p.spawn())
    print(p.join())
    print(p.buffer())
    print(Other().total())
    print(total())
}
"#;
    let checked = accepted(source);
    assert_eq!(
        checked.signatures["Point.total"].params,
        [bhasa::ast::Type::Record(0)]
    );
    assert!(checked.signatures.contains_key("Other.total"));
    assert!(checked.signatures.contains_key("total"));
    assert_eq!(checked.program.records[0].methods[0].name, "total");
    let ir = bhasa::codegen::emit_llvm(&checked, "aarch64-apple-darwin").unwrap();
    assert!(ir.contains("@bhasa.fn.Point.total(ptr"));
    assert!(ir.contains("@bhasa.fn.Other.total(ptr"));
    assert!(ir.contains("@bhasa.fn.total()"));
}

#[test]
fn invalid_methods_receivers_signatures_and_members_are_rejected() {
    for source in [
        "struct P { def m() {} }",
        "struct P { def m(other) {} }",
        "struct P { def m(self: P) {} }",
        "struct P { def m(self, self: P) {} }",
        "struct P { extern def m(self) }",
        "struct P { def m(self) }",
    ] {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
    for (source, message) in [
        ("struct P { m: int; def m(self) {} }", "conflicts"),
        (
            "struct P { def m(self) {} def m(self) {} }",
            "more than once",
        ),
        (
            "struct P { def m(self) { self = P() } }",
            "cannot be rebound",
        ),
        (
            "struct P { x: int; def m(self) -> int { return x } }",
            "unknown variable 'x'",
        ),
        ("struct P { def m(self) -> int {} }", "every path"),
        (
            "struct P { def m(self, x: int) {} } def main() { P().m() }",
            "expects",
        ),
        (
            "struct P { def m(self, x: int) {} } def main() { P().m(true) }",
            "requires int",
        ),
        (
            "struct P { def m(self) {} } def main() { P().m(1) }",
            "expects",
        ),
        ("struct P {} def main() { P().missing() }", "no method"),
        ("struct P { x: int } def main() { P(1).x() }", "no method"),
        (
            "struct P { def m(self) {} } def main() { var f = P().m }",
            "no field",
        ),
        ("def main() { 1.m() }", "receiver must be a record"),
        (
            "struct P { def m(self) {} } def main() { P.m() }",
            "unknown variable",
        ),
    ] {
        rejected(source, message);
    }
}

#[test]
fn method_receivers_obey_direct_and_later_argument_alias_rules() {
    let prefix = r#"
struct Leaf {
    text: buffer
    def use(self, n: int) -> int { return len(self.text) + n }
    def size(self) -> int { return len(self.text) }
    def replace(self) -> int { self.text = buffer("new"); return 0 }
    def consume(self, text: buffer) {}
    def pair(self, other: Leaf) {}
}
struct Pair {
    child: Leaf
    def replace(self) -> int { self.child = Leaf(buffer("new")); return 0 }
    def value(self) -> Pair { return self }
}
"#;
    for expression in [
        "p.child.consume(p.child.text)",
        "p.child.pair(p.child)",
        "p.child.use(p.replace())",
        "p.child.use(len(to_buffer(p.replace())))",
        "p.child.use(p.value().replace() + p.replace())",
    ] {
        rejected(
            &format!("{prefix} def main() {{ var p = Pair(Leaf(buffer(\"old\"))); {expression} }}"),
            "overlapping",
        );
    }
    for expression in [
        "p.child.use(p.child.size())",
        "p.child.use(p.child.replace())",
        "p.child.use(len(p.child.text))",
        "p.child.pair(Leaf(p.child.text))",
        "p.value().child.use(p.replace())",
    ] {
        accepted(&format!(
            "{prefix} def main() {{ var p = Pair(Leaf(buffer(\"old\"))); {expression} }}"
        ));
    }
}

#[test]
fn worker_safety_sees_resolved_methods_and_recursive_method_graphs() {
    for methods in [
        "def work(self, n: int) -> int { return foreign(n) }",
        "def work(self, n: int) -> int { return self.next(n) } def next(self, n: int) -> int { return foreign(n) }",
        "def work(self, n: int) -> int { if n == 0 { return foreign(n) } return self.next(n - 1) } def next(self, n: int) -> int { return self.work(n) }",
        "def work(self, n: int) -> int { return global(n) }",
    ] {
        let record = format!("struct Worker {{ {methods} }}");
        let globals = "extern def foreign(n: int) -> int\ndef global(n: int) -> int { return foreign(n) }\ndef worker(n: int) -> int { return Worker().work(n) }\ndef main() { var job = spawn(worker(3)); print(join(job)) }";
        // Join must itself be a complete initializer; isolate worker safety from
        // that independent restriction so the expected diagnostic is decisive.
        let globals = globals.replace("print(join(job))", "var answer = join(job); print(answer)");
        rejected(&format!("{globals}\n{record}"), "can call extern function 'foreign'");
        rejected(&format!("{record}\n{globals}"), "can call extern function 'foreign'");
    }
    accepted("struct Worker { def work(self, n: int) -> int { if n == 0 { return 1 } return self.work(n - 1) } } def worker(n: int) -> int { return Worker().work(n) } def main() { var job = spawn(worker(3)); var answer = join(job); print(answer) }");
    accepted("extern def foreign(n: int) -> int\nstruct Worker { def foreign(self, n: int) -> int { return n } def work(self, n: int) -> int { return self.foreign(n) } } def worker(n: int) -> int { return Worker().work(n) } def main() { var job = spawn(worker(3)); var answer = join(job) }");
    rejected("struct Worker { def work(self, n: int) -> int { return n } } def main() { var w = Worker(); var job = spawn(w.work(3)); var answer = join(job) }", "spawn does not accept method calls");
}

#[test]
fn malformed_checked_method_targets_cannot_enter_the_scalar_worker_abi() {
    use bhasa::ast::{ExprKind, StmtKind};
    let source = "struct Worker { def work(self, n: int) -> int { return n } } def worker(n: int) -> int { return n } def main() { var job = spawn(worker(3)); var result = join(job) }";
    let mut checked = accepted(source);
    let main = checked
        .program
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap();
    let StmtKind::Var { value, .. } = &mut main.body.as_mut().unwrap()[0].kind else {
        unreachable!()
    };
    let ExprKind::Call { args, .. } = &mut value.kind else {
        unreachable!()
    };
    let ExprKind::Call { name, .. } = &mut args[0].kind else {
        unreachable!()
    };
    *name = "Worker.work".into();
    let errors = bhasa::codegen::emit_llvm(&checked, "aarch64-apple-darwin").unwrap_err();
    assert!(errors[0]
        .message
        .contains("defined global one-argument Bhasa worker"));
}

#[test]
fn imported_methods_remap_receiver_return_and_parameter_types() {
    let workspace = Workspace::new("methods-imports");
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"methods\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n",
    );
    workspace.write("types.bh", "struct Other {}\nstruct Value { n: int; def add(self, other: Value) -> Value { return Value(self.n + other.n) } def get(self) -> int { return self.n } }\n");
    workspace.write("use.bh", "import \"types.bh\"\nstruct Wrapper { item: Value; def value(self) -> Value { return self.item } }\n");
    let source = workspace.write("main.bh", "import \"use.bh\"\nimport \"types.bh\"\nstruct Local { x: bool }\ndef main() { var w = Wrapper(Value(20)); print(w.value().add(Value(22)).get()) }\n");
    succeeds(&workspace.run(&["check"]));
    if clang().is_some() {
        let executable = build(&workspace, &source, false);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "42\n");
    }
    workspace.write("types.bh", "extern def foreign(n: int) -> int\nstruct Worker { def run(self, n: int) -> int { return foreign(n) } }\n");
    workspace.write(
        "use.bh",
        "import \"types.bh\"\ndef worker(n: int) -> int { return Worker().run(n) }\n",
    );
    workspace.write(
        "main.bh",
        "import \"use.bh\"\ndef main() { var job = spawn(worker(1)); var answer = join(job) }\n",
    );
    let result = workspace.run(&["check"]);
    fails(&result);
    assert!(stderr(&result).contains("can call extern function 'foreign'"));
}

const OWNERSHIP: &str = r#"
struct Counter { count: int }
struct Leaf {
    text: buffer
    values: ints
    def append(self, suffix: str) -> buffer { append(self.text, suffix); push(self.values, 42); return self.text }
    def copy(self) -> Leaf { return self }
    def bytes(self) -> buffer { return self.text }
    def length(self) -> int { return len(self.text) }
    def use(self, value: int) -> int { return value + len(self.text) }
    def set(self, text: buffer) { self.text = text }
}
struct Pair {
    leaf: Leaf
    def copy(self) -> Pair { return self }
    def child(self) -> Leaf { return self.leaf }
    def replace(self) -> buffer { self.leaf = Leaf(buffer("replacement"), ints()); return buffer("assigned") }
    def early(self, take: bool) -> Leaf {
        var local = self.leaf
        var i = 0
        while i < 2 {
            var nested = Leaf(buffer("loop"), ints())
            if take { return local }
            i = i + 1
        }
        return Leaf(buffer("fallback"), ints())
    }
}
def make(counter: Counter) -> Pair {
    counter.count = counter.count + 1
    return Pair(Leaf(buffer("temporary"), ints()))
}
def main() -> int {
    var counter = Counter(0)
    var parent = Pair(Leaf(buffer("old"), ints()))
    var returned = parent.leaf.append("!")
    if not same(returned, "old!") { return 1 }
    set(returned, 0, 88)
    if not same(parent.leaf.text, "old!") { return 2 }
    if get(parent.leaf.values, 0) != 42 { return 3 }
    var copy = parent.copy()
    copy.leaf.set(buffer("copy"))
    if not same(parent.leaf.text, "old!") { return 4 }
    if parent.leaf.use(parent.leaf.length()) != 8 { return 5 }
    var child = parent.child()
    child.text = buffer("child")
    if not same(parent.leaf.text, "old!") { return 6 }
    var temporary = make(counter).child().append("!")
    if counter.count != 1 { return 7 }
    if not same(temporary, "temporary!") { return 8 }
    var projected = make(counter).copy().leaf.bytes()
    if counter.count != 2 { return 9 }
    if not same(projected, "temporary") { return 10 }
    parent.leaf.text = parent.replace()
    if not same(parent.leaf.text, "assigned") { return 11 }
    var i = 0
    while i < 50 {
        var a = parent.early(true)
        var b = parent.early(false)
        if not same(a.text, "assigned") { return 12 }
        if not same(b.text, "fallback") { return 13 }
        make(counter).leaf.append("discarded")
        i = i + 1
    }
    if counter.count != 52 { return 14 }
    print("method ownership ok")
    return 0
}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn methods_keep_receivers_and_owned_returns_alive_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("methods-sanitizers");
    let source = workspace.write("methods.bh", OWNERSHIP);
    let ir_path = workspace.file("methods.ll");
    succeeds(&workspace.run(&["emit-llvm", as_str(&source), "-o", as_str(&ir_path)]));
    let ir = fs::read_to_string(&ir_path)
        .unwrap()
        .replace("@malloc(", "@tracked_malloc(")
        .replace("@free(", "@tracked_free(");
    let ir = ir
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    fs::write(&ir_path, ir).unwrap();
    let runtime = workspace.write("tracked.c", &format!(r#"
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) {{ if (live) {{ fprintf(stderr, "live allocations: %ld\n", live); _Exit(99); }} }}
void *tracked_malloc(size_t n) {{ if (!registered) {{ registered = 1; atexit(check_live); }} void *p = malloc(n); if (p) ++live; return p; }}
void *tracked_realloc(void *p, size_t n) {{ int empty = p == NULL; void *q = realloc(p, n); if (q && empty) ++live; return q; }}
void tracked_free(void *p) {{ if (p) --live; free(p); }}
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
{}
"#, include_str!("../runtime/core.c")));
    fs::copy(fixture("runtime/core.h"), workspace.file("core.h")).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("methods");
        let result = Command::new(&clang)
            .args([
                optimization,
                "-fsanitize=address,undefined",
                "-fno-sanitize-recover=all",
                "-fno-omit-frame-pointer",
            ])
            .arg(&ir_path)
            .arg(&runtime)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&result);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "method ownership ok\n");
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}
