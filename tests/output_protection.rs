mod common;
use common::*;
use std::fs;
use std::process::Output;

fn project(label: &str) -> Workspace {
    let workspace = Workspace::new(label);
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[dependencies]\nmath = \"library\"\n",
    );
    workspace.write(
        "main.bh",
        "import \"@math\"\ndef main() { print(answer()) }\n",
    );
    fs::create_dir(workspace.file("library")).unwrap();
    workspace.write(
        "library/bhasa.toml",
        "[package]\nname = \"math\"\nversion = \"0.1.0\"\nentry = \"lib.bh\"\n",
    );
    workspace.write("library/lib.bh", "def answer() -> int { return 42 }\n");
    workspace
}

fn adapter(workspace: &Workspace) {
    workspace.write("library/bhasa.toml", "[package]\nname = \"math\"\nversion = \"0.1.0\"\nentry = \"lib.bh\"\n[rust-dependencies]\nadapter = \"../adapter\"\n");
    fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    workspace.write("adapter/Cargo.toml", "[package]\nname = \"adapter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\ncrate-type = [\"staticlib\"]\n");
    workspace.write(
        "adapter/src/lib.rs",
        "#[no_mangle] pub extern \"C\" fn adapter() -> i64 { 42 }\n",
    );
}

fn without_tools(workspace: &Workspace, args: &[&str]) -> Output {
    workspace
        .command()
        .args(args)
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", workspace.file("absent-clang"))
        .output()
        .unwrap()
}

fn protected(output: &Output) {
    assert!(!output.status.success(), "{}", describe(output));
    let message = stderr(output);
    assert!(
        message.contains("refusing to overwrite input")
            || message.contains("Rust dependency source directory")
            || message.contains("aliases Rust dependency source"),
        "{message}"
    );
    assert!(!message.contains("cannot launch"), "{message}");
}

#[test]
fn existing_and_future_locks_of_every_loaded_package_are_protected() {
    let workspace = project("protect-locks");
    for path in ["bhasa.lock", "library/bhasa.lock"] {
        for action in ["emit-llvm", "build"] {
            protected(&without_tools(&workspace, &[action, "main.bh", "-o", path]));
            assert!(!workspace.file(path).exists());
        }
        protected(&without_tools(
            &workspace,
            &["build", "main.bh", "--emit", "object", "-o", path],
        ));
        assert!(!workspace.file(path).exists());
    }
    succeeds(
        &workspace
            .command()
            .current_dir(workspace.file("library"))
            .args(["lock", "create"])
            .env("CARGO", workspace.file("absent-cargo"))
            .env("BHASA_CLANG", workspace.file("absent-clang"))
            .output()
            .unwrap(),
    );
    succeeds(&without_tools(&workspace, &["lock", "create"]));
    for path in ["bhasa.lock", "library/bhasa.lock"] {
        let before = fs::read(workspace.file(path)).unwrap();
        for args in [
            vec!["emit-llvm", "main.bh", "-o", path],
            vec!["build", "main.bh", "-o", path],
            vec!["build", "main.bh", "--emit", "object", "-o", path],
        ] {
            protected(&without_tools(&workspace, &args));
            assert_eq!(fs::read(workspace.file(path)).unwrap(), before);
        }
    }
    succeeds(&without_tools(&workspace, &["lock", "check"]));
    assert!(!workspace.file("build").exists());
}

#[test]
fn absent_optional_files_do_not_block_unrelated_ir_or_object_outputs() {
    let workspace = project("protect-optional");
    adapter(&workspace);
    succeeds(&without_tools(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "artifacts/main.ll"],
    ));
    assert!(fs::read_to_string(workspace.file("artifacts/main.ll"))
        .unwrap()
        .contains("@main"));
    // Object emission must not invoke Cargo or create its lock or cache.
    let output = workspace
        .command()
        .args([
            "build",
            "main.bh",
            "--emit",
            "object",
            "-o",
            "artifacts/main.o",
        ])
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", "clang")
        .output()
        .unwrap();
    succeeds(&output);
    assert!(
        fs::metadata(workspace.file("artifacts/main.o"))
            .unwrap()
            .len()
            > 0
    );
    for file in [
        "bhasa.lock",
        "library/bhasa.lock",
        "adapter/Cargo.lock",
        "adapter/bhasa-adapter.json",
        "adapter/target",
    ] {
        assert!(!workspace.file(file).exists(), "unexpected {file}");
    }
}

#[test]
fn all_declared_adapter_inputs_and_future_source_paths_are_protected_before_mkdir() {
    let workspace = project("protect-adapter");
    adapter(&workspace);
    for path in [
        "adapter/Cargo.toml",
        "adapter/src/lib.rs",
        "adapter/Cargo.lock",
        "adapter/bhasa-adapter.json",
        "adapter/new/deep/artifact",
    ] {
        let before = fs::read(workspace.file(path)).ok();
        for args in [
            vec!["emit-llvm", "main.bh", "-o", path],
            vec!["build", "main.bh", "-o", path],
            vec!["build", "main.bh", "--emit", "object", "-o", path],
        ] {
            protected(&without_tools(&workspace, &args));
            assert_eq!(fs::read(workspace.file(path)).ok(), before);
            assert!(!workspace.file("adapter/new").exists());
        }
    }
    assert!(!workspace.file("build").exists());
    assert!(!workspace.file("adapter/target").exists());
}

