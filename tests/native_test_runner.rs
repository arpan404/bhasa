mod common;
use bhasa::json::{self, Value};
use common::*;
use std::fs;

fn report(output: &std::process::Output) -> Value {
    json::parse(&stdout(output))
        .unwrap_or_else(|e| panic!("invalid test report: {e}: {}", describe(output)))
}
fn frontends() -> Vec<&'static str> {
    if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    }
}
#[test]
fn native_boolean_tests_filter_capture_and_isolate_failures() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("native-tests");
    workspace.write("checks.bh", "def test_good() -> bool { print(\"captured\"); return true }\ndef test_false() -> bool { return false }\ndef test_trap() -> bool { var zero = 0; print(1 / zero); return true }\n");
    for frontend in frontends() {
        let output = workspace.run(&["test", "checks.bh", "--frontend", frontend, "--json"]);
        assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
        let value = report(&output);
        assert_eq!(value.get("passed").and_then(Value::as_i64), Some(1));
        assert_eq!(value.get("failed").and_then(Value::as_i64), Some(2));
        let tests = value.get("tests").and_then(Value::as_array).unwrap();
        assert_eq!(tests.len(), 3);
        assert_eq!(
            tests[1].get("name").and_then(Value::as_str),
            Some("test_good")
        );
        assert_eq!(
            tests[1].get("stdout").and_then(Value::as_str),
            Some("captured\n")
        );
        let output = workspace.run(&[
            "test",
            "checks.bh",
            "--frontend",
            frontend,
            "--filter",
            "good",
            "--release",
            "--json",
        ]);
        succeeds(&output);
        assert_eq!(
            report(&output).get("passed").and_then(Value::as_i64),
            Some(1)
        );
    }
}
#[test]
fn test_timeout_and_output_limit_preserve_following_results() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("native-test-timeouts");
    workspace.write("checks.bh", "def test_a_timeout() -> bool { while true { } return true }\ndef test_b_verbose() -> bool { var i = 0; while i < 900 { print(\"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz\"); i = i + 1 } return true }\ndef test_c_after() -> bool { return true }\n");
    let output = workspace.run(&["test", "checks.bh", "--timeout", "1", "--json"]);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let value = report(&output);
    let tests = value.get("tests").and_then(Value::as_array).unwrap();
    assert_eq!(
        tests[0].get("status").and_then(Value::as_str),
        Some("timeout")
    );
    assert_eq!(tests[1].get("output_truncated"), Some(&Value::Bool(true)));
    assert_eq!(
        tests[1]
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap()
            .len(),
        65536
    );
    assert_eq!(
        tests[2].get("status").and_then(Value::as_str),
        Some("passed")
    );
}
#[test]
fn package_test_discovery_uses_aliases_private_helpers_and_nested_folders() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("native-package-tests");
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"tests\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n",
    );
    for dir in ["src", "tests", "tests/nested"] {
        fs::create_dir_all(workspace.file(dir)).unwrap();
    }
    workspace.write("src/main.bh", "def main() { print(42) }\n");
    workspace.write(
        "src/lib.bh",
        "def double(value: int) -> int { return value * 2 }\n",
    );
    workspace.write("tests/checks.bh", "import \"../src/lib.bh\" as math\nprivate def helper() -> bool { return math.double(21) == 42 }\nprivate def test_answer() -> bool { return helper() }\n");
    workspace.write(
        "tests/nested/other.bh",
        "def test_float() -> bool { return 2.5 * 2.0 == 5.0 }\n",
    );
    workspace.write(
        "tests/helpers.bh",
        "def identity(value: int) -> int { return value }\n",
    );
    for frontend in frontends() {
        let output = workspace.run(&["test", "--frontend", frontend, "--json"]);
        succeeds(&output);
        assert_eq!(
            report(&output).get("passed").and_then(Value::as_i64),
            Some(2)
        );
    }
    assert!(fs::read_dir(workspace.file("build"))
        .unwrap()
        .all(|item| !item
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".bhasa-tests-")));
}
#[test]
fn test_usage_discovery_and_compile_errors_are_reported_without_panic() {
    let workspace = Workspace::new("native-invalid-tests");
    for (source, expected) in [
        (
            "def main() {}\ndef test_yes() -> bool { return true }",
            "must not define main",
        ),
        (
            "def test_bad(value: int) -> bool { return true }",
            "no arguments returning bool",
        ),
        ("def test_bad() -> int { return 1 }", "returning bool"),
        ("def helper() -> bool { return true }", "no matching tests"),
        (
            "def test_bad() -> bool { return missing }",
            "unknown variable",
        ),
    ] {
        workspace.write("checks.bh", source);
        let output = workspace.run(&["test", "checks.bh", "--json"]);
        assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
        assert!(
            report(&output)
                .get("error")
                .and_then(Value::as_str)
                .unwrap()
                .contains(expected),
            "{}",
            describe(&output)
        );
    }
    for args in [
        vec!["test", "--timeout", "0"],
        vec!["test", "--frontend", "unknown"],
        vec!["test", "--filter"],
        vec!["test", "checks.bh", "--json", "--json"],
    ] {
        fails(&workspace.run(&args));
    }
}
#[cfg(unix)]
#[test]
fn discovery_refuses_symlink_files() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new("native-test-symlink");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write("outside.bh", "def test_yes() -> bool { return true }\n");
    symlink(
        workspace.file("outside.bh"),
        workspace.file("tests/check.bh"),
    )
    .unwrap();
    let output = workspace.run(&["test", "tests", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(report(&output)
        .get("error")
        .and_then(Value::as_str)
        .unwrap()
        .contains("symlink"));
}

#[test]
fn generic_test_templates_are_rejected_even_when_unused() {
    let workspace = Workspace::new("native-generic-tests");
    for source in [
        "def test_generic[T]() -> bool { return true }\ndef test_good() -> bool { return true }",
        "private def test_generic[T]() -> bool { return true }\ndef test_good() -> bool { return true }",
        "def test_generic[T]() -> bool { return true }\ndef helper() -> bool { return test_generic[int]() }",
    ] {
        workspace.write("checks.bh", source);
        let output = workspace.run(&["test", "checks.bh", "--json"]);
        assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
        assert!(report(&output)
            .get("error")
            .and_then(Value::as_str)
            .unwrap()
            .contains("test_generic' must be a non-generic"), "{}", describe(&output));
    }
}

#[test]
fn a_later_compile_error_preserves_completed_results() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("native-partial-results");
    fs::create_dir(workspace.file("tests")).unwrap();
    workspace.write("tests/a.bh", "def test_passed() -> bool { return true }\n");
    workspace.write(
        "tests/b.bh",
        "def test_broken() -> bool { return missing }\n",
    );
    let output = workspace.run(&["test", "tests", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
    let value = report(&output);
    assert_eq!(value.get("passed").and_then(Value::as_i64), Some(1));
    assert_eq!(value.get("failed").and_then(Value::as_i64), Some(0));
    let tests = value.get("tests").and_then(Value::as_array).unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(
        tests[0].get("name").and_then(Value::as_str),
        Some("test_passed")
    );
    assert!(value
        .get("error")
        .and_then(Value::as_str)
        .unwrap()
        .contains("unknown variable"));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn detached_descendant_cannot_hold_test_output_collection_open() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("native-detached-output");
    let source = workspace.write(
        "descendant.c",
        r#"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    int ready[2];
    if (pipe(ready) != 0) return 3;
    pid_t child = fork();
    if (child < 0) return 4;
    if (child == 0) {
        close(ready[0]);
        if (setsid() < 0) _exit(5);
        FILE *file = fopen(argv[1], "w");
        if (!file) _exit(6);
        fprintf(file, "%ld", (long)getpid());
        if (fclose(file) != 0) _exit(7);
        if (write(ready[1], "!", 1) != 1) _exit(8);
        close(ready[1]);
        sleep(20);
        _exit(0);
    }
    close(ready[1]);
    char value;
    return read(ready[0], &value, 1) == 1 ? 0 : 9;
}
"#,
    );
    let helper = workspace.file("descendant");
    succeeds(
        &Command::new(clang)
            .arg(source)
            .arg("-o")
            .arg(&helper)
            .output()
            .unwrap(),
    );
    let pid_file = workspace.file("descendant.pid");
    struct Descendant(std::path::PathBuf);
    impl Drop for Descendant {
        fn drop(&mut self) {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            if let Some(pid) = fs::read_to_string(&self.0)
                .ok()
                .and_then(|text| text.parse::<i32>().ok())
            {
                if pid > 1 {
                    unsafe {
                        kill(pid, 9);
                    }
                }
            }
        }
    }
    let _cleanup = Descendant(pid_file.clone());
    workspace.write("checks.bh", &format!(
        "def test_detached() -> bool {{ var command = buffer(); args_push(command, {}); args_push(command, {}); var status = process_run(command); print(\"finished\"); return status == 0 }}\n",
        Value::String(helper.to_str().unwrap().into()),
        Value::String(pid_file.to_str().unwrap().into()),
    ));
    let mut child = workspace
        .command()
        .args(["test", "checks.bh", "--timeout", "1", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("test runner did not finish: {}", describe(&output));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    succeeds(&output);
    assert!(
        pid_file.exists(),
        "fixture did not launch its detached child"
    );
    let value = report(&output);
    let tests = value.get("tests").and_then(Value::as_array).unwrap();
    assert_eq!(
        tests[0].get("stdout").and_then(Value::as_str),
        Some("finished\n")
    );
    assert!(
        tests[0].get("duration_ms").and_then(Value::as_i64).unwrap() < 1000,
        "output collection exceeded the test deadline: {}",
        describe(&output)
    );
}
