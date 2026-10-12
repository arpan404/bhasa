mod common;
use common::*;
use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn check_bounded(workspace: &Workspace) -> std::process::Output {
    let mut child = workspace
        .command()
        .args(["check", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("package loading blocked for ten seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn manifest_rejects_directories_and_oversized_files() {
    let workspace = Workspace::new("manifest-limits");
    let path = workspace.file("bhasa.toml");
    fs::create_dir(&path).unwrap();
    let output = check_bounded(&workspace);
    fails(&output);
    assert!(
        stdout(&output).contains("regular UTF-8 file"),
        "{}",
        describe(&output)
    );
    fs::remove_dir(&path).unwrap();
    fs::File::create(&path)
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    let output = check_bounded(&workspace);
    fails(&output);
    assert!(
        stdout(&output).contains("size limit"),
        "{}",
        describe(&output)
    );
}

#[cfg(unix)]
#[test]
fn manifest_and_module_fifos_are_rejected_without_opening_them() {
    let workspace = Workspace::new("fifo-inputs");
    let manifest = workspace.file("bhasa.toml");
    succeeds(
        &std::process::Command::new("mkfifo")
            .arg(&manifest)
            .output()
            .unwrap(),
    );
    let output = check_bounded(&workspace);
    fails(&output);
    assert!(
        stdout(&output).contains("regular UTF-8 file"),
        "{}",
        describe(&output)
    );
    fs::remove_file(&manifest).unwrap();
    fs::write(
        &manifest,
        "[package]\nname = \"fifo\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n",
    )
    .unwrap();
    succeeds(
        &std::process::Command::new("mkfifo")
            .arg(workspace.file("main.bh"))
            .output()
            .unwrap(),
    );
    let output = check_bounded(&workspace);
    fails(&output);
    assert!(
        stdout(&output).contains("regular UTF-8 file"),
        "{}",
        describe(&output)
    );
}

#[test]
fn module_size_limit_is_checked_before_loading_contents() {
    let workspace = Workspace::new("module-size-limit");
    workspace.write(
        "bhasa.toml",
        "[package]\nname = \"large\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n",
    );
    fs::File::create(workspace.file("main.bh"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let output = check_bounded(&workspace);
    fails(&output);
    assert!(
        stdout(&output).contains("size limit"),
        "{}",
        describe(&output)
    );
}
