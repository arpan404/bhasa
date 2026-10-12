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
    assert!(
        !timed_out,
        "storage acceptance deadline: {}",
        describe(&output)
    );
    output
}
fn source() -> String {
    let fixture_function = |path: &str, name: &str| {
        fs::read_to_string(fixture(path))
            .unwrap()
            .strip_prefix("import \"@std\"\n")
            .unwrap()
            .replacen("def main()->int", &format!("def {name}()->int"), 1)
    };
    format!(
        "import \"@std\"\n{}\n{}\n{}",
        fixture_function(
            "tests/fixtures/text/private-storage.bh",
            "verify_text_storage"
        ),
        fixture_function(
            "tests/fixtures/storage_invariants.bh",
            "verify_collection_storage"
        ),
        include_str!("fixtures/storage/ownership-exits.bh")
    )
}

#[test]
fn private_storage_snapshots_and_early_exits_release_every_owned_allocation() {
    let Some(clang) = clang() else { return };
    let modes: &[&str] = if cfg!(feature = "bhasa-frontend") {
        &["rust", "bhasa"]
    } else {
        &["rust"]
    };
    for frontend in modes {
        let workspace = Workspace::new("private-storage-ownership");
        fs::create_dir(workspace.file("std")).unwrap();
        for entry in fs::read_dir(fixture("std")).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                fs::copy(entry.path(), workspace.file("std").join(entry.file_name())).unwrap();
            }
        }
        workspace.write("bhasa.toml", "[package]\nname=\"storage-ownership\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[dependencies]\nstd=\"std\"\n");
        workspace.write("main.bh", &source());
        let ir_path = workspace.file("storage.ll");
        succeeds(&bounded(
            workspace
                .command()
                .args(["emit-llvm", "main.bh", "--frontend", frontend, "-o"])
                .arg(&ir_path),
        ));
        let original = fs::read_to_string(&ir_path).unwrap();
        let renamed = original
            .replace("@malloc(", "@storage_malloc(")
            .replace("@calloc(", "@storage_calloc(")
            .replace("@realloc(", "@storage_realloc(")
            .replace("@free(", "@storage_free(");
        let instrumented: String = renamed
            .lines()
            .map(|line| {
                if line.starts_with("define ") {
                    format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
                } else {
                    format!("{line}\n")
                }
            })
            .collect();
        fs::write(&ir_path, instrumented).unwrap();
        let mut runtimes = vec![workspace.write(
            "tracker.c",
            include_str!("fixtures/storage/ownership-tracker.c"),
        )];
        for (marker, name) in [
            ("core", "core"),
            ("text", "text"),
            ("net", "net"),
            ("collections", "collections"),
            ("threads", "concurrency"),
            ("file", "file"),
        ] {
            if !original.contains(&format!("; bhasa runtime: {marker}")) {
                continue;
            }
            let wrapper = format!(
                r#"
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <stdlib.h>
void *storage_malloc(size_t);
void *storage_calloc(size_t, size_t);
void *storage_realloc(void *, size_t);
void storage_free(void *);
#define malloc storage_malloc
#define calloc storage_calloc
#define realloc storage_realloc
#define free storage_free
#include "{}"
"#,
                fixture(&format!("runtime/{name}.c")).display()
            );
            runtimes.push(workspace.write(&format!("tracked-{name}.c"), &wrapper));
        }
        for optimization in ["-O0", "-O2"] {
            let executable = workspace.file("sanitized-storage");
            succeeds(&bounded(
                Command::new(&clang)
                    .args([
                        "-std=c11",
                        optimization,
                        "-fsanitize=address,undefined",
                        "-fno-sanitize-recover=all",
                        "-fno-omit-frame-pointer",
                        "-pthread",
                    ])
                    .arg(&ir_path)
                    .args(&runtimes)
                    .arg("-o")
                    .arg(&executable),
            ));
            let output = bounded(
                Command::new(&executable)
                    .current_dir(workspace.path())
                    .env("UBSAN_OPTIONS", "halt_on_error=1")
                    .env("ASAN_OPTIONS", "detect_leaks=0:halt_on_error=1"),
            );
            succeeds(&output);
            assert_eq!(stdout(&output), "private Text storage passed\nprivate collection and JSON storage passed\nprivate storage ownership passed\n", "{frontend} {optimization}");
            assert!(
                output.stderr.is_empty(),
                "{frontend} {optimization}: {}",
                describe(&output)
            );
        }
    }
}
