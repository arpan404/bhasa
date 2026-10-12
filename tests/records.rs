use std::fs;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

mod common;
use common::*;

fn accepted(source: &str) {
    if let Err(diagnostics) = bhasa::check(source) {
        panic!("valid record program rejected:\n{source}\n{diagnostics:?}");
    }
}

fn rejected(source: &str) -> Vec<bhasa::Diagnostic> {
    let diagnostics = bhasa::check(source).expect_err(source);
    assert!(!diagnostics.is_empty(), "missing diagnostic for {source}");
    for diagnostic in &diagnostics {
        assert!(!diagnostic.message.trim().is_empty());
        assert!(diagnostic.span.start <= diagnostic.span.end);
        assert!(diagnostic.span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.span.start));
        assert!(source.is_char_boundary(diagnostic.span.end));
    }
    diagnostics
}

const SCALARS: &str = r#"
struct Line {
    start: Point
    end: Point
    visible: bool
    label: str
}
struct Point { x: int; y: int }
def translate(point: Point, dx: int) -> Point {
    point.x = point.x + dx
    return point
}
def origin() -> Point { return Point(0, 0) }
def main() {
    var point: Point = Point(2, 3)
    var line: Line = Line(point, origin(), true, "segment")
    var copy = line
    copy.start.x = 20
    copy.end = Point(4, 5)
    print(line.start.x)
    print(copy.start.x)
    print(copy.end.y)
    var moved: Point = translate(point, 7)
    moved.y = 99
    print(point.x)
    print(point.y)
    print(moved.y)
    print(line.visible)
    print(line.label)
    print(origin().x)
}
"#;

#[test]
fn record_types_fields_and_forward_references_check() {
    accepted(SCALARS);
    accepted("struct Empty {}\ndef main() { var empty: Empty = Empty() }");
    accepted(
        "struct Box { value: int }\ndef identity(value: Box) -> Box { return value }\n\
         def main() { var b = identity(Box(42)); b = b; b.value = b.value }",
    );
}

#[test]
fn tcp_handles_are_valid_record_fields_and_typed_constructor_arguments() {
    accepted(
        "struct Session { socket: tcp_socket; listener: tcp_listener }\n\
         def wrap(socket: tcp_socket, listener: tcp_listener) -> Session {\n\
             return Session(socket, listener)\n}\n\
         def copy(session: Session) -> Session { return session }",
    );
}

#[test]
fn parse_tokens_preserves_record_syntax() {
    let tokens = bhasa::lexer::lex(SCALARS).unwrap();
    let program = bhasa::parser::parse_tokens(tokens).unwrap();
    bhasa::sema::check(program).unwrap();
}

#[test]
fn duplicate_record_fields_types_and_constructor_names_are_rejected() {
    for source in [
        "struct Point { x: int; x: bool }",
        "struct Point { x: int }\nstruct Point { y: int }",
        "struct Point { x: int }\ndef Point(value: int) -> int { return value }",
        "def Point(value: int) -> int { return value }\nstruct Point { x: int }",
        "struct print { value: int }",
        "struct task { value: int }",
    ] {
        rejected(source);
    }
}

#[test]
fn constructors_require_the_declared_field_count_and_nominal_types() {
    for expression in ["Point()", "Point(1)", "Point(1, 2, 3)", "Point(true, 2)"] {
        rejected(&format!(
            "struct Point {{ x: int; y: int }}\ndef main() {{ var p = {expression} }}"
        ));
    }
    for statement in [
        "var a: A = B(1)",
        "var a = A(1); a = B(2)",
        "consume(B(1))",
        "var p = Pair(B(1))",
    ] {
        let diagnostics = rejected(&format!(
            "struct A {{ x: int }}\nstruct B {{ x: int }}\nstruct Pair {{ a: A }}\n\
             def consume(a: A) {{}}\ndef main() {{ {statement} }}"
        ));
        let messages = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            messages.contains('A') && messages.contains('B'),
            "{messages}"
        );
    }
    rejected("struct A { x: int }\nstruct B { x: int }\ndef make() -> A { return B(1) }");
}

