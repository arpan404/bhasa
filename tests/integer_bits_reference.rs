mod common;
use bhasa::ast::{Expr, ExprKind, Span, StmtKind, Type, UnaryOp};
use bhasa::lexer::{lex_rust, TokenKind};
use common::*;
use std::process::Command;

fn expression(source: &str) -> Expr {
    let source = format!("def main() {{ var value = {source} }}");
    let parsed = bhasa::parser::parse_tokens(lex_rust(&source).unwrap()).unwrap();
    let StmtKind::Var { value, .. } = &parsed.functions[0].body.as_ref().unwrap()[0].kind else {
        panic!("expected expression binding")
    };
    value.clone()
}

fn shape(expression: &Expr) -> String {
    match &expression.kind {
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Name(name) => name.clone(),
        ExprKind::Unary { op, operand } => format!("({op:?} {})", shape(operand)),
        ExprKind::Binary { op, left, right } => {
            format!("({op:?} {} {})", shape(left), shape(right))
        }
        other => panic!("unexpected expression {other:?}"),
    }
}

#[test]
fn integer_tokens_keep_radix_spelling_and_malformed_spans() {
    let source = "0x0 0XfF 0b101 0B01 0x1e2 & | ^ ~ << >> && || <= >=";
    let tokens = lex_rust(source).unwrap();
    for (token, spelling) in tokens.iter().zip(["0x0", "0XfF", "0b101", "0B01", "0x1e2"]) {
        assert_eq!(token.kind, TokenKind::Int(spelling.into()));
        assert_eq!(&source[token.span.start..token.span.end], spelling);
    }
    assert_eq!(
        tokens[5..]
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            TokenKind::BitAnd,
            TokenKind::BitOr,
            TokenKind::BitXor,
            TokenKind::BitNot,
            TokenKind::ShiftLeft,
            TokenKind::ShiftRight,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::LessEqual,
            TokenKind::GreaterEqual,
            TokenKind::Eof
        ]
    );
    for spelling in [
        "0x", "0X", "0b", "0B", "0xG", "0b102", "0B2", "0x1_2", "0b_1", "0x1u64", "0b1e2", "0x_",
        "123abc", "1_000",
    ] {
        let source = format!("# é\r\n{spelling} + 7");
        let errors = lex_rust(&source).unwrap_err();
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].code, "L002");
        assert_eq!(&source[errors[0].span.start..errors[0].span.end], spelling);
    }
    // A decimal exponent and a hexadecimal digit 'e' remain distinct.
    assert!(matches!(
        lex_rust("1e2").unwrap()[0].kind,
        TokenKind::Float(_)
    ));
    assert!(matches!(
        lex_rust("0x1e2").unwrap()[0].kind,
        TokenKind::Int(_)
    ));
}

#[test]
fn precedence_and_signed_literal_boundaries_are_explicit() {
    assert_eq!(shape(&expression("a or b and c == d < e | f ^ g & h << i + j * ~k")),
        "(Or a (And b (Eq c (Lt d (BitOr e (BitXor f (BitAnd g (Shl h (Add i (Mul j (BitNot k)))))))))))");
    assert_eq!(
        shape(&expression("flags & mask != 0")),
        "(Ne (BitAnd flags mask) 0)"
    );
    assert_eq!(shape(&expression("8 >> 1 << 2")), "(Shl (Shr 8 1) 2)");
    assert_eq!(
        shape(&expression("~1 & 7 ^ 4 | 8")),
        "(BitOr (BitXor (BitAnd (BitNot 1) 7) 4) 8)"
    );
    for (literal, expected) in [
        ("0x7fffffffffffffff", i64::MAX),
        ("0X7FFFFFFFFFFFFFFF", i64::MAX),
        ("-0x8000000000000000", i64::MIN),
        ("-0X8000000000000000", i64::MIN),
        (
            "0b111111111111111111111111111111111111111111111111111111111111111",
            i64::MAX,
        ),
        (
            "-0B1000000000000000000000000000000000000000000000000000000000000000",
            i64::MIN,
        ),
        ("0x000000000000000000000000000000000000000001", 1),
    ] {
        assert!(
            matches!(expression(literal).kind, ExprKind::Int(actual) if actual == expected),
            "{literal}"
        );
    }
    for literal in [
        "0x8000000000000000",
        "0xffffffffffffffff",
        "0x10000000000000000",
        "-0x8000000000000001",
        "-(0x8000000000000000)",
        "0b1000000000000000000000000000000000000000000000000000000000000000",
        "-0b1000000000000000000000000000000000000000000000000000000000000001",
    ] {
        let source = format!("def main() {{ print({literal}) }}");
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "P002"),
            "{literal}: {errors:?}"
        );
    }
}

