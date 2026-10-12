#![cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
mod common;
use common::*;

fn wrapper(workspace: &Workspace) -> Option<PathBuf> {
    let clang = clang()?;
    workspace.write("clang-name", clang.to_str().unwrap());
    let path=workspace.write("clang-wrapper", "#!/bin/sh\nset -eu\nprobe=no\nfor arg do\n case \"$arg\" in --version|-###|-E|-emit-llvm|-print-*) probe=yes ;; esac\ndone\nif [ \"$probe\" = no ]; then printf 'compile\\n' >> \"$BHASA_CACHE_CALLS\"; fi\nexec \"$BHASA_CACHE_REAL_CLANG\" \"$@\"\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Some(path)
}
fn configure(command: &mut Command, workspace: &Workspace, tool: &Path) {
    command
        .env("BHASA_CLANG", tool)
        .env(
            "BHASA_CACHE_REAL_CLANG",
            fs::read_to_string(workspace.file("clang-name")).unwrap(),
        )
        .env("BHASA_CACHE_CALLS", workspace.file("calls"))
        .env("BHASA_CACHE_DEBUG", "1");
}
fn count(workspace: &Workspace) -> usize {
    fs::read_to_string(workspace.file("calls"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn run_cli(workspace: &Workspace, tool: &Path, args: &[&str]) -> Output {
    let mut command = workspace.command();
    configure(&mut command, workspace, tool);
    let output = command.args(args).output().unwrap();
    succeeds(&output);
    output
}
fn entries(workspace: &Workspace) -> Vec<PathBuf> {
    fs::read_dir(workspace.file("build/native-artifacts-v1"))
        .unwrap()
        .map(|item| item.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "bhc"))
        .collect()
}

#[test]
fn unchanged_cli_builds_and_runs_reuse_then_invalidate_or_recover() {
    let workspace = Workspace::new("incremental-cli");
    let Some(tool) = wrapper(&workspace) else {
        return;
    };
    workspace.write("main.bh", "def main() { print(buffer(\"first\")) }\n");
    run_cli(&workspace, &tool, &["build", "main.bh", "-o", "app"]);
    assert_eq!(count(&workspace), 1);
    run_cli(&workspace, &tool, &["build", "main.bh", "-o", "app"]);
    assert_eq!(count(&workspace), 1);
    let output = run_cli(&workspace, &tool, &["run", "main.bh"]);
    assert_eq!(stdout(&output), "first\n");
    assert_eq!(count(&workspace), 1);
    workspace.write("main.bh", "# A source-only comment does not change generated LLVM.\ndef main() { print(buffer(\"first\")) }\n");
    run_cli(&workspace, &tool, &["build", "main.bh", "-o", "app"]);
    assert_eq!(count(&workspace), 1);
    // Output publication remains protected even when a valid artifact is cached.
    let mut command = workspace.command();
    configure(&mut command, &workspace, &tool);
    fails(
        &command
            .args(["build", "main.bh", "-o", "main.bh"])
            .output()
            .unwrap(),
    );
    assert_eq!(count(&workspace), 1);
    workspace.write("main.bh", "def main() { print(buffer(\"second\")) }\n");
    assert_eq!(
        stdout(&run_cli(&workspace, &tool, &["run", "main.bh"])),
        "second\n"
    );
    assert_eq!(count(&workspace), 2);
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 3);
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 3);
    // Compiler wrapper changes participate even when --version remains identical.
    let mut contents = fs::read_to_string(&tool).unwrap();
    contents.push_str("# tool revision two\n");
    fs::write(&tool, contents).unwrap();
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 4);
    for path in entries(&workspace) {
        fs::write(path, b"partial/corrupt cache entry").unwrap();
    }
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 5);
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 5);
    for path in entries(&workspace) {
        let mut bytes = fs::read(&path).unwrap();
        if bytes.len() > 256 {
            let index = bytes.len() - 1;
            bytes[index] ^= 1;
            fs::write(&path, bytes).unwrap();
        }
    }
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 6);
    for path in entries(&workspace) {
        fs::remove_file(path).unwrap();
    }
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), 7);
    // Concurrent cold writers must publish complete artifacts and converge to a hit.
    for path in entries(&workspace) {
        fs::remove_file(path).unwrap();
    }
    let mut children = Vec::new();
    for name in ["one", "two"] {
        let mut command = workspace.command();
        configure(&mut command, &workspace, &tool);
        children.push(
            command
                .args(["build", "main.bh", "--release", "-o", name])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        succeeds(&child.wait_with_output().unwrap());
    }
    let calls = count(&workspace);
    assert!((8..=9).contains(&calls));
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(count(&workspace), calls);
    assert_eq!(
        stdout(&execute(&workspace.file("one"), &workspace)),
        "second\n"
    );
    assert_eq!(
        stdout(&execute(&workspace.file("two"), &workspace)),
        "second\n"
    );
    // Cache symlinks cannot redirect publication outside the intended directory.
    let outside = workspace.file("untouched");
    fs::write(&outside, b"sentinel").unwrap();
    for path in entries(&workspace) {
        fs::remove_file(&path).unwrap();
        symlink(&outside, path).unwrap();
    }
    run_cli(&workspace, &tool, &["run", "main.bh", "--release"]);
    assert_eq!(fs::read(&outside).unwrap(), b"sentinel");
    let directory = workspace.file("build/native-artifacts-v1");
    fs::rename(&directory, workspace.file("cache-held")).unwrap();
    fs::create_dir(workspace.file("outside-cache")).unwrap();
    symlink(workspace.file("outside-cache"), &directory).unwrap();
    run_cli(&workspace, &tool, &["build", "main.bh", "-o", "app"]);
    assert_eq!(
        fs::read_dir(workspace.file("outside-cache"))
            .unwrap()
            .count(),
        0
    );
}

