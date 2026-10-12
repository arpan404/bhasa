//! Bounded acceptance and execution comparisons. No byte-for-byte IR oracle.
mod common;
#[path = "method_acceptance/mod.rs"]
mod method_acceptance;

use common::*;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "BHASA_DIFFERENTIAL_CHILD";

struct Case {
    name: String,
    source: String,
    accepts: Option<bool>,
    entry: Option<PathBuf>,
}

fn case(name: impl Into<String>, source: impl Into<String>, accepts: bool) -> Case {
    Case {
        name: name.into(),
        source: source.into(),
        accepts: Some(accepts),
        entry: None,
    }
}

// Logs go to files so a verbose/crashing subprocess cannot fill a pipe and
// deadlock its parent. These programs never launch subprocesses themselves.
fn bounded(command: &mut Command, workspace: &Workspace, label: &str, seconds: u64) -> Output {
    let out = workspace.file(&format!("{label}.stdout"));
    let err = workspace.file(&format!("{label}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(File::create(&out).unwrap())
        .stderr(File::create(&err).unwrap());
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{command:?}: {error}"));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(seconds) {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{label} exceeded {seconds}s; last compiler input: {}\nstdout: {}\nstderr: {}",
                fs::read_to_string(workspace.file("progress")).unwrap_or_default(),
                fs::read_to_string(out).unwrap_or_default(),
                fs::read_to_string(err).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: fs::read(out).unwrap(),
        stderr: fs::read(err).unwrap(),
    }
}

// Execute compiler calls in a separate process, not a timeout thread: a native
// frontend crash or stack overflow must not take down the entire test runner.
#[test]
fn differential_child() {
    let Some(root) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let root = PathBuf::from(root);
    let frontend = std::env::var("BHASA_DIFFERENTIAL_FRONTEND").unwrap();
    let target = std::env::var("BHASA_DIFFERENTIAL_TARGET").unwrap();
    let count: usize = std::env::var("BHASA_DIFFERENTIAL_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    for index in 0..count {
        let source = fs::read_to_string(root.join(format!("{index}.bh"))).unwrap();
        fs::write(
            root.join("progress"),
            format!("{frontend} case {index}: {source}"),
        )
        .unwrap();
        let entry_path = root.join(format!("{index}.entry"));
        let result = if entry_path.exists() {
            let entry = PathBuf::from(fs::read_to_string(entry_path).unwrap());
            let loaded = bhasa::project::load_unparsed(Some(&entry)).unwrap();
            assert_eq!(bhasa::project::flattened_source(&loaded.files), source);
            #[cfg(feature = "bhasa-lexer")]
            for file in &loaded.files {
                assert_eq!(
                    bhasa::lexer::lex(&file.source),
                    bhasa::lexer::lex_rust(&file.source),
                    "scanner parity: {}",
                    file.path.display()
                );
            }
            match frontend.as_str() {
                "rust" => bhasa::project::load(Some(&entry))
                    .map_err(|errors| {
                        errors
                            .into_iter()
                            .map(|error| {
                                let file = loaded
                                    .files
                                    .iter()
                                    .find(|file| file.path == error.path)
                                    .expect(
                                        "package fixture diagnostic must belong to a loaded source",
                                    );
                                assert_eq!(file.source, error.source);
                                let mut diagnostic = error.diagnostic;
                                // Loader/parser errors are file-local; later phases
                                // report virtual offsets into the combined source.
                                assert!(
                                    diagnostic.span.start <= diagnostic.span.end
                                        && diagnostic.span.end <= file.source.len()
                                );
                                diagnostic.span.start += file.base;
                                diagnostic.span.end += file.base;
                                diagnostic
                            })
                            .collect()
                    })
                    .and_then(|loaded| bhasa::sema::check(loaded.program))
                    .and_then(|checked| bhasa::codegen::emit_llvm(&checked, &target)),
                #[cfg(feature = "bhasa-frontend")]
                "bhasa" => bhasa::compile_files_with_bhasa(&loaded.files, &target),
                _ => panic!("unsupported test frontend: {frontend}"),
            }
        } else {
            #[cfg(feature = "bhasa-lexer")]
            assert_eq!(
                bhasa::lexer::lex(&source),
                bhasa::lexer::lex_rust(&source),
                "scanner parity: {source:?}"
            );
            match frontend.as_str() {
                "rust" => bhasa::compile_to_llvm(&source, &target),
                #[cfg(feature = "bhasa-frontend")]
                "bhasa" => bhasa::compile_with_bhasa(&source, &target),
                _ => panic!("unsupported test frontend: {frontend}"),
            }
        };
        let base = root.join(format!("{frontend}-{index}"));
        match result {
            Ok(ir) => fs::write(base.with_extension("ll"), ir).unwrap(),
            Err(errors) => {
                assert!(!errors.is_empty(), "empty diagnostic list for {source:?}");
                for error in &errors {
                    assert!(
                        error.span.start <= error.span.end && error.span.end <= source.len(),
                        "{error:?}, {source:?}"
                    );
                    assert!(
                        source.is_char_boundary(error.span.start)
                            && source.is_char_boundary(error.span.end),
                        "{error:?}"
                    );
                    assert!(
                        !error.code.starts_with('H'),
                        "frontend integration error: {error:?}"
                    );
                }
                fs::write(base.with_extension("errors"), format!("{errors:?}")).unwrap();
            }
        }
    }
}

fn compile_batch(workspace: &Workspace, cases: &[Case], frontend: &str, target: &str) -> Vec<bool> {
    for (index, input) in cases.iter().enumerate() {
        workspace.write(&format!("{index}.bh"), &input.source);
        if let Some(entry) = &input.entry {
            workspace.write(&format!("{index}.entry"), as_str(entry));
        }
    }
    let output = bounded(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "differential_child", "--nocapture"])
            .env(CHILD_ENV, workspace.path())
            .env("BHASA_DIFFERENTIAL_FRONTEND", frontend)
            .env("BHASA_DIFFERENTIAL_TARGET", target)
            .env("BHASA_DIFFERENTIAL_COUNT", cases.len().to_string()),
        workspace,
        frontend,
        30,
    );
    assert!(
        output.status.success(),
        "{}\n{}",
        describe(&output),
        fs::read_to_string(workspace.file("progress")).unwrap_or_default()
    );
    cases
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let accepted = workspace.file(&format!("{frontend}-{index}.ll")).exists();
            let diagnostic = workspace.file(&format!("{frontend}-{index}.errors"));
            assert!(
                accepted || diagnostic.exists(),
                "missing compiler result for {}",
                input.name
            );
            if let Some(expected) = input.accepts {
                assert_eq!(
                    accepted,
                    expected,
                    "{frontend}: {}\n{}\n{}",
                    input.name,
                    input.source,
                    fs::read_to_string(diagnostic).unwrap_or_default()
                );
            }
            accepted
        })
        .collect()
}