#[cfg(unix)]
#[test]
fn locks_and_adapter_sources_reject_symlinks_hardlinks_and_parent_aliases() {
    use std::os::unix::fs::symlink;
    let workspace = project("protect-aliases");
    adapter(&workspace);
    succeeds(&without_tools(&workspace, &["lock", "create"]));
    for (input, alias) in [
        ("bhasa.lock", "lock"),
        ("adapter/src/lib.rs", "rust"),
        ("adapter/Cargo.toml", "cargo"),
    ] {
        let source = workspace.file(input);
        let before = fs::read(&source).unwrap();
        for (kind, path) in [
            ("symbolic", format!("{alias}-symlink")),
            ("hard", format!("{alias}-hardlink")),
        ] {
            if kind == "symbolic" {
                symlink(&source, workspace.file(&path)).unwrap();
            } else {
                fs::hard_link(&source, workspace.file(&path)).unwrap();
            }
            for args in [
                vec!["emit-llvm", "main.bh", "-o", &path],
                vec!["build", "main.bh", "-o", &path],
                vec!["build", "main.bh", "--emit", "object", "-o", &path],
            ] {
                protected(&without_tools(&workspace, &args));
            }
            assert_eq!(fs::read(&source).unwrap(), before);
        }
    }
    symlink(workspace.file("adapter"), workspace.file("adapter-alias")).unwrap();
    protected(&without_tools(
        &workspace,
        &[
            "emit-llvm",
            "main.bh",
            "-o",
            "adapter-alias/new/artifact.ll",
        ],
    ));
    assert!(!workspace.file("adapter/new").exists());
    symlink(
        workspace.file("library/bhasa.lock"),
        workspace.file("future-lock"),
    )
    .unwrap();
    protected(&without_tools(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "future-lock"],
    ));
    assert!(!workspace.file("library/bhasa.lock").exists());
}

#[test]
fn object_and_ir_keep_sidecar_validation_read_only_and_native_keeps_preflight() {
    let workspace = project("protect-sidecar-preflight");
    adapter(&workspace);
    workspace.write("adapter/bhasa-adapter.json", "{ invalid metadata }");
    succeeds(&without_tools(
        &workspace,
        &["emit-llvm", "main.bh", "-o", "artifacts/main.ll"],
    ));
    let object = workspace
        .command()
        .args([
            "build",
            "main.bh",
            "--emit",
            "object",
            "-o",
            "artifacts/main.o",
        ])
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", "clang")
        .output()
        .unwrap();
    succeeds(&object);
    let native = without_tools(&workspace, &["build", "main.bh", "-o", "artifacts/main"]);
    assert!(!native.status.success());
    assert!(
        stderr(&native).contains("invalid adapter contract"),
        "{}",
        describe(&native)
    );
    assert!(!workspace.file("adapter/Cargo.lock").exists());
    assert!(!workspace.file("artifacts/main").exists());
}

#[cfg(unix)]
#[test]
fn run_default_output_rejects_a_protected_adapter_directory_before_cargo() {
    use std::os::unix::fs::symlink;
    let workspace = project("protect-run-default");
    adapter(&workspace);
    symlink(workspace.file("adapter"), workspace.file("build")).unwrap();
    let output = workspace
        .command()
        .args(["run", "main.bh"])
        .env("CARGO", workspace.file("absent-cargo"))
        .env("BHASA_CLANG", "clang")
        .output()
        .unwrap();
    protected(&output);
    assert!(!workspace.file("adapter/app").exists());
    assert!(!workspace.file("adapter/Cargo.lock").exists());
    assert!(!workspace.file("adapter/rust").exists());

    let locked = project("protect-run-lock");
    succeeds(&without_tools(&locked, &["lock", "create"]));
    let before = fs::read(locked.file("bhasa.lock")).unwrap();
    fs::create_dir(locked.file("build")).unwrap();
    symlink(locked.file("bhasa.lock"), locked.file("build/app")).unwrap();
    symlink(locked.file("bhasa.lock"), locked.file("build/main")).unwrap();
    for args in [vec!["run"], vec!["run", "main.bh"]] {
        let output = locked
            .command()
            .args(args)
            .env("CARGO", locked.file("absent-cargo"))
            .env("BHASA_CLANG", "clang")
            .output()
            .unwrap();
        protected(&output);
    }
    assert_eq!(fs::read(locked.file("bhasa.lock")).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn prospective_reserved_names_and_case_aliased_adapter_roots_are_protected() {
    let workspace = project("protect-case-aliases");
    adapter(&workspace);
    for path in ["BHASA.LOCK", "library/BHASA.LOCK"] {
        protected(&without_tools(
            &workspace,
            &["emit-llvm", "main.bh", "-o", path],
        ));
        assert!(!workspace.file(path).exists());
    }
    // Only this assertion depends on the filesystem's case behavior.
    if workspace.file("ADAPTER").is_dir() {
        protected(&without_tools(
            &workspace,
            &["emit-llvm", "main.bh", "-o", "ADAPTER/new/result.ll"],
        ));
        assert!(!workspace.file("adapter/new").exists());
    }
}
