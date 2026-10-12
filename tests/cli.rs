use std::fs;

mod common;
use common::*;

#[test]
fn frontend_selection_is_explicit_and_never_silently_falls_back() {
    let workspace = Workspace::new("frontend-selection");
    let source = workspace.write("main.bh", "def main() { print(42) }\n");
    succeeds(&workspace.run(&["check", as_str(&source), "--frontend", "rust"]));
    fails(&workspace.run(&["check", as_str(&source), "--frontend", "unknown"]));
    let output = workspace.run(&["check", as_str(&source), "--frontend", "bhasa", "--json"]);
    if cfg!(feature = "bhasa-frontend") {
        succeeds(&output);
        assert_eq!(stdout(&output), "[]\n");
    } else {
        fails(&output);
        assert!(stdout(&output).contains("H002"), "{}", describe(&output));
        assert!(
            stdout(&output).contains("build-hybrid.sh"),
            "{}",
            describe(&output)
        );
    }
}

#[test]
fn check_and_ir_emission_work_without_clang() {
    let workspace = Workspace::new("without-clang");
    let source = workspace.write("input with spaces.bh", "def main() { print(42) }");
    let unavailable = workspace.file("missing-clang");
    for subcommand in ["check", "emit-llvm"] {
        let output = workspace
            .command()
            .arg(subcommand)
            .arg(&source)
            .env("BHASA_CLANG", &unavailable)
            .output()
            .unwrap();
        succeeds(&output);
        if subcommand == "emit-llvm" {
            let ir = stdout(&output);
            assert!(ir.contains("target triple ="), "{ir}");
            assert!(ir.contains("define "), "{ir}");
            assert!(!ir.contains("Successfully"), "IR stdout contains chatter");
        }
    }
}

#[test]
fn emit_to_file_and_resolve_target_aliases() {
    let workspace = Workspace::new("emit-targets");
    let source = fixture("tests/fixtures/library.bh");
    for (alias, triple) in [
        ("linux-x64", "x86_64-unknown-linux-gnu"),
        ("linux-arm64", "aarch64-unknown-linux-gnu"),
        ("windows-x64", "x86_64-pc-windows-msvc"),
        ("macos-arm64", "arm64-apple-macosx"),
        ("macos-x64", "x86_64-apple-macosx"),
        ("wasm32", "wasm32-unknown-unknown"),
    ] {
        for target in [alias, triple] {
            let output_path = workspace.file("nested output/library.ll");
            let output = workspace
                .command()
                .arg("emit-llvm")
                .arg(&source)
                .args(["--target", target, "-o"])
                .arg(&output_path)
                .env("BHASA_CLANG", workspace.file("missing-clang"))
                .output()
                .unwrap();
            succeeds(&output);
            let ir = fs::read_to_string(output_path).unwrap();
            assert!(
                ir.contains(&format!("target triple = \"{triple}\"")),
                "{ir}"
            );
            assert!(
                !stdout(&output).contains("define "),
                "-o leaked IR to stdout"
            );
        }
    }
}

#[test]
fn help_version_and_targets_describe_the_cli() {
    let workspace = Workspace::new("help");
    let help = workspace.run(&["--help"]);
    succeeds(&help);
    let text = format!("{}{}", stdout(&help), stderr(&help));
    for subcommand in ["check", "emit-llvm", "build", "run", "targets"] {
        assert!(text.contains(subcommand), "{text}");
    }
    let version = workspace.run(&["--version"]);
    succeeds(&version);
    assert!(stdout(&version).contains(env!("CARGO_PKG_VERSION")));
    let targets = workspace.run(&["targets"]);
    succeeds(&targets);
    for target in [
        "native",
        "linux-x64",
        "linux-arm64",
        "windows-x64",
        "macos-arm64",
        "macos-x64",
        "wasm32",
    ] {
        assert!(stdout(&targets).contains(target), "{}", describe(&targets));
    }
}

#[test]
fn bad_source_reports_path_message_and_json_diagnostics() {
    let workspace = Workspace::new("diagnostics");
    let source = workspace.write("bad source.bh", "def main() {\n    print(missing)\n}\n");
    let plain = workspace.run(&["check", as_str(&source)]);
    fails(&plain);
    let error = stderr(&plain);
    assert!(error.contains("bad source.bh"), "{error}");
    assert!(error.contains("missing"), "{error}");
    assert!(
        error.contains('2'),
        "diagnostic omitted source line: {error}"
    );

    let json = workspace.run(&["check", as_str(&source), "--json"]);
    fails(&json);
    let text = stdout(&json);
    let text = text.trim();
    assert!(
        (text.starts_with('[') && text.ends_with(']'))
            || (text.starts_with('{') && text.ends_with('}')),
        "JSON stdout is not one document: {}",
        describe(&json)
    );
    for field in ["\"code\"", "\"message\"", "missing"] {
        assert!(text.contains(field), "missing {field} in {text}");
    }
    assert!(!text.contains("panicked"));
}

#[test]
fn invalid_sources_fail_all_compilation_commands() {
    let workspace = Workspace::new("bad-source");
    let source = fixture("tests/fixtures/invalid/type_mismatch.bh");
    for subcommand in ["check", "emit-llvm", "build", "run"] {
        let output = workspace.run(&[subcommand, as_str(&source)]);
        fails(&output);
        assert!(!stderr(&output).trim().is_empty(), "{}", describe(&output));
    }
}

#[test]
fn invalid_cli_inputs_fail_without_panicking() {
    let workspace = Workspace::new("bad-options");
    let source = fixture("examples/basics/hello.bh");
    for args in [
        vec!["unknown-command"],
        vec!["check"],
        vec!["check", "missing.bh"],
        vec!["check", as_str(&source), "--bogus"],
        vec!["emit-llvm", as_str(&source), "--target", "mystery-cpu"],
        vec!["emit-llvm", as_str(&source), "--target"],
        vec!["build", as_str(&source), "-o"],
        vec!["build", as_str(&source), "--emit", "banana"],
        vec!["build", as_str(&source), "--link"],
        vec!["run", as_str(&source), "--link"],
    ] {
        let output = workspace.run(&args);
        fails(&output);
        assert!(
            !stderr(&output).trim().is_empty(),
            "{args:?}: {}",
            describe(&output)
        );
    }
}

#[test]
fn missing_clang_has_an_actionable_build_error() {
    let workspace = Workspace::new("missing-clang");
    let output = workspace
        .command()
        .arg("build")
        .arg(fixture("examples/basics/hello.bh"))
        .env("BHASA_CLANG", workspace.file("missing-clang"))
        .output()
        .unwrap();
    fails(&output);
    let error = stderr(&output).to_lowercase();
    assert!(error.contains("clang"), "{error}");
    assert!(
        error.contains("not found")
            || error.contains("no such file")
            || error.contains("could not"),
        "{error}"
    );
}
