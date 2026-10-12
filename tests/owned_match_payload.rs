const CASES: &[(&str, &str, bool, Option<&str>)] = &[
    (
        "buffer_payload",
        r##"enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer("x")){case Box.some(value){sink(move value)}}}"##,
        true,
        None,
    ),
    (
        "ints_payload",
        r##"enum Box{some(value:ints)} def sink(owned x:ints){} def main(){match Box.some(ints()){case Box.some(value){sink(move value)}}}"##,
        true,
        None,
    ),
    (
        "result_success_payload",
        r##"enum Result{ok(value:buffer);error(value:int)} def sink(owned x:buffer){} def main(){var r=Result.ok(buffer("ok"));match r{case Result.ok(value){sink(move value)} case Result.error(code){print(code)}}}"##,
        true,
        None,
    ),
    (
        "local_initializer_payload",
        r##"enum Box{some(value:buffer)} def main(){match Box.some(buffer("x")){case Box.some(value){var moved=move value;print(moved)}}}"##,
        true,
        None,
    ),
    (
        "return_payload",
        r##"enum Box{some(value:buffer)} def f()->buffer{match Box.some(buffer("x")){case Box.some(value){return move value}}} def main(){var x=f();print(x)}"##,
        true,
        None,
    ),
    (
        "payload_reinit",
        r##"enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer("x")){case Box.some(value){sink(move value);value=buffer("new");print(value)}}}"##,
        true,
        None,
    ),
    (
        "moved_payload_read",
        r##"enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer("x")){case Box.some(value){sink(move value);print(value)}}}"##,
        false,
        Some("S038"),
    ),
    (
        "moved_payload_mutate",
        r##"enum Box{some(value:ints)} def sink(owned x:ints){} def main(){match Box.some(ints()){case Box.some(value){sink(move value);push(value,1)}}}"##,
        false,
        Some("S038"),
    ),
    (
        "double_payload_move",
        r##"enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer("x")){case Box.some(value){sink(move value);sink(move value)}}}"##,
        false,
        Some("S038"),
    ),
    (
        "nested_arm_move",
        r##"enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer("x")){case Box.some(value){if true {sink(move value)}}}}"##,
        false,
        Some("S038"),
    ),
    (
        "enum_container_move",
        r##"enum Box{some(value:buffer)} def main(){var x=Box.some(buffer());var y=move x}"##,
        false,
        Some("S038"),
    ),
    (
        "pending_result_container",
        r##"enum Result{ok(value:buffer);error(value:int)} def main(){var x=Result.ok(buffer());var y=move x}"##,
        false,
        Some("S038"),
    ),
    (
        "pending_nested_result_payload",
        r##"enum Result{ok(value:buffer);error(value:int)} enum Box{some(value:Result)} def main(){match Box.some(Result.ok(buffer())){case Box.some(value){print(1)}}}"##,
        false,
        Some("S037"),
    ),
];
#[test]
fn match_payload_transfer_and_pending_obligations() {
    for &(name, source, accept, code) in CASES {
        let result =
            bhasa::check(source).and_then(|p| bhasa::codegen::emit_llvm(&p, "arm64-apple-macosx"));
        assert_eq!(result.is_ok(), accept, "{name}: {result:?}");
        if let Some(code) = code {
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|e| e.code == code), "{name}: {errors:?}");
            if name == "pending_result_container" {
                assert!(errors.iter().any(|e| e.code == "S037"));
            }
        }
    }
}
#[test]
fn emitter_rejects_corrupted_moved_payload_read() {
    use bhasa::ast::*;
    let mut p=bhasa::check("enum Box{some(value:buffer)} def sink(owned x:buffer){} def main(){match Box.some(buffer()){case Box.some(value){print(value);print(value)}}}").unwrap();
    if let StmtKind::Match { arms, .. } = &mut p.program.functions[1].body.as_mut().unwrap()[0].kind
    {
        let span = arms[0].body[0].span;
        arms[0].body[0].kind = StmtKind::Expr(Expr {
            kind: ExprKind::Call {
                name: "sink".into(),
                args: vec![Expr {
                    kind: ExprKind::Move {
                        name: "value".into(),
                        name_span: span,
                    },
                    ty: Some(Type::Buffer),
                    span,
                }],
            },
            ty: Some(Type::Void),
            span,
        });
    } else {
        panic!("expected match");
    }
    let errors = bhasa::codegen::emit_llvm(&p, "arm64-apple-macosx").unwrap_err();
    assert!(errors.iter().any(|e| e.code == "S038"));
}
#[cfg(feature = "bhasa-frontend")]
#[test]
fn match_payload_native_parity() {
    for &(name, source, accept, code) in CASES {
        let result = bhasa::compile_with_bhasa(source, "arm64-apple-macosx");
        assert_eq!(result.is_ok(), accept, "{name}: {result:?}");
        if let Some(code) = code {
            let native = if code == "S037" { "BH221" } else { "BH222" };
            assert!(
                result.unwrap_err().iter().any(|e| e.code == native),
                "{name}"
            );
        }
    }
}
