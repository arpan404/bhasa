mod common;
use common::*;
use std::fs;
use std::path::Path;

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
fn project(label: &str) -> Workspace {
    let workspace = Workspace::new(label);
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"namespace-test\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n",
    );
    fs::create_dir_all(workspace.file("src")).unwrap();
    workspace
}
fn check_both(workspace: &Workspace, accepted: bool, message: &str) {
    let mut frontends = vec!["rust"];
    if cfg!(feature = "bhasa-frontend") {
        frontends.push("bhasa");
    }
    for frontend in frontends {
        let output = workspace.run(&["check", "--frontend", frontend, "--json"]);
        if accepted {
            succeeds(&output);
        } else {
            fails(&output);
            assert!(
                stdout(&output).contains(message),
                "{frontend}: {}",
                describe(&output)
            );
            assert!(!stdout(&output).contains("__bhm_"), "{}", describe(&output));
        }
    }
}

#[test]
fn alias_namespaces_isolate_functions_records_enums_and_generics() {
    let workspace = project("namespaced-types");
    write(
        workspace.path(),
        "src/left.bh",
        r#"
struct Point { x: int }
enum Choice { none; value(number: int) }
def double(value: int) -> int { return value * 2 }
def identity[T](value: T) -> T { return value }
"#,
    );
    write(
        workspace.path(),
        "src/right.bh",
        r#"
struct Point { x: int }
def double(value: int) -> int { return value * 3 }
"#,
    );
    write(
        workspace.path(),
        "src/main.bh",
        r#"import "left.bh" as left
import "right.bh" as right
def main() {
    var a: left.Point = left.Point(4)
    var b: right.Point = right.Point(5)
    var c = left.identity[left.Point](a)
    var choice = left.Choice.value(7)
    print(left.double(c.x) + right.double(b.x))
    match choice {
        case left.Choice.none { print(0) }
        case left.Choice.value(number) { print(number) }
    }
}
"#,
    );
    check_both(&workspace, true, "");
    if clang().is_some() {
        let output = workspace.run(&["run"]);
        succeeds(&output);
        assert_eq!(stdout(&output), "23\n7\n");
    }
}

#[test]
fn private_top_level_declarations_and_unqualified_alias_members_are_inaccessible() {
    let workspace = project("private-declarations");
    write(
        workspace.path(),
        "src/library.bh",
        r#"
private def secret() -> int { return 42 }
private struct Hidden { value: int }
private enum Kind { one }
private extern def native_secret() -> int
def public_value() -> int { return secret() }
"#,
    );
    for expression in [
        "library.secret()",
        "library.Hidden(1)",
        "library.Kind.one()",
        "library.native_secret()",
    ] {
        write(
            workspace.path(),
            "src/main.bh",
            &format!(
                "import \"library.bh\" as library\ndef main() {{ var result = {expression} }}\n"
            ),
        );
        check_both(&workspace, false, "private to its defining module");
    }
    for expression in ["secret()", "public_value()", "native_secret()"] {
        write(
            workspace.path(),
            "src/main.bh",
            &format!(
                "import \"library.bh\" as library\ndef main() {{ var result = {expression} }}\n"
            ),
        );
        check_both(&workspace, false, "not visible in this module");
    }
    write(workspace.path(), "src/main.bh", "import \"library.bh\" as library\ndef main() { var native_secret = 1; print(native_secret()) }\n");
    check_both(&workspace, false, "is not callable");
    write(
        workspace.path(),
        "src/main.bh",
        "import \"library.bh\" as library\ndef main() { print(library.public_value()) }\n",
    );
    check_both(&workspace, true, "");
}

#[test]
fn typed_privacy_rejects_chained_fields_methods_constructors_and_generic_receivers() {
    let workspace = project("typed-member-privacy");
    write(
        workspace.path(),
        "src/library.bh",
        r#"
struct Secret {
    private value: int
    private def reveal(self) -> int { return self.value }
    def read(self) -> int { return self.reveal() }
}
struct Wrapper { inner: Secret }
struct Public { value: int; def reveal(self) -> int { return self.value } }
struct Box[T] {
    private item: T
    private def hidden(self) -> T { return self.item }
    def read(self) -> T { return self.hidden() }
}
def create() -> Secret { return Secret(42) }
def wrapper() -> Wrapper { return Wrapper(create()) }
def box() -> Box[int] { return Box[int](42) }
"#,
    );
    for expression in [
        "library.create().value",
        "library.wrapper().inner.value",
        "library.wrapper().inner.reveal()",
        "library.Secret(1)",
        "library.Box[int](1)",
        "library.box().item",
        "library.box().hidden()",
    ] {
        write(
            workspace.path(),
            "src/main.bh",
            &format!(
                "import \"library.bh\" as library\ndef main() {{ var result = {expression} }}\n"
            ),
        );
        check_both(&workspace, false, "private");
    }
    write(workspace.path(),"src/main.bh","import \"library.bh\" as library\ndef main() { var value = library.wrapper(); value.inner.value = 1 }\n");
    check_both(&workspace, false, "private");
    write(workspace.path(),"src/main.bh","import \"library.bh\" as library\ndef main() { print(library.create().read()); print(library.box().read()); var visible = library.Public(3); print(visible.value + visible.reveal()) }\n");
    check_both(&workspace, true, "");
}

