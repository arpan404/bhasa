mod common;
use common::*;
use std::{fs, path::Path, process::Command};

const PROGRAM: &str = r#"
struct Sample {
    value: float
    label: buffer
    def scale(self, factor: float) -> float { self.value = self.value * factor; return self.value }
}
enum Reading { value(sample: Sample); absent }
struct Box[T] { value:T; def copy(self)->T { return self.value } }
def identity[T](value:T)->T { return value }
def main() -> int {
    var x: float = 1.5
    if x + 0.75 != 2.25 { return 1 }
    if x - 0.5 != 1.0 { return 2 }
    if x * 2.0 != 3.0 { return 3 }
    if x / 2.0 != 0.75 { return 4 }
    if not (x > 1.0 and x >= 1.5 and x < 2.0 and x <= 1.5) { return 5 }
    if -x != -1.5 { return 6 }
    if float(9007199254740993) != 9007199254740992.0 { return 7 }
    if int(-2.9) != -2 or int(2.9) != 2 or int(-0.0) != 0 { return 8 }
    if int(-9223372036854775808.0) != -9223372036854775808 { return 9 }
    if int(9223372036854774784.0) != 9223372036854774784 { return 10 }
    var nan = 0.0 / 0.0
    if nan == nan or nan < 0.0 or nan <= 0.0 or nan > 0.0 or nan >= 0.0 { return 11 }
    if not (nan != nan) { return 12 }
    if not (1.0 / 0.0 > 1.0e308) { return 13 }
    if not (1.0 / -0.0 < -1.0e308) { return 14 }
    var sample = Sample(1.25, buffer("old"))
    var reading = Reading.value(sample)
    if sample.scale(2.0) != 2.5 { return 15 }
    match reading {
        case Reading.absent { return 16 }
        case Reading.value(snapshot) {
            if snapshot.value != 1.25 or not same(snapshot.label, "old") { return 17 }
            snapshot.value = 42.0
            set(snapshot.label, 0, 88)
        }
    }
    if not same(sample.label, "old") { return 18 }
    var box = identity[Box[float]](Box[float](3.125))
    if box.copy() != 3.125 { return 19 }
    print(0.1)
    print(box.copy())
    print(-0.0)
    print("float ownership ok")
    return 0
}
"#;
const EXPECTED: &str = "0.10000000000000001\n3.125\n-0\nfloat ownership ok\n";

#[test]
fn decimal_scanning_types_and_formatting_are_explicit() {
    use bhasa::lexer::TokenKind;
    let source = "float 0.1 1e3 1E+3 25e-2 0002.500 1.0e0 1.member";
    let tokens = bhasa::lexer::lex_rust(source).unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::FloatType));
    for token in &tokens[1..7] {
        assert!(matches!(token.kind, TokenKind::Float(_)), "{token:?}");
    }
    assert!(matches!(tokens[7].kind, TokenKind::Int(_)));
    assert!(matches!(tokens[8].kind, TokenKind::Dot));
    for spelling in ["1e", "1e+", "1e-", "1.0e", "1e3f", "1.0_1", "1e+2_3"] {
        let errors = bhasa::lexer::lex_rust(spelling).unwrap_err();
        assert_eq!(errors[0].code, "L008", "{spelling}");
        assert_eq!(errors[0].span, bhasa::ast::Span::new(0, spelling.len()));
    }
    for expression in [".5", "1."] {
        assert!(bhasa::check(&format!("def main() {{ print({expression}) }}")).is_err());
    }
    let source = "def main(){var x:float=1.25e+2;print(int(x));print(float(2))}";
    let formatted = bhasa::format::format_source(source).unwrap();
    assert!(formatted.contains("int(x)"));
    assert!(formatted.contains("float(2)"));
    assert!(formatted.contains("1.25e+2"));
    bhasa::check(&formatted).unwrap();
    assert_eq!(formatted, bhasa::format::format_source(&formatted).unwrap());
}