#[test]
fn bitwise_types_are_int_only_and_checked_ast_validation_rejects_corruption() {
    for operator in ["&", "|", "^", "<<", ">>"] {
        for operands in [
            ("true", "false"),
            ("1.0", "1.0"),
            ("buffer()", "1"),
            ("1", "false"),
            ("1", "1.0"),
        ] {
            let source = format!(
                "def main() {{ print({} {operator} {}) }}",
                operands.0, operands.1
            );
            let errors = bhasa::check(&source).unwrap_err();
            assert!(
                errors.iter().any(|error| error.code == "S010"),
                "{source}: {errors:?}"
            );
        }
    }
    for operand in ["true", "1.0", "buffer()", "print(1)"] {
        assert!(bhasa::check(&format!("def main() {{ print(~{operand}) }}")).is_err());
    }
    let mut checked = bhasa::check("def main() { var value = 1 & 3 }").unwrap();
    let StmtKind::Var { value, .. } =
        &mut checked.program.functions[0].body.as_mut().unwrap()[0].kind
    else {
        panic!()
    };
    let ExprKind::Binary { right, .. } = &mut value.kind else {
        panic!()
    };
    right.ty = Some(Type::Bool);
    assert!(bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err());
    let expression = Expr {
        kind: ExprKind::Unary {
            op: UnaryOp::BitNot,
            operand: Box::new(Expr {
                kind: ExprKind::Int(1),
                span: Span::default(),
                ty: Some(Type::Int),
            }),
        },
        span: Span::default(),
        ty: Some(Type::Bool),
    };
    let StmtKind::Var { value, .. } =
        &mut checked.program.functions[0].body.as_mut().unwrap()[0].kind
    else {
        panic!()
    };
    *value = expression;
    assert!(bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_bit_patterns_evaluation_order_and_shift_traps_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("integer-bits-reference");
    let source = r#"
struct Counter { trace:int; def mark(self, value:int)->int { self.trace=self.trace*10+value; return value } }
def shifts(value:int, count:int)->int { return (value << count) >> count }
def main()->int {
    if 0Xff != 255 or 0B10101 != 21 or 0x1e2 != 482 { return 1 }
    if (0x55 & 0x0f) != 5 or (0x55 | 0x0f) != 95 or (0x55 ^ 0x0f) != 90 { return 2 }
    if ~0 != -1 or ~(-1) != 0 or ~(-0x8000000000000000) != 0x7fffffffffffffff { return 3 }
    if 1 << 63 != -0x8000000000000000 or -1 << 1 != -2 or -0x8000000000000000 << 1 != 0 { return 4 }
    if -1 >> 63 != -1 or -8 >> 2 != -2 or 0x7fffffffffffffff >> 63 != 0 { return 5 }
    if shifts(0x1234, 8) != 0x1234 or 5 << 0 != 5 or -5 >> 0 != -5 { return 6 }
    if 1 | 2 ^ 3 & 6 << 1 + 1 * 2 != 3 or 0xff & 0x08 == 0 { return 7 }
    var counter=Counter(0)
    if counter.mark(0) & counter.mark(2) != 0 or counter.trace != 2 { return 8 }
    if counter.mark(1) | counter.mark(3) != 3 or counter.trace != 213 { return 9 }
    if counter.mark(4) ^ counter.mark(5) != 1 or counter.trace != 21345 { return 10 }
    print("integer bits ok")
    return 0
}
"#;
    let triple = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let ir = bhasa::compile_to_llvm(source, &triple).unwrap();
    assert!(ir.contains("icmp ugt i64"));
    assert!(ir.contains("ashr i64"));
    assert!(!ir.contains("shl nsw") && !ir.contains("shl nuw"));
    for optimization in ["-O0", "-O2"] {
        let file = workspace.write("bits.ll", &ir);
        let executable = workspace.file("bits");
        succeeds(
            &Command::new(&clang)
                .arg(optimization)
                .arg(&file)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = Command::new(&executable).output().unwrap();
        succeeds(&output);
        assert_eq!(stdout(&output), "integer bits ok\n");
        for expression in [
            "0 << -1",
            "1 << 64",
            "0 >> 64",
            "-1 >> -1",
            "1 << 9223372036854775807",
            "1 >> -9223372036854775808",
            "--0x8000000000000000",
            "0x7fffffffffffffff + 1",
        ] {
            let source = format!("def main() {{ print({expression}) }}");
            let ir = bhasa::compile_to_llvm(&source, &triple).unwrap();
            let file = workspace.write("trap.ll", &ir);
            succeeds(
                &Command::new(&clang)
                    .arg(optimization)
                    .arg(&file)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap(),
            );
            let output = Command::new(&executable).output().unwrap();
            assert!(
                !output.status.success(),
                "{expression} {optimization} failed to trap"
            );
        }
    }
}
