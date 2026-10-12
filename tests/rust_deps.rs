#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::*;

#[test]
fn rust_cache_child() {
    let Some(root) = std::env::var_os("BHASA_CACHE_TEST_ROOT") else {
        return;
    };
    let root = fs::canonicalize(PathBuf::from(root)).unwrap();
    let mut dependencies = BTreeMap::new();
    dependencies.insert("adapter".to_owned(), "adapter".to_owned());
    let target = std::env::var("BHASA_CACHE_TEST_TARGET").ok();
    let result = bhasa::rust_deps::build(
        &root.join("bhasa.toml"),
        &dependencies,
        target.as_deref(),
        std::env::var_os("BHASA_CACHE_TEST_RELEASE").is_some(),
        &root.join("build"),
    )
    .unwrap();
    assert_eq!(result.archives.len(), 1);
    assert!(result.protected_roots.contains(&root.join("adapter")));
    assert!(result.protected_roots.contains(&root.join("helper")));
    fs::write(
        root.join("report"),
        format!("{}\n{}", result.fresh, result.archives[0].display()),
    )
    .unwrap();
}

fn rust_tools() -> bool {
    match Command::new("cargo").arg("--version").output() {
        Ok(output) => {
            succeeds(&output);
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP Rust dependency test: Cargo was not found");
            false
        }
        Err(error) => panic!("cannot execute Cargo: {error}"),
    }
}

fn fixture_crates(workspace: &Workspace) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    fs::create_dir_all(workspace.file("helper/src")).unwrap();
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"cache-test\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n",
    );
    workspace.write("adapter/Cargo.toml", "[package]\nname = \"cached_adapter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\ncrate-type = [\"staticlib\"]\n[dependencies]\ncached_helper = { path = \"../helper\" }\n[workspace]\n");
    workspace.write(
        "helper/Cargo.toml",
        "[package]\nname = \"cached_helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    workspace.write("helper/src/lib.rs", "pub fn number() -> i64 { 42 }\n");
    workspace.write(
        "adapter/src/lib.rs",
        "#[no_mangle]\npub extern \"C\" fn cached_number() -> i64 { cached_helper::number() }\n",
    );
    let wrapper = workspace.write(
        "rustc-wrapper.sh",
        "#!/bin/sh\nset -eu\nprevious=\nfor argument do\n  if [ \"$previous\" = --crate-name ]; then\n    case \"$argument\" in cached_adapter|cached_helper) printf '%s\\n' \"$argument\" >> \"$BHASA_TEST_RUSTC_LOG\" ;; esac\n  fi\n  previous=$argument\ndone\nexec \"$@\"\n",
    );
    fs::set_permissions(wrapper, fs::Permissions::from_mode(0o755)).unwrap();
}

fn cached_build(
    workspace: &Workspace,
    release: bool,
    target: Option<&str>,
    rustflags: Option<&str>,
) -> (bool, PathBuf) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("rust_cache_child")
        .arg("--nocapture")
        .env("BHASA_CACHE_TEST_ROOT", workspace.path())
        .env("RUSTC_WRAPPER", workspace.file("rustc-wrapper.sh"))
        .env("BHASA_TEST_RUSTC_LOG", workspace.file("compiler-calls"))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("BHASA_CACHE_TEST_TARGET")
        .env_remove("BHASA_CACHE_TEST_RELEASE");
    if release {
        command.env("BHASA_CACHE_TEST_RELEASE", "1");
    }
    if let Some(target) = target {
        command.env("BHASA_CACHE_TEST_TARGET", target);
    }
    if let Some(flags) = rustflags {
        command.env("RUSTFLAGS", flags);
    }
    succeeds(&command.output().unwrap());
    let report = fs::read_to_string(workspace.file("report")).unwrap();
    let (fresh, archive) = report.split_once('\n').unwrap();
    (fresh == "true", PathBuf::from(archive))
}

