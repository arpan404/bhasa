mod common;
use bhasa::json::Value;
use common::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

fn fixture(label: &str) -> Workspace {
    let workspace = Workspace::new(label);
    workspace.write("bhasa.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[dependencies]\nmath = \"library\"\n");
    workspace.write(
        "main.bh",
        "import \"@math\"\ndef main() { print(double(21)) }\n",
    );
    fs::create_dir(workspace.file("library")).unwrap();
    workspace.write(
        "library/bhasa.toml",
        "[package]\nname = \"math\"\nversion = \"1.0.0\"\nentry = \"lib.bh\"\n",
    );
    workspace.write(
        "library/lib.bh",
        "def double(value: int) -> int { return value * 2 }\n",
    );
    workspace
}
fn invoke(workspace: &Workspace, args: &[&str]) -> std::process::Output {
    workspace
        .command()
        .args(args)
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", workspace.file("absent-clang"))
        .output()
        .unwrap()
}
fn report(output: &std::process::Output) -> Value {
    bhasa::json::parse(&stdout(output)).unwrap()
}
fn tree(path: &Path) -> BTreeMap<std::path::PathBuf, (Vec<u8>, SystemTime)> {
    let mut result = BTreeMap::new();
    for item in fs::read_dir(path).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            result.extend(tree(&item.path()));
        } else if item.file_type().unwrap().is_file() {
            result.insert(
                item.path(),
                (
                    fs::read(item.path()).unwrap(),
                    item.metadata().unwrap().modified().unwrap(),
                ),
            );
        }
    }
    result
}

