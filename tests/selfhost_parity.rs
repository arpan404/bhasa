#![cfg(any(target_os = "macos", target_os = "linux"))]

mod common;

use common::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn seed(workspace: &Workspace) -> PathBuf {
    let source = ["lexer.bh", "emitter.bh", "driver.bh"]
        .into_iter()
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    build(workspace, &workspace.write("compiler.bh", &source), true)
}

fn target(clang: &std::ffi::OsStr, workspace: &Workspace) -> String {
    let output = Command::new(clang)
        .args(["-S", "-emit-llvm", "-x", "c"])
        .arg(workspace.write("empty.c", ""))
        .args(["-o", "-"])
        .output()
        .unwrap();
    succeeds(&output);
    stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("target triple = \"")?.strip_suffix('"'))
        .unwrap()
        .to_owned()
}

fn emit(seed: &Path, source: &Path, target: &str) -> Output {
    let mut command = Command::new(seed);
    command
        .arg("emit-llvm")
        .arg(source)
        .args(["--target", target]);
    bounded(command)
}

fn link(
    clang: &std::ffi::OsStr,
    workspace: &Workspace,
    ir: &[u8],
    sources: &[PathBuf],
    optimization: &str,
) -> PathBuf {
    let input = workspace.file("program.ll");
    fs::write(&input, ir).unwrap();
    let output = workspace.file("linked");
    let mut command = Command::new(clang);
    command
        .args([optimization, "-fsanitize=address,undefined", "-pthread"])
        .arg(input)
        .args(sources)
        .arg("-o")
        .arg(&output);
    if cfg!(target_os = "linux") {
        command.arg("-ldl");
    }
    succeeds(&command.output().unwrap());
    output
}

// A broken network lowering must fail the test instead of waiting forever in
// accept/read. Drain both pipes while waiting so compiler output cannot block.
fn bounded(mut command: Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        err.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break (child.wait().unwrap(), true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: out_reader.join().unwrap(),
        stderr: err_reader.join().unwrap(),
    };
    assert!(
        !timed_out,
        "subprocess exceeded 30 seconds: {}",
        describe(&output)
    );
    output
}

#[test]
fn bhasa_frontend_matches_network_types_and_status_rules() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("frontend-network-checks");
    let compiler = seed(&workspace);
    let mut cases: Vec<(&str, bool)> = vec![
        ("def main() { var listener: tcp_listener = tcp_listen(0); var socket: tcp_socket = tcp_accept(listener); var data = buffer(); var read = tcp_read(socket, data, 8); var wrote = tcp_write(socket, data); print(tcp_listener_status(listener)); print(tcp_socket_status(socket)); print(tcp_local_port(listener)) }", true),
        ("def connect(host: buffer) -> tcp_socket { return tcp_connect(host, 1) }\ndef main() { var socket = connect(buffer(\"127.0.0.1\")); var second = tcp_connect(\"127.0.0.1\", 2); socket = second; socket = socket }", true),
        ("def copy(listener: tcp_listener) -> tcp_listener { return listener }", true),
        ("def copy(socket: tcp_socket) -> tcp_socket { return socket }", true),
        ("def take(socket: tcp_socket) { tcp_read(socket, buffer(), 8) }", false),
        ("def take(socket: tcp_socket) { tcp_write(socket, buffer()) }", false),
        ("def take(socket: tcp_socket) { socket = tcp_connect(\"127.0.0.1\", 1) }", false),
        ("def take(listener: tcp_listener) { listener = tcp_listen(0) }", false),
        ("def main() { var socket: tcp_socket = tcp_listen(0) }", false),
        ("def main() { var listener: tcp_listener = tcp_connect(\"127.0.0.1\", 1) }", false),
        ("def take(socket: tcp_socket) { print(socket) }", false),
        ("def take(socket: tcp_socket) { print(socket == socket) }", false),
        ("extern def native(socket: tcp_socket)", false),
        ("extern def native() -> tcp_listener", false),
        ("def tcp_listen(port: int) -> int { return port }", false),
        ("def tcp_socket_status(socket: tcp_socket) -> int { return 0 }", false),
    ];
    for body in [
        "var x = tcp_listen()",
        "var x = tcp_listen(true)",
        "var x = tcp_listen(0, 1)",
        "var x = tcp_connect()",
        "var x = tcp_connect(1, 80)",
        "var x = tcp_connect(\"localhost\", true)",
        "var x = tcp_connect(\"localhost\", 80, 0)",
        "var x = tcp_accept(buffer())",
        "var x = tcp_listener_status(tcp_connect(\"\", 0))",
        "var x = tcp_socket_status(tcp_listen(0))",
        "var x = tcp_local_port(tcp_connect(\"\", 0))",
        "var x = tcp_read(tcp_listen(0), buffer(), 1)",
        "var x = tcp_read(tcp_connect(\"\", 0), ints(), 1)",
        "var x = tcp_read(tcp_connect(\"\", 0), buffer(), false)",
        "var x = tcp_write(tcp_connect(\"\", 0), \"bytes\")",
    ] {
        let source = format!("def main() {{ {body} }}");
        assert!(
            bhasa::compile_to_llvm(&source, "x86_64-unknown-linux-gnu").is_err(),
            "{source}"
        );
        let output = emit(
            &compiler,
            &workspace.write("invalid.bh", &source),
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            output.status.code(),
            Some(1),
            "{source}\n{}",
            describe(&output)
        );
        assert!(output.stdout.is_empty());
        assert!(
            stderr(&output).contains("error BH"),
            "{}",
            describe(&output)
        );
    }
    for (source, valid) in cases.drain(..) {
        let rust = bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu");
        assert_eq!(rust.is_ok(), valid, "Rust baseline: {source}\n{rust:?}");
        let output = emit(
            &compiler,
            &workspace.write("input.bh", source),
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            output.status.success(),
            valid,
            "Bhasa: {source}\n{}",
            describe(&output)
        );
        if !valid {
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert!(
                stderr(&output).contains("error BH"),
                "{}",
                describe(&output)
            );
        }
    }
}

