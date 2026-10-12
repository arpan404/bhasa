use bhasa::ast::Span;

mod common;
use common::*;

fn checked(source: &str) -> bhasa::ast::CheckedProgram {
    bhasa::check(source).unwrap_or_else(|errors| panic!("{errors:?}\n{source}"))
}
fn rejected(source: &str, code: &str) {
    let errors = bhasa::check(source).expect_err(source);
    assert!(
        errors.iter().any(|error| error.code == code),
        "{errors:?}\n{source}"
    );
    assert!(errors.iter().all(|error| error.span.end <= source.len()));
    assert!(
        errors.iter().all(|error| !error.message.contains("__bhg_")),
        "{errors:?}"
    );
}

const OWNERSHIP: &str = r#"
struct Box[T] {
    value: T
    def copy(self) -> T { return self.value }
    def replace(self, next: T) { self.value = next }
}
enum Option[T] { none; some(value: T) }
enum Result[T, E] { ok(value: T); err(error: E) }
def identity[T](value: T) -> T { return value }
def unwrap[T](value: Option[T], fallback: T) -> T {
    match value {
        case Option[T].none { return fallback }
        case Option[T].some(item) { return item }
    }
}
def main() -> int {
    var box: Box[buffer] = Box[buffer](buffer("old"))
    var option = Option[Box[buffer]].some(identity[Box[buffer]](box))
    box.replace(buffer("new"))
    match option {
        case Option[Box[buffer]].none { return 1 }
        case Option[Box[buffer]].some(snapshot) {
            if not same(snapshot.copy(), "old") { return 2 }
            snapshot.replace(buffer("changed"))
        }
    }
    match option {
        case Option[Box[buffer]].none { return 3 }
        case Option[Box[buffer]].some(snapshot) {
            if not same(snapshot.copy(), "old") { return 4 }
        }
    }
    if not same(box.copy(), "new") { return 5 }
    if unwrap[int](Option[int].none(), 42) != 42 { return 6 }
    var values = ints()
    push(values, 9)
    var result = Result[Box[ints], buffer].ok(Box[ints](values))
    match result {
        case Result[Box[ints], buffer].err(message) { return 7 }
        case Result[Box[ints], buffer].ok(snapshot) {
            if get(snapshot.copy(), 0) != 9 { return 8 }
        }
    }
    print("generic ownership ok")
    return 0
}
"#;

#[test]
fn nested_struct_enum_function_and_method_specializations_check() {
    let program = checked(OWNERSHIP);
    assert!(program.program.records.len() >= 2);
    assert!(program.program.enums.len() >= 3);
    for target in [
        "aarch64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "wasm32-unknown-unknown",
    ] {
        bhasa::codegen::emit_llvm(&program, target).unwrap();
    }
}

#[test]
fn repeated_instances_are_shared_but_different_arguments_are_nominal() {
    let source = "struct Box[T] { value:T } def id[T](x:T) -> T { return x } def main() { var a = id[int](1); var b = id[int](2); var x = Box[int](a); var y = Box[bool](true) }";
    let program = checked(source);
    assert_eq!(program.program.records.len(), 2);
    assert_eq!(program.program.functions.len(), 2);
    rejected(
        "struct Box[T] { value:T } def f(x:Box[int]) {} def main() { f(Box[bool](true)) }",
        "S010",
    );
}

