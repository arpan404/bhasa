#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn native_tls_verifies_peers_bounds_operations_and_releases_resources() {
    let Some(clang) = clang() else { return };
    let available = Command::new("pkg-config")
        .args(["--exists", "openssl >= 3"])
        .status();
    if !available.is_ok_and(|status| status.success()) {
        eprintln!("SKIP TLS runtime: pkg-config could not find OpenSSL 3");
        return;
    }
    let flags = Command::new("pkg-config")
        .args(["--cflags", "--libs", "openssl"])
        .output()
        .unwrap();
    succeeds(&flags);
    let prefix = Command::new("pkg-config")
        .args(["--variable=prefix", "openssl"])
        .output()
        .unwrap();
    succeeds(&prefix);
    let openssl = std::path::Path::new(stdout(&prefix).trim()).join("bin/openssl");
    let workspace = Workspace::new("tls-runtime");
    let certificates = workspace.file("certificates");
    succeeds(
        &Command::new("sh")
            .arg(fixture("tests/fixtures/tls/generate.sh"))
            .arg(&certificates)
            .arg(&openssl)
            .output()
            .unwrap(),
    );
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("tls-checks");
        let options = [
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
        for runtime in ["core", "net", "tls"] {
            let object = workspace.file(&format!("{runtime}.o"));
            let mut command = Command::new(&clang);
            command.args(options).args([
                "-Dmalloc=bhasa_tls_test_malloc",
                "-Drealloc=bhasa_tls_test_realloc",
                "-Dfree=bhasa_tls_test_free",
            ]);
            if runtime == "tls" {
                command.args([
                    "-DSSL_read_ex=bhasa_tls_test_SSL_read_ex",
                    "-Dread=bhasa_tls_test_read",
                ]);
            }
            command
                .args(
                    stdout(&flags)
                        .split_whitespace()
                        .filter(|flag| flag.starts_with("-I")),
                )
                .arg("-c")
                .arg(fixture(&format!("runtime/{runtime}.c")))
                .arg("-o")
                .arg(&object);
            succeeds(&command.output().unwrap());
            objects.push(object);
        }
        let mut command = Command::new(&clang);
        command.args(options).arg("-I");
        command
            .arg(fixture("runtime"))
            .args(objects)
            .arg(fixture("tests/fixtures/tls/runtime.c"))
            .args(stdout(&flags).split_whitespace())
            .arg("-o")
            .arg(&executable);
        succeeds(&command.output().unwrap());
        let mut child = Command::new(&executable)
            .arg(&certificates)
            .env("UBSAN_OPTIONS", "halt_on_error=1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if child.try_wait().unwrap().is_some() {
                let output = child.wait_with_output().unwrap();
                succeeds(&output);
                assert_eq!(
                    stdout(&output),
                    "TLS verification and ownership checks passed\n"
                );
                assert!(output.stderr.is_empty(), "{}", describe(&output));
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!(
                    "TLS fixture exceeded 45 seconds: {}",
                    describe(&child.wait_with_output().unwrap())
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
