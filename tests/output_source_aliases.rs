#![cfg(unix)]
mod common;
use common::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Output;

fn project(label: &str) -> Workspace {
    let workspace = Workspace::new(label);
    workspace.write("bhasa.toml", "[package]\nname=\"app\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[rust-dependencies]\nadapter=\"adapter\"\n");
    workspace.write("main.bh", "def main() { print(42) }\n");
    fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    workspace.write("adapter/Cargo.toml", "[package]\nname=\"adapter\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\ncrate-type=[\"staticlib\"]\n");
    workspace
}
fn invoke(workspace: &Workspace, args: &[&str]) -> Output {
    workspace
        .command()
        .args(args)
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", workspace.file("absent-clang"))
        .output()
        .unwrap()
}
fn reject_all(workspace: &Workspace, destination: &Path) {
    let before = fs::read(destination).ok();
    let path = as_str(destination);
    for args in [
        vec!["emit-llvm", "main.bh", "-o", path],
        vec!["build", "main.bh", "-o", path],
        vec!["build", "main.bh", "--emit", "object", "-o", path],
    ] {
        let output = invoke(workspace, &args);
        assert!(!output.status.success(), "{}", describe(&output));
        assert!(
            stderr(&output).contains("aliases Rust dependency source"),
            "{}",
            describe(&output)
        );
        assert_eq!(fs::read(destination).ok(), before);
    }
    assert!(!workspace.file("adapter/Cargo.lock").exists());
    assert!(!workspace.file("adapter/target").exists());
}

#[test]
fn external_single_linked_file_targets_and_all_output_aliases_are_protected() {
    let workspace = project("protect-source-file-links");
    let shared = workspace.write("shared.rs", "pub fn shared() {}\n");
    symlink("../../shared.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    // This source initially has one hard link; nlink is not an alias guard.
    reject_all(&workspace, &shared);
    let alias = workspace.file("output-symlink");
    symlink(&shared, &alias).unwrap();
    reject_all(&workspace, &alias);
    let alias = workspace.file("output-hardlink");
    fs::hard_link(&shared, &alias).unwrap();
    reject_all(&workspace, &alias);
    if workspace.file("SHARED.RS").exists() {
        reject_all(&workspace, &workspace.file("SHARED.RS"));
    }
    succeeds(&invoke(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "artifacts/main.ll"],
    ));
    assert_eq!(fs::read_to_string(shared).unwrap(), "pub fn shared() {}\n");
}

#[test]
fn external_directory_targets_nested_aliases_and_cycles_are_walked_once() {
    let workspace = project("protect-source-directory-links");
    fs::remove_dir(workspace.file("adapter/src")).unwrap();
    fs::create_dir(workspace.file("shared-src")).unwrap();
    fs::create_dir(workspace.file("second-src")).unwrap();
    symlink("../shared-src", workspace.file("adapter/src")).unwrap();
    symlink("../second-src", workspace.file("shared-src/inner")).unwrap();
    symlink("../shared-src", workspace.file("second-src/back")).unwrap();
    let source = workspace.write("second-src/lib.rs", "pub fn shared() {}\n");
    reject_all(&workspace, &source);
    reject_all(&workspace, &workspace.file("second-src/new/deep.ll"));
    assert!(!workspace.file("second-src/new").exists());
    let hardlink = workspace.file("linked-output");
    fs::hard_link(&source, &hardlink).unwrap();
    reject_all(&workspace, &hardlink);
    if workspace.file("SHARED-SRC").is_dir() {
        reject_all(&workspace, &workspace.file("SHARED-SRC/new/deep.ll"));
        assert!(!workspace.file("shared-src/new").exists());
    }
    // Directory identity dedup must finish despite the back edge.
    succeeds(&invoke(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "artifacts/main.ll"],
    ));
}

#[test]
fn dangling_targets_reserve_files_directories_ancestors_and_case_variants() {
    let workspace = project("protect-dangling-source-links");
    symlink("../../future.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    for path in ["future.rs", "FUTURE.RS", "future.rs/nested/output.ll"] {
        reject_all(&workspace, &workspace.file(path));
        assert!(!workspace.file("future.rs").exists());
    }
    fs::remove_file(workspace.file("adapter/src/lib.rs")).unwrap();
    fs::remove_dir(workspace.file("adapter/src")).unwrap();
    symlink("../future-src/deep", workspace.file("adapter/src")).unwrap();
    for path in [
        "future-src",
        "future-src/deep/lib.rs",
        "FUTURE-SRC/DEEP/new/output.ll",
    ] {
        reject_all(&workspace, &workspace.file(path));
        assert!(!workspace.file("future-src").exists());
    }
    succeeds(&invoke(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "artifacts/main.ll"],
    ));
}

#[test]
fn nested_file_aliases_protect_intermediate_and_final_names() {
    let workspace = project("protect-nested-source-links");
    let source = workspace.write("shared.rs", "pub fn shared() {}\n");
    symlink("../../middle.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    symlink("shared.rs", workspace.file("middle.rs")).unwrap();
    reject_all(&workspace, &source);
    reject_all(&workspace, &workspace.file("middle.rs"));
    fs::remove_file(&source).unwrap();
    reject_all(&workspace, &source);
    reject_all(&workspace, &workspace.file("middle.rs"));
    assert!(!source.exists());
}

#[test]
fn unresolved_cycles_and_ambiguous_unicode_targets_fail_before_mkdir() {
    let workspace = project("protect-uncertain-source-links");
    symlink("../../first.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    symlink("second.rs", workspace.file("first.rs")).unwrap();
    symlink("first.rs", workspace.file("second.rs")).unwrap();
    let output = invoke(&workspace, &["emit-llvm", "main.bh", "-o", "new/output.ll"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("too many symlinks"),
        "{}",
        describe(&output)
    );
    assert!(!workspace.file("new").exists());
    fs::remove_file(workspace.file("adapter/src/lib.rs")).unwrap();
    symlink("../../résumé.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    reject_all(&workspace, &workspace.file("RÉSUMÉ.RS"));
    assert!(!workspace.file("RÉSUMÉ.RS").exists());
}

#[test]
fn run_default_destination_cannot_replace_an_external_source_alias() {
    let workspace = project("protect-run-source-links");
    let source = workspace.write("shared.rs", "pub fn shared() {}\n");
    symlink("../../shared.rs", workspace.file("adapter/src/lib.rs")).unwrap();
    fs::create_dir(workspace.file("build")).unwrap();
    symlink(&source, workspace.file("build/app")).unwrap();
    symlink(&source, workspace.file("build/main")).unwrap();
    for args in [vec!["run"], vec!["run", "main.bh"]] {
        let output = workspace
            .command()
            .args(args)
            .env("CARGO", workspace.file("absent-cargo"))
            .env("BHASA_CLANG", "clang")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", describe(&output));
        assert!(
            stderr(&output).contains("aliases Rust dependency source"),
            "{}",
            describe(&output)
        );
    }
    assert_eq!(fs::read_to_string(&source).unwrap(), "pub fn shared() {}\n");
    assert!(!workspace.file("adapter/Cargo.lock").exists());
}