#[test]
fn explicit_type_arguments_are_validated_even_in_unused_templates() {
    for (source, code) in [
        ("struct Box[T] { value:T } def main() { Box[]() }", "G002"),
        (
            "struct Box[T] { value:T } def main() { Box[int,bool](1) }",
            "G002",
        ),
        (
            "struct Box[T] { value:T } def main() { Box[Missing](1) }",
            "G003",
        ),
        (
            "struct Box[T] { value:T } def main() { Box[task](1) }",
            "G003",
        ),
        (
            "struct Box[T] { value:T } def main() { Box[void](1) }",
            "G003",
        ),
        ("struct Box[T] { value:T } def f(value:Box) {}", "G002"),
        (
            "def id[T](x:T)->T { return x } struct Box { x:id[int] }",
            "G003",
        ),
        ("def unused[T](x:T) { missing[int](1) }", "G002"),
        (
            "struct Box[T] { value:T } def unused[T](x:T) { Box[Missing](x) }",
            "G003",
        ),
        (
            "struct Box[T] { value:T } def unused[T](x:T) { Box[int,bool](x) }",
            "G002",
        ),
        ("def f[T,T](x:T) {}", "G002"),
        ("struct Box[int] { value:int }", "G002"),
        ("def main[T]() {}", "G002"),
        ("extern def foreign[T](x:T)->T", "G007"),
        ("struct Box[T] { value:T; def f[U](self,x:U) {} }", "G007"),
        (
            "struct Box { def f(self) {} } def main() { Box().f[int]() }",
            "G007",
        ),
    ] {
        rejected(source, code);
    }
    for source in [
        "struct Box[T] { value:T } def main() { Box(1) }",
        "def unused[T](x:T) { var = 1 }",
        "enum E[T] { none } def main() { E[int].missing() }",
    ] {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
}

#[test]
fn multiline_type_annotations_and_template_editor_shapes_are_preserved() {
    checked(
        "def identity[T](value:\nT) -> T { return value } def main() { print(identity[int](1)) }",
    );
    let source = "# hé\nstruct Box[T] { value:T; def copy(self)->T { return self.value } } enum Option[T] { none; some(value:T) } def identity[T](value:T)->T { return value }";
    let syntax = bhasa::generics::template_program(source, &[]).unwrap();
    assert_eq!(syntax.records[0].name, "Box");
    assert_eq!(syntax.records[0].methods[0].name, "copy");
    assert_eq!(syntax.enums[0].name, "Option");
    assert_eq!(syntax.functions[0].name, "identity");
    assert_eq!(syntax.records[0].span.start, source.find("struct").unwrap());
}

#[test]
fn declaration_collisions_and_generated_prefix_are_rejected() {
    for source in [
        "struct Box[T] { value:T } struct Box { value:int }",
        "struct Box { value:int } struct Box[T] { value:T }",
        "def f[T](x:T) {} def f[U](x:U) {}",
        "def print[T](x:T) {}",
        "struct Box[T] { value:T } def __bhg_0() {}",
    ] {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
}

#[test]
fn strings_comments_local_names_and_nongeneric_source_are_untouched() {
    let source = "# héllo Box[int]\ndef main() { print(\"Option[buffer] and id[T] hé\") }\n";
    let expansion = bhasa::generics::expand(source, &[]).unwrap();
    assert!(expansion.is_identity());
    assert_eq!(expansion.source, source);
    for index in (0..=source.len()).filter(|&index| source.is_char_boundary(index)) {
        assert_eq!(
            expansion.map_span(Span::new(index, index)),
            Span::new(index, index)
        );
    }
    let source = "struct Box[T] { value:T } struct Other { n:int; def value(self)->int { return self.n } } def main() { var Box = Other(4); print(Box.value()); print(\"Box[int]\") }";
    let expansion = bhasa::generics::expand(source, &[]).unwrap();
    assert!(!expansion.is_identity());
    assert!(expansion.source.contains("\"Box[int]\""));
    checked(source);
}

#[test]
fn diagnostics_map_instantiations_and_template_bodies_to_original_utf8_offsets() {
    let source = "# hé🙂\nstruct Box[T] { value:T } def main() { var x = Box[buffer](1) }";
    let errors = bhasa::check(source).unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S010"));
    assert!(
        errors
            .iter()
            .all(|error| error.span.start >= source.find("def main").unwrap()
                && error.span.end <= source.len()),
        "{errors:?}"
    );
    let source = "# hé🙂\ndef bad[T](x:T)->int { return x } def main() { print(bad[bool](true)) }";
    let errors = bhasa::check(source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.span.start >= source.find("return x").unwrap()
                && error.span.end <= source.find("def main").unwrap()),
        "{errors:?}"
    );
    let expanded =
        bhasa::generics::expand("struct Box[T] { value:T } def main() { Box[int](1) }", &[])
            .unwrap();
    assert_eq!(expanded.original_name("__bhg_0.copy"), "Box.copy");
    assert_eq!(expanded.display_names("type __bhg_0"), "type Box[int]");
}

#[test]
fn templates_across_modules_preserve_eof_boundaries_and_original_spans() {
    let first = "def extract(x:Box[int])->int { return x.value }";
    let second = "struct Box[T] { value:T }";
    let source = format!("{first}\n{second}\n");
    let expanded = bhasa::generics::expand(&source, &[first.len(), source.len() - 1]).unwrap();
    let parsed = bhasa::generics::parse_expansion(&expanded).unwrap();
    let checked = bhasa::sema::check(parsed).unwrap();
    assert!(checked.program.records[0].span.start > first.len());
    assert!(checked.program.functions[0].span.end <= first.len());
    for first in ["def f(x:Box[int]) {", "struct Box[T] {", "def f(x:Box["] {
        let source = format!("{first}\n{second}\n}}");
        let outcome = bhasa::generics::expand(&source, &[first.len()])
            .and_then(|expanded| bhasa::generics::parse_expansion(&expanded));
        assert!(outcome.is_err(), "module boundary was crossed: {source}");
    }
}

#[test]
fn imported_generic_library_builds_and_runs() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("generic-imports");
    workspace.write("types.bh", "struct Box[T] { value:T; def get(self)->T { return self.value } } enum Option[T] { none; some(value:T) } def id[T](x:T)->T { return x }");
    let source = workspace.write("main.bh", "import \"types.bh\"\ndef main() { var box = id[Box[buffer]](Box[buffer](buffer(\"imported\"))); match Option[Box[buffer]].some(box) { case Option[Box[buffer]].none {} case Option[Box[buffer]].some(value) { print(value.get()) } } }");
    let executable = build(&workspace, &source, false);
    let result = execute(&executable, &workspace);
    succeeds(&result);
    assert_eq!(stdout(&result), "imported\n");
}