#[test]
fn no_implicit_coercions_or_expanded_foreign_abi() {
    for source in [
        "def main() { var x:float = 1 }",
        "def main() { print(1 + 1.0) }",
        "def main() { print(1.0 + 1) }",
        "def main() { print(1 == 1.0) }",
        "def main() { print(1.0 % 0.5) }",
        "def main() { print(not 1.0) }",
        "def main() { print(1.0 and 2.0) }",
        "def main() { print(int(1)) }",
        "def main() { print(float(1.0)) }",
        "def main() { print(float(true)) }",
        "def main() { print(int()) }",
        "def main() { print(float(1,2)) }",
        "extern def f(value:float)->int",
        "extern def f(value:int)->float",
        "def main()->float { return 1.0 }",
    ] {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
    bhasa::check("def worker(value:int)->int { return int(float(value) * 2.0) } def main() { var task = spawn(worker(21)); var result = join(task); print(result) }").unwrap();
    bhasa::check("def worker(value:float)->float { return value } def main() { var task = spawn(worker(1.0)); var result = join(task) }").unwrap();
}

#[test]
fn finite_literals_and_checked_ast_validation_preserve_binary64_bits() {
    for literal in [
        "0.1",
        "1e308",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4e-324",
        "1e-9999999999999999",
        "0e999999999999999999",
    ] {
        let source = format!("def main() {{ print({literal}) }}");
        let program =
            bhasa::check(&source).unwrap_or_else(|errors| panic!("{literal}: {errors:?}"));
        let ir = bhasa::codegen::emit_llvm(&program, "x86_64-unknown-linux-gnu").unwrap();
        assert!(
            ir.contains(&format!(
                "0x{:016X}",
                literal.parse::<f64>().unwrap().to_bits()
            )),
            "{literal}\n{ir}"
        );
        assert!(!ir.contains(" fast "));
        assert!(!ir.contains(" fastmath "));
    }
    for literal in ["1e309", "1.7976931348623159e308", "9e99999999999999"] {
        let errors = bhasa::check(&format!("def main() {{ print({literal}) }}")).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "P005"),
            "{errors:?}"
        );
    }
    let mut program = bhasa::check("def main() { print(1.0) }").unwrap();
    let bhasa::ast::StmtKind::Expr(expression) =
        &mut program.program.functions[0].body.as_mut().unwrap()[0].kind
    else {
        panic!()
    };
    let bhasa::ast::ExprKind::Call { args, .. } = &mut expression.kind else {
        panic!()
    };
    args[0].kind = bhasa::ast::ExprKind::Float(f64::NAN);
    assert!(bhasa::codegen::emit_llvm(&program, "x86_64-unknown-linux-gnu").is_err());
}

#[test]
fn reference_executes_float_operations_ownership_and_checked_casts_at_o0_and_o2() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("numbers-reference");
    let source = workspace.write("numbers.bh", PROGRAM);
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), EXPECTED);
        for value in [
            "9223372036854775808.0",
            "-9223372036854777856.0",
            "1.0 / 0.0",
            "-1.0 / 0.0",
            "0.0 / 0.0",
            "float(9223372036854775807)",
        ] {
            let source = workspace.write(
                "invalid-cast.bh",
                &format!("def main() {{ print(int({value})) }}"),
            );
            let executable = build(&workspace, &source, release);
            let result = execute(&executable, &workspace);
            assert!(!result.status.success(), "{value}: {}", describe(&result));
            assert!(result.stdout.is_empty(), "{value}: {}", describe(&result));
        }
    }
}

