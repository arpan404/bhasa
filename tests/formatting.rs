mod common;
use common::*;
use std::fs;

#[test]
fn fmt_check_preserves_source_and_fmt_is_idempotent() {
    let workspace = Workspace::new("fmt-cli");
    let original = "# keep\ndef main(){var answer=40+2;print(answer);for i in range(4){if i==1{continue};if i==3{break};print(i)};var values=List[buffer]();values.push(buffer(\"owned\"));for value in values{print(value);break}}\n";
    let source = workspace.write("main.bh", original);
    let check = workspace.run(&["fmt", as_str(&source), "--check"]);
    fails(&check);
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
    succeeds(&workspace.run(&["fmt", as_str(&source)]));
    let formatted = fs::read_to_string(&source).unwrap();
    assert!(formatted.contains("    var answer = 40 + 2;\n"));
    assert!(formatted.starts_with("# keep\n"));
    succeeds(&workspace.run(&["fmt", as_str(&source), "--check"]));
    succeeds(&workspace.run(&["fmt", as_str(&source)]));
    assert_eq!(fs::read_to_string(&source).unwrap(), formatted);
    assert_eq!(
        bhasa::compile_to_llvm(original, "x86_64-unknown-linux-gnu"),
        bhasa::compile_to_llvm(&formatted, "x86_64-unknown-linux-gnu")
    );
}

#[test]
fn invalid_format_input_never_overwrites_the_file() {
    let workspace = Workspace::new("fmt-invalid");
    let original = "def main() {print(\"bad\\q\")}";
    let source = workspace.write("main.bh", original);
    fails(&workspace.run(&["fmt", as_str(&source)]));
    assert_eq!(fs::read_to_string(&source).unwrap(), original);
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 1);
}
