mod common;
use bhasa::{json::Value, package_lock, project};
use common::*;
use std::fs;

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
#[cfg(unix)]
fn nested(label: &str) -> Workspace {
    let workspace = Workspace::new(label);
    fs::create_dir(workspace.file("nested")).unwrap();
    workspace.write("bhasa.toml", "[package]\nname=\"outer\"\nversion=\"0.1.0\"\nentry=\"entry.bh\"\n[dependencies]\ninner=\"nested\"\n");
    workspace.write(
        "nested/bhasa.toml",
        "[package]\nname=\"inner\"\nversion=\"0.1.0\"\nentry=\"lib.bh\"\n",
    );
    workspace.write(
        "nested/main.bh",
        "import \"@inner\"\ndef main(){print(answer())}\n",
    );
    workspace.write("nested/lib.bh", "def answer()->int{return 42}\n");
    std::os::unix::fs::symlink("nested/main.bh", workspace.file("entry.bh")).unwrap();
    workspace
}

#[cfg(unix)]
#[test]
fn outer_alias_preserves_authority_in_loader_default_cli_and_explicit_cli() {
    let workspace = nested("package-root-outer");
    let loaded = project::load_unparsed_for_lock_update(Some(&workspace.file("entry.bh"))).unwrap();
    let outer = workspace.file("bhasa.toml").canonicalize().unwrap();
    assert_eq!(loaded.root_manifest_path.as_ref(), Some(&outer));
    assert!(loaded.project.is_none());
    assert_eq!(package_lock::root_manifest(&loaded).unwrap().name, "outer");
    let wrong = loaded.manifests.iter().find(|m| m.name == "inner").unwrap();
    assert!(package_lock::Snapshot::capture(&loaded, wrong).is_err());
    let created = invoke(&workspace, &["lock", "create", "--json"]);
    succeeds(&created);
    assert_eq!(
        report(&created).get("lock").and_then(Value::as_str),
        Some(as_str(
            &workspace.file("bhasa.lock").canonicalize().unwrap()
        ))
    );
    assert!(!workspace.file("nested/bhasa.lock").exists());
    for args in [
        &["lock", "check", "--json"][..],
        &["lock", "check", "entry.bh", "--json"][..],
        &["check", "--json"][..],
        &["check", "entry.bh", "--json"][..],
    ] {
        succeeds(&invoke(&workspace, args));
    }
    for args in [
        &["inspect", "--json"][..],
        &["inspect", "entry.bh", "--json"][..],
    ] {
        let output = invoke(&workspace, args);
        succeeds(&output);
        let value = report(&output);
        assert_eq!(
            value
                .get("lock")
                .unwrap()
                .get("path")
                .and_then(Value::as_str),
            Some(as_str(
                &workspace.file("bhasa.lock").canonicalize().unwrap()
            ))
        );
        assert_eq!(
            value.get("lock").unwrap().get("verified"),
            Some(&Value::Bool(true))
        );
    }
}

#[cfg(unix)]
#[test]
fn source_change_rejects_default_explicit_lock_inspect_and_test_loads_at_outer_lock() {
    let workspace = nested("package-root-stale");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write("tests/basic.bh", "def test_ok()->bool{return true}\n");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    assert!(project::load_test_graph(&workspace.file("tests/basic.bh")).is_ok());
    assert!(project::load_editor_graph(&workspace.file("tests/basic.bh")).is_ok());
    workspace.write("nested/lib.bh", "def answer()->int{return 43}\n");
    let lock_bytes = fs::read(workspace.file("bhasa.lock")).unwrap();
    for frontend in ["rust", "bhasa"] {
        for args in [
            vec!["check", "--frontend", frontend, "--json"],
            vec!["check", "entry.bh", "--frontend", frontend, "--json"],
        ] {
            let output = invoke(&workspace, &args);
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            assert!(stdout(&output).contains("checksum mismatch"));
            assert!(stdout(&output).contains("bhasa.lock"));
        }
    }
    for args in [&["lock", "check", "--json"][..], &["inspect", "--json"][..]] {
        let output = invoke(&workspace, args);
        assert_eq!(output.status.code(), Some(1));
        assert!(stdout(&output).contains("checksum mismatch"));
    }
    for loader in [project::load_test_graph, project::load_editor_graph] {
        let errors = loader(&workspace.file("tests/basic.bh")).unwrap_err();
        assert_eq!(
            errors[0].path,
            workspace.file("bhasa.lock").canonicalize().unwrap()
        );
        assert!(errors[0].diagnostic.message.contains("checksum mismatch"));
    }
    let test = invoke(&workspace, &["test", "tests/basic.bh", "--json"]);
    assert_eq!(test.status.code(), Some(2));
    assert!(stdout(&test).contains("checksum mismatch"));
    assert_eq!(fs::read(workspace.file("bhasa.lock")).unwrap(), lock_bytes);
    assert!(!workspace.file("nested/bhasa.lock").exists());
    assert!(!workspace.file("build").exists());
}

