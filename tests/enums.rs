use std::fs;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

mod common;
use common::*;

fn accepted(source: &str) -> bhasa::ast::CheckedProgram {
    bhasa::check(source).unwrap_or_else(|errors| panic!("{errors:?}\n{source}"))
}

fn rejected(source: &str, code: &str) {
    let errors = bhasa::check(source).expect_err(source);
    assert!(
        errors.iter().any(|error| error.code == code),
        "{errors:?}\n{source}"
    );
    assert!(errors.iter().all(|error| error.span.end <= source.len()));
}

#[test]
fn enums_are_nominal_and_match_bindings_are_checked_and_scoped() {
    let checked = accepted(
        r#"
enum OptionInt { none; some(value: int) }
enum Result { ok(value: OptionInt); err(message: buffer) }
def number(result: Result) -> int {
    match result {
        case Result.ok(value) {
            match value { case OptionInt.none { return 0 } case OptionInt.some(n) { return n } }
        }
        case Result.err(message) { return len(message) }
    }
}
def main() { print(number(Result.ok(OptionInt.some(42)))) }
"#,
    );
    assert_eq!(checked.program.enums.len(), 2);
    assert_eq!(
        checked.signatures["number"].params,
        [bhasa::ast::Type::Enum(1)]
    );
    let ir = bhasa::codegen::emit_llvm(&checked, "aarch64-apple-darwin").unwrap();
    assert!(ir.contains("switch i64"));
    assert!(ir.contains("@bhasa.enum.1.clone"));
    assert!(ir.contains("; bhasa runtime: core"));
    accepted("enum E { zero(); one(x: int,); } def main() { match E.zero() { case E.zero() {} case E.one(x,) { print(x) } } }");
    rejected(
        "enum E { some(x:int) } def main() { match E.some(1) { case E.some(n) {} } print(n) }",
        "S008",
    );
    rejected(
        "enum A { one } enum B { one } def f(a:A) {} def main() { f(B.one()) }",
        "S010",
    );
}

#[test]
fn invalid_enum_schemas_and_forged_payload_access_are_rejected() {
    for source in [
        "enum E {}",
        "enum E { a; a }",
        "enum E { a(x:int, x:bool) }",
        "enum E { a(x:void) }",
        "enum E { a(x:task) }",
        "enum E { a(x:E) }",
        "enum E { a(x:R) } struct R { value:E }",
        "enum E { a } enum E { b }",
        "enum E { a } struct E {}",
    ] {
        rejected(source, "S030");
    }
    rejected("enum E { a(x:Missing) }", "S025");
    for expression in [
        "E()",
        "E.a",
        "E.missing()",
        "E.a().tag",
        "E.a().payload",
        "E.a().a()",
    ] {
        assert!(
            bhasa::check(&format!(
                "enum E {{ a }} def main() {{ var x = {expression} }}"
            ))
            .is_err(),
            "{expression}"
        );
    }
    rejected(
        "enum E { a(x:int) } def main() { var x = E.a(true) }",
        "S010",
    );
    for expression in ["E.a()", "E.a(1,2)"] {
        rejected(
            &format!("enum E {{ a(x:int) }} def main() {{ var x = {expression} }}"),
            "S012",
        );
    }
    // Type names resolve as a namespace only when no local shadows them.
    rejected("enum E { a } def main() { var E = 1; E.a() }", "S029");
}

#[test]
fn match_rejects_missing_duplicate_wrong_and_unbound_cases() {
    let prefix =
        "enum E { none; some(value:int) } enum F { none } def main() { var e = E.some(1); ";
    for (body, code) in [
        ("match e {}", "S032"),
        ("match e { case E.none {} }", "S032"),
        (
            "match e { case E.none {} case E.none {} case E.some(x) {} }",
            "S032",
        ),
        ("match e { case F.none {} case E.some(x) {} }", "S031"),
        ("match e { case E.none {} case E.missing(x) {} }", "S031"),
        ("match e { case E.none {} case E.some {} }", "S031"),
        ("match e { case E.none(x) {} case E.some(x) {} }", "S031"),
        ("match 1 { case E.none {} case E.some(x) {} }", "S031"),
    ] {
        rejected(&format!("{prefix}{body} }}"), code);
    }
    rejected(
        "enum E { pair(a:int,b:int) } def main() { match E.pair(1,2) { case E.pair(x,x) {} } }",
        "S007",
    );
    rejected("enum E { none; some(x:int) } def f(e:E) -> int { match e { case E.none {} case E.some(x) { return x } } }", "S014");
    rejected(
        "enum E { none } def f(e:E) -> int { match e { case E.none { return 1 } } return 2 }",
        "S015",
    );
}

