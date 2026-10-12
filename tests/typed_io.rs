#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::fs;
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
    assert!(!timed_out, "typed I/O deadline: {}", describe(&output));
    output
}
fn library(workspace: &Workspace) {
    for name in ["io.bh", "text.bh"] {
        fs::copy(fixture(&format!("std/{name}")), workspace.file(name)).unwrap();
    }
}

#[test]
fn typed_file_errors_preserve_status_privacy_and_path_types() {
    let workspace = Workspace::new("typed-io-check");
    library(&workspace);
    let valid = workspace.write("valid.bh", PROGRAM);
    succeeds(&bounded(workspace.command().arg("check").arg(valid)));
    for (index, body) in INVALID.iter().enumerate() {
        let source = workspace.write(
            &format!("invalid{index}.bh"),
            &format!("import \"io.bh\" as io\nimport \"text.bh\"\n{body}"),
        );
        let output = bounded(workspace.command().arg("check").arg(source));
        fails(&output);
        if index < 5 {
            assert!(stderr(&output).contains("private"), "{}", describe(&output));
        }
    }
}

#[test]
fn typed_file_results_own_payloads_and_preserve_faults_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-io-native");
    library(&workspace);
    let source = workspace.write("program.bh", PROGRAM);
    let ir = workspace.file("program.ll");
    succeeds(&bounded(
        workspace
            .command()
            .arg("emit-llvm")
            .arg(source)
            .arg("-o")
            .arg(&ir),
    ));
    let text = fs::read_to_string(&ir)
        .unwrap()
        .replace("@malloc(", "@io_test_malloc(")
        .replace("@realloc(", "@io_test_realloc(")
        .replace("@free(", "@io_test_free(");
    let instrumented = text
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    fs::write(&ir, instrumented).unwrap();
    let runtime = workspace.write("io_runtime.c", RUNTIME);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("typed-io");
        let object = workspace.file("io_runtime.o");
        succeeds(&bounded(
            Command::new(&clang)
                .args([
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
                ])
                .arg("-I")
                .arg(fixture("runtime"))
                .arg("-c")
                .arg(&runtime)
                .arg("-o")
                .arg(&object),
        ));
        succeeds(&bounded(
            Command::new(&clang)
                .args([
                    "-pthread",
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                    optimization,
                ])
                .arg(&ir)
                .arg(&object)
                .arg("-o")
                .arg(&executable),
        ));
        let output = bounded(
            Command::new(&executable)
                .current_dir(workspace.path())
                .env("UBSAN_OPTIONS", "halt_on_error=1"),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "typed file I/O passed\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

const INVALID: &[&str] = &[
    "def use(error:io.Error){ print(error.code_value) } def main(){}",
    "def use(error:io.Error){ error.code_value=1 } def main(){}",
    "def main(){var e=io.Error(io.ErrorDomain.os(),1,\"read_file\",text(\"forged\"))}",
    "def main(){var e=io.file_error(1,\"read_file\",\"forged\")}",
    "def use(error:io.Error){print(error.message_value.bytes())} def main(){}",
    "def main(){var result=io.file_read(1)}",
    "def main(){var result=io.file_read_str(buffer(\"path\"))}",
    "def main(){var result=io.file_write(buffer(\"path\"),1)}",
    "def main(){var result=io.file_write_str(buffer(\"path\"),buffer())}",
    "def main(){var result=io.IoResult[int].error(text(\"wrong payload\"))}",
];

const PROGRAM: &str = r#"
import "io.bh" as io
extern def io_test_fault(mode: int) -> int
extern def io_test_status(mode: int) -> int

def check_error(error: io.Error, expected: int, operation: str, message: str) -> int {
    if expected <= 0 or error.code() != expected { return 1 }
    if !same(buffer(error.operation()), operation) { return 2 }
    if !same(error.message().bytes(), message) { return 3 }
    match error.domain() { case io.ErrorDomain.os {} }
    var copy = error
    var changed = copy.message()
    changed.clear()
    if !same(error.message().bytes(), message) or !same(copy.message().bytes(), message) { return 4 }
    return 0
}

def read_failure(path: buffer) -> int {
    var untouched = buffer("untouched")
    var status = read_file(path, untouched)
    if status <= 0 or !same(untouched, "untouched") { return 10 }
    match io.file_read(path) {
        case io.IoResult[buffer].ok(value) { return 11 }
        case io.IoResult[buffer].error(error) {
            return check_error(error, status, "read_file", "file read failed")
        }
    }
}

def write_failure(path: buffer, contents: buffer) -> int {
    var status = write_file(path, contents)
    if status <= 0 { return 12 }
    match io.file_write(path, contents) {
        case io.IoResult[int].ok(count) { return 13 }
        case io.IoResult[int].error(error) {
            return check_error(error, status, "write_file", "file write failed")
        }
    }
}

def roundtrip(path: buffer, contents: buffer) -> int {
    match io.file_write(path, contents) {
        case io.IoResult[int].error(error) { return 20 }
        case io.IoResult[int].ok(count) { if count != len(contents) { return 21 } }
    }
    match io.file_read(path) {
        case io.IoResult[buffer].error(error) { return 22 }
        case io.IoResult[buffer].ok(value) { if !same(value, contents) { return 23 } }
    }
    return 0
}

def injected_read(mode: int) -> int {
    io_test_fault(mode)
    var untouched = buffer("untouched")
    var status = read_file("binary data.bin", untouched)
    if status != io_test_status(mode) or !same(untouched, "untouched") { return 30 }
    io_test_fault(mode)
    match io.file_read_str("binary data.bin") {
        case io.IoResult[buffer].ok(contents) { return 31 }
        case io.IoResult[buffer].error(error) {
            return check_error(error, status, "read_file", "file read failed")
        }
    }
}

def injected_write(mode: int) -> int {
    var contents = buffer("new replacement bytes")
    io_test_fault(mode)
    var status = write_file("fault output.bin", contents)
    if status != io_test_status(mode) { return 32 }
    io_test_fault(mode)
    match io.file_write_str("fault output.bin", contents) {
        case io.IoResult[int].ok(count) { return 33 }
        case io.IoResult[int].error(error) {
            var result = check_error(error, status, "write_file", "file write failed")
            if result != 0 { return result }
        }
    }
    io_test_fault(0)
    var observed = buffer()
    var read_status = read_file("fault output.bin", observed)
    if read_status != 0 { return 34 }
    if mode == 3 or mode == 5 {
        if !same(observed, "new") { return 35 }
    } else {
        if !same(observed, contents) { return 36 }
    }
    return 0
}

def main() -> int {
    var bytes = buffer()
    var index = 0
    while index < 10240 { push(bytes, index % 256); index = index + 1 }
    var result = roundtrip(buffer("binary data.bin"), bytes)
    if result != 0 { return result }
    result = roundtrip(buffer("empty.bin"), buffer())
    if result != 0 { return result }
    var raw_path = buffer("raw-"); push(raw_path, 255)
    var raw_status = write_file(raw_path, bytes)
    if raw_status == 0 { result = roundtrip(raw_path, bytes) }
    else { result = write_failure(raw_path, bytes) }
    if result != 0 { return result }
    match io.file_write_str("string path.bin", bytes) {
        case io.IoResult[int].error(error) { return 40 }
        case io.IoResult[int].ok(count) { if count != len(bytes) { return 41 } }
    }
    match io.file_read_str("string path.bin") {
        case io.IoResult[buffer].error(error) { return 42 }
        case io.IoResult[buffer].ok(value) { if !same(value, bytes) { return 43 } }
    }
    index = 0
    while index < 32 {
        result = read_failure(buffer("missing-file-for-typed-io"))
        if result != 0 { return result }
        result = read_failure(buffer("."))
        if result != 0 { return result }
        var nul_path = buffer("binary data.bin"); push(nul_path, 0)
        result = read_failure(nul_path)
        if result != 0 { return result }
        result = write_failure(nul_path, bytes)
        if result != 0 { return result }
        result = write_failure(buffer("missing-directory-for-typed-io/file.bin"), bytes)
        if result != 0 { return result }
        index = index + 1
    }
    result = injected_read(1)
    if result != 0 { return result }
    result = injected_read(2)
    if result != 0 { return result }
    result = injected_read(6)
    if result != 0 { return result }
    io_test_fault(0)
    result = injected_write(3)
    if result != 0 { return result }
    result = injected_write(4)
    if result != 0 { return result }
    result = injected_write(5)
    if result != 0 { return result }
    print("typed file I/O passed")
    return 0
}
"#;

const RUNTIME: &str = r#"
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int64_t live;
static int registered, mode, reading, partial_read, partial_write;
static FILE *active;
static void check_live(void) {
    if (live != 0 || active != NULL) { fprintf(stderr, "leaked typed I/O allocations/file: %lld\n", (long long)live); _Exit(99); }
}
void *io_test_malloc(size_t size) {
    if (!registered) { assert(atexit(check_live) == 0); registered = 1; }
    void *pointer = malloc(size); if (pointer != NULL) ++live; return pointer;
}
void *io_test_realloc(void *pointer, size_t size) {
    int empty = pointer == NULL; void *result = realloc(pointer, size);
    if (result != NULL && empty) ++live;
    return result;
}
void io_test_free(void *pointer) { if (pointer != NULL) { assert(live > 0); --live; } free(pointer); }
int64_t io_test_fault(int64_t value) {
    assert(active == NULL && value >= 0 && value <= 6);
    mode = (int)value; partial_read = 0; partial_write = 0; return 0;
}
int64_t io_test_status(int64_t value) {
    if (value == 1 || value == 4 || value == 6) return EIO;
    if (value == 2) return EBADF;
    if (value == 3 || value == 5) return ENOSPC;
    return 0;
}
static FILE *fault_fopen(const char *path, const char *access) {
    assert(active == NULL);
    active = fopen(path, access); reading = access[0] == 'r';
    partial_read = 0; partial_write = 0;
    return active;
}
static size_t fault_fread(void *out, size_t size, size_t count, FILE *stream) {
    if (stream == active && (mode == 1 || mode == 6)) {
        assert(size == 1 && count >= 3 && !partial_read);
        size_t copied = fread(out, 1, 3, stream); assert(copied == 3);
        partial_read = 1; return copied;
    }
    return fread(out, size, count, stream);
}
static size_t fault_fwrite(const void *value, size_t size, size_t count, FILE *stream) {
    if (stream == active && (mode == 3 || mode == 5)) {
        assert(size == 1 && count >= 3 && !partial_write);
        size_t copied = fwrite(value, 1, 3, stream); assert(copied == 3);
        partial_write = 1; return copied;
    }
    return fwrite(value, size, count, stream);
}
static int fault_ferror(FILE *stream) {
    if (stream == active && partial_read) { errno = EIO; return 1; }
    if (stream == active && partial_write) { errno = ENOSPC; return 1; }
    return ferror(stream);
}
static int fault_fclose(FILE *stream) {
    assert(stream == active); int was_reading = reading;
    int result = fclose(stream); active = NULL;
    if ((was_reading && (mode == 2 || mode == 6)) || (!was_reading && (mode == 4 || mode == 5))) {
        errno = was_reading ? EBADF : EIO; return EOF;
    }
    return result;
}
#define malloc io_test_malloc
#define realloc io_test_realloc
#define free io_test_free
#define fopen fault_fopen
#define fread fault_fread
#define fwrite fault_fwrite
#define ferror fault_ferror
#define fclose fault_fclose
#include "core.c"
#include "text.c"
#include "file.c"
"#;
