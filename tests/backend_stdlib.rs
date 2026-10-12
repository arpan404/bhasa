#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn package(workspace: &Workspace, source: &str) {
    fs::create_dir(workspace.file("std")).unwrap();
    for entry in fs::read_dir(fixture("std")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), workspace.file("std").join(entry.file_name())).unwrap();
        }
    }
    workspace.write("bhasa.toml", "[package]\nname = \"backend-test\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[dependencies]\nstd = \"std\"\n");
    workspace.write("main.bh", source);
}

fn modes() -> Vec<&'static str> {
    let result = vec!["rust"];
    #[cfg(feature = "bhasa-frontend")]
    let result = [result, vec!["bhasa"]].concat();
    result
}

fn compile(workspace: &Workspace, mode: &str, release: bool) -> std::path::PathBuf {
    let executable = workspace.file(&format!("backend-{mode}-{release}"));
    let mut command = workspace.command();
    command
        .args(["build", "main.bh", "--frontend", mode, "-o"])
        .arg(&executable);
    if release {
        command.arg("--release");
    }
    succeeds(&command.output().unwrap());
    executable
}

fn run(executable: &Path) -> String {
    let output = Command::new(executable).output().unwrap();
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    stdout(&output)
}

#[test]
fn json_values_unicode_integer_bounds_and_malformed_inputs() {
    if clang().is_none() {
        return;
    }
    let mut source = String::from(
        r#"import "@std"
def accepted(bytes: buffer) -> bool {
    match json_parse(bytes) {
        case JsonResult.ok(document) { return true }
        case JsonResult.error(message) { return false }
    }
}
def main() {
"#,
    );
    let valid = [
        "null",
        " true \r\n",
        "false",
        "-0",
        "123",
        "1.2e-300",
        "[null,true,false,{},[]]",
        "{\"a\":[1,2],\"b\":\"hé😀\"}",
        "\"\\uD83D\\uDE00\\u0000\"",
        "9223372036854775808",
    ];
    let invalid = [
        "",
        " ",
        "01",
        "-",
        "1.",
        "1e+",
        "+1",
        "NaN",
        "Infinity",
        "[1,]",
        "{\"a\":1,}",
        "[",
        "{\"a\" 1}",
        "truefalse",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD800\\u0041\"",
        "\"\\x00\"",
        "\"raw\nline\"",
        "{a:1}",
    ];
    for value in valid.into_iter().chain(invalid) {
        source.push_str(&format!("print(accepted(buffer({value:?})))\n"));
    }
    source.push_str(r#"
    var invalid_utf8 = buffer(); push(invalid_utf8, 255)
    print(accepted(invalid_utf8))
    var deep = buffer()
    var depth = 0
    while depth < 66 { push(deep, 91); depth = depth + 1 }
    append(deep, "0")
    depth = 0
    while depth < 66 { push(deep, 93); depth = depth + 1 }
    print(accepted(deep))
    match json_parse(buffer("{\"word\":\"\\uD83D\\uDE00\\u0000hé\",\"numbers\":[-9223372036854775808,9223372036854775807,9223372036854775808,1.2]}")) {
        case JsonResult.error(message) { print("unexpected JSON error") }
        case JsonResult.ok(document) {
            match document.find(0, text("word")) {
                case OptionInt.none { print("missing word") }
                case OptionInt.some(node) {
                    match document.string(node) {
                        case ResultText.error(message) { print("invalid word") }
                        case ResultText.ok(value) {
                            print(value.count()); print(value.codepoint(0)); print(value.codepoint(1))
                            match json_quote(value) {
                                case ResultText.error(message) { print("quote failed") }
                                case ResultText.ok(quoted) { print(quoted.bytes()) }
                            }
                        }
                    }
                }
            }
            var node = 3
            while node < 7 {
                match document.integer(node) {
                    case ResultInt.error(message) { print("integer error") }
                    case ResultInt.ok(value) { print(value) }
                }
                node = node + 1
            }
        }
    }
}
"#);
    let expected = format!("{}{}false\nfalse\n4\n128512\n0\n\"😀\\u0000hé\"\n-9223372036854775808\n9223372036854775807\ninteger error\ninteger error\n", "true\n".repeat(valid.len()), "false\n".repeat(invalid.len()));
    for mode in modes() {
        let workspace = Workspace::new("json-stdlib");
        package(&workspace, &source);
        for release in [false, true] {
            assert_eq!(
                run(&compile(&workspace, mode, release)),
                expected,
                "{mode}, release={release}"
            );
        }
    }
}

#[test]
fn http_parser_rejects_ambiguous_framing_and_counts_utf8_response_bytes() {
    if clang().is_none() {
        return;
    }
    let mut source = String::from(
        r#"import "@std"
def accepted(bytes: buffer) -> bool {
    match http_parse_head(bytes) {
        case HttpResult.ok(request) { return true }
        case HttpResult.error(message) { return false }
    }
}
def main() {
"#,
    );
    let valid = [
        "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}",
        "GET / HTTP/1.0\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: LOCALHOST:0\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: api.example.com:65535\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: example.com.\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: xn--bcher-kva.example\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:8080\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 0.0.0.0\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]:443\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [2001:db8::1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:7:8]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:7::]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::ffff:192.0.2.1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:192.0.2.1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:000080\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc.ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc.ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd.:65535\r\n\r\n",
        "GET /h%C3%A9?q=a%20b HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a%23b?x=%2F%3f HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a-._~!$&'()*+,;=:@/b?x=1&y=/?? HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /? HTTP/1.1\r\nHost: localhost\r\n\r\n",
    ];
    let invalid = [
        "GET / HTTP/1.1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a\r\nHOST: b\r\n\r\n",
        "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 0\r\ncontent-length: 0\r\n\r\n",
        "POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n",
        "GET / HTTP/1.1\r\nHost : a\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a\r\n folded\r\n\r\n",
        "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: +1\r\n\r\na",
        "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 1048577\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a\r\n\r\nextra",
        "GET  / HTTP/1.1\r\nHost: a\r\n\r\n",
        "CONNECT target HTTP/1.1\r\nHost: a\r\n\r\n",
        "GET / HTTP/2.0\r\nHost: a\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a b\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a\tb\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a,b\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: user@example.com\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:abc\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:65536\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:-1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:+80\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: localhost:80x\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: :80\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: -host\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: host-\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a..b\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: a..\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: _host\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc.ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddda\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 999.0.0.1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 127.1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 127.0.0.01\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: ::1\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: []\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [127.0.0.1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]x\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]:\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]:65536\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1]:a\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [:::1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1::2::3]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:7]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:7:8:9]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [1:2:3:4:5:6:7:8::]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [12345::]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [gggg::]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::ffff:256.0.0.1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::ffff:192.0.2.1:1]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [::1%25lo0]\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: [v1.address]\r\n\r\n",
        "GET /#fragment HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a?x=#fragment HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /% HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /%0 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /%G0 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /%0G HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /hé HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /😀 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a\\b HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a|b HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a[b] HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /a<b> HTTP/1.1\r\nHost: localhost\r\n\r\n",
    ];
    for value in valid.into_iter().chain(invalid) {
        source.push_str(&format!("print(accepted(buffer({value:?})))\n"));
    }
    source.push_str(
        r#"
    match http_response(200, text("\"hé😀\"")) {
        case ResultBuffer.error(message) { print("response error") }
        case ResultBuffer.ok(bytes) { print(bytes) }
    }
}
"#,
    );
    let expected = format!("{}{}HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: 9\r\nConnection: close\r\n\r\n\"hé😀\"\n", "true\n".repeat(valid.len()), "false\n".repeat(invalid.len()));
    for mode in modes() {
        let workspace = Workspace::new("http-parse");
        package(&workspace, &source);
        assert_eq!(run(&compile(&workspace, mode, true)), expected, "{mode}");
    }
}

fn exchange(port: u16, pieces: &[&[u8]]) -> Vec<u8> {
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    for piece in pieces {
        socket.write_all(piece).unwrap();
        std::thread::sleep(Duration::from_millis(3));
    }
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    response
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn reset_client(port: u16) {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct Linger {
        enabled: i32,
        seconds: i32,
    }
    extern "C" {
        fn setsockopt(fd: i32, level: i32, option: i32, value: *const Linger, size: u32) -> i32;
    }
    #[cfg(target_os = "macos")]
    let (level, option) = (0xffff, 0x80);
    #[cfg(target_os = "linux")]
    let (level, option) = (1, 13);
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let linger = Linger {
        enabled: 1,
        seconds: 0,
    };
    // Zero linger closes with RST, forcing the server's error-response write to fail.
    assert_eq!(
        unsafe {
            setsockopt(
                socket.as_raw_fd(),
                level,
                option,
                &linger,
                std::mem::size_of::<Linger>() as u32,
            )
        },
        0
    );
    socket.write_all(b"GET /health HTTP/1.1\r\nHost:").unwrap();
    std::thread::sleep(Duration::from_millis(30));
}

fn response(bytes: &[u8], status: u16, body: &[u8]) {
    let end = bytes
        .windows(4)
        .position(|p| p == b"\r\n\r\n")
        .expect("response headers")
        + 4;
    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
    assert!(
        headers.starts_with(&format!("HTTP/1.1 {status} ")),
        "{headers}"
    );
    assert!(
        headers.contains(&format!("\r\nContent-Length: {}\r\n", body.len())),
        "{headers}"
    );
    assert!(headers.contains("\r\nConnection: close\r\n"));
    assert_eq!(&bytes[end..], body);
}

#[test]
fn native_http_service_handles_fragmented_json_state_and_total_deadline() {
    if clang().is_none() {
        return;
    }
    for mode in modes() {
        let workspace = Workspace::new("http-service");
        package(
            &workspace,
            &fs::read_to_string(fixture("examples/http/main.bh")).unwrap(),
        );
        let executable = compile(&workspace, mode, true);
        let mut child = ChildGuard(
            Command::new(executable)
                .args(["0", "10"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut output = BufReader::new(child.0.stdout.take().unwrap());
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut announcement = String::new();
            let result = output.read_line(&mut announcement).map(|_| announcement);
            let _ = send.send(result);
        });
        let announcement = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("server startup deadline")
            .unwrap();
        let port: u16 = announcement.trim().parse().unwrap();
        reset_client(port);
        response(
            &exchange(port, &[b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n"]),
            200,
            b"{\"ok\":true}",
        );
        for count in [1, 2] {
            response(
                &exchange(port, &[b"GET /counter HTTP/1.1\r\nHost: localhost\r\n\r\n"]),
                200,
                format!("{{\"count\":{count}}}").as_bytes(),
            );
        }
        let body = "{\"message\":\"hé😀\",\"values\":[true,null,1.2e3]}";
        let headers = format!(
            "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        response(
            &exchange(
                port,
                &[
                    headers.as_bytes(),
                    &body.as_bytes()[..7],
                    &body.as_bytes()[7..],
                ],
            ),
            200,
            body.as_bytes(),
        );
        response(
            &exchange(
                port,
                &[b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\n["],
            ),
            400,
            b"{\"error\":\"invalid JSON\"}",
        );
        response(&exchange(port,&[b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 1\r\n\r\n"]),400,b"{\"error\":\"invalid request\"}");
        response(
            &exchange(
                port,
                &[b"POST /echo HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n"],
            ),
            400,
            b"{\"error\":\"invalid request\"}",
        );
        response(
            &exchange(port, &[b"GET /missing HTTP/1.1\r\nHost: localhost\r\n\r\n"]),
            404,
            b"{\"error\":\"not found\"}",
        );
        let start = Instant::now();
        response(
            &exchange(port, &[b"GET /health HTTP/1.1\r\nHost:"]),
            400,
            b"{\"error\":\"invalid request\"}",
        );
        assert!(start.elapsed() < Duration::from_secs(7));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() > deadline {
                child.0.kill().unwrap();
                panic!("HTTP service did not exit");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut errors = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(errors.is_empty(), "{errors}");
    }
}

#[test]
fn json_http_and_collection_ownership_under_native_sanitizers() {
    let Some(clang) = clang() else { return };
    for mode in modes() {
        let workspace = Workspace::new("backend-sanitizers");
        package(
            &workspace,
            r#"import "@std"
def main() {
    var iteration = 0
    while iteration < 20 {
        match json_parse(buffer("{\"nested\":[null,true,{\"x\":\"\\uD83D\\uDE00\"}],\"count\":42}")) {
            case JsonResult.error(message) { print("unexpected JSON error") }
            case JsonResult.ok(document) {
                var copied = document
                match copied.find(0, text("count")) {
                    case OptionInt.none { print("missing count") }
                    case OptionInt.some(node) {
                        match copied.integer(node) {
                            case ResultInt.error(message) { print("bad count") }
                            case ResultInt.ok(number) { if number != 42 { print("wrong count") } }
                        }
                    }
                }
            }
        }
        var values = string_map()
        values.put(text("key"), text("hé😀"))
        match values.get(text("key")) {
            case OptionText.none { print("missing value") }
            case OptionText.some(value) {
                values.put(text("key"), text("replacement"))
                match json_quote(value) {
                    case ResultText.error(message) { print("bad quote") }
                    case ResultText.ok(quoted) {
                        match http_response(200, quoted) {
                            case ResultBuffer.error(message) { print("bad response") }
                            case ResultBuffer.ok(bytes) { if len(bytes) < 9 { print("short response") } }
                        }
                    }
                }
            }
        }
        match http_parse_head(buffer("POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}")) {
            case HttpResult.error(message) { print("bad request") }
            case HttpResult.ok(request) { if !same(request.body, "{}") { print("bad body") } }
        }
        iteration = iteration + 1
    }
    print("backend ownership ok")
}
"#,
        );
        let ir_path = workspace.file("sanitized.ll");
        succeeds(
            &workspace
                .command()
                .args(["emit-llvm", "main.bh", "--frontend", mode, "-o"])
                .arg(&ir_path)
                .output()
                .unwrap(),
        );
        let ir = fs::read_to_string(&ir_path)
            .unwrap()
            .replace("@malloc(", "@tracked_malloc(")
            .replace("@free(", "@tracked_free(");
        let ir = ir
            .lines()
            .map(|line| {
                if line.starts_with("define ") {
                    format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        fs::write(&ir_path, ir).unwrap();
        let tracker = workspace.write("tracker.c",r#"
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) { if(live) {fprintf(stderr,"live allocations: %ld\n",live);_Exit(99);} }
void *tracked_malloc(size_t n) {if(!registered){registered=1;atexit(check_live);}void *p=malloc(n);if(p)++live;return p;}
void *tracked_realloc(void *p,size_t n){int empty=p==NULL;void *q=realloc(p,n);if(q&&empty)++live;return q;}
void tracked_free(void *p){if(p)--live;free(p);}
"#);
        let mut runtime = vec![tracker];
        for name in ["core", "net", "text"] {
            let contents = format!(
                r#"
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <stdlib.h>
void *tracked_malloc(size_t);
void *tracked_realloc(void *,size_t);
void tracked_free(void *);
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
#include "{}"
"#,
                fixture(&format!("runtime/{name}.c")).display()
            );
            runtime.push(workspace.write(&format!("tracked-{name}.c"), &contents));
        }
        for optimization in ["-O0", "-O2"] {
            let executable = workspace.file("sanitized");
            succeeds(
                &Command::new(&clang)
                    .args([
                        optimization,
                        "-fsanitize=address,undefined",
                        "-fno-sanitize-recover=all",
                        "-fno-omit-frame-pointer",
                        "-pthread",
                    ])
                    .arg(&ir_path)
                    .args(&runtime)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap(),
            );
            assert_eq!(
                run(&executable),
                "backend ownership ok\n",
                "{mode}, {optimization}"
            );
        }
    }
}