fn seed(workspace: &Workspace) -> std::path::PathBuf {
    let source = ["lexer.bh", "emitter.bh", "driver.bh"]
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .join("\n");
    let source = workspace.write("seed.bh", &source);
    let seed = build(workspace, &source, true);
    let saved = workspace.file("seed");
    fs::rename(seed, &saved).unwrap();
    saved
}
fn emit_seed(seed: &Path, source: &Path) -> std::process::Output {
    Command::new(seed)
        .args([
            "emit-llvm",
            as_str(source),
            "--target",
            &bhasa::target::Target::Native.triple_for_ir().unwrap(),
        ])
        .output()
        .unwrap()
}
fn link_and_run(
    workspace: &Workspace,
    clang: &std::ffi::OsStr,
    ir: &str,
    optimization: &str,
) -> std::process::Output {
    let ir_path = workspace.write("numbers.ll", ir);
    let executable = workspace.file("numbers");
    succeeds(
        &Command::new(clang)
            .arg(optimization)
            .arg(&ir_path)
            .arg(fixture("runtime/core.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap(),
    );
    execute(&executable, workspace)
}

#[test]
fn bhasa_seed_matches_float_literal_boundaries_operations_and_rejections() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("numbers-seed");
    let seed = seed(&workspace);
    let expanded = bhasa::generics::expand(PROGRAM, &[]).unwrap();
    let source = workspace.write("numbers.bh", &expanded.source);
    let emitted = emit_seed(&seed, &source);
    succeeds(&emitted);
    for optimization in ["-O0", "-O2"] {
        let result = link_and_run(&workspace, &clang, &stdout(&emitted), optimization);
        succeeds(&result);
        assert_eq!(stdout(&result), EXPECTED);
    }
    let literals = [
        "0.1",
        "0002.500",
        "1e3",
        "1E+3",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4e-324",
        "1e-9999999999999999",
        "0e999999999999999999",
    ];
    let source_text = format!(
        "def main() {{ {} }}",
        literals
            .iter()
            .map(|literal| format!("print({literal});"))
            .collect::<String>()
    );
    let source = workspace.write("boundaries.bh", &source_text);
    let reference = build(&workspace, &source, false);
    let expected = execute(&reference, &workspace);
    succeeds(&expected);
    let emitted = emit_seed(&seed, &source);
    succeeds(&emitted);
    let actual = link_and_run(&workspace, &clang, &stdout(&emitted), "-O0");
    succeeds(&actual);
    assert_eq!(stdout(&actual), stdout(&expected));
    for expression in [
        "1e309",
        "1.7976931348623159e308",
        "1e",
        "1e+",
        "1e3f",
        "1.0_1",
        "1 + 1.0",
        "1.0 % 0.5",
        "float(true)",
        "int(1)",
        "int()",
    ] {
        let source_text = format!("def main() {{ print({expression}) }}");
        assert!(bhasa::check(&source_text).is_err());
        let source = workspace.write("rejected.bh", &source_text);
        let emitted = emit_seed(&seed, &source);
        assert!(
            !emitted.status.success(),
            "{expression}: {}",
            describe(&emitted)
        );
        assert!(emitted.stdout.is_empty());
    }
    for value in ["9223372036854775808.0", "0.0 / 0.0", "1.0 / 0.0"] {
        let source = workspace.write("cast.bh", &format!("def main() {{ print(int({value})) }}"));
        let emitted = emit_seed(&seed, &source);
        succeeds(&emitted);
        for optimization in ["-O0", "-O2"] {
            let result = link_and_run(&workspace, &clang, &stdout(&emitted), optimization);
            assert!(!result.status.success(), "{value}: {}", describe(&result));
        }
    }
}

#[cfg(feature = "bhasa-frontend")]
#[test]
fn hybrid_float_lexer_and_native_compiler_match_the_reference() {
    for source in [
        PROGRAM,
        "def main() { print(1e3); print(1.0e-3) }",
        "def main() { print(1e+) }",
    ] {
        assert_eq!(bhasa::lexer::lex(source), bhasa::lexer::lex_rust(source));
    }
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("numbers-hybrid");
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let result = link_and_run(&workspace, &clang, &ir, "-O2");
    succeeds(&result);
    assert_eq!(stdout(&result), EXPECTED);
}
