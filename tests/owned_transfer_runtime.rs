#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use common::*;
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
    assert!(
        !timed_out,
        "owned transfer fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

#[test]
fn owned_transfer_copies_payloads_and_all_exit_paths_have_exact_cleanup() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("owned-transfer-runtime");
    let mut runtime = include_str!("../runtime/core.c").to_owned();
    // Instrument the current production runtime, so this fixture cannot silently
    // keep testing an old copied implementation after the runtime changes.
    for (signature, counter) in [
        ("bhasa_buffer *bhasa_rt_buffer_new(void) {", "allocations"),
        (
            "bhasa_buffer *bhasa_rt_buffer_clone(const bhasa_buffer *value) {",
            "copies",
        ),
        ("void bhasa_rt_buffer_free(bhasa_buffer *value) {", "drops"),
        ("bhasa_ints *bhasa_rt_ints_new(void) {", "allocations"),
        (
            "bhasa_ints *bhasa_rt_ints_clone(const bhasa_ints *value) {",
            "copies",
        ),
        ("void bhasa_rt_ints_free(bhasa_ints *value) {", "drops"),
    ] {
        assert_eq!(runtime.matches(signature).count(), 1, "{signature}");
        runtime = runtime.replace(signature, &format!("{signature}\n ++{counter};"));
    }
    let runtime = workspace.write(
        "tracked-core.c",
        &format!("{}{}", include_str!("fixtures/owned/counters.h"), runtime),
    );
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    for (name, expected_stdout, counts) in [
        ("transfer_counts", "transfer\nreset\n", (7, 0, 7)),
        ("early_counts", "branch\nfallthrough\n", (3, 0, 3)),
        ("try_counts", "7\nok\n", (7, 4, 7)),
        ("copy_counts", "copy!\nCopy\ncopy!?\n4\n9\n", (5, 3, 5)),
        ("payload_counts", "payload\n0\n", (4, 2, 4)),
    ] {
        let source =
            std::fs::read_to_string(fixture(&format!("tests/fixtures/owned/{name}.bh"))).unwrap();
        let mut outputs = vec![("rust", bhasa::compile_to_llvm(&source, &target).unwrap())];
        #[cfg(feature = "bhasa-frontend")]
        outputs.push((
            "bhasa",
            bhasa::compile_with_bhasa(&source, &target).unwrap(),
        ));
        for (frontend, ir) in outputs.drain(..) {
            // Aggregate nodes allocate in LLVM rather than runtime/core.c.
            // Include them in the same live allocation ledger.
            let ir = ir
                .replace("@malloc(", "@bhasa_owned_test_malloc(")
                .replace("@free(", "@bhasa_owned_test_free(");
            // Clang's sanitizer flag alone does not mark existing LLVM function
            // definitions for ASan. Instrument guest functions as well as C.
            let ir: String = ir
                .lines()
                .map(|line| {
                    if line.starts_with("define ") && line.ends_with(" {") {
                        format!("{} sanitize_address {{\n", &line[..line.len() - 2])
                    } else {
                        format!("{line}\n")
                    }
                })
                .collect();
            assert!(ir
                .lines()
                .filter(|line| line.starts_with("define "))
                .all(|line| line.contains(" sanitize_address ")));
            let ir = workspace.write("guest.ll", &ir);
            for optimization in ["-O0", "-O2"] {
                let executable = workspace.file("owned-guest");
                succeeds(&bounded(
                    Command::new(&clang)
                        .args([
                            optimization,
                            "-fsanitize=address,undefined",
                            "-fno-sanitize-recover=all",
                            "-fno-omit-frame-pointer",
                        ])
                        .arg(&ir)
                        .arg(&runtime)
                        .arg("-I")
                        .arg(fixture("runtime"))
                        .arg("-o")
                        .arg(&executable),
                ));
                let output = bounded(
                    Command::new(&executable)
                        .env("ASAN_OPTIONS", "detect_leaks=0")
                        .env("UBSAN_OPTIONS", "halt_on_error=1"),
                );
                succeeds(&output);
                assert_eq!(
                    stdout(&output),
                    expected_stdout,
                    "{frontend}/{name}/{optimization}"
                );
                assert_eq!(
                    stderr(&output),
                    format!(
                        "COUNTERS allocations={} copies={} drops={} live=0\n",
                        counts.0, counts.1, counts.2
                    ),
                    "{frontend}/{name}/{optimization}"
                );
            }
        }
    }
}
