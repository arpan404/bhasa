#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    assert!(
        !timed_out,
        "typed task fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

#[test]
fn mixed_owned_tasks_preserve_pool_bounds_frames_and_worker_guards() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-task-runtime");
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
        succeeds(&bounded(
            Command::new(&clang)
                .args(flags)
                .args([
                    "-Dmalloc=bhasa_typed_test_malloc",
                    "-Dfree=bhasa_typed_test_free",
                    "-Dpthread_create=bhasa_typed_test_create",
                    "-Dpthread_join=bhasa_typed_test_join",
                    "-Dpthread_cond_wait=bhasa_typed_test_wait",
                    "-c",
                ])
                .arg(fixture("runtime/concurrency.c"))
                .arg("-o")
                .arg(&object),
        ));
        let executable = workspace.file("typed-tasks");
        succeeds(&bounded(
            Command::new(&clang)
                .args(flags)
                .arg("-I")
                .arg(fixture("runtime"))
                .arg(&object)
                .arg(fixture("tests/fixtures/typed_tasks/runtime.c"))
                .arg("-o")
                .arg(&executable),
        ));
        for mode in [None, Some("cold-start")] {
            let mut command = Command::new(&executable);
            if let Some(mode) = mode {
                command.arg(mode);
            }
            let output = bounded(command.env("UBSAN_OPTIONS", "halt_on_error=1"));
            succeeds(&output);
            assert_eq!(stdout(&output), "typed task runtime passed\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
        for (mode, diagnostic) in [
            ("null-worker", "cannot spawn a null worker"),
            ("null-pool-worker", "cannot spawn a null worker"),
            ("null-handle", "cannot join a null task"),
            ("negative-size", "invalid task frame size"),
            ("allocation", "cannot allocate a task frame"),
            ("task-allocation", "cannot allocate a task"),
            ("scalar-as-owned", "task result ABI mismatch"),
            ("owned-as-scalar", "task result ABI mismatch"),
            (
                "nested-owned-pool",
                "async_spawn is forbidden inside task workers",
            ),
            (
                "nested-owned-thread",
                "async_spawn is forbidden inside task workers",
            ),
            (
                "nested-scalar-submit",
                "async_spawn is forbidden inside task workers",
            ),
            (
                "scalar-owned-submit",
                "async_spawn is forbidden inside task workers",
            ),
            (
                "owned-await-worker",
                "await is forbidden inside task workers",
            ),
            (
                "owned-join-pool-worker",
                "await is forbidden inside task workers",
            ),
        ] {
            let output = bounded(
                Command::new(&executable)
                    .arg(mode)
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert!(!output.status.success(), "{mode}: {}", describe(&output));
            assert_ne!(
                output.status.code(),
                Some(99),
                "fixture fell through: {mode}"
            );
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
