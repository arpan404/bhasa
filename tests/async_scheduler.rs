#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn frontends() -> Vec<&'static str> {
    let mut values = vec!["rust"];
    if cfg!(feature = "bhasa-frontend") {
        values.push("bhasa");
    }
    values
}

fn bounded(command: &mut Command) -> Output {
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
            child.kill().unwrap();
            panic!(
                "async language fixture exceeded 40 seconds: {}",
                describe(&child.wait_with_output().unwrap())
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn async_tasks_preserve_scoped_linear_ownership() {
    let workspace = Workspace::new("async-linear");
    for (body, accepted) in [
        (
            "var t = async_spawn(worker(21)); var result = await(t); print(result)",
            true,
        ),
        ("var t = async_spawn(worker(21)); await(t)", true),
        (
            "var t = async_spawn(worker(21)); var result = join(t); print(result)",
            true,
        ),
        (
            "var t = spawn(worker(21)); var result = await(t); print(result)",
            true,
        ),
        (
            "if true { var t = async_spawn(worker(21)); await(t) }",
            true,
        ),
        ("var t = async_spawn(worker(21))", false),
        ("var t = async_spawn(worker(21)); return", false),
        (
            "var t = async_spawn(worker(21)); if true { return }; await(t)",
            false,
        ),
        (
            "var t = async_spawn(worker(21)); var copy = t; await(t)",
            false,
        ),
        ("var t = async_spawn(worker(21)); t = 1; await(t)", false),
        ("var t = async_spawn(worker(21)); await(t); await(t)", false),
        ("var t = async_spawn(worker(21)); join(t); await(t)", false),
        (
            "var t = async_spawn(worker(21)); if true { await(t) }",
            false,
        ),
        (
            "var t = async_spawn(worker(21)); while false { await(t) }",
            false,
        ),
        (
            "var t = async_spawn(worker(21)); if true { var t = 1 }; await(t)",
            false,
        ),
        ("var t = async_spawn(worker(21)); print(await(t))", false),
        (
            "var t = async_spawn(worker(21)); var value = false and await(t) == 0",
            false,
        ),
        ("async_spawn(worker(21))", false),
        ("var t = async_spawn(21)", false),
        ("var t = async_spawn(worker(true)); await(t)", false),
        ("var t = async_spawn(worker(21)); print(t); await(t)", false),
        ("var value = await(21)", false),
    ] {
        let source =
            format!("def worker(value:int) -> int {{ return value * 2 }} def main() {{ {body} }}");
        let path = workspace.write("linear.bh", &source);
        for frontend in frontends() {
            let mut command = workspace.command();
            command
                .arg("check")
                .arg(&path)
                .args(["--frontend", frontend, "--json"]);
            let output = bounded(&mut command);
            assert_eq!(
                output.status.success(),
                accepted,
                "{frontend}: {source}\n{}",
                describe(&output)
            );
            if !accepted {
                assert!(stdout(&output).contains("error"), "{}", describe(&output));
                assert!(
                    !stderr(&output).contains("panicked"),
                    "{}",
                    describe(&output)
                );
            }
        }
    }
}

#[test]
fn workers_reject_transitive_pooled_scheduling_and_foreign_calls() {
    let workspace = Workspace::new("async-worker-checks");
    let pure = "def pure(n:int) -> int { return n + 1 } ";
    for definitions in [
        "def worker(n:int) -> int { var task = async_spawn(pure(n)); var value = join(task); return value }",
        "def worker(n:int) -> int { var task = spawn(pure(n)); var value = await(task); return value }",
        "def helper(n:int) -> int { var task = async_spawn(pure(n)); var value = await(task); return value } def worker(n:int) -> int { return helper(n) }",
        "struct Runner { def run(self, n:int) -> int { var task = async_spawn(pure(n)); var value = join(task); return value } } def worker(n:int) -> int { return Runner().run(n) }",
        "enum E { value(n:int) } def worker(n:int) -> int { match E.value(n) { case E.value(value) { var task = async_spawn(pure(value)); var result = await(task); return result } } }",
        "extern def foreign(n:int) -> int\nenum E { value(n:int) } def worker(n:int) -> int { match E.value(n) { case E.value(value) { return foreign(value) } } }",
        "extern def foreign(n:int) -> int\nstruct Runner { def run(self, n:int) -> int { return foreign(n) } } def worker(n:int) -> int { return Runner().run(n) }",
        "def helper(n:int) -> int { if n > 0 { return worker(n - 1) }; var task = async_spawn(pure(n)); var value = join(task); return value } def worker(n:int) -> int { return helper(n) }",
        "def helper[T](n:T) -> int { var task = async_spawn(pure(1)); var value = join(task); return value } def worker(n:int) -> int { return helper[int](n) }",
        "def helper[T](n:T) -> int { var task = spawn(pure(1)); var value = await(task); return value } def worker(n:int) -> int { return helper[int](n) }",
        "extern def foreign(n:int) -> int\ndef helper[T](n:T) -> int { return foreign(1) } def worker(n:int) -> int { return helper[int](n) }",
    ] {
        for operation in ["spawn", "async_spawn"] {
            let source = format!("{pure}{definitions} def main() {{ var task = {operation}(worker(21)); var result = join(task); print(result) }}");
            let path = workspace.write("worker.bh", &source);
            for frontend in frontends() {
                let mut command = workspace.command();
                command.arg("check").arg(&path).args(["--frontend", frontend, "--json"]);
                let output = bounded(&mut command);
                assert!(!output.status.success(), "{frontend}: {source}\n{}", describe(&output));
                let code = if frontend == "rust" { "S020" } else { "BH209" };
                assert!(stdout(&output).contains(code), "{frontend}: {source}\n{}", describe(&output));
            }
        }
    }
    for definitions in [
        // Worker arguments execute in the caller, where pooled scheduling is safe.
        "def prepare(n:int) -> int { var task = async_spawn(pure(n)); var result = await(task); return result } def main() { var task = async_spawn(pure(prepare(21))); var value = await(task); print(value) }",
        // Methods sharing builtin spellings resolve by their record type.
        "struct Runner { def async_spawn(self, n:int) -> int { return n } def await(self, n:int) -> int { return n } } def worker(n:int) -> int { var r = Runner(); return r.async_spawn(r.await(n)) } def main() { var task = async_spawn(worker(21)); var value = await(task); print(value) }",
        "def identity[T](value:T) -> T { return value } def main() { var task = async_spawn(identity[int](21)); var value = await(task); print(value) }",
    ] {
        let path = workspace.write("caller.bh", &format!("{pure}{definitions}"));
        for frontend in frontends() {
            let mut command = workspace.command();
            command.arg("check").arg(&path).args(["--frontend", frontend]);
            succeeds(&bounded(&mut command));
        }
    }
}

#[test]
fn compiled_async_jobs_queue_and_interoperate_with_legacy_threads() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("async-native-language");
    let mut source = String::from("def worker(value:int) -> int { return value * 2 }\ndef nested(value:int) -> int { var child = spawn(worker(value)); var result = join(child); return result }\ndef main() {\nvar sum = 0\n");
    for i in 0..96 {
        source.push_str(&format!("var job{i} = async_spawn(worker({i}))\n"));
    }
    for i in (0..96).rev() {
        source.push_str(&format!(
            "var result{i} = await(job{i})\nsum = sum + result{i}\n"
        ));
    }
    source.push_str("print(sum)\nvar native = spawn(worker(100)); var a = await(native); print(a)\nvar pooled = async_spawn(worker(101)); var b = join(pooled); print(b)\nvar parent = async_spawn(nested(21)); var c = await(parent); print(c)\n}\n");
    let path = workspace.write("queued.bh", &source);
    for frontend in frontends() {
        let mut command = workspace.command();
        command
            .arg("emit-llvm")
            .arg(&path)
            .args(["--frontend", frontend]);
        let ir = bounded(&mut command);
        succeeds(&ir);
        assert!(stdout(&ir).contains("; bhasa runtime: threads"));
        assert!(stdout(&ir).contains("@bhasa_rt_async_spawn_i64"));
        assert!(stdout(&ir).contains("@bhasa_rt_await_i64"));
        for release in [false, true] {
            let mut command = workspace.command();
            command.arg("run").arg(&path).args(["--frontend", frontend]);
            if release {
                command.arg("--release");
            }
            let output = bounded(&mut command);
            succeeds(&output);
            assert_eq!(stdout(&output), "9120\n200\n202\n42\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}
