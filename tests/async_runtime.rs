#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!(
                "async fixture exceeded 40 seconds: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn fixed_pool_backpressure_results_cleanup_and_worker_guards() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("async-runtime");
    for optimization in ["-O0", "-O2"] {
        let flags = [
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-pthread",
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-fno-omit-frame-pointer",
            optimization,
        ];
        let object = workspace.file("concurrency.o");
        let output = Command::new(&clang)
            .args(flags)
            .args([
                "-Dmalloc=bhasa_async_test_malloc",
                "-Dfree=bhasa_async_test_free",
                "-Dpthread_create=bhasa_async_test_pthread_create",
                "-Dpthread_join=bhasa_async_test_pthread_join",
                "-Dpthread_cond_wait=bhasa_async_test_cond_wait",
                "-c",
            ])
            .arg(fixture("runtime/concurrency.c"))
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        succeeds(&output);
        let executable = workspace.file("async-runtime");
        let output = Command::new(&clang)
            .args(flags)
            .arg("-I")
            .arg(fixture("runtime"))
            .arg(&object)
            .arg(fixture("tests/fixtures/async/runtime.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);
        for mode in [None, Some("cold-start")] {
            let mut command = Command::new(&executable);
            if let Some(mode) = mode {
                command.arg(mode);
            }
            let output = bounded(command.env("UBSAN_OPTIONS", "halt_on_error=1"));
            succeeds(&output);
            assert_eq!(stdout(&output), "bounded async runtime passed\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
        for (mode, diagnostic) in [
            (
                "nested-pool",
                "async_spawn is forbidden inside task workers",
            ),
            (
                "nested-thread",
                "async_spawn is forbidden inside task workers",
            ),
            ("await-worker", "await is forbidden inside task workers"),
            ("join-pool-worker", "await is forbidden inside task workers"),
            ("null-worker", "cannot spawn a null worker"),
            ("null-handle", "cannot join a null task"),
        ] {
            let output = bounded(Command::new(&executable).arg(mode));
            assert!(!output.status.success(), "{mode}: {}", describe(&output));
            assert!(
                stderr(&output).contains(diagnostic),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                !stderr(&output).contains("AddressSanitizer"),
                "{mode}: {}",
                describe(&output)
            );
            assert!(
                !stderr(&output).contains("runtime error:"),
                "{mode}: {}",
                describe(&output)
            );
        }
    }
}