#[test]
fn unknown_types_and_fields_report_the_source_names() {
    for source in [
        "struct Box { value: Missing }",
        "def consume(value: Missing) {}",
        "def make() -> Missing { return 1 }",
        "def main() { var value: Missing = 1 }",
    ] {
        let diagnostics = rejected(source);
        assert!(diagnostics.iter().any(|d| d.message.contains("Missing")));
    }
    for statement in ["print(p.missing)", "p.missing = 2"] {
        let diagnostics = rejected(&format!(
            "struct Point {{ x: int }}\ndef main() {{ var p = Point(1); {statement} }}"
        ));
        assert!(diagnostics.iter().any(|d| d.message.contains("missing")));
    }
    rejected("def main() { var p = 1; print(p.x) }");
    rejected("def main() { var p = 1; p.x = 2 }");
}

#[test]
fn recursive_layouts_and_nonstorable_fields_are_rejected() {
    for source in [
        "struct Node { next: Node }",
        "struct A { b: B }\nstruct B { a: A }",
        "struct A { b: B }\nstruct B { c: C }\nstruct C { a: A }",
        "struct Box { value: void }",
        "struct Box { value: task }",
    ] {
        rejected(source);
    }
    accepted("struct A { b: B; c: B }\nstruct B { value: int }");
}

#[test]
fn record_layout_depth_limit_applies_in_either_declaration_order() {
    let mut declarations = (0..128)
        .map(|index| format!("struct R{index} {{ next: R{} }}\n", index + 1))
        .collect::<Vec<_>>();
    declarations.push("struct R128 { value: int }\n".to_owned());
    rejected(&declarations.concat());
    declarations.reverse();
    rejected(&declarations.concat());
}

#[test]
fn borrowed_records_cannot_be_rebound_and_fields_keep_their_types() {
    for source in [
        "struct Point { x: int }\ndef replace(p: Point) { p = Point(2) }",
        "struct Point { x: int }\ndef main() { var p = Point(1); p.x = false }",
        "struct Point { x: int }\ndef make() -> Point { return Point(1) }\n\
         def main() { make().x = 2 }",
        "struct Point { x: int }\ndef main() { Point(1).x = 2 }",
        "struct Point { x: int }\nextern def native(p: Point)",
        "struct Point { x: int }\nextern def native() -> Point",
    ] {
        rejected(source);
    }
    accepted("struct Point { x: int }\ndef update(p: Point) { p.x = 2 }");
    accepted(
        "struct Leaf { text: buffer }\nstruct Box { leaf: Leaf }\n\
         def update(box: Box) { box.leaf = Leaf(buffer(\"new\")) }",
    );
}

const ALIAS_TYPES: &str = r#"
struct Leaf { text: buffer; values: ints }
struct Pair { left: Leaf; right: Leaf }
def records(first: Pair, second: Pair) {}
def nested(parent: Pair, child: Leaf) {}
def reversed(child: Leaf, parent: Pair) {}
def bytes(parent: Pair, text: buffer) {}
def numbers(parent: Pair, values: ints) {}
def leaves(first: Leaf, second: Leaf) {}
def disjoint(first: buffer, second: ints) {}
"#;

#[test]
fn overlapping_mutable_record_arguments_are_rejected() {
    for call in [
        "records(p, p)",
        "nested(p, p.left)",
        "reversed(p.right, p)",
        "bytes(p, p.left.text)",
        "numbers(p, p.right.values)",
        "leaves(p.left, p.left)",
    ] {
        rejected(&format!(
            "{ALIAS_TYPES}\ndef main() {{\n\
             var leaf = Leaf(buffer(), ints())\n\
             var p = Pair(leaf, leaf)\n{call}\n}}"
        ));
    }
}