const NETWORK_OWNERSHIP: &str = r#"
def listener() -> tcp_listener {
    var original = tcp_listen(0)
    var copied = original
    return copied
}
def socket_copy(socket: tcp_socket) -> tcp_socket { return socket }
def listener_copy(listener: tcp_listener) -> tcp_listener { return listener }
def receive(socket: tcp_socket, bytes: buffer, count: int) -> int {
    var chunk = buffer()
    while len(bytes) < count {
        var status = tcp_read(socket, chunk, count - len(bytes))
        if status != 0 { return status }
        if len(chunk) == 0 { return 90 }
        append(bytes, chunk)
    }
    return 0
}
def main() -> int {
    var original = listener()
    if tcp_listener_status(original) != 0 { return 10 }
    var copied = listener_copy(original)
    var port = tcp_local_port(copied)
    original = tcp_listen(-1)
    if tcp_listener_status(original) == 0 { return 11 }
    copied = copied
    if tcp_local_port(copied) != port { return 12 }
    var client = tcp_connect(buffer("127.0.0.1"), port)
    if tcp_socket_status(client) != 0 { return 13 }
    var server = tcp_accept(copied)
    if tcp_socket_status(server) != 0 { return 14 }
    var retained = socket_copy(client)
    client = tcp_connect("", -1)
    if tcp_socket_status(client) == 0 { return 15 }
    retained = retained
    var request = buffer("ping")
    push(request, 0); push(request, 255)
    var status = tcp_write(retained, request)
    if status != 0 { return status }
    var bytes = buffer()
    status = receive(server, bytes, 6)
    if status != 0 { return status }
    if not same(bytes, request) { return 16 }
    status = tcp_write(server, bytes)
    if status != 0 { return status }
    var response = buffer()
    status = receive(retained, response, 6)
    if status != 0 { return status }
    if not same(response, request) { return 17 }
    status = tcp_read(retained, response, 0)
    if status != 0 or len(response) != 0 { return 18 }
    var count = 0
    while count < 32 {
        var failed = tcp_connect("", 65536)
        var another = failed
        another = another
        if tcp_socket_status(another) == 0 { return 19 }
        count = count + 1
    }
    tcp_listen(-1)
    tcp_connect("", -1)
    print("network ownership ok")
    return 0
}
"#;

#[test]
fn bhasa_frontend_network_clones_and_loopback_match_rust_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("frontend-network-native");
    let compiler = seed(&workspace);
    let triple = target(&clang, &workspace);
    let loopback = fs::read_to_string(fixture("examples/network/loopback.bh")).unwrap();
    for (source, expected) in [
        (NETWORK_OWNERSHIP, "network ownership ok\n"),
        (loopback.as_str(), "TCP loopback OK\n"),
    ] {
        let rust = bhasa::compile_to_llvm(source, &triple).unwrap();
        let bhasa = emit(&compiler, &workspace.write("network.bh", source), &triple);
        succeeds(&bhasa);
        for ir in [rust.as_bytes(), bhasa.stdout.as_slice()] {
            let text = std::str::from_utf8(ir).unwrap();
            assert!(text.contains("; bhasa runtime: core\n"));
            assert!(text.contains("; bhasa runtime: net\n"));
            for optimization in ["-O0", "-O2"] {
                let executable = link(
                    &clang,
                    &workspace,
                    ir,
                    &[
                        fixture("runtime/core.c"),
                        fixture("runtime/net.c"),
                        fixture("runtime/concurrency.c"),
                    ],
                    optimization,
                );
                let mut command = Command::new(executable);
                command.current_dir(workspace.path());
                let output = bounded(command);
                succeeds(&output);
                assert_eq!(stdout(&output), expected);
                assert!(output.stderr.is_empty(), "{}", describe(&output));
            }
        }
    }
}