#[test]
fn strings_comments_bindings_and_scoped_alias_shadowing_survive_lowering() {
    let workspace = project("module-local-shadowing");
    write(workspace.path(),"src/math.bh","struct Point { value: int }\ndef identity[T](value: T) -> T { return value }\ndef double(value: int) -> int { return value * 2 }\n");
    write(
        workspace.path(),
        "src/main.bh",
        r#"import "math.bh" as math
struct Local { double: int }
def local(math: Local) -> int { return math.double }
def main() {
    # math.double() and private def injected() {}
    print("math.double() private struct Fake {}")
    var value: math.Point = math.Point(7)
    var Point = 2
    var copied = math.identity[math.Point](value)
    if true { var math = Local(4); print(math.double) }
    print(math.double(copied.value) + Point + local(Local(3)))
}
"#,
    );
    check_both(&workspace, true, "");
    if clang().is_some() {
        let output = workspace.run(&["run"]);
        succeeds(&output);
        assert_eq!(
            stdout(&output),
            "math.double() private struct Fake {}\n4\n19\n"
        );
    }
}

#[test]
fn imports_retain_transitive_exports_but_reject_distinct_collisions() {
    let workspace = project("module-exports");
    write(
        workspace.path(),
        "src/base.bh",
        "def value() -> int { return 42 }\n",
    );
    write(
        workspace.path(),
        "src/left.bh",
        "import \"base.bh\"\ndef left() -> int { return value() }\n",
    );
    write(
        workspace.path(),
        "src/right.bh",
        "import \"base.bh\"\ndef right() -> int { return value() }\n",
    );
    write(workspace.path(),"src/main.bh","import \"left.bh\"\nimport \"right.bh\"\nprivate def hidden() {}\ndef main() { print(value() + left() + right()) }\n");
    check_both(&workspace, true, "");
    write(
        workspace.path(),
        "src/right.bh",
        "def value() -> int { return 7 }\n",
    );
    check_both(&workspace, false, "duplicate exported declaration 'value'");
    write(
        workspace.path(),
        "src/main.bh",
        "import \"left.bh\" as same\nimport \"right.bh\" as same\ndef main() {}\n",
    );
    check_both(&workspace, false, "duplicate or reserved module namespace");
}

#[test]
fn namespace_diagnostics_map_to_imported_original_bytes_and_eof() {
    let workspace = project("module-diagnostic-map");
    let source = "# unicode α\ndef broken() -> int {\n return missing\n}\n";
    write(workspace.path(), "src/broken.bh", source);
    write(
        workspace.path(),
        "src/main.bh",
        "import \"broken.bh\" as library\ndef main() { print(library.broken()) }\n",
    );
    let output = workspace.run(&["check", "--json"]);
    fails(&output);
    assert!(
        stdout(&output).contains("broken.bh\""),
        "{}",
        describe(&output)
    );
    assert!(
        stdout(&output).contains(&format!("\"start\":{}", source.find("missing").unwrap())),
        "{}",
        describe(&output)
    );
    write(
        workspace.path(),
        "src/main.bh",
        "import \"broken.bh\" as library\ndef main() {",
    );
    write(
        workspace.path(),
        "src/broken.bh",
        "}\ndef broken() -> int { return 1 }\n",
    );
    for frontend in if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    } {
        let output = workspace.run(&["check", "--frontend", frontend, "--json"]);
        fails(&output);
        let errors = bhasa::json::parse(&stdout(&output)).unwrap();
        let first = &errors.as_array().unwrap()[0];
        assert!(
            first
                .get("file")
                .and_then(bhasa::json::Value::as_str)
                .unwrap()
                .ends_with("src/main.bh"),
            "{}",
            describe(&output)
        );
        let span = first.get("span").unwrap();
        let eof = "import \"broken.bh\" as library\ndef main() {".len() as i64;
        assert_eq!(
            span.get("start").and_then(bhasa::json::Value::as_i64),
            Some(eof),
            "{}",
            describe(&output)
        );
        assert_eq!(
            span.get("end").and_then(bhasa::json::Value::as_i64),
            Some(eof),
            "{}",
            describe(&output)
        );
        assert_eq!(
            first.get("code").and_then(bhasa::json::Value::as_str),
            Some(if frontend == "rust" { "P001" } else { "BH200" })
        );
    }
}

