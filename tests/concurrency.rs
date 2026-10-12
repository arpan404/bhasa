use std::process::Command;

mod common;
use common::*;

fn rejects(body: &str, expected: &str) {
    let source =
        format!("def worker(value: int) -> int {{ return value + 1 }}\ndef main() {{ {body} }}");
    let errors = bhasa::check(&source).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains(expected)),
        "{source}\n{errors:?}"
    );
}

#[test]
fn task_ownership_rejects_leaks_copies_and_conditional_joins() {
    for (body, message) in [
        ("var job = spawn(worker(1))", "must be joined"),
        ("var job = spawn(worker(1)); return", "must be joined"),
        (
            "var job = spawn(worker(1)); if true { return }; join(job)",
            "must be joined",
        ),
        (
            "var job = spawn(worker(1)); var copy = job; join(job)",
            "can only be consumed",
        ),
        (
            "var job = spawn(worker(1)); job = 2; join(job)",
            "cannot be reassigned",
        ),
        (
            "var job = spawn(worker(1)); join(job); join(job)",
            "already been joined",
        ),
        (
            "var job = spawn(worker(1)); if true { join(job) }",
            "same lexical block",
        ),
        (
            "var job = spawn(worker(1)); while false { join(job) }",
            "same lexical block",
        ),
        (
            "var job = spawn(worker(1)); if true { var job = 1 }; join(job)",
            "cannot be shadowed",
        ),
        (
            "var job = spawn(worker(1)); print(job); join(job)",
            "can only be consumed",
        ),
        (
            "var job = spawn(worker(1)); print(join(job))",
            "complete variable initializer",
        ),
        (
            "var job = spawn(worker(1)); var x = false and join(job) == 0",
            "complete variable initializer",
        ),
        ("spawn(worker(1))", "complete variable initializer"),
        ("var job = spawn(1)", "requires a call"),
        (
            "var job = spawn(worker(1)); var x = worker(job); join(job)",
            "can only be consumed",
        ),
        ("var value = join(1)", "handle name"),
    ] {
        rejects(body, message);
    }
}

#[test]
fn workers_reject_foreign_calls_through_recursive_call_graphs() {
    for source in [
        "extern def foreign(value: int) -> int\ndef main() { var job = spawn(foreign(1)); join(job) }",
        "extern def foreign(value: int) -> int\ndef worker(value: int) -> int { return foreign(value) }\ndef main() { var job = spawn(worker(1)); join(job) }",
        "extern def foreign(value: int) -> int\ndef first(value: int) -> int { if value == 0 { return foreign(value) }; return second(value - 1) }\ndef second(value: int) -> int { return first(value) }\ndef main() { var job = spawn(second(1)); join(job) }",
    ] {
        let errors = bhasa::check(source).unwrap_err();
        assert!(errors.iter().any(|error| error.code == "S020"), "{errors:?}");
    }
    // Only the worker runs concurrently. Its argument is evaluated by the caller.
    bhasa::check("extern def foreign(value: int) -> int\ndef worker(value: int) -> int { return value }\ndef main() { var job = spawn(worker(foreign(1))); join(job) }").unwrap();
    for worker in [
        "def worker() -> int { return 1 }",
        "def worker(value: bool) -> int { return 1 }",
        "def worker(value: int) -> bool { return true }",
        "def worker(value: int) { print(value) }",
    ] {
        let source = format!("{worker}\ndef main() {{ var job = spawn(worker(1)); join(job) }}");
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "S020"),
            "{errors:?}"
        );
    }
}

#[test]
fn borrowed_containers_and_fallible_builtins_are_checked() {
    let errors = bhasa::check("def replace(data: buffer) { data = buffer() }").unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("cannot be rebound")));
    bhasa::check("def change(data: buffer) { push(data, 65); if true { var data = buffer(); push(data, 66) } }").unwrap();
    for expression in [
        "read_file(\"missing\", data)",
        "write_file(\"output\", data)",
        "read_stdin(data)",
        "write_stdout(data)",
        "write_stderr(data)",
        "process_run(data)",
        "gpu_dispatch(\"source\", \"entry\", data)",
    ] {
        let source = format!("def main() {{ var data = buffer(); {expression} }}");
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("status")
                    && error.message.contains("must be used")),
            "{errors:?}"
        );
    }
    let errors = bhasa::check("extern def bhasa_rt_join_i64(value: int) -> int").unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("reserved")));
    let errors = bhasa::check("def main() { print(ints()) }").unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("print requires")));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_tasks_join_with_exact_results_in_both_modes() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("tasks");
    let source = workspace.write(
        "tasks.bh",
        r#"
def fib(n: int) -> int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
def nested(n: int) -> int {
    var child = spawn(fib(n))
    var value = join(child)
    return value
}
def main() {
    var first = spawn(nested(12))
    var second = spawn(fib(10))
    if true {
        var local = spawn(fib(6))
        var result = join(local)
        print(result)
    }
    var a = join(first)
    var b = join(second)
    print(a + b)
    var i = 0
    while i < 4 {
        var job = spawn(fib(i))
        var value = join(job)
        print(value)
        i = i + 1
    }
}
"#,
    );
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let output = execute(&executable, &workspace);
        succeeds(&output);
        assert_eq!(stdout(&output), "8\n199\n0\n1\n1\n2\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn runtime_workers_execute_simultaneously() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("concurrency-runtime");
    let source = workspace.write(
        "test.c",
        r#"
#define _POSIX_C_SOURCE 200809L
#include "concurrency.h"
#include <assert.h>
#include <stdatomic.h>
#include <stddef.h>
#include <time.h>
static atomic_int entered;
static int64_t worker(int64_t value) {
    atomic_fetch_add_explicit(&entered, 1, memory_order_release);
    struct timespec pause = {0, 1000000};
    int attempts = 0;
    while (atomic_load_explicit(&entered, memory_order_acquire) < 2) {
        assert(++attempts < 5000);
        nanosleep(&pause, NULL);
    }
    return value + 1;
}
int main(void) {
    for (int i = 0; i < 50; ++i) {
        atomic_store(&entered, 0);
        void *first = bhasa_rt_spawn_i64(worker, 41);
        void *second = bhasa_rt_spawn_i64(worker, -43);
        assert(bhasa_rt_join_i64(second) == -42);
        assert(bhasa_rt_join_i64(first) == 42);
    }
    return 0;
}
"#,
    );
    let executable = workspace.file("test-runtime");
    let output = Command::new(clang)
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-pthread",
        ])
        .arg("-I")
        .arg(fixture("runtime"))
        .arg(&source)
        .arg(fixture("runtime/concurrency.c"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    succeeds(&output);
    succeeds(&execute(&executable, &workspace));
}