fn both(workspace: &Workspace, cases: &[Case], target: &str) {
    let reference = compile_batch(workspace, cases, "rust", target);
    #[cfg(feature = "bhasa-frontend")]
    {
        let alternate = compile_batch(workspace, cases, "bhasa", target);
        for (index, input) in cases.iter().enumerate() {
            assert_eq!(
                reference[index], alternate[index],
                "acceptance mismatch: {}\n{}",
                input.name, input.source
            );
        }
    }
    #[cfg(not(feature = "bhasa-frontend"))]
    let _ = reference;
}

fn frontends() -> &'static [&'static str] {
    if cfg!(feature = "bhasa-frontend") {
        &["rust", "bhasa"]
    } else {
        &["rust"]
    }
}

fn acceptance_cases() -> Vec<Case> {
    let worker = "def worker(n: int) -> int { if n == 0 { return 0 }; return worker(n - 1) + 1 }\n";
    let mut cases = Vec::new();
    for (name, body, valid) in [
        (
            "branch-local",
            "if true { var j = spawn(worker(2)); var n = join(j); print(n) } else { var j = spawn(worker(3)); join(j) }",
            true,
        ),
        (
            "loop-local",
            "var i = 0; while i < 3 { var j = spawn(worker(i)); var n = join(j); i = i + 1 }",
            true,
        ),
        (
            "joined-before-return",
            "var j = spawn(worker(2)); join(j); if true { return }",
            true,
        ),
        (
            "outer-join-branches",
            "var j = spawn(worker(2)); if true { join(j) } else { join(j) }",
            false,
        ),
        (
            "outer-join-loop",
            "var j = spawn(worker(2)); while false { join(j) }",
            false,
        ),
        (
            "nested-return-leak",
            "var j = spawn(worker(2)); while false { if false { return } }; join(j)",
            false,
        ),
        (
            "joined-handle-argument",
            "var j = spawn(worker(2)); join(j); worker(j)",
            false,
        ),
        (
            "join-in-condition",
            "var j = spawn(worker(2)); if join(j) == 2 { print(1) }",
            false,
        ),
        (
            "nested-spawn",
            "var j = spawn(worker(spawn(worker(1)))); join(j)",
            false,
        ),
        (
            "loop-return-leak",
            "while true { var j = spawn(worker(2)); return }",
            false,
        ),
        (
            "branch-return-leak",
            "if true { var j = spawn(worker(2)); return }",
            false,
        ),
        (
            "shadow-after-join",
            "var j = spawn(worker(2)); join(j); if true { var j = 1 }",
            false,
        ),
    ] {
        cases.push(case(
            name,
            format!("{worker}def main() {{ {body} }}"),
            valid,
        ));
    }
    for foreign in ["hidden", "printf", "bhasa_rt_buffer_free", "print", "main"] {
        cases.push(case(
            format!("extern-{foreign}"),
            format!("extern def {foreign}(n: int) -> int\n"),
            matches!(foreign, "hidden" | "printf"),
        ));
    }
    for (name, source, valid) in [
        (
            "printf-collision",
            "extern def printf(n: int) -> int\ndef main() { print(1) }",
            false,
        ),
        (
            "mangled-printf",
            "def printf(n: int) -> int { return n }; def main() { print(printf(7)) }",
            true,
        ),
        (
            "mutual-worker-extern",
            "extern def hidden(n: int) -> int\ndef a(n: int) -> int { if n == 0 { return hidden(n) }; return b(n - 1) }\ndef b(n: int) -> int { return a(n) }\ndef main() { var j = spawn(b(1)); join(j) }",
            false,
        ),
        (
            "owned-parameter-rebind",
            "def change(x: buffer) { x = buffer() }",
            false,
        ),
        (
            "owned-parameter-return",
            "def copy(x: buffer) -> buffer { return x }",
            true,
        ),
        (
            "buffer-alias",
            "def change(a: buffer, b: buffer) { append(a,b) }; def main() { var b = buffer(\"x\"); change(b,b) }",
            true,
        ),
        (
            "ignored-status",
            "def main() { write_stdout(buffer(\"x\")) }",
            false,
        ),
        (
            "status-temporary",
            "def main() { var status = write_stdout(buffer(\"x\")) }",
            true,
        ),
        (
            "max-literal",
            "def main() { print(9223372036854775807) }",
            true,
        ),
        (
            "min-literal",
            "def main() { print(-0009223372036854775808) }",
            true,
        ),
        (
            "positive-range",
            "def main() { print(9223372036854775808) }",
            false,
        ),
        (
            "negative-range",
            "def main() { print(-9223372036854775809) }",
            false,
        ),
        ("return-line", "def f() -> int { return\n1 }", false),
        (
            "missing-else-return",
            "def f(n: bool) -> int { if n { return 1 } }",
            false,
        ),
        (
            "unreachable",
            "def f() -> int { return 1; print(2) }",
            false,
        ),
        (
            "short-circuit-trap",
            "def main() { print(false and 1 / 0 == 1); print(true or 1 / 0 == 1) }",
            true,
        ),
    ] {
        cases.push(case(name, source, valid));
    }
    // Fixed seed and bounded operands keep every generated expression defined.
    let mut seed = 0x9e37_79b9_u32;
    for index in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = i64::from(seed % 201) - 100;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let b = i64::from(seed % 31) + 1;
        let expression = match index % 5 {
            0 => format!("({a} + {b}) * 3 - {a}"),
            1 => format!("({a} / {b}) + ({a} % {b})"),
            2 => format!("not ({a} < {b}) or ({a} == {b})"),
            3 => format!("({a} <= {b}) and ({a} != {b})"),
            _ => format!("-({a}) + {b}"),
        };
        cases.push(case(
            format!("generated-{index}"),
            format!("def main() {{ print({expression}) }}"),
            true,
        ));
    }
    cases
}

