#![allow(dead_code)]

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub struct Workspace(PathBuf);

impl Workspace {
    pub fn new(label: &str) -> Self {
        for _ in 0..100 {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bhasa-tests-{}-{sequence}-{label}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create {}: {error}", path.display()),
            }
        }
        panic!("could not allocate a fresh test directory");
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.file(name);
        fs::write(&path, contents).unwrap();
        path
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bhasa"));
        command.current_dir(self.path());
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

pub fn as_str(path: &Path) -> &str {
    path.to_str().expect("test path must be UTF-8")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout must be UTF-8")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr must be UTF-8")
}

pub fn describe(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    )
}

pub fn succeeds(output: &Output) {
    assert!(output.status.success(), "{}", describe(output));
}

pub fn fails(output: &Output) {
    assert!(!output.status.success(), "{}", describe(output));
    assert!(
        !stderr(output).contains("panicked at"),
        "compiler panicked: {}",
        describe(output)
    );
}

/// Only an absent executable permits a native-test skip. A broken installation
/// or a failed compilation remains a test failure.
pub fn clang() -> Option<OsString> {
    let executable = std::env::var_os("BHASA_CLANG").unwrap_or_else(|| "clang".into());
    match Command::new(&executable).arg("--version").output() {
        Ok(output) => {
            succeeds(&output);
            Some(executable)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!("SKIP native test: Clang executable {executable:?} was not found");
            None
        }
        Err(error) => panic!("cannot execute Clang {executable:?}: {error}"),
    }
}

pub fn build(workspace: &Workspace, source: &Path, release: bool) -> PathBuf {
    let executable = workspace.file(if cfg!(windows) {
        "program.exe"
    } else {
        "program"
    });
    let mut command = workspace.command();
    command.arg("build").arg(source).arg("-o").arg(&executable);
    if release {
        command.arg("--release");
    }
    let output = command.output().unwrap();
    succeeds(&output);
    assert!(executable.is_file(), "build did not create its output");
    executable
}

pub fn execute(executable: &Path, workspace: &Workspace) -> Output {
    Command::new(executable)
        .current_dir(workspace.path())
        .output()
        .unwrap_or_else(|error| panic!("cannot execute {}: {error}", executable.display()))
}