fn direct(
    workspace: &Workspace,
    tool: &Path,
    ordinal: usize,
    source: &str,
    flags: &[&str],
    libs: &[PathBuf],
) -> bool {
    let temporary = workspace.file(&format!("temporary-{ordinal}"));
    fs::create_dir(&temporary).unwrap();
    let source_path = temporary.join("runtime.c");
    fs::write(&source_path, source).unwrap();
    let artifact = temporary.join("program");
    let mut command = Command::new(tool);
    configure(&mut command, workspace, tool);
    command.current_dir(workspace.path());
    command
        .args(flags)
        .args(["-x", "c"])
        .arg(&source_path)
        .arg("-o")
        .arg(&artifact)
        .env("CPATH", workspace.file("headers"));
    if !libs.is_empty() {
        command.args(["-x", "none"]).args(libs);
    }
    let entry =
        bhasa::build_cache::prepare(&command, &artifact, &temporary, &workspace.file("build"))
            .unwrap()
            .expect("known Clang plan must support caching");
    let hit = entry.restore(&artifact).unwrap();
    if !hit {
        succeeds(&command.output().unwrap());
        entry.store(&artifact).unwrap();
    }
    hit
}
#[test]
fn native_source_preprocessed_headers_libraries_flags_and_targets_are_inputs() {
    let workspace = Workspace::new("incremental-inputs");
    let Some(tool) = wrapper(&workspace) else {
        return;
    };
    fs::create_dir(workspace.file("headers")).unwrap();
    workspace.write("headers/value.h", "#define VALUE 40\n");
    let source = "#include <value.h>\nint main(void) { return VALUE; }\n";
    assert!(!direct(&workspace, &tool, 0, source, &["-O0"], &[]));
    assert!(direct(&workspace, &tool, 1, source, &["-O0"], &[]));
    assert_eq!(count(&workspace), 1);
    workspace.write("headers/value.h", "#define VALUE 41\n");
    assert!(!direct(&workspace, &tool, 2, source, &["-O0"], &[]));
    assert!(direct(&workspace, &tool, 3, source, &["-O0"], &[]));
    let revised = "#include <value.h>\nint main(void) { return VALUE + 1; }\n";
    assert!(!direct(&workspace, &tool, 4, revised, &["-O0"], &[]));
    assert!(!direct(&workspace, &tool, 5, revised, &["-O2"], &[]));
    assert!(!direct(
        &workspace,
        &tool,
        6,
        revised,
        &["-O2", "-DUNUSED_BUILD_FLAG=1"],
        &[]
    ));
    let object = "int value(void) { return 42; }\n";
    // Reuse the installed host CPU backend with a different object platform.
    // This requires neither an optional Wasm backend nor a foreign SDK/linker.
    let cross_target = format!(
        "{}-unknown-{}",
        std::env::consts::ARCH,
        if cfg!(target_os = "macos") {
            "linux-gnu"
        } else {
            "freebsd"
        }
    );
    assert!(!direct(
        &workspace,
        &tool,
        7,
        object,
        &["-c", "-target", &cross_target],
        &[]
    ));
    assert!(direct(
        &workspace,
        &tool,
        8,
        object,
        &["-c", "-target", &cross_target],
        &[]
    ));
    assert!(!direct(&workspace, &tool, 9, object, &["-c"], &[]));
    let library = workspace.file("adapter.o");
    for (step, value) in [(10, 1), (12, 2)] {
        let source = workspace.write(
            "adapter.c",
            &format!("int adapter(void) {{ return {value}; }}\n"),
        );
        succeeds(
            &Command::new(fs::read_to_string(workspace.file("clang-name")).unwrap())
                .arg("-c")
                .arg(source)
                .arg("-o")
                .arg(&library)
                .output()
                .unwrap(),
        );
        let app = "extern int adapter(void); int main(void) { return adapter(); }\n";
        assert!(!direct(
            &workspace,
            &tool,
            step,
            app,
            &["-O0"],
            std::slice::from_ref(&library)
        ));
        assert!(direct(
            &workspace,
            &tool,
            step + 1,
            app,
            &["-O0"],
            std::slice::from_ref(&library)
        ));
    }
}