#[test]
fn bounded_generated_acceptance_matches() {
    let workspace = Workspace::new("differential-acceptance");
    both(&workspace, &acceptance_cases(), "x86_64-unknown-linux-gnu");
}

#[test]
fn malformed_source_never_crashes_and_acceptance_matches() {
    let mut cases = Vec::new();
    for (index, source) in [
        "\0",
        "é",
        "def main( {",
        "def main() { print(\"\\q\") }",
        "def main() { var x = 1abc }",
        "def main() { print(\"unterminated)",
        "def main() { print(\"raw\nline\") }",
        "def main() { var j = spawn((",
        "def main() { var b = buffer(); set(b,) }",
        "extern def",
        "def main() { return & }",
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(case(format!("malformed-{index}"), source, false));
    }
    // Removing one character can produce either a valid program or a syntax
    // error. Compare acceptance, rather than assuming every mutation is invalid.
    for (seed_index, seed) in [
        "def twice(n: int) -> int { return n * 2 }\ndef main() { print(twice(21)) }",
        "def main() { var b = buffer(\"α\"); if len(b) > 0 { print(b) } }",
        "def f(n: int) -> int { return n }; def main() { var j = spawn(f(1)); var n = join(j) }",
        "struct Box { value: int }\ndef main() { var b = Box(1); print(b.value) }",
    ]
    .into_iter()
    .enumerate()
    {
        for (offset, character) in seed.char_indices() {
            let mut source = seed.to_owned();
            source.replace_range(offset..offset + character.len_utf8(), "");
            cases.push(Case {
                name: format!("delete-{seed_index}-{offset}"),
                source,
                accepts: None,
                entry: None,
            });
        }
    }
    let workspace = Workspace::new("differential-malformed");
    both(&workspace, &cases, "x86_64-unknown-linux-gnu");
}