#[test]
fn enum_borrows_obey_record_alias_rules_while_constructors_snapshot() {
    let prefix = r#"
enum E { value(text:buffer) }
struct Holder { value:E; def replace(self) -> int { self.value = E.value(buffer("new")); return 0 } }
enum Snapshot { pair(value:E, next:int) }
def use(value:E, n:int) {}
def both(value:E, holder:Holder) {}
"#;
    for expression in [
        "use(holder.value, holder.replace())",
        "both(holder.value, holder)",
    ] {
        rejected(&format!("{prefix} def main() {{ var holder = Holder(E.value(buffer(\"old\"))); {expression} }}"), "S028");
    }
    accepted(&format!("{prefix} def main() {{ var holder = Holder(E.value(buffer(\"old\"))); var snapshot = Snapshot.pair(holder.value, holder.replace()) }}"));
}

#[test]
fn merged_modules_resolve_enum_names_and_payload_ids() {
    let mut program = bhasa::parser::parse("def value(result:Result) -> Item { match result { case Result.ok(item) { return item } } }").unwrap();
    program.merge(
        bhasa::parser::parse(
            "struct Unrelated {} struct Item { n:int } enum Result { ok(item:Item) }",
        )
        .unwrap(),
    );
    let checked = bhasa::sema::check(program).unwrap();
    let result_id = checked
        .program
        .type_names
        .iter()
        .position(|name| name == "Result")
        .unwrap();
    let item_id = checked
        .program
        .type_names
        .iter()
        .position(|name| name == "Item")
        .unwrap();
    assert_eq!(
        checked.signatures["value"].params,
        [bhasa::ast::Type::Enum(result_id)]
    );
    assert_eq!(
        checked.program.enums[0].variants[0].fields[0].ty,
        bhasa::ast::Type::Record(item_id)
    );
    bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").unwrap();
}

#[test]
fn checked_ast_cannot_forge_enum_payloads_or_nonexhaustive_dispatch() {
    use bhasa::ast::{StmtKind, Type};
    let checked = accepted("enum E { none; some(n:int) } def main() { match E.some(1) { case E.none {} case E.some(n) { print(n) } } }");
    for mutation in 0..6 {
        let mut program = checked.clone();
        match mutation {
            0 => program.program.enums[0].variants[1].fields[0].ty = Type::Task,
            1 => program.program.enums[0].variants[1].fields[0].ty = Type::Enum(0),
            2 => program.program.enums[0].variants[1].fields[0].ty = Type::Enum(999),
            _ => {
                let StmtKind::Match { arms, .. } =
                    &mut program.program.functions[0].body.as_mut().unwrap()[0].kind
                else {
                    panic!()
                };
                match mutation {
                    3 => {
                        arms.pop();
                    }
                    4 => {
                        arms[1].variant = "none".to_owned();
                    }
                    5 => {
                        arms[1].bindings[0].ty = Some(Type::Bool);
                    }
                    _ => unreachable!(),
                }
            }
        }
        assert!(
            bhasa::codegen::emit_llvm(&program, "x86_64-unknown-linux-gnu").is_err(),
            "mutation {mutation}"
        );
    }
    let ir = bhasa::codegen::emit_llvm(&checked, "wasm32-unknown-unknown").unwrap();
    assert!(ir.contains("declare ptr @malloc(i32)"));
    assert!(ir.contains("to i32)"));
}

#[test]
fn enum_match_preserves_worker_and_linear_task_rules() {
    let prefix = "enum E { value(n:int) } extern def foreign(n:int) -> int\ndef worker(n:int) -> int { match E.value(n) { case E.value(value) { return foreign(value) } } }";
    rejected(&format!("{prefix} def main() {{ var job = spawn(worker(1)); var result = join(job); print(result) }}"), "S020");
    let prefix = "enum E { value(n:int) } def worker(n:int) -> int { return n } ";
    rejected(&format!("{prefix} def main() {{ match E.value(1) {{ case E.value(n) {{ var job = spawn(worker(n)) }} }} }}"), "S019");
    rejected(&format!("{prefix} def main() {{ var job = spawn(worker(1)); match E.value(1) {{ case E.value(n) {{ join(job) }} }} }}"), "S019");
    rejected(&format!("{prefix} def main() {{ var job = spawn(worker(1)); match E.value(1) {{ case E.value(job) {{}} }} join(job) }}"), "S019");
    accepted(&format!("{prefix} def main() {{ match E.value(1) {{ case E.value(n) {{ var job = spawn(worker(n)); var result = join(job); print(result) }} }} }}"));
}