#[test]
fn later_argument_mutations_cannot_invalidate_an_earlier_borrowed_field() {
    for call in [
        "consume(p.left.text, replace(p))",
        "consume(p.left.text, 1 + replace(p))",
        "get(p.left.text, replace(p))",
        "set(p.left.text, 0, replace(p))",
        "consume(p.left.text, len(replace_bytes(p)))",
        "consume(p.left.text, len(slice(replace_bytes(p), 0, 1)))",
        "consume(p.left.text, len(replace_box(p).left.text))",
        "consume(p.left.text, len(Wrap(replace_box(p)).pair.left.text))",
        "consume(p.left.text, get(to_buffer(replace(p)), 0))",
        "use_text(p.left.text, Wrap(replace_box(p)))",
        "use_leaf(p.left, replace(p))",
        "consume(p.left.text, inspect(p))",
    ] {
        let diagnostics = rejected(&format!(
            "{ALIAS_TYPES}\n\
             struct Wrap {{ pair: Pair }}\n\
             def consume(text: buffer, ignored: int) {{}}\n\
             def use_text(text: buffer, wrapped: Wrap) {{}}\n\
             def use_leaf(leaf: Leaf, ignored: int) {{}}\n\
             def inspect(p: Pair) -> int {{ return len(p.left.text) }}\n\
             def replace(p: Pair) -> int {{\n\
                 p.left = Leaf(buffer(\"new\"), ints())\nreturn 0\n}}\n\
             def replace_bytes(p: Pair) -> buffer {{\n\
                 p.left = Leaf(buffer(\"new\"), ints())\nreturn p.left.text\n}}\n\
             def replace_box(p: Pair) -> Pair {{\n\
                 p.left = Leaf(buffer(\"new\"), ints())\nreturn p\n}}\n\
             def main() {{\n\
                 var p = Pair(Leaf(buffer(\"old\"), ints()), Leaf(buffer(), ints()))\n\
                 {call}\n}}"
        ));
        assert!(
            diagnostics.iter().any(|d| d.message.contains("overlap")),
            "expected an alias diagnostic for {call}: {diagnostics:?}"
        );
    }
}

#[test]
fn later_argument_reads_and_constructor_copies_preserve_record_borrows() {
    accepted(
        r#"
struct Box { text: buffer }
struct Wrap { box: Box }
struct Envelope { wrap: Wrap }
def consume(b: Box, size: int) { print(size) }
def main() {
    var b = Box(buffer("old"))
    consume(b, len(b.text))
    consume(b, len(slice(b.text, 0, len(b.text))))
    consume(b, len(to_buffer(len(b.text))))
    consume(b, get(b.text, len(b.text) - 1))
    consume(b, len(Wrap(b).box.text))
    consume(b, len(Envelope(Wrap(b)).wrap.box.text))
}
"#,
    );
    accepted(
        r#"
struct Box { text: buffer }
struct Wrap { box: Box }
struct Envelope { wrap: Wrap }
def consume(b: Box, copied: Wrap) {}
def consume_nested(b: Box, copied: Envelope) {}
def main() {
    var b = Box(buffer("old"))
    consume(b, Wrap(b))
    consume(b, Wrap(Box(slice(b.text, 0, len(b.text)))))
    consume(b, Envelope(Wrap(b)).wrap)
    consume_nested(b, Envelope(Wrap(b)))
}
"#,
    );
    accepted(
        r#"
struct Box { text: buffer }
struct Wrap { box: Box }
def replace(b: Box) -> int { b.text = buffer("new"); return 3 }
def inspect(b: Box) -> int { return len(b.text) }
def consume(b: Box, size: int) {}
def consume_parent(parent: Wrap, size: int) {}
def main() {
    var b = Box(buffer("old"))
    consume(b, replace(b))
    consume(b, inspect(b))
    var wrapped = Wrap(b)
    consume(wrapped.box, replace(wrapped.box))
    consume_parent(wrapped, replace(wrapped.box))
}
"#,
    );
}