#[test]
fn namespaced_worker_calls_keep_transitive_extern_safety_checks() {
    let workspace = project("namespaced-worker");
    write(workspace.path(),"src/worker.bh","extern def native(value: int) -> int\ndef job(value: int) -> int { return native(value) }\n");
    write(workspace.path(),"src/main.bh","import \"worker.bh\" as worker\ndef main() { var task = spawn(worker.job(1)); var result = join(task); print(result) }\n");
    for frontend in if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    } {
        let output = workspace.run(&["check", "--frontend", frontend, "--json"]);
        fails(&output);
        let errors = bhasa::json::parse(&stdout(&output)).unwrap();
        assert!(
            errors.as_array().unwrap().iter().any(|error| error
                .get("code")
                .and_then(bhasa::json::Value::as_str)
                == Some(if frontend == "rust" { "S020" } else { "BH209" })),
            "{}",
            describe(&output)
        );
        assert!(!stdout(&output).contains("__bhm_"));
    }
}

#[test]
fn package_aliases_and_confined_parent_imports_work() {
    let workspace = project("module-package-alias");
    write(
        workspace.path(),
        "library/bhasa.toml",
        "[package]\nname = \"library\"\nversion = \"0.1.0\"\nentry = \"lib.bh\"\n",
    );
    write(
        workspace.path(),
        "library/lib.bh",
        "def value() -> int { return 42 }\n",
    );
    write(workspace.path(),"bhasa.toml","[package]\nname = \"namespace-test\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n[dependencies]\nlibrary = \"library\"\n");
    write(
        workspace.path(),
        "src/main.bh",
        "import \"@library\" as library\ndef main() { print(library.value()) }\n",
    );
    check_both(&workspace, true, "");
    write(
        workspace.path(),
        "tests/main.bh",
        "import \"../src/lib.bh\" as library\ndef main() { print(library.value()) }\n",
    );
    write(
        workspace.path(),
        "src/lib.bh",
        "def value() -> int { return 42 }\n",
    );
    succeeds(&workspace.run(&["check", "tests/main.bh"]));
    // A sibling can be reached only when it remains under the manifest root.
    let outside = Workspace::new("module-outside");
    outside.write("outside.bh", "def value() -> int { return 1 }\n");
    let outside_name = outside.path().file_name().unwrap().to_str().unwrap();
    write(
        workspace.path(),
        "tests/main.bh",
        &format!("import \"../../{outside_name}/outside.bh\" as outside\ndef main() {{}}\n"),
    );
    let output = workspace.run(&["check", "tests/main.bh"]);
    fails(&output);
    assert!(
        stderr(&output).contains("escapes its package root"),
        "{}",
        describe(&output)
    );
}

#[test]
fn reserved_module_names_cannot_be_spelled_to_bypass_privacy() {
    let workspace = project("module-prefix");
    write(
        workspace.path(),
        "src/main.bh",
        "def __bhm_1_secret() {}\ndef main() {}\n",
    );
    let output = workspace.run(&["check"]);
    fails(&output);
    assert!(
        stderr(&output).contains("reserved for module lowering"),
        "{}",
        describe(&output)
    );
}

#[test]
fn collection_getters_preserve_private_generic_record_receivers() {
    let workspace = project("collections-private-receivers");
    write(
        workspace.path(),
        "src/library.bh",
        r#"
struct Box[T] {
    private value: T
    private def hidden(self) -> T { return self.value }
    def read(self) -> T { return self.hidden() }
}
def values() -> List[Box[int]] {
    var values = List[Box[int]]()
    values.push(Box[int](42))
    return values
}
def entries() -> Map[int, Box[int]] {
    var values = Map[int, Box[int]]()
    values.put(1, Box[int](42))
    return values
}
"#,
    );
    for expression in [
        "List[library.Box[int]]().get(0).value",
        "Map[int, library.Box[int]]().get(0).hidden()",
        "library.values().get(0).value",
        "library.entries().get(1).hidden()",
    ] {
        write(
            workspace.path(),
            "src/main.bh",
            &format!("import \"library.bh\" as library\ndef main() {{ print({expression}) }}\n"),
        );
        check_both(&workspace, false, "private to its defining module");
    }
    write(workspace.path(), "src/main.bh", "import \"library.bh\" as library\ndef main() { print(library.values().get(0).read() + library.entries().get(1).read()) }\n");
    check_both(&workspace, true, "");
}