#[test]
fn bhasa_frontend_checks_current_extern_task_and_gpu_rules() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("frontend-acceptance");
    let compiler = seed(&workspace);
    let worker = "def worker(n: int) -> int { return n + 1 }\n";
    let invalid_bodies = [
        "var job = spawn(worker(1))",
        "var job = spawn(worker(1)); return",
        "var job = spawn(worker(1)); if true { return }; join(job)",
        "var job = spawn(worker(1)); var copy = job; join(job)",
        "var job = spawn(worker(1)); job = 2; join(job)",
        "var job = spawn(worker(1)); join(job); join(job)",
        "var job = spawn(worker(1)); if true { join(job) }",
        "var job = spawn(worker(1)); while false { join(job) }",
        "var job = spawn(worker(1)); if true { var job = 1 }; join(job)",
        "var job = spawn(worker(1)); join(job); if true { var job = 1 }",
        "var job = spawn(worker(1)); print(job); join(job)",
        "var job = spawn(worker(1)); print(join(job))",
        "var job = spawn(worker(1)); var x = join(job) + 1",
        "var job = spawn(worker(1)); var x = false and join(job) == 0",
        "var job = spawn(worker(1)); var x = worker(job); join(job)",
        "var job = spawn(worker(1)); var x = join((job + 1))",
        "spawn(worker(1))",
        "var job = spawn(1)",
        "var job = spawn(worker())",
        "var job = spawn(worker(1, 2))",
        "var job = spawn(worker(1), 2)",
        "var job = spawn(worker(true))",
        "var value = join(1)",
        "var value = join(missing)",
        "var job = spawn(worker(1)); var value = join(job, job)",
        "var job = spawn()",
        "var job = spawn((worker(1))",
        "var job = spawn(worker(1)); join()",
        "var job = spawn(worker(1)); join((job)",
        "var data = buffer(); gpu_dispatch(\"source\", \"entry\", data)",
        "var result = gpu_dispatch(1, \"entry\", buffer())",
        "var result = gpu_dispatch(\"source\", 1, buffer())",
        "var result = gpu_dispatch(\"source\", \"entry\", 1)",
        "var result = gpu_available(1)",
    ];
    let mut cases: Vec<(String, bool)> = invalid_bodies
        .into_iter()
        .map(|body| (format!("{worker}def main() {{ {body} }}"), false))
        .collect();
    cases.push((
        format!(
            "{worker}def main() {{ var job = spawn({}worker(1){}); join(job) }}",
            "(".repeat(300),
            ")".repeat(300)
        ),
        false,
    ));
    cases.extend([
        ("extern def foreign(n: bool) -> int".into(), false),
        ("extern def foreign(n: int) -> bool".into(), false),
        ("extern def main() -> int".into(), false),
        ("extern def foreign() { return }".into(), false),
        ("extern def foreign() -> int\ndef foreign() -> int { return 1 }".into(), false),
        ("extern def bhasa_rt_join_i64(n: int) -> int".into(), false),
        ("extern def foreign(n: int) -> int\ndef main() { var job = spawn(foreign(1)); join(job) }".into(), false),
        ("extern def foreign(n: int) -> int\ndef worker(n: int) -> int { return other(n) }\ndef other(n: int) -> int { if n == 0 { return foreign(n) }; return worker(n - 1) }\ndef main() { var job = spawn(worker(1)); join(job) }".into(), false),
        ("def worker(n: bool) -> int { return 1 }\ndef main() { var job = spawn(worker(true)); join(job) }".into(), false),
        ("def worker(n: int) -> bool { return true }\ndef main() { var job = spawn(worker(1)); join(job) }".into(), false),
        ("def worker() -> int { return 1 }\ndef main() { var job = spawn(worker()); join(job) }".into(), false),
        ("extern def foreign(n: int) -> int\ndef worker(n: int) -> int { return n }\ndef main() { var job = spawn(worker(foreign(1))); join(job) }".into(), true),
        (format!("{worker}def main() {{ var job = ((spawn((worker(1,)),))); var result = ((join((job),))); print(result) }}"), true),
        (format!("{worker}def main() {{ while false {{ var job = spawn(worker(1)); join(job) }} }}"), true),
        ("extern def foreign(n: int) -> int\nextern def notify(n: int)\ndef main() { notify(foreign(1)) }".into(), true),
        ("extern def printf(n: int) -> int".into(), true),
        ("def main() { var available = gpu_available(); var data = buffer(); var result = gpu_dispatch(\"source\", buffer(\"entry\"), data); print(result) }".into(), true),
    ]);
    for (index, (source, valid)) in cases.into_iter().enumerate() {
        let rust = bhasa::compile_to_llvm(&source, "x86_64-unknown-linux-gnu");
        assert_eq!(
            rust.is_ok(),
            valid,
            "Rust baseline case {index}: {source}\n{rust:?}"
        );
        let output = emit(
            &compiler,
            &workspace.write("input.bh", &source),
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            output.status.success(),
            valid,
            "Bhasa case {index}: {source}\n{}",
            describe(&output)
        );
        if !valid {
            assert!(output.stdout.is_empty());
            let error = stderr(&output);
            let span = error.trim().split(" at byte ").nth(1).unwrap();
            let (start, end) = span.split_once("..").unwrap();
            let start: usize = start.parse().unwrap();
            let end: usize = end.parse().unwrap();
            assert!(start <= end && end <= source.len(), "{error}");
        }
    }
}