#[cfg(unix)]
#[test]
fn direct_nested_source_selects_its_own_manifest_without_replacing_outer_identity() {
    let workspace = nested("package-root-direct");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let outer_bytes = fs::read(workspace.file("bhasa.lock")).unwrap();
    let loaded = project::load_unparsed(Some(&workspace.file("nested/lib.bh"))).unwrap();
    assert_eq!(package_lock::root_manifest(&loaded).unwrap().name, "inner");
    assert_eq!(
        loaded.root_manifest_path.as_ref(),
        Some(&workspace.file("nested/bhasa.toml").canonicalize().unwrap())
    );
    let created = invoke(&workspace, &["lock", "create", "nested/lib.bh", "--json"]);
    succeeds(&created);
    assert!(workspace.file("nested/bhasa.lock").exists());
    succeeds(&invoke(
        &workspace,
        &["lock", "check", "nested/lib.bh", "--json"],
    ));
    succeeds(&invoke(&workspace, &["lock", "check", "--json"]));
    assert_eq!(fs::read(workspace.file("bhasa.lock")).unwrap(), outer_bytes);
    workspace.write("nested/lib.bh", "def answer()->int{return 44}\n");
    for args in [
        &["check", "--json"][..],
        &["check", "nested/lib.bh", "--json"][..],
    ] {
        assert_eq!(invoke(&workspace, args).status.code(), Some(1));
    }
}

#[test]
fn editor_reroots_pinned_modules_only_after_primary_disk_graph_verification() {
    let workspace = Workspace::new("editor-pinned-root");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"app\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    workspace.write(
        "main.bh",
        "import \"a.bh\"\nimport \"other.bh\"\ndef main(){print(a())}\n",
    );
    workspace.write("a.bh", "import \"b.bh\"\ndef a()->int{return b()}\n");
    workspace.write("b.bh", "def b()->int{return 42}\n");
    workspace.write("other.bh", "def other()->int{return 1}\n");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let lock_bytes = fs::read(workspace.file("bhasa.lock")).unwrap();
    assert!(project::load_unparsed(Some(&workspace.file("a.bh"))).is_err());
    let loaded = project::load_editor_graph(&workspace.file("a.bh")).unwrap();
    assert_eq!(package_lock::root_manifest(&loaded).unwrap().name, "app");
    assert_eq!(loaded.files.len(), 2);
    workspace.write("new.bh", "def newly_added()->int{return 2}\n");
    assert!(
        project::load_editor_graph(&workspace.file("new.bh")).unwrap_err()[0]
            .diagnostic
            .message
            .contains("absent or changed")
    );
    workspace.write("other.bh", "def other()->int{return 3}\n");
    assert!(
        project::load_editor_graph(&workspace.file("a.bh")).unwrap_err()[0]
            .diagnostic
            .message
            .contains("checksum mismatch")
    );
    assert_eq!(fs::read(workspace.file("bhasa.lock")).unwrap(), lock_bytes);
    assert!(!workspace.file("build").exists());
}

#[test]
fn editor_and_tests_share_checked_test_source_policy_without_weakening_normal_loading() {
    let workspace = Workspace::new("editor-checked-tests");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"app\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    workspace.write("main.bh", "def main(){}\n");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write("tests/helper.bh", "def helper()->bool{return true}\n");
    workspace.write(
        "tests/basic.bh",
        "import \"helper.bh\"\ndef test_ok()->bool{return helper()}\n",
    );
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let entry = workspace.file("tests/basic.bh");
    assert!(project::load_unparsed(Some(&entry)).is_err());
    for loader in [project::load_test_graph, project::load_editor_graph] {
        assert_eq!(loader(&entry).unwrap().files.len(), 2);
    }
    workspace.write("extra.bh", "def extra()->bool{return true}\n");
    workspace.write(
        "tests/basic.bh",
        "import \"../extra.bh\"\ndef test_ok()->bool{return extra()}\n",
    );
    for loader in [project::load_test_graph, project::load_editor_graph] {
        assert!(loader(&entry).is_err());
    }
}

#[cfg(unix)]
#[test]
fn explicit_entry_alias_cannot_escape_the_selected_manifest_root() {
    let workspace = Workspace::new("package-root-confinement");
    fs::create_dir(workspace.file("outer")).unwrap();
    workspace.write(
        "outer/bhasa.toml",
        "[package]\nname=\"outer\"\nversion=\"0.1.0\"\nentry=\"entry.bh\"\n",
    );
    workspace.write("external.bh", "def main(){}\n");
    std::os::unix::fs::symlink("../external.bh", workspace.file("outer/entry.bh")).unwrap();
    let error = project::load_unparsed(Some(&workspace.file("outer/entry.bh"))).unwrap_err();
    assert!(error[0].diagnostic.message.contains("escapes"));
    assert!(!workspace.file("outer/bhasa.lock").exists());
}

#[cfg(unix)]
#[test]
fn editor_context_retains_checked_outer_identity_after_overlay_path_canonicalization() {
    let workspace = nested("editor-root-context");
    succeeds(&invoke(&workspace, &["lock", "create"]));
    let source = workspace.file("entry.bh").canonicalize().unwrap();
    let outer = workspace.file("bhasa.toml").canonicalize().unwrap();
    let loaded = project::load_editor_graph_in_package(&source, &outer).unwrap();
    assert_eq!(loaded.root_manifest_path.as_ref(), Some(&outer));
    assert_eq!(package_lock::root_manifest(&loaded).unwrap().name, "outer");
    workspace.write("nested/lib.bh", "def answer()->int{return 99}\n");
    assert!(
        project::load_editor_graph_in_package(&source, &outer).unwrap_err()[0]
            .diagnostic
            .message
            .contains("checksum mismatch")
    );
}
