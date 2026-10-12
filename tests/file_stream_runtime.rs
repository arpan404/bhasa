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
                "streaming file runtime probe timed out: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn streaming_file_shared_cursor_close_and_failure_paths_release_resources() {
    let Some(clang) = clang() else {
        return;
    };
    let workspace = Workspace::new("streaming-file-runtime");
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
                    optimization,
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                ])
                .arg("-I")
                .arg(fixture("runtime"))
                .arg(fixture("tests/file_stream_runtime.c"))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(
            Command::new(&executable)
                .env("ASAN_OPTIONS", "detect_leaks=0")
                .env("UBSAN_OPTIONS", "halt_on_error=1"),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "file stream native tests passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}
