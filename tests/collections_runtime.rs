#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!(
                "collection fixture exceeded deadline: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn typed_slot_ownership_collisions_growth_and_failures_are_checked() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collections-runtime");
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("collections-runtime");
        succeeds(
            &Command::new(&clang)
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-pedantic",
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                    optimization,
                ])
                .arg(fixture("tests/fixtures/collections/runtime.c"))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        assert_eq!(stdout(&output), "generic collections runtime passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
        for (mode, message) in [
            ("null-list", "invalid list"),
            ("bad-size", "invalid collection slot size"),
            ("overflow-size", "invalid collection slot size"),
            ("bad-callback", "invalid map callbacks"),
            ("list-get", "list index out of bounds"),
            ("list-set", "list index out of bounds"),
            ("list-output", "collection output must use independent"),
            ("list-allocation", "allocation failed"),
            ("map-missing", "map key not found"),
            ("map-allocation", "allocation failed"),
        ] {
            let output = bounded(
                Command::new(&executable)
                    .arg(mode)
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert_eq!(
                output.status.code(),
                Some(1),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                stderr(&output).contains(&format!("Bhasa runtime error: {message}")),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                !stderr(&output).contains("AddressSanitizer"),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                !stderr(&output).contains("UndefinedBehaviorSanitizer"),
                "{mode}: {}",
                describe(&output)
            );
        }
    }
}
