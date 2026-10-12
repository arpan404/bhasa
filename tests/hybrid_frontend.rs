#![cfg(feature = "bhasa-frontend")]
mod common;
use common::*;
use std::fs;

#[test]
fn bhasa_frontend_runs_through_the_host_and_module_loader() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("hybrid-frontend-cli");
    workspace.write("worker.bh", "def square(n: int) -> int { return n * n }\n");
    let source = workspace.write("main.bh", "import \"worker.bh\"\ndef main() { var task = spawn(square(7)); var value = join(task); var text = to_buffer(value); print(text) }\n");
    for frontend in ["rust", "bhasa"] {
        let output = workspace.run(&["run", as_str(&source), "--frontend", frontend]);
        succeeds(&output);
        assert_eq!(stdout(&output), "49\n");
    }
    fs::write(
        workspace.file("worker.bh"),
        "# original line\ndef square(n: int) -> int { return missing }\n",
    )
    .unwrap();
    let output = workspace.run(&["check", as_str(&source), "--frontend", "bhasa", "--json"]);
    assert!(!output.status.success());
    assert!(
        stdout(&output).contains("worker.bh"),
        "{}",
        describe(&output)
    );
    assert!(
        stdout(&output).contains("\"line\":2"),
        "{}",
        describe(&output)
    );
    assert!(
        stdout(&output).contains("\"code\":\"BH202\""),
        "{}",
        describe(&output)
    );
}

#[test]
fn full_native_frontend_handles_parallel_compilations() {
    std::thread::scope(|scope| {
        for worker in 0..8 {
            scope.spawn(move || {
                for index in 0..20 {
                    let source = format!("def main() {{ var value = buffer(\"thread{worker}-{index}\"); print(value) }}");
                    let ir = bhasa::compile_with_bhasa(&source, "x86_64-unknown-linux-gnu").unwrap();
                    assert!(ir.contains("define i32 @main("));
                    assert!(ir.contains("; bhasa runtime: core"));
                    assert!(bhasa::compile_with_bhasa("def main(){return absent}", "x86_64-unknown-linux-gnu").is_err());
                }
            });
        }
    });
}

#[test]
fn module_boundaries_cannot_complete_another_files_syntax() {
    let workspace = Workspace::new("hybrid-file-boundaries");
    for (entry, library) in [
        ("def main() { print(1)\n", "}\n"),
        ("def main(\n", ") {}\n"),
        ("def\n", "main() {}\n"),
        ("struct R { x: int\n", "}\ndef main() {}\n"),
    ] {
        let source = workspace.write("main.bh", &format!("import \"lib.bh\"\n{entry}"));
        workspace.write("lib.bh", library);
        for frontend in ["rust", "bhasa"] {
            let output =
                workspace.run(&["check", as_str(&source), "--frontend", frontend, "--json"]);
            fails(&output);
            assert!(!stdout(&output).contains("H001"), "{}", describe(&output));
        }
    }
    let source = workspace.write(
        "main.bh",
        "import \"lib.bh\"\ndef main() { print(value()) }",
    );
    workspace.write(
        "lib.bh",
        "def value() -> int { return 42 } # no final newline",
    );
    succeeds(&workspace.run(&["check", as_str(&source), "--frontend", "bhasa"]));
    for ending in ["\r", "\r\n", "\n", ""] {
        workspace.write(
            "lib.bh",
            &format!("def value() -> int {{ return 42 }}{ending}"),
        );
        succeeds(&workspace.run(&["check", as_str(&source), "--frontend", "bhasa"]));
    }
}
