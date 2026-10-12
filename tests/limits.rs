mod common;
use common::*;

#[test]
fn oversized_expression_reports_a_diagnostic_instead_of_aborting() {
    let workspace = Workspace::new("expression-limit");
    let flat = format!("def main() {{ print({}) }}", vec!["1"; 20_000].join(" + "));
    let grouped = format!(
        "def main() {{ print({}1{}) }}",
        "(".repeat(1_000),
        ")".repeat(1_000)
    );
    let combined = format!(
        "def main() {{ {} print({}1{}) }} {}",
        "if true { ".repeat(120),
        "(".repeat(120),
        ")".repeat(120),
        "}".repeat(120)
    );
    for (name, contents) in [
        ("flat.bh", flat),
        ("grouped.bh", grouped),
        ("combined.bh", combined),
    ] {
        let path = workspace.write(name, &contents);
        let output = workspace.run(&["check", as_str(&path)]);
        assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
        assert!(stderr(&output).contains("limit"), "{}", describe(&output));
        assert!(!stderr(&output).contains("overflowed its stack"));
    }
}
