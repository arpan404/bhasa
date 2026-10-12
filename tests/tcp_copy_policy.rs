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
            let _ = child.kill();
            panic!(
                "TCP copy probe timed out: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn tcp_copy_has_no_fallible_descriptor_dup_and_releases_final_owner_once() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("tcp-copy-policy");
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("probe");
        succeeds(
            &Command::new(&clang)
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-pedantic",
                    "-pthread",
                    optimization,
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                ])
                .arg("-I")
                .arg(fixture("runtime"))
                .arg(fixture("tests/fixtures/network/tcp_copy_policy.c"))
                .arg(fixture("runtime/core.c"))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        assert_eq!(stdout(&output), "TCP shared copy policy passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
        for (mode, expected) in [
            ("allocation", "allocation failed"),
            ("allocation-shared", "allocation failed"),
            ("allocation-success-clone", "allocation failed"),
            ("overflow", "TCP reference count overflow"),
        ] {
            let output = bounded(
                Command::new(&executable)
                    .arg(mode)
                    .env("ASAN_OPTIONS", "detect_leaks=0")
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            assert!(output.stdout.is_empty());
            assert_eq!(
                stderr(&output),
                format!("Bhasa runtime error: {expected}\n")
            );
        }
    }
}
