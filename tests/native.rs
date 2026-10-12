use std::fs;
use std::process::Command;

mod common;
use common::*;

#[test]
fn basic_programs_produce_exact_host_output_in_both_modes() {
    if clang().is_none() {
        return;
    }
    let examples = [
        ("hello", "Hello, Bhasa!\nनमस्ते • こんにちは • 🌍\n"),
        ("recursion", "3628800\n55\n"),
        ("scopes", "20\n30\n20\n10\n11\n"),
        ("loops", "14\ntrue\n"),
        ("arithmetic", "14\n20\n12\n10\n-2\n-1\n1\n9223372036854775807\n-9223372036854775808\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nfalse\ntrue\ntrue\n"),
        ("short_circuit", "false\ntrue\nfalse\ntrue\n10\ntrue\n20\ntrue\n30\ntrue\n40\ntrue\nfalse\ntrue\n50\ntrue\n"),
        ("strings", "नमस्ते\n你好 🌍\nline one\nline two\ntab\tquote\"slash\\end\ncarriage\rreturn\n100% literal %s %d\n\n"),
    ];
    for release in [false, true] {
        for (name, expected) in examples {
            let workspace = Workspace::new(name);
            let source = fixture(&format!("examples/basics/{name}.bh"));
            let executable = build(&workspace, &source, release);
            let output = execute(&executable, &workspace);
            succeeds(&output);
            assert_eq!(stdout(&output), expected, "{name}, release={release}");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn run_command_preserves_stdout_exit_code_and_source() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("run");
    let contents = "def main() -> int { print(42); return 23 }";
    let source = workspace.write("input with spaces.bh", contents);
    for release in [false, true] {
        let mut command = workspace.command();
        command.arg("run").arg(&source);
        if release {
            command.arg("--release");
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(23), "{}", describe(&output));
        assert_eq!(stdout(&output), "42\n");
        assert!(
            !stderr(&output).contains("-Woverride-module"),
            "{}",
            describe(&output)
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), contents);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn explicit_macos_target_uses_resolved_ir_metadata() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("resolved-macos-target");
    let target = if cfg!(target_arch = "aarch64") {
        "macos-arm64"
    } else {
        "macos-x64"
    };
    let source = fixture("tests/fixtures/library.bh");
    let output = workspace.run(&[
        "build",
        as_str(&source),
        "--target",
        target,
        "--emit",
        "object",
    ]);
    succeeds(&output);
    assert!(
        !stderr(&output).contains("-Woverride-module"),
        "{}",
        describe(&output)
    );
}

#[test]
fn default_build_output_is_under_build_directory() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("default-output");
    let contents = "def main() { print(17) }";
    let source = workspace.write("default.bh", contents);
    let output = workspace.run(&["build", as_str(&source)]);
    succeeds(&output);
    let build_directory = workspace.file("build");
    assert!(
        build_directory.is_dir(),
        "default output did not create build/"
    );
    assert!(fs::read_dir(build_directory).unwrap().next().is_some());
    assert_eq!(fs::read_to_string(source).unwrap(), contents);
}

#[test]
fn overflow_and_division_errors_trap_in_both_build_modes() {
    if clang().is_none() {
        return;
    }
    let cases = [
        ("add-overflow", "9223372036854775807 + 1"),
        ("subtract-overflow", "minimum() - 1"),
        ("multiply-overflow", "9223372036854775807 * 2"),
        ("negate-overflow", "-minimum()"),
        ("divide-zero", "1 / 0"),
        ("remainder-zero", "1 % 0"),
        ("divide-overflow", "minimum() / -1"),
        ("remainder-overflow", "minimum() % -1"),
    ];
    for release in [false, true] {
        for (name, expression) in cases {
            let workspace = Workspace::new(name);
            let source = workspace.write(
                "trap.bh",
                &format!(
                    "def minimum() -> int {{ return -9223372036854775807 - 1 }}\n\
                     def main() {{ print({expression}); print(\"survived\") }}"
                ),
            );
            let executable = build(&workspace, &source, release);
            let output = execute(&executable, &workspace);
            assert!(
                !output.status.success(),
                "{name}, release={release}: {}",
                describe(&output)
            );
            assert!(
                !stdout(&output).contains("survived"),
                "{name} did not stop execution"
            );
        }
    }
}

#[test]
fn boundary_arithmetic_and_nested_boolean_loops_work() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("boundaries");
    let source = workspace.write(
        "boundary.bh",
        r#"
def minimum() -> int { return -9223372036854775807 - 1 }
def main() {
    print(minimum() / 1)
    print(minimum() % 1)
    print(9223372036854775807 + 0)
    print(minimum() - 0)
    print(minimum() * 1)
    print(-(-9223372036854775807))
    var n = 0
    while n < 6 and (n < 3 or n == 3) {
        n = n + 1
    }
    print(n)
}
"#,
    );
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let output = execute(&executable, &workspace);
        succeeds(&output);
        assert_eq!(stdout(&output), "-9223372036854775808\n0\n9223372036854775807\n-9223372036854775808\n-9223372036854775808\n9223372036854775807\n4\n");
    }
}

#[test]
fn internal_function_names_do_not_collide_with_libc() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("mangling");
    let source = workspace.write(
        "names.bh",
        "def puts(n: int) -> int { return n + 1 }\n\
        def printf(n: int) -> int { return puts(n) + 1 }\n\
        def main() { print(printf(40)); print(\"still works\") }",
    );
    let executable = build(&workspace, &source, false);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "42\nstill works\n");
}