#[test]
fn constructor_copies_and_disjoint_fields_do_not_alias() {
    accepted(&format!(
        "{ALIAS_TYPES}\ndef main() {{\n\
         var leaf = Leaf(buffer(), ints())\n\
         var p = Pair(leaf, leaf)\n\
         var q = p\nrecords(p, q)\nleaves(p.left, p.right)\n\
         disjoint(p.left.text, p.left.values)\n}}"
    ));
}

#[test]
fn record_examples_check() {
    for file in [
        "examples/records/points.bh",
        "examples/records/resources.bh",
    ] {
        accepted(&fs::read_to_string(fixture(file)).unwrap());
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_scalar_records_have_value_semantics_and_mutable_borrows() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("records-scalars");
    let source = workspace.write("scalars.bh", SCALARS);
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "2\n20\n5\n9\n3\n99\ntrue\nsegment\n0\n");
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}

const OWNERSHIP: &str = r#"
struct Leaf { text: buffer; values: ints }
struct Pair { left: Leaf; right: Leaf; title: buffer }
struct Box { text: buffer }
struct Wrap { box: Box }
struct Envelope { wrap: Wrap }
def make(text: str) -> Pair {
    var numbers = ints()
    push(numbers, 7)
    return Pair(Leaf(buffer(text), numbers), Leaf(buffer("right"), ints()), buffer("title"))
}
def change_and_copy(value: Pair) -> Pair {
    append(value.left.text, "!")
    push(value.left.values, 8)
    return value
}
def copy_child(value: Pair) -> Leaf { return value.left }
def copy_bytes(value: Pair) -> buffer { return value.left.text }
def early(value: Pair, choice: bool) -> Pair {
    var local = value
    if choice {
        var unused = make("branch temporary")
        return local
    }
    var count = 0
    while count < 2 {
        var iteration = make("iteration temporary")
        if count == 1 { return iteration }
        count = count + 1
    }
    return make("fallback")
}
def change_disjoint(first: Leaf, second: Leaf) {
    first.text = buffer("first")
    second.text = buffer("second")
    push(first.values, 11)
    push(second.values, 12)
}
def consume_size(b: Box, size: int) -> int {
    var previous_size = len(b.text)
    b.text = buffer("resized")
    return size - previous_size
}
def consume_copy(b: Box, copied: Wrap) -> bool {
    var previous_text = b.text
    b.text = buffer("mutated original")
    if not same(copied.box.text, previous_text) { return false }
    copied.box.text = buffer("mutated copy")
    return same(b.text, "mutated original")
}
def consume_envelope(b: Box, copied: Envelope) -> bool {
    return consume_copy(b, copied.wrap)
}
def replace_box(b: Box) -> int {
    b.text = buffer("updated")
    return 7
}
def replace_leaf(leaf: Leaf) -> int {
    leaf.text = buffer("changed")
    leaf.values = ints()
    return 7
}
def consume_pair_size(pair: Pair, size: int) -> bool {
    return len(pair.left.text) == size
}
def main() -> int {
    var bytes = buffer("seed")
    var numbers = ints()
    push(numbers, 7)
    var leaf = Leaf(bytes, numbers)
    set(bytes, 0, 88)
    set(numbers, 0, 99)
    if not same(leaf.text, "seed") { return 11 }
    if get(leaf.values, 0) != 7 { return 12 }
    var original = Pair(leaf, leaf, buffer("title"))
    set(leaf.text, 0, 89)
    if not same(original.left.text, "seed") { return 13 }
    var copied = original
    set(copied.left.text, 0, 67)
    set(copied.left.values, 0, 42)
    copied.title = buffer("copy title")
    if not same(original.left.text, "seed") { return 14 }
    if get(original.left.values, 0) != 7 { return 15 }
    if not same(copied.right.text, "seed") { return 16 }
    if not same(original.title, "title") { return 17 }
    original = original
    original.left = original.left
    original.left.text = original.left.text
    original.left.values = original.left.values
    var returned = change_and_copy(original)
    if not same(original.left.text, "seed!") { return 18 }
    if len(original.left.values) != 2 { return 19 }
    set(returned.left.text, 0, 82)
    set(returned.left.values, 0, 88)
    if not same(original.left.text, "seed!") { return 20 }
    if get(original.left.values, 0) != 7 { return 21 }
    var child = copy_child(original)
    set(child.text, 0, 65)
    if not same(original.left.text, "seed!") { return 22 }
    var returned_bytes = copy_bytes(original)
    push(returned_bytes, 63)
    if not same(original.left.text, "seed!") { return 23 }
    original.right = original.left
    set(original.right.text, 0, 66)
    if not same(original.left.text, "seed!") { return 24 }
    original = Pair(original.right, original.left, original.title)
    if not same(original.left.text, "Beed!") { return 25 }
    if not same(original.right.text, "seed!") { return 26 }
    change_disjoint(original.left, original.right)
    if not same(original.left.text, "first") { return 27 }
    if not same(original.right.text, "second") { return 28 }
    if get(original.left.values, 2) != 11 { return 29 }
    if get(original.right.values, 2) != 12 { return 30 }
    var extracted = make("survivor").left
    var text = make("bytes survive").left.text
    var values = make("numbers survive").left.values
    if not same(extracted.text, "survivor") { return 31 }
    if not same(text, "bytes survive") { return 32 }
    if get(values, 0) != 7 { return 33 }
    if len(make("temporary").left.text) != 9 { return 34 }
    if not same(make("temporary").left.text, "temporary") { return 35 }
    append(make("discarded borrowed temporary").left.text, "!")
    var count = 0
    while count < 40 {
        var transient = make("loop")
        var first = early(transient, true)
        var second = early(transient, false)
        if not same(first.left.text, "loop") { return 36 }
        if not same(second.left.text, "iteration temporary") { return 37 }
        transient.left = copy_child(first)
        transient = second
        count = count + 1
    }
    var b = Box(buffer("old"))
    if consume_size(b, len(b.text)) != 0 { return 38 }
    if consume_size(b, len(slice(b.text, 0, len(b.text)))) != 0 { return 39 }
    if consume_size(b, len(Envelope(Wrap(b)).wrap.box.text)) != 0 { return 40 }
    if not consume_copy(b, Wrap(b)) { return 41 }
    if not consume_copy(b, Envelope(Wrap(b)).wrap) { return 42 }
    if not consume_envelope(b, Envelope(Wrap(b))) { return 43 }
    if consume_size(b, replace_box(b)) != 0 { return 44 }
    if not consume_pair_size(original, replace_leaf(original.left)) { return 45 }
    make("discarded record")
    make("discarded field").left
    make("discarded nested field").left.text
    print("record ownership ok")
    return 0
}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn nested_record_copies_borrows_returns_and_cleanup_pass_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("records-ownership");
    let source = workspace.write("ownership.bh", OWNERSHIP);
    let ir_path = workspace.file("ownership.ll");
    succeeds(&workspace.run(&["emit-llvm", as_str(&source), "-o", as_str(&ir_path)]));
    // Count direct record allocations as well as core runtime containers. This
    // catches leaks on macOS, where ASan does not provide LeakSanitizer.
    let ir = fs::read_to_string(&ir_path)
        .unwrap()
        .replace("@malloc(", "@record_tracked_malloc(")
        .replace("@realloc(", "@record_tracked_realloc(")
        .replace("@free(", "@record_tracked_free(");
    // Clang's sanitizer flag alone does not instrument loads and stores in
    // preexisting LLVM IR. Mark every generated function, including clone/drop
    // helpers, so ASan checks field accesses and catches record use-after-free.
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
    fs::write(&ir_path, ir).unwrap();
    let tracked = format!(
        r#"
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) {{
    if (live != 0) {{ fprintf(stderr, "live Bhasa allocations: %ld\n", live); _Exit(99); }}
}}
void *record_tracked_malloc(size_t n) {{
    if (!registered) {{ registered = 1; atexit(check_live); }}
    void *p = malloc(n); if (p) ++live; return p;
}}
void *record_tracked_realloc(void *p, size_t n) {{
    int was_null = p == NULL;
    void *q = realloc(p, n); if (q && was_null) ++live; return q;
}}
void record_tracked_free(void *p) {{ if (p) --live; free(p); }}
#define malloc record_tracked_malloc
#define realloc record_tracked_realloc
#define free record_tracked_free
{}
"#,
        include_str!("../runtime/core.c")
    );
    let runtime = workspace.write("tracked.c", &tracked);
    fs::copy(fixture("runtime/core.h"), workspace.file("core.h")).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("records");
        let output = Command::new(&clang)
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
        succeeds(&output);
        let output = execute(&executable, &workspace);
        succeeds(&output);
        assert_eq!(stdout(&output), "record ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

#[test]
fn imported_records_share_types_and_duplicate_imports_load_once() {
    let workspace = Workspace::new("records-imports");
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"records\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n",
    );
    workspace.write("types.bh", "struct Point { x: int; y: int }\n");
    workspace.write(
        "make.bh",
        "import \"types.bh\"\ndef make() -> Point { return Point(20, 22) }\n",
    );
    workspace.write(
        "use.bh",
        "import \"types.bh\"\ndef sum(p: Point) -> int { return p.x + p.y }\n",
    );
    workspace.write(
        "main.bh",
        "import \"make.bh\"\nimport \"use.bh\"\ndef main() { print(sum(make())) }\n",
    );
    let output = workspace.run(&["check", "--json"]);
    succeeds(&output);
    assert_eq!(stdout(&output), "[]\n");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if clang().is_some() {
        let output = workspace.run(&["run"]);
        succeeds(&output);
        assert_eq!(stdout(&output), "42\n");
    }
}

