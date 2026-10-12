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
    workspace.write("bhasa.toml", "[package]\nname=\"storage-invariant-test\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[dependencies]\nstd=\"std\"\n");
    workspace.write("main.bh", source);
}
#[test]
fn collection_and_json_public_snapshots_preserve_private_storage() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("storage-invariant-copies");
    package(&workspace, include_str!("fixtures/storage_invariants.bh"));
    for frontend in modes() {
        for release in [false, true] {
            let executable = workspace.file(&format!("storage-{frontend}-{release}"));
            let mut command = workspace.command();
            command
                .args(["build", "main.bh", "--frontend", frontend, "-o"])
                .arg(&executable);
            if release {
                command.arg("--release");
            }
            succeeds(&command.output().unwrap());
            let output = Command::new(&executable).output().unwrap();
            succeeds(&output);
            assert_eq!(
                stdout(&output),
                "private collection and JSON storage passed\n"
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}
#[test]
fn constructors_fields_and_json_parser_mutators_reject_in_importing_modules() {
    for (case, source, expected) in [
        (
            "list-constructor",
            "def main(){var v=StringList(buffer(),ints())}",
            "M005",
        ),
        (
            "list-storage",
            "def f(v:StringList){print(len(v.storage))}def main(){}",
            "M005",
        ),
        (
            "list-offsets",
            "def f(v:StringList){set(v.offsets,0,1)}def main(){}",
            "M005",
        ),
        (
            "intmap-constructor",
            "def main(){var v=StringIntMap(string_list(),ints())}",
            "M005",
        ),
        (
            "intmap-keys",
            "def f(v:StringIntMap){v.keys.clear()}def main(){}",
            "M005",
        ),
        (
            "intmap-values",
            "def f(v:StringIntMap){push(v.values,1)}def main(){}",
            "M005",
        ),
        (
            "stringmap-constructor",
            "def main(){var v=StringMap(string_list(),string_list())}",
            "M005",
        ),
        (
            "stringmap-values",
            "def f(v:StringMap){v.values.clear()}def main(){}",
            "M005",
        ),
        (
            "snapshot-offsets",
            "def f(v:StringMap){print(len(v.keys_snapshot().offsets))}def main(){}",
            "M005",
        ),
        (
            "json-constructor",
            "def main(){var v=JsonDoc(buffer(),ints(),0,0)}",
            "M005",
        ),
        (
            "json-source",
            "def f(d:JsonDoc){set(d.source,0,255)}def main(){}",
            "M005",
        ),
        (
            "json-nodes",
            "def f(d:JsonDoc){push(d.nodes,99)}def main(){}",
            "M005",
        ),
        (
            "json-cursor",
            "def f(d:JsonDoc){d.cursor=0}def main(){}",
            "M005",
        ),
        (
            "json-error",
            "def f(d:JsonDoc){d.error=0}def main(){}",
            "M005",
        ),
        (
            "json-white",
            "def f(d:JsonDoc){d.white()}def main(){}",
            "M005",
        ),
        (
            "json-peek",
            "def f(d:JsonDoc){print(d.peek())}def main(){}",
            "M005",
        ),
        (
            "json-hex",
            "def f(d:JsonDoc){print(d.hex())}def main(){}",
            "M005",
        ),
        (
            "json-scan-string",
            "def f(d:JsonDoc){d.scan_string()}def main(){}",
            "M005",
        ),
        (
            "json-digits",
            "def f(d:JsonDoc){print(d.digits())}def main(){}",
            "M005",
        ),
        (
            "json-number",
            "def f(d:JsonDoc){d.number()}def main(){}",
            "M005",
        ),
        (
            "json-literal",
            "def f(d:JsonDoc){d.literal(\"null\",4)}def main(){}",
            "M005",
        ),
        (
            "json-value",
            "def f(d:JsonDoc){print(d.value(-1,-1,-1,0))}def main(){}",
            "M005",
        ),
        (
            "json-unquote",
            "def main(){var value=json_unquote(buffer(\"null\"),0,4)}",
            "M003",
        ),
        ("json-hex-helper", "def main(){print(json_hex(65))}", "M003"),
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
            let parsed = bhasa::json::parse(&stdout(&output)).unwrap();
            let errors = parsed.as_array().unwrap();
            assert_eq!(errors.len(), 1, "{}", describe(&output));
            assert_eq!(errors[0].get("code").unwrap().as_str(), Some(expected));
            assert_eq!(
                errors[0].get("file").unwrap().as_str(),
                workspace.file("main.bh").canonicalize().unwrap().to_str()
            );
            assert_eq!(errors[0].get("line").unwrap().as_i64(), Some(3));
        }
    }
}