const OWNERSHIP: &str = r#"
enum Maybe { none; some(value: buffer) }
struct Box {
    value: Maybe
    def replace(self) -> int { self.value = Maybe.some(buffer("new")); return 7 }
    def copy(self) -> Maybe { return self.value }
}
struct Counter { count:int }
enum Result { ok(value: Box, values: ints, label: str, active: bool); err(message: buffer) }
enum Pair { value(old: Maybe, number: int) }
def copy(value: Maybe) -> Maybe { return value }
def read(value: Maybe) -> buffer {
    match value { case Maybe.none { return buffer("none") } case Maybe.some(text) { return text } }
}
def make(counter: Counter) -> Maybe {
    counter.count = counter.count + 1
    return Maybe.some(buffer("made"))
}
def main() -> int {
    var parent = Box(Maybe.some(buffer("old")))
    var pair = Pair.value(parent.value, parent.replace())
    match pair {
        case Pair.value(old, number) {
            if not same(read(old), "old") { return 1 }
            if number != 7 { return 2 }
        }
    }
    if not same(read(parent.value), "new") { return 3 }
    var original = copy(parent.copy())
    match original {
        case Maybe.none { return 4 }
        case Maybe.some(text) {
            original = Maybe.none()
            set(text, 0, 88)
            if not same(text, "Xew") { return 5 }
        }
    }
    if not same(read(parent.value), "new") { return 6 }
    var values = ints()
    push(values, 42)
    var result = Result.ok(parent, values, "success", true)
    match result {
        case Result.err(message) { return 7 }
        case Result.ok(value, snapshot, label, active) {
            value.value = Maybe.none()
            set(snapshot, 0, 99)
            if not active { return 8 }
            print(label)
        }
    }
    match result {
        case Result.err(message) { return 9 }
        case Result.ok(value, snapshot, label, active) {
            if not same(read(value.value), "new") { return 10 }
            if get(snapshot, 0) != 42 { return 11 }
        }
    }
    var counter = Counter(0)
    var i = 0
    while i < 30 {
        match make(counter) {
            case Maybe.none { return 12 }
            case Maybe.some(text) { if not same(text, "made") { return 13 } }
        }
        var empty = Maybe.none()
        var duplicate = empty
        var early = read(Maybe.some(buffer("early")))
        if not same(early, "early") { return 14 }
        i = i + 1
    }
    if counter.count != 30 { return 15 }
    print("enum ownership ok")
    return 0
}
"#;

#[test]
fn enums_execute_owned_snapshots_and_nested_payloads_at_o0_and_o2() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("enums-native");
    let source = workspace.write("enums.bh", OWNERSHIP);
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "success\nenum ownership ok\n");
    }
}

#[test]
fn enum_payloads_can_own_tcp_handles_and_workers_can_match() {
    let source = r#"
enum Connection { listener(value:tcp_listener); socket(value:tcp_socket); empty }
enum Number { value(n:int) }
def worker(n:int) -> int { match Number.value(n) { case Number.value(value) { return value * 2 } } }
def main() {
    var handle = Connection.listener(tcp_listen_on("invalid-address", 0))
    var duplicate = handle
    match duplicate {
        case Connection.listener(value) { print(tcp_listener_status(value) != 0) }
        case Connection.socket(value) { print(tcp_socket_status(value) != 0) }
        case Connection.empty {}
    }
    var job = spawn(worker(21))
    var result = join(job)
    print(result)
}
"#;
    let checked = accepted(source);
    let ir = bhasa::codegen::emit_llvm(&checked, "aarch64-apple-darwin").unwrap();
    assert!(ir.contains("; bhasa runtime: net"));
    assert!(ir.contains("; bhasa runtime: threads"));
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("enums-network-worker");
    let source = workspace.write("main.bh", source);
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "true\n42\n");
    }
}

#[test]
fn aggregate_schema_depth_is_bounded_across_records_and_enums() {
    let workspace = Workspace::new("enum-depth");
    for count in [128, 129] {
        let mut source = String::from("enum E0 { unit }\n");
        for index in 1..count {
            if index % 2 == 0 {
                source.push_str(&format!("enum E{index} {{ value(v:E{}) }}\n", index - 1));
            } else {
                source.push_str(&format!("struct E{index} {{ value:E{} }}\n", index - 1));
            }
        }
        let path = workspace.write("depth.bh", &source);
        let result = workspace.run(&["check", as_str(&path)]);
        if count == 128 {
            succeeds(&result);
        } else {
            fails(&result);
            assert!(stderr(&result).contains("128"));
        }
    }
}

#[test]
fn nested_match_parser_depth_has_the_existing_cli_boundary() {
    let workspace = Workspace::new("match-depth");
    for (depth, valid) in [(128, true), (129, false)] {
        let source = format!(
            "enum E {{ unit }} def main() {{ var value = E.unit(); {} {} }}",
            "match value { case E.unit {".repeat(depth),
            "} }".repeat(depth)
        );
        let path = workspace.write("nested.bh", &source);
        let ir = workspace.file("nested.ll");
        let result = workspace.run(&["emit-llvm", as_str(&path), "-o", as_str(&ir)]);
        if valid {
            succeeds(&result);
        } else {
            fails(&result);
            assert!(stderr(&result).contains("P004"));
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn enum_native_ownership_is_instrumented_and_frees_every_allocation() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("enum-sanitizers");
    let source = workspace.write("enums.bh", OWNERSHIP);
    let ir_path = workspace.file("enums.ll");
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
        let executable = workspace.file("enums");
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
        assert_eq!(stdout(&result), "success\nenum ownership ok\n");
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}
