#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
mod common;
use common::*;

const OWNERSHIP: &str = r#"
def changed(value: buffer) -> buffer {
    append(value, "!")
    return value
}
def early(value: buffer, select: bool) -> buffer {
    var local = slice(value, 0, len(value))
    if select {
        var nested = buffer("temporary")
        return local
    }
    return buffer("fallback")
}
def main() {
    var original = buffer("hello")
    var copied = original
    set(copied, 0, 72)
    print(original)
    print(copied)
    var returned = changed(original)
    push(returned, 63)
    print(original)
    print(returned)
    original = original
    print(early(original, true))
    var values = ints()
    push(values, -9223372036854775807 - 1)
    push(values, 42)
    var independent = values
    set(independent, 1, 7)
    print(get(values, 1))
    print(get(independent, 1))
    var i = 0
    while i < 50 {
        var transient = slice(original, 0, 3)
        transient = early(transient, false)
        i = i + 1
    }
    print(same(to_buffer(get(values, 0)), "-9223372036854775808"))
    buffer("discarded temporary")
}
"#;
const EXPECTED: &str = "hello\nHello\nhello!\nhello!?\nhello!\n42\n7\ntrue\n";

#[test]
fn owned_values_copy_borrow_return_and_cleanup_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("owned-values");
    let source = workspace.write("ownership.bh", OWNERSHIP);
    let ir = workspace.file("ownership.ll");
    succeeds(&workspace.run(&["emit-llvm", as_str(&source), "-o", as_str(&ir)]));
    // Count only allocations inside the runtime, including temporary values.
    // libc's own internal buffers are outside the language ownership contract.
    let tracked = format!(
        r#"
#define _POSIX_C_SOURCE 200809L
#ifdef __APPLE__
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) {{
    if (live != 0) {{ fprintf(stderr, "live Bhasa allocations: %ld\n", live); _Exit(99); }}
}}
static void *tracked_malloc(size_t n) {{
    if (!registered) {{ registered = 1; atexit(check_live); }}
    void *p = malloc(n); if (p) ++live; return p;
}}
static void *tracked_realloc(void *p, size_t n) {{
    int was_null = p == NULL;
    void *q = realloc(p, n); if (q && was_null) ++live; return q;
}}
static void tracked_free(void *p) {{ if (p) --live; free(p); }}
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
{}
"#,
        include_str!("../runtime/core.c")
    );
    let runtime = workspace.write("tracked.c", &tracked);
    fs::copy(fixture("runtime/core.h"), workspace.file("core.h")).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("owned");
        let result = Command::new(&clang)
            .args([
                optimization,
                "-fsanitize=address,undefined",
                "-fno-omit-frame-pointer",
            ])
            .arg(&ir)
            .arg(&runtime)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&result);
        let result = execute(&executable, &workspace);
        succeeds(&result);
        assert_eq!(stdout(&result), EXPECTED);
        assert!(result.stderr.is_empty(), "{}", describe(&result));
    }
}

#[test]
fn binary_io_args_and_subprocesses_work_from_bhasa() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("owned-io");
    let source = workspace.write(
        "io.bh",
        r#"
def main() -> int {
    if arg_count() != 3 { return 90 }
    var source = buffer()
    if read_file(arg(1), source) != 0 { return 91 }
    if write_file(arg(2), source) != 0 { return 92 }
    var keep = buffer("keep")
    if read_file("missing-file", keep) == 0 { return 93 }
    if not same(keep, "keep") { return 94 }
    var input = buffer()
    if read_stdin(input) != 0 { return 95 }
    if write_stdout(input) != 0 { return 96 }
    if write_stderr(buffer("stderr bytes")) != 0 { return 97 }
    var command = buffer()
    args_push(command, "/usr/bin/printf")
    args_push(command, "%s")
    args_push(command, "$(touch should-not-exist)")
    return process_run(command)
}
"#,
    );
    let input = workspace.file("input binary with spaces");
    let output = workspace.file("output binary with spaces");
    fs::write(&input, [0, 1, 255, 10]).unwrap();
    let mut command = workspace.command();
    command.args([
        "run",
        as_str(&source),
        "--",
        as_str(&input),
        as_str(&output),
    ]);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"from stdin\0")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    succeeds(&result);
    assert_eq!(result.stdout, b"from stdin\0$(touch should-not-exist)");
    assert_eq!(result.stderr, b"stderr bytes");
    assert_eq!(fs::read(output).unwrap(), [0, 1, 255, 10]);
    assert!(!workspace.file("should-not-exist").exists());
}

#[test]
fn checked_access_rejects_invalid_indices_and_byte_values() {
    if clang().is_none() {
        return;
    }
    for expression in [
        "get(buffer(), 0)",
        "get(ints(), -1)",
        "push(buffer(), 256)",
        "slice(buffer(\"a\"), 0, 2)",
    ] {
        let workspace = Workspace::new("bounds");
        let source = workspace.write("bounds.bh", &format!("def main() {{ {expression} }}"));
        let result = workspace.run(&["run", as_str(&source)]);
        fails(&result);
        assert!(
            stderr(&result).contains("Bhasa runtime error:"),
            "{}",
            describe(&result)
        );
    }
}

#[test]
fn scalar_tasks_own_their_file_buffers_and_read_process_arguments() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("threaded-io");
    let first = workspace.file("first file");
    let second = workspace.file("second file");
    fs::write(&first, [0, 1, 2, 3, 255]).unwrap();
    fs::write(&second, b"longer contents").unwrap();
    let source = fixture("examples/concurrency/file_sizes.bh");
    let result = workspace.run(&[
        "run",
        as_str(&source),
        "--",
        as_str(&first),
        as_str(&second),
    ]);
    succeeds(&result);
    assert_eq!(stdout(&result), "5\n15\n");
    assert!(result.stderr.is_empty(), "{}", describe(&result));
}
