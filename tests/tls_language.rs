#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::Command;

#[test]
fn compiled_tls_and_generic_https_transport_work_in_both_frontends() {
    if clang().is_none() {
        return;
    }
    let output = Command::new("pkg-config")
        .args(["--atleast-version=3.0", "openssl"])
        .output();
    if !output.is_ok_and(|out| out.status.success()) {
        eprintln!("SKIP compiled TLS: OpenSSL 3 development libraries unavailable");
        return;
    }
    let workspace = Workspace::new("tls-language");
    let prefix = Command::new("pkg-config")
        .args(["--variable=prefix", "openssl"])
        .output()
        .unwrap();
    succeeds(&prefix);
    let openssl = std::path::Path::new(stdout(&prefix).trim()).join("bin/openssl");
    let certificates = workspace.file("certificates");
    succeeds(
        &Command::new("sh")
            .arg(fixture("tests/fixtures/tls/generate.sh"))
            .arg(&certificates)
            .arg(openssl)
            .output()
            .unwrap(),
    );
    let client_source = workspace.write("client.bh", r#"
struct Box[T] { value: T }
enum Outcome[T] { ok(value: T); error(code: int) }
def keep[T](value: T) -> T { return value }
def decimal(bytes: buffer) -> int {
    var value = 0; var index = 0
    while index < len(bytes) { value = value * 10 + get(bytes, index) - 48; index = index + 1 }
    return value
}
def main() -> int {
    var socket = tls_connect("127.0.0.1", decimal(arg(1)), arg(2), arg(3), 2000)
    print(tls_status(socket))
    if tls_status(socket) != 0 { return 0 }
    var outcome = Outcome[tls_socket].ok(socket)
    match outcome {
        case Outcome[tls_socket].ok(value) { if tls_status(value) != 0 { return 1 } }
        case Outcome[tls_socket].error(code) { return 1 }
    }
    var owner = keep[Box[tls_socket]](Box[tls_socket](socket))
    var written = stream_write(owner.value, buffer("GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n"), 2000)
    if written != 0 { return 1 }
    var response = buffer(); var chunk = buffer(); var finished = false
    while !finished {
        var error = stream_read(owner.value, chunk, 4096, 2000)
        if error != 0 { return 1 }
        if len(chunk) == 0 { finished = true } else { append(response, chunk) }
    }
    # The example's fixed health body is the final 11 bytes.
    if len(response) < 11 { return 1 }
    print(slice(response, len(response) - 11, 11))
    var closed = tls_shutdown(owner.value, 2000)
    return 0
}
"#);
    let frontends = if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    };
    for frontend in frontends {
        for release in [false, true] {
            let server = workspace.file("server");
            let client = workspace.file("client");
            for (source, executable) in [
                (fixture("examples/https/main.bh"), &server),
                (client_source.clone(), &client),
            ] {
                let mut command = workspace.command();
                command
                    .arg("build")
                    .arg(source)
                    .args(["--frontend", frontend])
                    .arg("-o")
                    .arg(executable);
                if release {
                    command.arg("--release");
                }
                succeeds(&command.output().unwrap());
            }
            let result = Command::new("python3")
                .arg(fixture("tests/fixtures/tls/language.py"))
                .arg(server)
                .arg(client)
                .arg(&certificates)
                .output()
                .unwrap();
            succeeds(&result);
            assert_eq!(stdout(&result), "compiled TLS and HTTPS checks passed\n");
            assert!(result.stderr.is_empty(), "{}", describe(&result));
        }
    }
}