#[test]
fn host_object_can_omit_main_but_executable_cannot() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("library-object");
    let source = fixture("tests/fixtures/library.bh");
    let object = workspace.file("nested objects/library.o");
    let output = workspace.run(&[
        "build",
        as_str(&source),
        "--emit",
        "object",
        "-o",
        as_str(&object),
    ]);
    succeeds(&output);
    assert!(fs::metadata(&object).unwrap().len() > 0);
    let output = workspace.run(&[
        "build",
        as_str(&source),
        "-o",
        as_str(&workspace.file("invalid-executable")),
    ]);
    fails(&output);
    assert!(
        stderr(&output).to_lowercase().contains("main"),
        "{}",
        describe(&output)
    );
}

#[test]
fn explicit_scalar_native_link_inputs_work_with_spaces() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("native-link");
    let native_source = workspace.write(
        "adapter with spaces.c",
        "#include <stdint.h>\n\
        static int64_t saved;\n\
        int64_t bhasa_test_double(int64_t n) { return n * 2; }\n\
        void bhasa_test_save(int64_t n) { saved = n; }\n\
        int64_t bhasa_test_load(void) { return saved; }\n",
    );
    let object = workspace.file("adapter with spaces.o");
    let output = Command::new(clang)
        .arg("-c")
        .arg(&native_source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    succeeds(&output);
    let source = workspace.write(
        "native.bh",
        "extern def bhasa_test_double(n: int) -> int\n\
        extern def bhasa_test_save(n: int)\nextern def bhasa_test_load() -> int\n\
        def main() { bhasa_test_save(bhasa_test_double(21)); print(bhasa_test_load()) }",
    );
    let executable = workspace.file(if cfg!(windows) {
        "linked.exe"
    } else {
        "linked"
    });
    let output = workspace.run(&[
        "build",
        as_str(&source),
        "--link",
        as_str(&object),
        "-o",
        as_str(&executable),
    ]);
    succeeds(&output);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "42\n");
    for release in [false, true] {
        let mut command = workspace.command();
        command.arg("run").arg(&source).arg("--link").arg(&object);
        if release {
            command.arg("--release");
        }
        let output = command.output().unwrap();
        succeeds(&output);
        assert_eq!(stdout(&output), "42\n");
    }
    let missing = workspace.run(&["build", as_str(&source), "--link", "missing-adapter.o"]);
    fails(&missing);
    assert!(
        stderr(&missing).contains("missing-adapter.o"),
        "{}",
        describe(&missing)
    );
}

#[test]
fn missing_native_implementation_explains_how_to_link_it() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("missing-native-implementation");
    let source = workspace.write(
        "missing.bh",
        "extern def bhasa_missing_adapter(n: int) -> int\n\
        def main() { print(bhasa_missing_adapter(21)) }",
    );
    for action in ["build", "run"] {
        let output = workspace.run(&[action, as_str(&source)]);
        fails(&output);
        let error = stderr(&output);
        assert!(error.contains("bhasa_missing_adapter"), "{error}");
        assert!(error.contains("--link"), "{error}");
        assert!(error.contains("extern"), "{error}");
        assert!(!error.contains("Install the target toolchain"), "{error}");
    }
}

#[test]
fn available_foreign_backend_emits_an_object() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("cross-object");
    let host = Command::new(&clang).arg("-dumpmachine").output().unwrap();
    succeeds(&host);
    let host = stdout(&host);
    let target = if host.contains("x86_64") && host.contains("linux") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let probe = workspace.write("probe.ll", "define i64 @probe() { ret i64 0 }\n");
    let output = Command::new(&clang)
        .args(["-target", target, "-x", "ir", "-c"])
        .arg(&probe)
        .arg("-o")
        .arg(workspace.file("probe.o"))
        .output()
        .unwrap();
    if !output.status.success() {
        let error = stderr(&output).to_lowercase();
        if error.contains("no available targets")
            || error.contains("unknown target triple")
            || error.contains("unable to create target")
        {
            eprintln!("SKIP foreign object test: installed Clang lacks {target}: {error}");
            return;
        }
        panic!(
            "foreign backend probe failed unexpectedly: {}",
            describe(&output)
        );
    }
    let object = workspace.file("foreign object.o");
    let source = fixture("tests/fixtures/library.bh");
    let output = workspace.run(&[
        "build",
        as_str(&source),
        "--target",
        target,
        "--emit",
        "object",
        "-o",
        as_str(&object),
    ]);
    succeeds(&output);
    let bytes = fs::read(&object).unwrap();
    assert!(bytes.len() >= 20, "foreign object is truncated");
    assert_eq!(&bytes[..4], b"\x7fELF", "Linux target did not emit ELF");
    assert_eq!(bytes[4], 2, "expected a 64-bit ELF object");
    assert_eq!(bytes[5], 1, "expected a little-endian ELF object");
    assert_eq!(
        u16::from_le_bytes([bytes[16], bytes[17]]),
        1,
        "expected a relocatable object"
    );
    assert_eq!(
        u16::from_le_bytes([bytes[18], bytes[19]]),
        if target.starts_with("aarch64") {
            183
        } else {
            62
        }
    );
}
