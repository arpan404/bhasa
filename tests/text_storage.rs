#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::fs;
use std::process::Command;

fn modes() -> Vec<&'static str> {
    if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    }
}
fn package(workspace: &Workspace, source: &str) {
    fs::create_dir(workspace.file("std")).unwrap();
    for entry in fs::read_dir(fixture("std")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), workspace.file("std").join(entry.file_name())).unwrap();
        }
    }
    workspace.write("bhasa.toml", "[package]\nname=\"private-text-test\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[dependencies]\nstd=\"std\"\n");
    workspace.write("main.bh", source);
}
fn executable(workspace: &Workspace, frontend: &str, release: bool) -> std::path::PathBuf {
    let executable = workspace.file(&format!("text-{frontend}-{release}"));
    let mut command = workspace.command();
    command
        .args(["build", "main.bh", "--frontend", frontend, "-o"])
        .arg(&executable);
    if release {
        command.arg("--release");
    }
    succeeds(&command.output().unwrap());
    executable
}

#[test]
fn text_storage_copies_methods_and_std_callers_keep_valid_utf8() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("private-text-copies");
    package(&workspace, include_str!("fixtures/text/private-storage.bh"));
    for frontend in modes() {
        for release in [false, true] {
            let output = Command::new(executable(&workspace, frontend, release))
                .output()
                .unwrap();
            succeeds(&output);
            assert_eq!(stdout(&output), "private Text storage passed\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn text_storage_and_positional_construction_are_private_in_loaded_modules() {
    for (case, source, expected) in [
        (
            "constructor",
            "def main(){var value=Text(buffer())}",
            "M005",
        ),
        (
            "read",
            "def main(){var value=text(\"safe\");print(value.storage)}",
            "M005",
        ),
        (
            "assign",
            "def main(){var value=text(\"safe\");value.storage=buffer()}",
            "M005",
        ),
        (
            "mutate",
            "def main(){var value=text(\"safe\");set(value.storage,0,255)}",
            "M005",
        ),
        (
            "nested",
            "def main(){var value=Box[Text](text(\"safe\"));print(value.value.storage)}",
            "M005",
        ),
        (
            "old-field",
            "def main(){var value=text(\"safe\");print(value.bytes)}",
            "S026",
        ),
    ] {
        let workspace = Workspace::new(case);
        package(
            &workspace,
            &format!("# UTF-8 locations λ 🙂\nimport \"@std\"\n{source}\n"),
        );
        for frontend in modes() {
            let output = workspace
                .command()
                .args(["check", "main.bh", "--frontend", frontend, "--json"])
                .output()
                .unwrap();
            fails(&output);
            let json = bhasa::json::parse(&stdout(&output)).unwrap();
            let errors = json.as_array().unwrap();
            assert_eq!(errors.len(), 1, "{}", describe(&output));
            let diagnostic = &errors[0];
            assert_eq!(diagnostic.get("code").unwrap().as_str(), Some(expected));
            assert_eq!(
                diagnostic.get("file").unwrap().as_str(),
                workspace.file("main.bh").canonicalize().unwrap().to_str()
            );
            assert_eq!(diagnostic.get("line").unwrap().as_i64(), Some(3));
        }
    }
}

#[test]
fn checked_text_constructor_invalid_utf8_is_a_fatal_runtime_failure() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("private-text-constructor-trap");
    package(&workspace, "import \"@std\"\ndef main(){var invalid=buffer();push(invalid,255);var forged=text_from_bytes(invalid);print(\"unexpected\")}\n");
    for frontend in modes() {
        let output = Command::new(executable(&workspace, frontend, false))
            .output()
            .unwrap();
        fails(&output);
        assert!(output.stdout.is_empty());
        assert!(
            stderr(&output).contains("Bhasa runtime error: invalid UTF-8"),
            "{}",
            describe(&output)
        );
    }
}
