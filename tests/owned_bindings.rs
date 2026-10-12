//! Buffer/ints transfer and independent public-emitter checks.
const CASES: &[(&str, &str, bool, Option<&str>)] = &[
    (
        "fresh",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { sink(buffer("a")) }
"##,
        true,
        None,
    ),
    (
        "local_transfer",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); var y=move x; print(y) }
"##,
        true,
        None,
    ),
    (
        "owned_return",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); var y=take(move x); print(y) }
"##,
        true,
        None,
    ),
    (
        "rhs_move_reinit",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); x=take(move x); print(x) }
"##,
        true,
        None,
    ),
    (
        "reinit",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); sink(move x); x=buffer(); print(x) }
"##,
        true,
        None,
    ),
    (
        "branch_local",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { if true { var x=buffer(); sink(move x) } }
"##,
        true,
        None,
    ),
    (
        "loop_local",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var i=0; while i<2 { var x=buffer(); sink(move x); i=i+1 } }
"##,
        true,
        None,
    ),
    (
        "shadow",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); if true { var x=buffer(); sink(move x) }; print(x) }
"##,
        true,
        None,
    ),
    (
        "named_owned",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); sink(x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "moved_read",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); sink(move x); print(x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "moved_mutate",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); sink(move x); push(x,1) }
"##,
        false,
        Some("S038"),
    ),
    (
        "double_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); two(move x,move x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "overlap_owned_first",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); pair(move x,x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "overlap_borrow_first",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); reverse(x,move x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "nested_overlap",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); reverse(x,take(move x)) }
"##,
        false,
        Some("S038"),
    ),
    (
        "later_mutate",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); pair(move x,to_buffer(len(x))) }
"##,
        false,
        Some("S038"),
    ),
    (
        "let_reinit",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { let x=buffer(); sink(move x); x=buffer() }
"##,
        false,
        None,
    ),
    (
        "outer_branch",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); if true { sink(move x) } }
"##,
        false,
        Some("S038"),
    ),
    (
        "outer_loop",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); while false { sink(move x) } }
"##,
        false,
        Some("S038"),
    ),
    (
        "condition_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); if flag(move x) {} }
"##,
        false,
        Some("S038"),
    ),
    (
        "while_condition",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); while flag(move x) {} }
"##,
        false,
        Some("S038"),
    ),
    (
        "short_circuit",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); var b=false and flag(move x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "self_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); var x=move x }
"##,
        false,
        None,
    ),
    (
        "nested_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); print(move x) }
"##,
        false,
        Some("S038"),
    ),
    (
        "scalar_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=1; var y=move x }
"##,
        false,
        Some("S038"),
    ),
    (
        "borrowed_move",
        r##"def bad(x:buffer)->buffer {return move x} def main() {}
"##,
        false,
        Some("S038"),
    ),
    (
        "context_names",
        r##"def move(owned: int)->int {return owned} def main() {print(move(1))}
"##,
        true,
        None,
    ),
    (
        "owned_rebind",
        r##"def f(owned x:buffer) {x=buffer("new")} def main() {f(buffer())}
"##,
        true,
        None,
    ),
    (
        "ordinary_return",
        r##"def f(x:buffer)->buffer {append(x,"!");return x} def main(){var x=buffer();var y=f(x);print(x);print(y)}
"##,
        true,
        None,
    ),
    (
        "ints_transfer",
        r##"def f(owned x:ints)->ints {push(x,1);return move x} def main(){var x=ints();var y=f(move x);print(len(y))}
"##,
        true,
        None,
    ),
    (
        "unsupported_record",
        r##"struct Box {n:int} def bad(owned x:Box) {} def main(){}
"##,
        false,
        Some("S038"),
    ),
    (
        "unsupported_string",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x="text"; var y=move x }
"##,
        false,
        Some("S038"),
    ),
    (
        "projected_move",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); var y=move x.n }
"##,
        false,
        None,
    ),
    (
        "conditional_reinit",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); sink(move x); if true {x=buffer()} }
"##,
        false,
        Some("S038"),
    ),
    (
        "immutable_destination",
        r##"def take(owned x: buffer) -> buffer { return move x }
def sink(owned x: buffer) {}
def pair(owned a: buffer, b: buffer) {}
def reverse(a: buffer, owned b: buffer) {}
def two(owned a: buffer, owned b: buffer) {}
def flag(owned a: buffer) -> bool { return true }
def main() { var x=buffer(); let y=move x; print(y) }
"##,
        true,
        None,
    ),
    (
        "borrowed_rebind",
        r##"def f(x:buffer){x=buffer()} def main(){}
"##,
        false,
        None,
    ),
    (
        "task_move",
        r##"def worker(n:int)->int{return n} def main(){var x=spawn(worker(1));var y=move x}
"##,
        false,
        Some("S038"),
    ),
    (
        "owned_task_param",
        r##"def f(owned x:task){} def main(){}
"##,
        false,
        Some("S038"),
    ),
    (
        "task_target",
        r##"def worker(owned x:buffer)->int{return 1} def main(){var x=spawn(worker(buffer()));var y=join(x);print(y)}
"##,
        false,
        Some("S038"),
    ),
    (
        "owned_method",
        r##"struct Box {n:int; def f(self,owned x:buffer) {}} def main(){}
"##,
        false,
        Some("S038"),
    ),
    (
        "result_move",
        r##"enum Result {ok(value:buffer);error(value:int)} def f()->Result{return Result.ok(buffer())} def main(){var x=f();var y=move x}
"##,
        false,
        Some("S038"),
    ),
];
#[test]
fn owned_bindings_host_acceptance() {
    for &(name, source, accept, code) in CASES {
        let result = bhasa::check(source)
            .and_then(|checked| bhasa::codegen::emit_llvm(&checked, "arm64-apple-macosx"));
        assert_eq!(result.is_ok(), accept, "{name}: {result:?}");
        if let Some(code) = code {
            assert!(
                result.unwrap_err().iter().any(|error| error.code == code),
                "{name}"
            );
        }
    }
}
#[test]
fn owned_bindings_emitter_rechecks_modes_and_initialization() {
    let mut checked =
        bhasa::check("def take(owned x:buffer)->buffer{return move x} def main(){}").unwrap();
    checked.signatures.get_mut("take").unwrap().modes.clear();
    let errors = bhasa::codegen::emit_llvm(&checked, "arm64-apple-macosx").unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S038"));
    let mut checked = bhasa::check("def main(){var x=buffer();var y=x;print(x)}").unwrap();
    if let bhasa::ast::StmtKind::Var { value, .. } =
        &mut checked.program.functions[0].body.as_mut().unwrap()[1].kind
    {
        value.kind = bhasa::ast::ExprKind::Move {
            name: "x".into(),
            name_span: value.span,
        };
    }
    let errors = bhasa::codegen::emit_llvm(&checked, "arm64-apple-macosx").unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S038"));
}
#[cfg(feature = "bhasa-frontend")]
#[test]
fn owned_bindings_native_parity() {
    for &(name, source, accept, code) in CASES {
        let result = bhasa::compile_with_bhasa(source, "arm64-apple-macosx");
        assert_eq!(result.is_ok(), accept, "{name}: {result:?}");
        if code == Some("S038") {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|error| error.code == "BH222"),
                "{name}"
            );
        }
    }
}