#[test]
fn nesting_and_flat_expression_limits_match() {
    let workspace = Workspace::new("differential-depth");
    let grouped = case(
        "grouped-depth",
        format!(
            "def main() {{ print({}1{}) }}",
            "(".repeat(300),
            ")".repeat(300)
        ),
        false,
    );
    let flat = case(
        "flat-depth",
        format!("def main() {{ print({}) }}", vec!["1"; 256].join(" + ")),
        false,
    );
    let cases = [grouped, flat];
    // The Bhasa emitter now tracks the same expression-depth limit as Rust.
    // Keep both outcomes strict; do not blanket-ignore nesting diagnostics.
    both(&workspace, &cases, "x86_64-unknown-linux-gnu");
}

#[test]
fn package_loaded_worker_graphs_have_the_same_acceptance() {
    let workspace = Workspace::new("differential-packages");
    let mut cases = Vec::new();
    for (name, base, accepted) in [
        (
            "local-dependency",
            "def base(n: int) -> int { return n * 2 }\n",
            true,
        ),
        (
            "transitive-extern-worker",
            "extern def foreign(n: int) -> int\ndef base(n: int) -> int { return foreign(n) }\n",
            false,
        ),
    ] {
        // Each snapshot needs its own files: child compilers reload the actual
        // graph after both cases have been assembled.
        for directory in ["app/src", "worker/src", "base/src"] {
            fs::create_dir_all(workspace.file(&format!("{name}/{directory}"))).unwrap();
        }
        for (package, dependencies) in [
            ("app", "[dependencies]\nworker = \"../worker\"\n"),
            ("worker", "[dependencies]\nbase = \"../base\"\n"),
            ("base", ""),
        ] {
            workspace.write(&format!("{name}/{package}/bhasa.toml"), &format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n{dependencies}"));
        }
        workspace.write(
            &format!("{name}/app/src/main.bh"),
            "import \"@worker\"\ndef main() { var j = spawn(worker(4)); var n = join(j); print(n) }\n",
        );
        workspace.write(
            &format!("{name}/worker/src/main.bh"),
            "import \"@base\"\ndef worker(n: int) -> int { return base(n) }\n",
        );
        workspace.write(&format!("{name}/base/src/main.bh"), base);
        let entry = workspace.file(&format!("{name}/app/src/main.bh"));
        let loaded = bhasa::project::load_unparsed(Some(&entry)).unwrap();
        let source = bhasa::project::flattened_source(&loaded.files);
        assert!(!source.contains("import"));
        let mut input = case(name, source, accepted);
        input.entry = Some(entry);
        cases.push(input);
    }
    both(&workspace, &cases, "x86_64-unknown-linux-gnu");
}

const RECORD_VALUES: &str = r#"
struct Pair { left: Leaf; right: Leaf }
struct Leaf { text: buffer; number: int }
def copy(p: Pair) -> Leaf { return p.right }
def replace(p: Pair) -> buffer {
    p.left = Leaf(buffer("new"), 3)
    return buffer("rhs")
}
def mutate(leaf: Leaf) -> Leaf {
    leaf.text = buffer("mutated")
    return leaf
}
def main() {
    var p = Pair(Leaf(buffer("a"), 1), Leaf(buffer("b"), 2))
    var copied = copy(p)
    append(copied.text, "!")
    p.left.text = replace(p)
    print(p.left.text); print(p.left.number)
    print(p.right.text); print(copied.text)
    var snapshot = p
    p.right.text = buffer("changed")
    print(snapshot.right.text)
    p.left = p.left
    print(p.left.text)
    print(Pair(Leaf(buffer("temp"), 4), Leaf(buffer("other"), 5)).left.text)
    var seed = Leaf(buffer("old"), 7)
    var captured = Pair(seed, mutate(seed))
    print(captured.left.text); print(captured.right.text)
}
"#;

#[test]
fn record_acceptance_matches() {
    let mut cases = vec![case("record-value-semantics", RECORD_VALUES, true)];
    for (name, source, accepted) in [
        (
            "record-cycle",
            "struct A { b: B }\nstruct B { a: A }",
            false,
        ),
        (
            "record-duplicate-field",
            "struct A { b: int; b: bool }",
            false,
        ),
        (
            "record-unknown-field-type",
            "struct A { b: Missing }",
            false,
        ),
        (
            "record-nominal-type",
            "struct A { n: int }\nstruct B { n: int }\ndef a() -> A { return B(1) }",
            false,
        ),
        (
            "record-temporary-assignment",
            "struct A { n: int }\ndef main() { A(1).n = 2 }",
            false,
        ),
        (
            "record-empty",
            "struct A {}\ndef main() { var a = A(); a = a }",
            true,
        ),
        (
            "record-extern-allocator",
            "struct A { n: int }\nextern def malloc(n: int) -> int\ndef main() { var a = A(1) }",
            false,
        ),
        (
            "record-extern-release",
            "struct A { n: int }\nextern def free(n: int)\ndef main() { var a = A(1) }",
            false,
        ),
        (
            "record-parent-child-borrow",
            "struct A { text: buffer }\ndef consume(a: A, text: buffer) {}\ndef main() { var a = A(buffer()); consume(a, a.text) }",
            false,
        ),
        (
            "record-later-invalidation",
            "struct A { text: buffer }\ndef change(a: A) -> int { a.text = buffer(); return 0 }\ndef consume(text: buffer, n: int) {}\ndef main() { var a = A(buffer()); consume(a.text, change(a)) }",
            false,
        ),
        (
            "record-nested-read",
            "struct A { text: buffer }\ndef consume(a: A, n: int) {}\ndef main() { var a = A(buffer()); consume(a, len(a.text)) }",
            true,
        ),
        (
            "record-nested-copy",
            "struct A { text: buffer }\nstruct B { a: A }\ndef consume(a: A, copy: B) {}\ndef main() { var a = A(buffer()); consume(a, B(a)) }",
            true,
        ),
    ] {
        cases.push(case(name, source, accepted));
    }
    let workspace = Workspace::new("differential-records");
    both(&workspace, &cases, "x86_64-unknown-linux-gnu");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn record_values_match_at_o0_and_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("differential-record-values");
    let target = native_target(&clang, &workspace);
    both(
        &workspace,
        &[case("record-value-semantics", RECORD_VALUES, true)],
        &target,
    );
    for frontend in frontends() {
        for optimization in ["-O0", "-O2"] {
            let binary = link(
                &clang,
                &workspace,
                &workspace.file(&format!("{frontend}-0.ll")),
                optimization,
            );
            let output = bounded(
                &mut Command::new(binary),
                &workspace,
                &format!("records-{frontend}-{optimization}"),
                30,
            );
            succeeds(&output);
            assert_eq!(
                stdout(&output),
                "rhs\n3\nb\nb!\nb\nrhs\ntemp\nold\nmutated\n",
                "{frontend}, {optimization}"
            );
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_target(clang: &std::ffi::OsStr, workspace: &Workspace) -> String {
    let probe = workspace.write("empty.c", "");
    let output = bounded(
        Command::new(clang)
            .args(["-S", "-emit-llvm", "-x", "c"])
            .arg(probe)
            .args(["-o", "-"]),
        workspace,
        "target",
        15,
    );
    succeeds(&output);
    stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("target triple = \"")?.strip_suffix('"'))
        .unwrap()
        .to_owned()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn link(clang: &std::ffi::OsStr, workspace: &Workspace, ir: &Path, optimization: &str) -> PathBuf {
    let binary = workspace.file("program");
    // Clang needs this attribute to instrument existing LLVM loads/stores.
    // The command-line sanitizer flags also instrument the C runtime.
    let instrumented = fs::read_to_string(ir)
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("define ") {
                let definition = line.strip_suffix(" {").expect("LLVM function header");
                format!("{definition} sanitize_address {{\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    let ir = workspace.write("instrumented.ll", &instrumented);
    let output = bounded(
        Command::new(clang)
            .args([
                optimization,
                "-fsanitize=address,undefined",
                "-fno-sanitize-recover=all",
                "-fno-omit-frame-pointer",
                "-pthread",
            ])
            .arg(ir)
            .arg(fixture("runtime/core.c"))
            .arg(fixture("runtime/concurrency.c"))
            .arg("-o")
            .arg(&binary),
        workspace,
        "link",
        30,
    );
    succeeds(&output);
    binary
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn generated_native_results_match_at_o0_and_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("differential-execution");
    let target = native_target(&clang, &workspace);
    let mut source = String::from("def main() {\n");
    let mut expected = String::new();
    let mut seed = 0x9e37_79b9_u32;
    for _ in 0..32 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = i64::from(seed % 201) - 100;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let b = i64::from(seed % 31) + 1;
        source.push_str(&format!("print(({a} / {b}) + ({a} % {b}));\n"));
        expected.push_str(&format!("{}\n", a / b + a % b));
    }
    source.push_str("print(false and 1 / 0 == 1); print(true or 1 / 0 == 1); print(-9223372036854775808 / 1); }\n");
    expected.push_str("false\ntrue\n-9223372036854775808\n");
    let cases = [
        case("generated-output", source, true),
        case(
            "owned-and-recursive-tasks",
            r#"
def touch(a: buffer, b: buffer) -> buffer { append(a,b); return b }
def transient(x: buffer) -> buffer { append(x,"!"); return x }
def worker(n: int) -> int {
    if n == 0 { return 1 }
    var local = to_buffer(n)
    var child = spawn(worker(n - 1))
    var result = join(child)
    return result + len(local)
}
def main() {
    var value = buffer("x")
    var returned = touch(value,value)
    push(returned,63)
    print(value); print(returned)
    print(touch(buffer("a"), transient(buffer("b"))))
    var copy = value; append(value,value); value = value
    print(value); print(copy)
    append(copy, transient(copy)); print(copy)
    var i = 0
    while i < 4 {
        if i % 2 == 0 { var j = spawn(worker(i)); var n = join(j); print(n) }
        else { var j = spawn(worker(i)); var n = join(j); print(n) }
        var temp = transient(slice(value,0,2)); i = i + 1
    }
}
"#,
            true,
        ),
    ];
    both(&workspace, &cases, &target);
    let expected = [
        expected.as_str(),
        "xx\nxx?\nb!\nxxxx\nxx\nxx!xx!\n1\n2\n3\n4\n",
    ];
    for frontend in frontends() {
        for (index, expected) in expected.iter().enumerate() {
            for optimization in ["-O0", "-O2"] {
                let binary = link(
                    &clang,
                    &workspace,
                    &workspace.file(&format!("{frontend}-{index}.ll")),
                    optimization,
                );
                let output = bounded(
                    Command::new(binary).current_dir(workspace.path()),
                    &workspace,
                    &format!("execute-{frontend}-{index}-{optimization}"),
                    30,
                );
                succeeds(&output);
                assert_eq!(
                    stdout(&output),
                    *expected,
                    "{frontend}, {optimization}, {}",
                    cases[index].name
                );
                assert!(output.stderr.is_empty(), "{}", describe(&output));
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn arithmetic_traps_stop_both_frontends_at_o0_and_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("differential-traps");
    let target = native_target(&clang, &workspace);
    let cases: Vec<_> = [
        "9223372036854775807 + 1",
        "-9223372036854775808 - 1",
        "9223372036854775807 * 2",
        "-(-9223372036854775808)",
        "1 / 0",
        "1 % 0",
        "-9223372036854775808 / -1",
        "-9223372036854775808 % -1",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, expression)| {
        case(
            format!("trap-{index}"),
            format!("def main() {{ print({expression}); print(\"survived\") }}"),
            true,
        )
    })
    .collect();
    both(&workspace, &cases, &target);
    for frontend in frontends() {
        for (index, input) in cases.iter().enumerate() {
            for optimization in ["-O0", "-O2"] {
                let binary = link(
                    &clang,
                    &workspace,
                    &workspace.file(&format!("{frontend}-{index}.ll")),
                    optimization,
                );
                let output = bounded(
                    &mut Command::new(binary),
                    &workspace,
                    &format!("trap-{frontend}-{index}-{optimization}"),
                    30,
                );
                assert!(
                    !output.status.success(),
                    "{}: {frontend}, {optimization}",
                    input.name
                );
                assert!(
                    !stdout(&output).contains("survived"),
                    "{}",
                    describe(&output)
                );
            }
        }
    }
}
