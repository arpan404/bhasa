#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::fs;
use std::process::Command;

#[test]
fn unicode_scalars_invalid_sequences_and_owned_slices() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("text-runtime");
    for optimization in ["-O0", "-O2"] {
        let flags = [
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-pthread",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
            optimization,
        ];
        let core = workspace.file("core.o");
        let output = Command::new(&clang)
            .args(flags)
            .args([
                "-Dmalloc=bhasa_text_test_malloc",
                "-Drealloc=bhasa_text_test_realloc",
                "-Dfree=bhasa_text_test_free",
                "-c",
            ])
            .arg(fixture("runtime/core.c"))
            .arg("-o")
            .arg(&core)
            .output()
            .unwrap();
        succeeds(&output);
        let executable = workspace.file("text-checks");
        let output = Command::new(&clang)
            .args(flags)
            .arg("-I")
            .arg(fixture("runtime"))
            .arg(core)
            .arg(fixture("runtime/text.c"))
            .arg(fixture("tests/fixtures/text/runtime.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);
        let output = Command::new(&executable)
            .env("UBSAN_OPTIONS", "halt_on_error=1")
            .output()
            .unwrap();
        succeeds(&output);
        assert_eq!(
            stdout(&output),
            "UTF-8 scalar and ownership checks passed\n"
        );
        assert!(output.stderr.is_empty(), "{}", describe(&output));
        for (mode, message) in [
            ("invalid", "invalid UTF-8"),
            ("range", "codepoint slice range out of bounds"),
            ("negative", "codepoint slice range out of bounds"),
        ] {
            let output = Command::new(&executable)
                .arg(mode)
                .env("ASAN_OPTIONS", "detect_leaks=0")
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            assert_eq!(stderr(&output), format!("Bhasa runtime error: {message}\n"));
        }
    }
}

#[test]
fn bhasa_text_and_concrete_collections_preserve_copies_and_optional_results() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("text-library");
    for name in ["text.bh", "collections.bh", "result.bh"] {
        fs::copy(fixture(&format!("std/{name}")), workspace.file(name)).unwrap();
    }
    fs::copy(
        fixture("tests/fixtures/text/library.bh"),
        workspace.file("main.bh"),
    )
    .unwrap();
    let mut frontends = vec!["rust"];
    if cfg!(feature = "bhasa-frontend") {
        frontends.push("bhasa");
    }
    for frontend in frontends {
        succeeds(&workspace.run(&["check", "main.bh", "--frontend", frontend]));
        for release in [false, true] {
            let mut command = workspace.command();
            command.args(["run", "main.bh", "--frontend", frontend]);
            if release {
                command.arg("--release");
            }
            let output = command.output().unwrap();
            succeeds(&output);
            assert_eq!(stdout(&output), "Text and concrete collections passed\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}