#[test]
fn explicit_create_check_update_and_idempotent_update_preserve_sources() {
    let workspace = fixture("lock-cli-lifecycle");
    let before = tree(workspace.path());
    let missing = invoke(&workspace, &["lock", "check", "--json"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(report(&missing).get("ok"), Some(&Value::Bool(false)));
    assert_eq!(tree(workspace.path()), before);
    let create = invoke(&workspace, &["lock", "create", "--json"]);
    succeeds(&create);
    assert!(create.stderr.is_empty());
    assert_eq!(report(&create).get("changed"), Some(&Value::Bool(true)));
    assert_eq!(
        report(&create).get("command").and_then(Value::as_str),
        Some("lock create")
    );
    let locked = tree(workspace.path());
    let duplicate = invoke(&workspace, &["lock", "create", "--json"]);
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(stdout(&duplicate).contains("already exists"));
    assert_eq!(tree(workspace.path()), locked);
    succeeds(&invoke(&workspace, &["lock", "check", "--json"]));
    let unchanged = invoke(&workspace, &["lock", "update", "--json"]);
    succeeds(&unchanged);
    assert_eq!(report(&unchanged).get("changed"), Some(&Value::Bool(false)));
    assert_eq!(tree(workspace.path()), locked);
    workspace.write(
        "library/lib.bh",
        "def double(value: int) -> int { return value * 3 }\n",
    );
    let stale = tree(workspace.path());
    assert_eq!(
        invoke(&workspace, &["lock", "check", "--json"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(tree(workspace.path()), stale);
    let update = invoke(&workspace, &["lock", "update", "--json"]);
    succeeds(&update);
    assert_eq!(report(&update).get("changed"), Some(&Value::Bool(true)));
    succeeds(&invoke(&workspace, &["lock", "check"]));
    assert_eq!(
        fs::read_to_string(workspace.file("library/lib.bh")).unwrap(),
        "def double(value: int) -> int { return value * 3 }\n"
    );
    assert!(!workspace.file("build").exists());
    assert!(fs::read_dir(workspace.path()).unwrap().all(|item| !item
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".bhasa-lock-")));
}

#[test]
fn stale_lock_rejects_both_frontend_modes_before_native_or_cargo() {
    let workspace = fixture("lock-cli-stale-frontends");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    workspace.write(
        "library/lib.bh",
        "def double(value: int) -> int { return value * 4 }\n",
    );
    let before = tree(workspace.path());
    for frontend in ["rust", "bhasa"] {
        for action in ["check", "build", "run"] {
            let output = invoke(&workspace, &[action, "--frontend", frontend]);
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            assert!(
                stderr(&output).contains("checksum mismatch"),
                "{}",
                describe(&output)
            );
            assert!(!stderr(&output).contains("cannot start Cargo"));
            assert!(!stderr(&output).contains("cannot start Clang"));
        }
    }
    let inspected = invoke(&workspace, &["inspect", "--json"]);
    assert_eq!(inspected.status.code(), Some(1));
    assert_eq!(report(&inspected).get("ok"), Some(&Value::Bool(false)));
    assert!(report(&inspected).get("declarations").is_none());
    assert_eq!(tree(workspace.path()), before);
    assert!(!workspace.file("build").exists());
}

#[test]
fn help_usage_exit_codes_and_source_argument_boundaries() {
    let workspace = fixture("lock-cli-args");
    for args in [
        &["--help"][..],
        &["lock", "--help"][..],
        &["lock", "check", "--help"][..],
    ] {
        let output = invoke(&workspace, args);
        succeeds(&output);
        assert!(stdout(&output).contains("lock create|update|check [FILE] [--json]"));
    }
    for args in [
        &["lock", "--json"][..],
        &["lock", "unknown", "--json"][..],
        &["lock", "create", "--json", "--json"][..],
        &["lock", "check", "--frontend", "rust", "--json"][..],
        &["lock", "update", "main.bh", "other.bh", "--json"][..],
    ] {
        let output = invoke(&workspace, args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(report(&output).get("ok"), Some(&Value::Bool(false)));
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        invoke(&workspace, &["lock", "update", "--json"])
            .status
            .code(),
        Some(1)
    );
    assert!(!workspace.file("bhasa.lock").exists());
    workspace.write("-main.bh", "def main() {}\n");
    succeeds(&invoke(
        &workspace,
        &["lock", "create", "--json", "--", "-main.bh"],
    ));
    succeeds(&invoke(&workspace, &["lock", "check", "--", "-main.bh"]));
    // A lock for an explicit alternate source does not silently lock the default.
    assert_eq!(
        invoke(&workspace, &["lock", "check"]).status.code(),
        Some(1)
    );
}

#[test]
fn inspection_exposes_checked_lock_snapshot_and_manifest_checksums_without_writes() {
    let workspace = fixture("inspect-lock-metadata");
    let absent = invoke(&workspace, &["inspect", "--json"]);
    succeeds(&absent);
    assert_eq!(
        report(&absent).get("lock").unwrap().get("present"),
        Some(&Value::Bool(false))
    );
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let before = tree(workspace.path());
    let first = invoke(&workspace, &["inspect", "--json"]);
    let second = invoke(&workspace, &["inspect", "--json"]);
    succeeds(&first);
    succeeds(&second);
    assert_eq!(first.stdout, second.stdout);
    let value = report(&first);
    let lock = value.get("lock").unwrap();
    assert_eq!(lock.get("present"), Some(&Value::Bool(true)));
    assert_eq!(lock.get("verified"), Some(&Value::Bool(true)));
    assert_eq!(
        lock.get("snapshot")
            .unwrap()
            .get("format")
            .and_then(Value::as_str),
        Some("bhasa-loaded-graph-v1")
    );
    for package in value.get("packages").unwrap().as_array().unwrap() {
        assert_eq!(
            package
                .get("checksum")
                .and_then(Value::as_str)
                .unwrap()
                .len(),
            64
        );
    }
    assert!(value
        .get("adapters")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(tree(workspace.path()), before);
}

#[test]
fn inspection_reports_optional_adapter_contracts_and_rejects_stale_review_without_cargo() {
    let workspace = fixture("inspect-adapter-metadata");
    let original = fs::read_to_string(workspace.file("bhasa.toml")).unwrap();
    workspace.write(
        "bhasa.toml",
        &format!("{original}[rust-dependencies]\nadapter = \"adapter\"\n"),
    );
    fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    workspace.write("adapter/Cargo.toml", "[package]\nname=\"adapter\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\ncrate-type=[\"staticlib\"]\n");
    const SOURCE: &str = "pub extern \"C\" fn adapter_double(value:i64)->i64 { value }\n";
    workspace.write("adapter/src/lib.rs", SOURCE);
    let no_sidecar = invoke(&workspace, &["inspect", "--json"]);
    succeeds(&no_sidecar);
    assert_eq!(
        report(&no_sidecar)
            .get("adapters")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("contract"),
        Some(&Value::Null)
    );
    workspace.write("adapter/bhasa-adapter.json", r#"{"format":"bhasa-scalar-adapter-v1","source_sha256":"68361a32fd2d9098c6ee0801ef5923393944dc00861ddd6f33230bd6eb37b467","reviewed_by":"fixture reviewer","runtime":"native-no-gc","functions":[{"symbol":"adapter_double","parameters":["i64-copy"],"result":"i64-copy","error":{"kind":"infallible"},"unwind":"abort","thread":"reentrant"}]}"#);
    let before = tree(workspace.path());
    let output = invoke(&workspace, &["inspect", "--json"]);
    succeeds(&output);
    let value = report(&output);
    let adapter = &value.get("adapters").unwrap().as_array().unwrap()[0];
    assert_eq!(
        adapter.get("source_review_verified"),
        Some(&Value::Bool(true))
    );
    assert_eq!(adapter.get("no_gc_proven"), Some(&Value::Bool(false)));
    assert_eq!(
        adapter.get("worker_extern_allowed"),
        Some(&Value::Bool(false))
    );
    let function = &adapter
        .get("contract")
        .unwrap()
        .get("functions")
        .unwrap()
        .as_array()
        .unwrap()[0];
    assert_eq!(
        function.get("parameters").unwrap().as_array().unwrap()[0].as_str(),
        Some("i64-copy")
    );
    assert_eq!(
        function
            .get("error")
            .unwrap()
            .get("kind")
            .and_then(Value::as_str),
        Some("infallible")
    );
    assert_eq!(tree(workspace.path()), before);
    workspace.write("adapter/src/lib.rs", "changed after review\n");
    let stale = tree(workspace.path());
    let failed = invoke(&workspace, &["inspect", "--json"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(stdout(&failed).contains("source changed after review"));
    assert!(report(&failed).get("declarations").is_none());
    assert_eq!(tree(workspace.path()), stale);
    assert!(!workspace.file("adapter/Cargo.lock").exists());
    assert!(!workspace.file("build").exists());
}

#[cfg(unix)]
#[test]
fn lock_mutation_rejects_symlinks_and_preserves_the_target() {
    let workspace = fixture("lock-cli-symlink");
    workspace.write("kept", "unchanged");
    std::os::unix::fs::symlink(workspace.file("kept"), workspace.file("bhasa.lock")).unwrap();
    for mode in ["create", "update", "check"] {
        assert_eq!(
            invoke(&workspace, &["lock", mode, "--json"]).status.code(),
            Some(1)
        );
    }
    assert_eq!(
        fs::read_to_string(workspace.file("kept")).unwrap(),
        "unchanged"
    );
    assert!(fs::symlink_metadata(workspace.file("bhasa.lock"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn default_and_explicit_symlink_entry_create_and_check_the_same_graph() {
    let workspace = fixture("lock-cli-entry-alias");
    fs::rename(workspace.file("main.bh"), workspace.file("real.bh")).unwrap();
    std::os::unix::fs::symlink("real.bh", workspace.file("main.bh")).unwrap();
    succeeds(&invoke(&workspace, &["lock", "create", "--json"]));
    succeeds(&invoke(&workspace, &["lock", "check", "main.bh", "--json"]));
    succeeds(&invoke(&workspace, &["lock", "check", "real.bh", "--json"]));
    let decoded = bhasa::package_lock::Snapshot::decode(
        &fs::read_to_string(workspace.file("bhasa.lock")).unwrap(),
    )
    .unwrap();
    assert!(decoded.encode().contains("\"entry\":\"real.bh\""));
}

#[test]
fn test_graph_policy_rejects_changed_primary_dependency_even_when_test_omits_it() {
    let workspace = fixture("lock-cli-test-policy-stale");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write("tests/basic.bh", "def test_ok() -> bool { return true }\n");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    workspace.write(
        "library/lib.bh",
        "def double(value: int) -> int { return value * 5 }\n",
    );
    let before = tree(workspace.path());
    let output = invoke(&workspace, &["test", "tests/basic.bh", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
    assert!(
        stdout(&output).contains("checksum mismatch"),
        "{}",
        describe(&output)
    );
    assert_eq!(tree(workspace.path()), before);
    assert!(!workspace.file("build").exists());
}

#[cfg(unix)]
#[test]
fn every_loaded_adapter_is_preflighted_before_any_cargo_or_clang_call() {
    use std::os::unix::fs::PermissionsExt;
    for across_manifests in [false, true] {
        let workspace = fixture("lock-cli-adapter-preflight");
        let original = fs::read_to_string(workspace.file("bhasa.toml")).unwrap();
        let root_deps = if across_manifests {
            "[rust-dependencies]\na = \"adapter-a\"\n"
        } else {
            "[rust-dependencies]\na = \"adapter-a\"\nz = \"adapter-z\"\n"
        };
        workspace.write("bhasa.toml", &format!("{original}{root_deps}"));
        if across_manifests {
            let original = fs::read_to_string(workspace.file("library/bhasa.toml")).unwrap();
            workspace.write(
                "library/bhasa.toml",
                &format!("{original}[rust-dependencies]\nz = \"../adapter-z\"\n"),
            );
        }
        for name in ["adapter-a", "adapter-z"] {
            fs::create_dir_all(workspace.file(&format!("{name}/src"))).unwrap();
            workspace.write(&format!("{name}/Cargo.toml"), &format!("[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\ncrate-type=[\"staticlib\"]\n"));
            workspace.write(
                &format!("{name}/src/lib.rs"),
                "pub extern \"C\" fn adapter_double(value:i64)->i64 { value }\n",
            );
            workspace.write(&format!("{name}/bhasa-adapter.json"), r#"{"format":"bhasa-scalar-adapter-v1","source_sha256":"68361a32fd2d9098c6ee0801ef5923393944dc00861ddd6f33230bd6eb37b467","reviewed_by":"fixture reviewer","runtime":"native-no-gc","functions":[{"symbol":"adapter_double","parameters":["i64-copy"],"result":"i64-copy","error":{"kind":"infallible"},"unwind":"abort","thread":"reentrant"}]}"#);
        }
        workspace.write("adapter-z/src/lib.rs", "stale review\n");
        for name in ["cargo-counter", "clang-counter"] {
            let path = workspace.write(name, "#!/bin/sh\nprintf x >> \"$0.calls\"\nexit 42\n");
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let before = tree(workspace.path());
        let output = workspace
            .command()
            .args(["build"])
            .env("CARGO", workspace.file("cargo-counter"))
            .env("BHASA_CLANG", workspace.file("clang-counter"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
        assert!(
            stderr(&output).contains("source changed after review"),
            "{}",
            describe(&output)
        );
        assert!(!workspace.file("cargo-counter.calls").exists());
        assert!(!workspace.file("clang-counter.calls").exists());
        assert_eq!(tree(workspace.path()), before);
        assert!(!workspace.file("build").exists());
    }
}

// The coordinator may run this small native acceptance fixture in a scheduled
// build slot. All other CLI tests are read-only or only write an explicit lock.
#[test]
fn locked_main_graph_allows_a_separate_checked_project_test() {
    let workspace = fixture("lock-cli-project-test-native");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write(
        "tests/basic.bh",
        "import \"@math\"\ndef test_double() -> bool { return double(21) == 42 }\n",
    );
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let frontends = if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    };
    for frontend in frontends {
        let output = workspace.run(&["test", "tests/basic.bh", "--frontend", frontend, "--json"]);
        succeeds(&output);
        let report = bhasa::json::parse(&stdout(&output)).unwrap();
        assert_eq!(
            report.get("passed").and_then(bhasa::json::Value::as_i64),
            Some(1)
        );
        assert_eq!(
            report.get("failed").and_then(bhasa::json::Value::as_i64),
            Some(0)
        );
        assert!(
            stdout(&output).contains("test_double"),
            "{}",
            describe(&output)
        );
    }
}

#[test]
fn explicit_update_repairs_malformed_locks_while_check_and_inspect_are_read_only() {
    let workspace = fixture("lock-cli-malformed-update");
    workspace.write("bhasa.lock", "malformed document\n");
    let before = tree(workspace.path());
    assert_eq!(
        invoke(&workspace, &["lock", "check", "--json"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        invoke(&workspace, &["inspect", "--json"]).status.code(),
        Some(1)
    );
    assert_eq!(
        invoke(&workspace, &["lock", "create", "--json"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(tree(workspace.path()), before);
    succeeds(&invoke(&workspace, &["lock", "update", "--json"]));
    succeeds(&invoke(&workspace, &["lock", "check", "--json"]));
    assert!(fs::read_dir(workspace.path()).unwrap().all(|item| !item
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".bhasa-lock-")));
    let standalone = Workspace::new("lock-cli-standalone");
    standalone.write("main.bh", "def main() {}\n");
    let output = invoke(&standalone, &["lock", "create", "main.bh", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("require a bhasa.toml package"));
    assert!(!standalone.file("bhasa.lock").exists());
}
