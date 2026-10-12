#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
#[path = "fixtures/channels/bounded.rs"]
mod channel_process;
use channel_process::{bounded, configured_clang};
use std::process::Command;

#[test]
fn bounded_channels_preserve_frames_close_backpressure_and_worker_guards() {
    let Some(clang) = configured_clang() else {
        return;
    };
    let workspace = Workspace::new("channel-runtime");
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
        let mut objects = Vec::new();
        for name in ["channel", "concurrency"] {
            let object = workspace.file(&format!("{name}.o"));
            succeeds(&bounded(
                Command::new(&clang)
                    .args(flags)
                    .args([
                        "-Dmalloc=bhasa_channel_test_malloc",
                        "-Dfree=bhasa_channel_test_free",
                        "-Dpthread_cond_wait=bhasa_channel_test_wait",
                        "-c",
                    ])
                    .arg(fixture(&format!("runtime/{name}.c")))
                    .arg("-o")
                    .arg(&object),
            ));
            objects.push(object);
        }
        let executable = workspace.file("channels");
        succeeds(&bounded(
            Command::new(&clang)
                .args(flags)
                .arg("-I")
                .arg(fixture("runtime"))
                .args(&objects)
                .arg(fixture("tests/fixtures/channels/runtime.c"))
                .arg("-o")
                .arg(&executable),
        ));
        let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        assert_eq!(stdout(&output), "channel runtime passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
        for (mode, diagnostic) in [
            ("unchecked", "unchecked payload type"),
            ("zero", "invalid capacity"),
            ("negative", "invalid capacity"),
            ("overflow", "invalid capacity"),
            ("null-output", "null receiver output"),
            ("null-endpoint", "null endpoint"),
            ("wrong-type", "payload type mismatch"),
            ("wrong-direction", "endpoint direction mismatch"),
            ("null-frame", "null payload frame"),
            ("null-recv", "null payload output"),
            ("worker-send", "blocking channel send"),
            ("pool-worker-send", "blocking channel send"),
            ("worker", "blocking channel receive"),
            ("pool-worker", "blocking channel receive"),
        ] {
            let output = bounded(
                Command::new(&executable)
                    .arg(mode)
                    .env("UBSAN_OPTIONS", "halt_on_error=1"),
            );
            assert!(!output.status.success(), "{mode}: {}", describe(&output));
            assert_ne!(output.status.code(), Some(99), "{mode}");
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
