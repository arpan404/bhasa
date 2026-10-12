#[cfg(unix)]
mod common;

#[cfg(unix)]
use common::*;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::process::{Command, Stdio};

#[test]
#[cfg(unix)]
fn runtime_ownership_io_and_process_boundaries() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("runtime-core");
    for optimize in ["-O0", "-O2"] {
        let executable = workspace.file("runtime core checks");
        let object = workspace.file("runtime.o");
        let flags = [
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer",
            "-pthread",
            optimize,
        ];
        let mut compile = Command::new(&clang);
        compile.args(flags).args([
            "-Dmalloc=bhasa_test_malloc",
            "-Drealloc=bhasa_test_realloc",
            "-Dfree=bhasa_test_free",
            "-Dfopen=bhasa_test_pipe_fopen",
            "-c",
        ]);
        #[cfg(target_os = "macos")]
        compile.args([
            "-Dposix_spawnattr_init=bhasa_test_spawnattr_init",
            "-Dposix_spawnattr_setflags=bhasa_test_spawnattr_setflags",
            "-Dposix_spawnattr_destroy=bhasa_test_spawnattr_destroy",
            "-Dposix_spawn_file_actions_init=bhasa_test_spawn_actions_init",
            "-Dposix_spawn_file_actions_addinherit_np=bhasa_test_spawn_actions_inherit",
            "-Dposix_spawn_file_actions_destroy=bhasa_test_spawn_actions_destroy",
        ]);
        let output = compile
            .arg(fixture("runtime/core.c"))
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        succeeds(&output);
        let output = Command::new(&clang)
            .args(flags)
            .arg("-I")
            .arg(fixture("runtime"))
            .arg(&object)
            .arg(fixture("tests/runtime_core.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);
        let output = Command::new(&executable)
            .arg("values")
            .current_dir(workspace.path())
            .output()
            .unwrap();
        succeeds(&output);
        assert!(output.stderr.is_empty(), "{}", describe(&output));

        for mode in [
            "pipe-stdout",
            "pipe-stderr",
            "pipe-file",
            "pipe-large",
            "pipe-pending",
            "pipe-blocked",
            "pipe-ignored",
        ] {
            let output = Command::new(&executable)
                .arg(mode)
                .current_dir(workspace.path())
                .output()
                .unwrap();
            succeeds(&output);
            assert!(output.stderr.is_empty(), "{mode}: {}", describe(&output));
            assert!(output.stdout.is_empty(), "{mode}: {}", describe(&output));
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new(&executable)
                .arg("spawn-closed-stdio")
                .current_dir(workspace.path())
                .output()
                .unwrap();
            succeeds(&output);
            assert!(output.stdout.is_empty() && output.stderr.is_empty());
        }

        for mode in ["stdio", "spawn-stdio"] {
            let mut process = Command::new(&executable)
                .arg(mode)
                .env("BHASA_CORE_TEST_ENV", "inherited value")
                .current_dir(workspace.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let bytes = b"binary\0input\n\xff";
            process.stdin.take().unwrap().write_all(bytes).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, [bytes.as_slice(), bytes, b"\n"].concat());
            assert_eq!(output.stderr, bytes);
        }

        for (mode, expected) in [
            ("buffer-empty-index", "index out of bounds"),
            ("buffer-negative-index", "index out of bounds"),
            ("byte-too-large", "byte must be in 0..255"),
            ("byte-negative", "byte must be in 0..255"),
            ("ints-empty-index", "index out of bounds"),
            ("slice-overflow", "slice out of bounds"),
            ("slice-negative", "slice out of bounds"),
            ("argument-index", "index out of bounds"),
            ("argument-nul", "process argument contains NUL"),
            ("capacity-overflow", "capacity overflow"),
            ("ints-capacity-overflow", "capacity overflow"),
            ("allocation-failure", "allocation failed"),
            ("growth-allocation-failure", "allocation failed"),
        ] {
            let output = Command::new(&executable)
                .arg(mode)
                .env("ASAN_OPTIONS", "detect_leaks=0")
                .current_dir(workspace.path())
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
            assert_eq!(
                stderr(&output),
                format!("Bhasa runtime error: {expected}\n"),
                "{mode}: {}",
                describe(&output)
            );
        }
    }
}
