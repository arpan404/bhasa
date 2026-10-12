#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn bounded_output(command: &mut Command) -> Output {
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
            let _ = child.kill();
            panic!(
                "deadline fixture exceeded 40 seconds: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn native_deadlines_preserve_buffers_and_shared_descriptor_flags() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("network-deadlines");
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
        let mut objects = Vec::new();
        for runtime in ["core", "net"] {
            let object = workspace.file(&format!("{runtime}.o"));
            let mut command = Command::new(&clang);
            command.args(flags).args([
                "-Dmalloc=bhasa_deadline_test_malloc",
                "-Drealloc=bhasa_deadline_test_realloc",
                "-Dfree=bhasa_deadline_test_free",
            ]);
            if runtime == "net" {
                command.args([
                    "-Dpoll=bhasa_deadline_test_poll",
                    "-Dsend=bhasa_deadline_test_send",
                    "-Dsocket=bhasa_deadline_test_socket",
                    "-Dfcntl=bhasa_deadline_test_fcntl",
                    "-Dsetsockopt=bhasa_deadline_test_setsockopt",
                ]);
            }
            let output = command
                .arg("-c")
                .arg(fixture(&format!("runtime/{runtime}.c")))
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            succeeds(&output);
            objects.push(object);
        }
        let executable = workspace.file("deadline-checks");
        let output = Command::new(&clang)
            .args(flags)
            .arg("-I")
            .arg(fixture("runtime"))
            .args(objects)
            .arg(fixture("tests/fixtures/deadlines/runtime.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);
        let output =
            bounded_output(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        let text = stdout(&output);
        assert!(text.ends_with("TCP deadline and clone-race checks passed\n"));
        let prefix = text
            .strip_suffix("TCP deadline and clone-race checks passed\n")
            .unwrap();
        if !prefix.is_empty() {
            assert!(
                prefix.starts_with("SKIP IPv6 deadline loopback: errno=")
                    && prefix.lines().count() == 1,
                "{text}"
            );
            eprint!("{prefix}");
        }
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

#[test]
fn compiled_deadline_builtins_handle_overloads_and_owned_statuses() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("network-deadline-language");
    let mut frontends = vec!["rust"];
    if cfg!(feature = "bhasa-frontend") {
        frontends.push("bhasa");
    }
    for frontend in frontends {
        for release in [false, true] {
            let mut command = workspace.command();
            command
                .arg("run")
                .arg(fixture("tests/fixtures/deadlines/library.bh"))
                .args(["--frontend", frontend]);
            if release {
                command.arg("--release");
            }
            let output = bounded_output(&mut command);
            succeeds(&output);
            assert_eq!(stdout(&output), "Bhasa TCP deadlines passed\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}