fn compiler_calls(workspace: &Workspace) -> usize {
    fs::read_to_string(workspace.file("compiler-calls"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn linked_value(workspace: &Workspace, archive: &Path, expected: &str) {
    if clang().is_none() {
        return;
    }
    let source = workspace.write(
        "main.bh",
        "extern def cached_number() -> int\ndef main() { print(cached_number()) }\n",
    );
    let output = workspace
        .command()
        .args(["run", as_str(&source), "--link", as_str(archive)])
        .output()
        .unwrap();
    succeeds(&output);
    assert_eq!(stdout(&output), expected);
}

#[test]
fn cargo_reuses_unchanged_code_and_tracks_transitive_sources_profiles_and_flags() {
    if !rust_tools() {
        return;
    }
    let workspace = Workspace::new("rust cache λ");
    fixture_crates(&workspace);
    let (fresh, archive) = cached_build(&workspace, false, None, None);
    assert!(!fresh);
    assert!(workspace.file("adapter/Cargo.lock").is_file());
    let first_calls = compiler_calls(&workspace);
    assert_eq!(first_calls, 2, "one compiler invocation per Rust crate");
    let modified = fs::metadata(&archive).unwrap().modified().unwrap();
    let bytes = fs::read(&archive).unwrap();
    linked_value(&workspace, &archive, "42\n");

    let (fresh, unchanged) = cached_build(&workspace, false, None, None);
    assert!(fresh);
    assert_eq!(unchanged, archive);
    assert_eq!(compiler_calls(&workspace), first_calls);
    assert_eq!(
        fs::metadata(&archive).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(fs::read(&archive).unwrap(), bytes);

    workspace.write("helper/src/lib.rs", "pub fn number() -> i64 { 63 }\n");
    let (fresh, changed) = cached_build(&workspace, false, None, None);
    assert!(!fresh);
    assert_eq!(compiler_calls(&workspace), first_calls + 2);
    linked_value(&workspace, &changed, "63\n");
    assert!(cached_build(&workspace, false, None, None).0);
    assert_eq!(compiler_calls(&workspace), first_calls + 2);

    workspace.write("adapter/src/lib.rs", "#[no_mangle]\npub extern \"C\" fn cached_number() -> i64 { cached_helper::number() + 7 }\n");
    assert!(!cached_build(&workspace, false, None, None).0);
    assert_eq!(compiler_calls(&workspace), first_calls + 3);
    linked_value(&workspace, &archive, "70\n");

    let (fresh, optimized) = cached_build(&workspace, true, None, None);
    assert!(!fresh);
    assert_ne!(optimized, archive);
    let release_calls = compiler_calls(&workspace);
    assert!(cached_build(&workspace, true, None, None).0);
    assert_eq!(compiler_calls(&workspace), release_calls);
    assert!(cached_build(&workspace, false, None, None).0);
    assert_eq!(compiler_calls(&workspace), release_calls);
    linked_value(&workspace, &optimized, "70\n");

    assert!(!cached_build(&workspace, false, None, Some("--cfg bhasa_cache_test")).0);
    let changed_flags_calls = compiler_calls(&workspace);
    assert_eq!(changed_flags_calls, release_calls + 2);
    assert!(cached_build(&workspace, false, None, Some("--cfg bhasa_cache_test")).0);
    assert_eq!(compiler_calls(&workspace), changed_flags_calls);

    let llvm = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let (fresh, explicit_target) = cached_build(&workspace, false, Some(&llvm), None);
    assert!(!fresh);
    assert_ne!(explicit_target, archive);
    let target_calls = compiler_calls(&workspace);
    assert!(cached_build(&workspace, false, Some(&llvm), None).0);
    assert_eq!(compiler_calls(&workspace), target_calls);
    linked_value(&workspace, &explicit_target, "70\n");
}

#[test]
fn rust_dependency_requires_a_real_staticlib_and_reports_offline_errors() {
    if !rust_tools() {
        return;
    }
    let workspace = Workspace::new("rust-invalid-adapter");
    fixture_crates(&workspace);
    workspace.write("adapter/Cargo.toml", "[package]\nname = \"cached_adapter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ncached_helper = { path = \"../helper\" }\n[workspace]\n");
    let dependencies = BTreeMap::from([("adapter".to_owned(), "adapter".to_owned())]);
    let error = bhasa::rust_deps::build(
        &workspace.file("bhasa.toml"),
        &dependencies,
        None,
        false,
        &workspace.file("build"),
    )
    .unwrap_err();
    assert!(error.contains("crate-type = [\"staticlib\"]"), "{error}");
    workspace.write("adapter/src/lib.rs", "this is invalid rust\n");
    let error = bhasa::rust_deps::build(
        &workspace.file("bhasa.toml"),
        &dependencies,
        None,
        false,
        &workspace.file("build"),
    )
    .unwrap_err();
    assert!(error.contains("Cargo failed"), "{error}");
    assert!(error.contains("expected"), "{error}");
}

#[test]
fn project_run_links_rust_dependencies_without_recompiling_for_bhasa_edits() {
    if !rust_tools() || clang().is_none() {
        return;
    }
    let workspace = Workspace::new("rust project cache");
    fixture_crates(&workspace);
    workspace.write("bhasa.toml", "[package]\nname = \"cache-test\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[rust-dependencies]\nadapter = \"adapter\"\n");
    workspace.write(
        "main.bh",
        "extern def cached_number() -> int\ndef main() { print(cached_number()) }\n",
    );
    let run = |arguments: &[&str]| {
        workspace
            .command()
            .args(arguments)
            .env("RUSTC_WRAPPER", workspace.file("rustc-wrapper.sh"))
            .env("BHASA_TEST_RUSTC_LOG", workspace.file("compiler-calls"))
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .output()
            .unwrap()
    };
    let output = run(&["run"]);
    succeeds(&output);
    assert_eq!(stdout(&output), "42\n");
    let calls = compiler_calls(&workspace);
    assert_eq!(calls, 2);
    let output = run(&["run"]);
    succeeds(&output);
    assert_eq!(stdout(&output), "42\n");
    assert_eq!(compiler_calls(&workspace), calls);

    workspace.write(
        "main.bh",
        "extern def cached_number() -> int\ndef main() { print(cached_number() + 5) }\n",
    );
    let output = run(&["run"]);
    succeeds(&output);
    assert_eq!(stdout(&output), "47\n");
    assert_eq!(compiler_calls(&workspace), calls);

    let source = fs::read(workspace.file("adapter/src/lib.rs")).unwrap();
    let output = run(&["build", "-o", "adapter/src/lib.rs"]);
    fails(&output);
    assert!(stderr(&output).contains("Rust dependency source directory"));
    assert_eq!(
        fs::read(workspace.file("adapter/src/lib.rs")).unwrap(),
        source
    );
    assert_eq!(compiler_calls(&workspace), calls);
}