#[test]
fn recursive_specializations_are_reused_and_expanding_recursion_is_bounded() {
    checked("def count[T](x:T,n:int)->int { if n == 0 { return 0 } return count[T](x,n-1) } def main() { print(count[bool](true,3)) }");
    rejected("struct Box[T] { value:T } def expand[T](x:T) { expand[Box[T]](Box[T](x)) } def main() { expand[int](1) }", "G006");
    rejected("struct Pair[A,B] { a:A; b:B } def expand[T](x:T) { expand[Pair[T,T]](Pair[T,T](x,x)) } def main() { expand[int](1) }", "G006");
    rejected(
        "struct Loop[T] { value:Loop[T] } def consume(x:Loop[int]) {}",
        "S025",
    );
}

#[test]
fn generic_borrows_and_worker_restrictions_still_apply() {
    rejected("struct Box[T] { value:T; def change(self)->int { self.value = buffer(\"new\"); return 0 } } def use[T](x:T,n:int) {} def main() { var box = Box[buffer](buffer(\"old\")); use[buffer](box.value,box.change()) }", "S028");
    rejected("extern def foreign(n:int)->int\ndef work[T](n:T)->int { return foreign(n) } def main() { var task = spawn(work[int](1)); print(join(task)) }", "S020");
    checked("def work[T](n:T)->T { return n } def main() { var task = spawn(work[int](21)); var result = join(task); print(result) }");
}

#[test]
fn owned_generic_specializations_execute_at_o0_and_o2() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("generic-ownership");
    let source = workspace.write("main.bh", OWNERSHIP);
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "generic ownership ok\n");
    }
}

#[cfg(feature = "bhasa-frontend")]
#[test]
fn bhasa_frontend_accepts_and_executes_the_same_specializations() {
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    bhasa::compile_with_bhasa(OWNERSHIP, &target).unwrap();
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("generic-hybrid");
    let source = workspace.write("main.bh", OWNERSHIP);
    for release in [false, true] {
        let executable = workspace.file("hybrid");
        let mut command = workspace.command();
        command.args([
            "build",
            as_str(&source),
            "--frontend",
            "bhasa",
            "-o",
            as_str(&executable),
        ]);
        if release {
            command.arg("--release");
        }
        succeeds(&command.output().unwrap());
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "generic ownership ok\n");
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn owned_generic_payloads_pass_instrumented_sanitizers_and_free_all_allocations() {
    use std::{fs, process::Command};
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("generic-sanitizers");
    let source = workspace.write("generics.bh", OWNERSHIP);
    let ir_path = workspace.file("generics.ll");
    succeeds(&workspace.run(&["emit-llvm", as_str(&source), "-o", as_str(&ir_path)]));
    let ir = fs::read_to_string(&ir_path)
        .unwrap()
        .replace("@malloc(", "@tracked_malloc(")
        .replace("@free(", "@tracked_free(");
    let instrumented = ir
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    fs::write(&ir_path, instrumented).unwrap();
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
        let executable = workspace.file("generics");
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
        assert_eq!(stdout(&result), "generic ownership ok\n");
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}

#[test]
fn direct_api_string_errors_match_lexer_codes_and_complete_character_spans() {
    let invalid = [
        "\"hé\\é\"",
        "\"hé\\🙂\"",
        "\"hé\\q\"",
        "\"hé\nnext\"",
        "\"hé\rnext\"",
        "\"hé\r\nnext\"",
        "\"hé\\\nnext\"",
        "\"hé\\\rnext\"",
        "\"hé\\\r\nnext\"",
        "\"hé\0next\"",
        "\"hé\\\0next\"",
        "\"hé\\0\"",
        "\"hé unfinished",
        "\"hé unfinished\\",
        "\0",
        "# hé\0",
        "// hé\0",
    ];
    for fragment in invalid {
        for prefix in ["", "def unused[T](value:T) { print("] {
            let source = format!("{prefix}{fragment}");
            let expected = bhasa::lexer::lex(&source).unwrap_err().remove(0);
            let actual = bhasa::check(&source).unwrap_err().remove(0);
            assert_eq!(actual, expected, "{source:?}");
            assert!(source.is_char_boundary(actual.span.start), "{source:?}");
            assert!(source.is_char_boundary(actual.span.end), "{source:?}");
            #[cfg(feature = "bhasa-frontend")]
            {
                let native = bhasa::compile_with_bhasa(&source, "x86_64-unknown-linux-gnu")
                    .unwrap_err()
                    .remove(0);
                assert_eq!(native, expected, "{source:?}");
            }
        }
    }
    checked("def main() { print(\"hé🙂\\n\\t\\r\\\\\\\"\") }");
    checked("def identity[T](value:T)->T { return value } def main() { print(identity[str](\"hé🙂\")) }");

    // A separator byte belongs to neither neighboring module, even after a
    // trailing escape or a CR whose following LF would otherwise form CRLF.
    for first in ["\"hé\\", "\"hé\r", "\"hé\\\r"] {
        let source = format!("{first}\nnext\"");
        let expected = bhasa::lexer::lex(first).unwrap_err().remove(0);
        let actual = bhasa::generics::expand(&source, &[first.len()])
            .unwrap_err()
            .remove(0);
        assert_eq!(actual, expected, "{source:?}");
    }
}