#[test]
fn rust_dependency_freshness_precedes_native_artifact_lookup() {
    let workspace = Workspace::new("incremental-rust");
    let Some(tool) = wrapper(&workspace) else {
        return;
    };
    match Command::new("cargo").arg("--version").output() {
        Ok(output) => succeeds(&output),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP incremental Rust adapter: Cargo absent");
            return;
        }
        Err(error) => panic!("{error}"),
    }
    fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    workspace.write("bhasa.toml","[package]\nname = \"incremental\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[rust-dependencies]\nadapter = \"adapter\"\n");
    workspace.write("adapter/Cargo.toml","[package]\nname = \"incremental_adapter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\ncrate-type = [\"staticlib\"]\n[workspace]\n");
    workspace.write(
        "adapter/src/lib.rs",
        "#[no_mangle] pub extern \"C\" fn cached_value() -> i64 { 40 }\n",
    );
    workspace.write(
        "main.bh",
        "extern def cached_value() -> int\ndef main() { print(cached_value()) }\n",
    );
    assert_eq!(stdout(&run_cli(&workspace, &tool, &["run"])), "40\n");
    assert_eq!(count(&workspace), 1);
    assert_eq!(stdout(&run_cli(&workspace, &tool, &["run"])), "40\n");
    assert_eq!(count(&workspace), 1);
    workspace.write(
        "adapter/src/lib.rs",
        "#[no_mangle] pub extern \"C\" fn cached_value() -> i64 { 42 }\n",
    );
    assert_eq!(stdout(&run_cli(&workspace, &tool, &["run"])), "42\n");
    assert_eq!(count(&workspace), 2);
}
