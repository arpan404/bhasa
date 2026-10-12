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
    let deadline = Instant::now() + Duration::from_secs(180);
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
        "file stream gate deadline: {}",
        describe(&output)
    );
    output
}
#[test]
fn typed_streaming_files_preserve_shared_cursor_try_cleanup_and_exact_errors() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("file-stream-gate");
    let mut command = Command::new("python3");
    command
        .arg(fixture("tests/verify_file_streams.py"))
        .arg("--repo")
        .arg(fixture("."))
        .arg("--output")
        .arg(workspace.path())
        .arg("--compiler")
        .arg(env!("CARGO_BIN_EXE_bhasa"))
        .arg("--frontends")
        .arg("rust");
    if cfg!(feature = "bhasa-frontend") {
        command.arg("bhasa");
    }
    let output = bounded(&mut command);
    succeeds(&output);
    assert_eq!(
        stdout(&output),
        "PASS: file stream runtime and selected production frontend gates\n"
    );
    assert!(output.stderr.is_empty(), "{}", describe(&output));
}