#[test]
fn imported_record_diagnostics_keep_the_actual_file_and_local_span() {
    let workspace = Workspace::new("records-import-errors");
    let broken = "# Unicode α\nstruct Point {\n    x: Missing\n}\n";
    workspace.write("types.bh", broken);
    workspace.write("main.bh", "import \"types.bh\"\ndef main() {}\n");
    let output = workspace.run(&["check", "main.bh", "--json"]);
    fails(&output);
    let diagnostics = stdout(&output);
    assert!(diagnostics.contains("types.bh\""), "{diagnostics}");
    assert!(diagnostics.contains("Missing"), "{diagnostics}");
    assert!(diagnostics.contains("\"line\":3"), "{diagnostics}");
    assert!(
        diagnostics.contains(&format!("\"start\":{}", broken.find("x:").unwrap()))
            || diagnostics.contains(&format!("\"start\":{}", broken.find("Missing").unwrap())),
        "{diagnostics}"
    );
}

#[test]
fn conflicting_record_declarations_in_distinct_modules_are_rejected() {
    let workspace = Workspace::new("records-duplicate-imports");
    workspace.write("left.bh", "struct Point { x: int }\n");
    workspace.write("right.bh", "struct Point { y: int }\n");
    workspace.write(
        "main.bh",
        "import \"left.bh\"\nimport \"right.bh\"\ndef main() {}\n",
    );
    let output = workspace.run(&["check", "main.bh", "--json"]);
    fails(&output);
    assert!(stdout(&output).contains("Point"), "{}", describe(&output));
    assert!(
        stdout(&output).contains("right.bh\""),
        "{}",
        describe(&output)
    );
}
