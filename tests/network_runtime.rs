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
            let output = child.wait_with_output().unwrap();
            panic!(
                "network runtime process exceeded 40 seconds: {}",
                describe(&output)
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn compiled_tcp_exchange_and_resource_copies() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("network-language");
    for (source, expected) in [
        ("examples/network/loopback.bh", "TCP loopback OK\n"),
        (
            "tests/fixtures/network/resources.bh",
            "TCP resource copies OK\n",
        ),
        (
            "tests/fixtures/network/listen_on.bh",
            "TCP explicit bind OK\n",
        ),
    ] {
        for release in [false, true] {
            let executable = build(&workspace, &fixture(source), release);
            let output = bounded_output(Command::new(&executable).current_dir(workspace.path()));
            succeeds(&output);
            assert_eq!(stdout(&output), expected);
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn native_tcp_loopback_errors_and_ownership() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("network-runtime");
    for optimize in ["-O0", "-O2"] {
        let flags = [
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
            "-pthread",
            optimize,
        ];
        let mut objects = Vec::new();
        for name in ["core", "net"] {
            let object = workspace.file(&format!("{name}.o"));
            let mut command = Command::new(&clang);
            command.args(flags).args([
                "-Dmalloc=bhasa_network_test_malloc",
                "-Drealloc=bhasa_network_test_realloc",
                "-Dfree=bhasa_network_test_free",
            ]);
            if name == "net" {
                for call in [
                    "socket",
                    "bind",
                    "listen",
                    "getsockname",
                    "connect",
                    "accept",
                    "accept4",
                    "close",
                    "fcntl",
                    "setsockopt",
                    "getsockopt",
                    "poll",
                    "recv",
                    "send",
                    "getaddrinfo",
                ] {
                    command.arg(format!("-D{call}=bhasa_network_test_{call}"));
                }
            }
            let output = command
                .arg("-c")
                .arg(fixture(&format!("runtime/{name}.c")))
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            succeeds(&output);
            objects.push(object);
        }
        let executable = workspace.file("network runtime checks");
        let output = Command::new(&clang)
            .args(flags)
            .arg("-I")
            .arg(fixture("runtime"))
            .args(&objects)
            .arg(fixture("tests/fixtures/network/runtime.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);

        for mode in [
            "loopback",
            "stream",
            "errors",
            "injected",
            "sigpipe",
            "listen-on",
            "listen-on-ipv6",
        ] {
            let output = bounded_output(
                Command::new(&executable)
                    .arg(mode)
                    .current_dir(workspace.path())
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert!(
                output.status.success(),
                "{optimize} {mode}: {}",
                describe(&output)
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
            if !output.stdout.is_empty() {
                let message = stdout(&output);
                assert!(
                    mode == "listen-on-ipv6"
                        && message.starts_with("SKIP native IPv6 loopback: errno=")
                        && message.lines().count() == 1,
                    "unexpected native output: {}",
                    describe(&output)
                );
                eprint!("{optimize}: {message}");
            }
        }
        for mode in [
            "allocation-listen",
            "allocation-connect",
            "allocation-clone",
            "allocation-accept",
            "allocation-socket-clone",
            "allocation-listen-on-str",
            "allocation-listen-on-buffer",
        ] {
            let output = bounded_output(
                Command::new(&executable)
                    .arg(mode)
                    .current_dir(workspace.path())
                    .env("ASAN_OPTIONS", "detect_leaks=0")
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert_eq!(
                output.status.code(),
                Some(1),
                "{mode}: {}",
                describe(&output)
            );
            assert_eq!(stderr(&output), "Bhasa runtime error: allocation failed\n");
        }
    }
}