#[test]
fn bhasa_frontend_links_foreign_adapters_and_runs_nested_tasks() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("frontend-native");
    let compiler = seed(&workspace);
    let triple = target(&clang, &workspace);
    let source = workspace.write(
        "native.bh",
        r#"
extern def foreign(n: int) -> int
extern def notify(n: int)
def worker(n: int) -> int {
    var bytes = to_buffer(n)
    var values = ints(); push(values, n)
    return get(values, 0) * get(values, 0) + len(bytes)
}
def nested(n: int) -> int {
    var child = spawn(worker(n))
    var value = join(child)
    return value
}
def main() {
    var first = spawn(nested(foreign(5)))
    var second = spawn(worker(8))
    var b = join(second)
    var a = join(first)
    print(a + b)
    var i = 0
    while i < 3 {
        var job = spawn(worker(i))
        var value = join(job)
        print(value)
        i = i + 1
    }
    notify(-42)
}
"#,
    );
    let adapter = workspace.write("adapter.c", "#include <stdint.h>\n#include <stdio.h>\nint64_t foreign(int64_t n) { return n + 1; }\nvoid notify(int64_t n) { printf(\"foreign:%lld\\n\", (long long)n); }\n");
    let output = emit(&compiler, &source, &triple);
    succeeds(&output);
    let ir = stdout(&output);
    assert!(ir.contains("; bhasa runtime: core\n"));
    assert!(ir.contains("; bhasa runtime: threads\n"));
    assert!(!ir.contains("; bhasa runtime: gpu\n"));
    assert!(ir.contains("declare i64 @foreign(i64)"));
    for optimization in ["-O0", "-O2"] {
        let executable = link(
            &clang,
            &workspace,
            &output.stdout,
            &[
                fixture("runtime/core.c"),
                fixture("runtime/concurrency.c"),
                adapter.clone(),
            ],
            optimization,
        );
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), "102\n1\n2\n5\nforeign:-42\n");
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}

#[test]
fn bhasa_frontend_gpu_calls_link_and_preserve_data_without_a_driver() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("frontend-gpu");
    let compiler = seed(&workspace);
    let triple = target(&clang, &workspace);
    let source = workspace.write(
        "gpu.bh",
        r#"
def main() {
    var data = buffer("unchanged")
    print(gpu_available())
    var first = gpu_dispatch("source", "entry", data)
    print(first)
    var source = buffer("source")
    var entry = buffer("entry")
    var second = gpu_dispatch(source, entry, data)
    print(second)
    print(data)
}
"#,
    );
    let output = emit(&compiler, &source, &triple);
    succeeds(&output);
    assert!(stdout(&output).contains("; bhasa runtime: gpu\n"));
    for optimization in ["-O0", "-O2"] {
        let executable = link(
            &clang,
            &workspace,
            &output.stdout,
            &[fixture("runtime/core.c"), fixture("runtime/cuda.c")],
            optimization,
        );
        let absent = workspace.file("no-driver-library");
        let result = Command::new(executable)
            .env("BHASA_CUDA_DRIVER", &absent)
            .env("BHASA_NVRTC", &absent)
            .output()
            .unwrap();
        succeeds(&result);
        assert_eq!(stdout(&result), "false\n1\n1\nunchanged\n");
        assert_eq!(
            stderr(&result),
            "bhasa GPU: CUDA driver or NVRTC library is unavailable\n".repeat(2)
        );
    }
}
