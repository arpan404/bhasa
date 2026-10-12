mod common;
use common::*;
use std::fs;
use std::path::Path;

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn manifest(root: &Path, name: &str, entry: &str, extra: &str) {
    write(
        root,
        "bhasa.toml",
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"{entry}\"\n{extra}"),
    );
}

#[test]
fn new_scaffolds_without_overwriting_and_projects_work_from_subdirectories() {
    let workspace = Workspace::new("new-project");
    succeeds(&workspace.run(&["new", "hello-world"]));
    let root = workspace.file("hello-world");
    assert!(root.join("bhasa.toml").is_file());
    assert!(root.join("src/main.bh").is_file());
    fails(&workspace.run(&["new", "hello-world"]));
    for name in ["../escape", "/absolute", "bad name", "-bad"] {
        fails(&workspace.run(&["new", name]));
    }
    let output = workspace
        .command()
        .current_dir(root.join("src"))
        .args(["check", "--json"])
        .output()
        .unwrap();
    succeeds(&output);
    assert_eq!(stdout(&output), "[]\n");
    if clang().is_some() {
        let output = workspace
            .command()
            .current_dir(&root)
            .arg("run")
            .output()
            .unwrap();
        succeeds(&output);
        assert_eq!(stdout(&output), "Hello from Bhasa!\n");
        let extension = if cfg!(windows) { ".exe" } else { "" };
        assert!(root.join(format!("build/hello-world{extension}")).is_file());
    }
}

#[test]
fn recursive_modules_deduplicate_and_retain_real_error_locations() {
    let workspace = Workspace::new("module-locations");
    manifest(workspace.path(), "test", "src/main.bh", "");
    write(workspace.path(), "src/main.bh", "# imports\nimport \"left.bh\"\nimport \"right.bh\"\ndef main() { print(left() + right()) }\n");
    write(
        workspace.path(),
        "src/left.bh",
        "import \"shared.bh\"\ndef left() -> int { return value() }\n",
    );
    write(
        workspace.path(),
        "src/right.bh",
        "import \"shared.bh\"\ndef right() -> int { return value() }\n",
    );
    write(
        workspace.path(),
        "src/shared.bh",
        "def value() -> int { return 21 }\n",
    );
    succeeds(&workspace.run(&["check"]));
    if clang().is_some() {
        let output = workspace.run(&["run"]);
        succeeds(&output);
        assert_eq!(stdout(&output), "42\n");
    }
    let broken = "# unicode α\ndef value() -> int {\n    return missing\n}\n";
    write(workspace.path(), "src/shared.bh", broken);
    let output = workspace.run(&["check", "--json"]);
    fails(&output);
    let text = stdout(&output);
    assert!(text.contains("shared.bh\""), "{text}");
    assert!(text.contains("\"line\":3,\"column\":12"), "{text}");
    assert!(
        text.contains(&format!("\"start\":{}", broken.find("missing").unwrap())),
        "{text}"
    );
    let output = workspace.run(&["check"]);
    assert!(
        stderr(&output).contains("shared.bh:3:12"),
        "{}",
        describe(&output)
    );
    write(workspace.path(), "src/shared.bh", "def value( {\n");
    let output = workspace.run(&["check", "--json"]);
    fails(&output);
    assert!(stdout(&output).contains("shared.bh\""));
    assert!(stdout(&output).contains("\"line\":1"));
}

#[test]
fn declared_local_dependencies_support_transitive_library_imports() {
    let workspace = Workspace::new("path-dependencies");
    let app = workspace.file("app");
    let library = workspace.file("library");
    let base = workspace.file("base");
    manifest(
        &app,
        "app",
        "src/main.bh",
        "[dependencies]\nmath = \"../library\"\n",
    );
    manifest(
        &library,
        "math",
        "src/lib.bh",
        "[dependencies]\nbase = \"../base\"\n",
    );
    manifest(&base, "base", "lib.bh", "");
    write(
        &app,
        "src/main.bh",
        "import \"@math\"\ndef main() { print(double(answer())) }\n",
    );
    write(
        &library,
        "src/lib.bh",
        "import \"@base\"\ndef double(x: int) -> int { return x * 2 }\n",
    );
    write(&base, "lib.bh", "def answer() -> int { return 21 }\n");
    let output = workspace
        .command()
        .current_dir(&app)
        .args(["check", "--json"])
        .output()
        .unwrap();
    succeeds(&output);
    if clang().is_some() {
        for release in [false, true] {
            let mut command = workspace.command();
            command.current_dir(&app).arg("run");
            if release {
                command.arg("--release");
            }
            let output = command.output().unwrap();
            succeeds(&output);
            assert_eq!(stdout(&output), "42\n");
        }
    }
    write(&app, "src/main.bh", "import \"@absent\"\ndef main() {}\n");
    let output = workspace
        .command()
        .current_dir(&app)
        .arg("check")
        .output()
        .unwrap();
    fails(&output);
    assert!(stderr(&output).contains("unknown local dependency 'absent'"));
}

#[test]
fn cycles_collisions_malformed_imports_and_root_escape_fail_clearly() {
    let workspace = Workspace::new("module-errors");
    manifest(workspace.path(), "test", "src/main.bh", "");
    write(
        workspace.path(),
        "src/other.bh",
        "import \"main.bh\"\ndef helper() {}\n",
    );
    for (source, expected) in [
        ("import \"other.bh\"\ndef main() {}", "import cycle"),
        (
            "import \"../outside.bh\"\ndef main() {}",
            "cannot read module",
        ),
        ("import \"/tmp/outside.bh\"\ndef main() {}", "relative .bh"),
        ("import \"missing.bh\"\ndef main() {}", "cannot read module"),
        ("import missing\ndef main() {}", "quoted module path"),
        ("import \"other.bh\" def main() {}", "newline or ';'"),
    ] {
        write(workspace.path(), "src/main.bh", source);
        let output = workspace.run(&["check"]);
        fails(&output);
        assert!(stderr(&output).contains(expected), "{}", describe(&output));
    }
    write(
        workspace.path(),
        "src/main.bh",
        "import \"other.bh\"\ndef helper() {}\ndef main() {}\n",
    );
    write(workspace.path(), "src/other.bh", "def helper() {}\n");
    let output = workspace.run(&["check"]);
    fails(&output);
    assert!(
        stderr(&output).contains("declared more than once"),
        "{}",
        describe(&output)
    );
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_the_importing_package() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new("module-symlink");
    let app = workspace.file("app");
    manifest(&app, "test", "main.bh", "");
    write(&app, "main.bh", "import \"outside.bh\"\ndef main() {}\n");
    let outside = workspace.write("outside.bh", "def outside() {}\n");
    symlink(&outside, app.join("outside.bh")).unwrap();
    let output = workspace
        .command()
        .current_dir(&app)
        .arg("check")
        .output()
        .unwrap();
    fails(&output);
    assert!(stderr(&output).contains("escapes its package root"));
}

#[test]
fn output_cannot_replace_imported_source_or_manifests() {
    let workspace = Workspace::new("module-output");
    manifest(workspace.path(), "test", "main.bh", "");
    write(
        workspace.path(),
        "main.bh",
        "import \"library.bh\"\ndef main() { print(value()) }\n",
    );
    write(
        workspace.path(),
        "library.bh",
        "def value() -> int { return 42 }\n",
    );
    for path in ["library.bh", "bhasa.toml"] {
        let before = fs::read(workspace.file(path)).unwrap();
        let output = workspace.run(&["emit-llvm", "main.bh", "-o", path]);
        fails(&output);
        assert_eq!(fs::read(workspace.file(path)).unwrap(), before);
        if clang().is_some() {
            fails(&workspace.run(&["build", "-o", path]));
            assert_eq!(fs::read(workspace.file(path)).unwrap(), before);
        }
    }
}

#[test]
fn malformed_manifests_are_rejected_without_fallback_or_network() {
    let workspace = Workspace::new("manifest-errors");
    write(workspace.path(), "main.bh", "def main() {}\n");
    for source in [
        "name = \"missing-section\"\n",
        "[unknown]\n",
        "[package]\nname = \"x\"\nname = \"y\"\n",
        "[package]\nname = \"x\"\nversion = \"0.1\"\nentry = \"main.bh\"\n",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nentry = \"../main.bh\"\n",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nentry = \"main.bh\"\n[dependencies]\nx = { git = \"https://example.com/x\" }\n",
    ] {
        write(workspace.path(), "bhasa.toml", source);
        let output = workspace.run(&["check", "--json"]);
        fails(&output); assert!(stdout(&output).contains("bhasa.toml\""));
    }
}

#[test]
fn manifest_discovery_requires_the_exact_lowercase_filename() {
    let workspace = Workspace::new("manifest-case");
    manifest(workspace.path(), "case", "main.bh", "");
    write(workspace.path(), "main.bh", "def main() {}\n");
    succeeds(&workspace.run(&["check"]));
    let temporary = workspace.file("renaming-manifest");
    fs::rename(workspace.file("bhasa.toml"), &temporary).unwrap();
    fs::rename(&temporary, workspace.file("Bhasa.toml")).unwrap();
    let output = workspace.run(&["check"]);
    fails(&output);
    assert!(
        stderr(&output).contains("no bhasa.toml found"),
        "{}",
        describe(&output)
    );
    assert!(bhasa::project::read_manifest(&workspace.file("Bhasa.toml")).is_err());
    fs::rename(workspace.file("Bhasa.toml"), &temporary).unwrap();
    fs::rename(&temporary, workspace.file("bhasa.toml")).unwrap();
    succeeds(&workspace.run(&["check"]));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn project_run_forwards_arguments_after_separator() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("project-args");
    manifest(workspace.path(), "args", "main.bh", "");
    write(
        workspace.path(),
        "main.bh",
        "def main() { print(arg_count()); print(arg(1)); print(arg(2)) }\n",
    );
    let output = workspace.run(&["run", "--", "a b", "--release"]);
    succeeds(&output);
    assert_eq!(stdout(&output), "3\na b\n--release\n");
}

#[test]
fn alternate_frontend_input_preserves_source_offsets_and_omits_imports() {
    let workspace = Workspace::new("module-frontend");
    manifest(workspace.path(), "test", "main.bh", "");
    write(
        workspace.path(),
        "main.bh",
        "import \"lib.bh\"\r\ndef main() { print(answer()) }\n",
    );
    write(
        workspace.path(),
        "lib.bh",
        "# α\ndef answer() -> int { return 42 }\n",
    );
    let loaded = bhasa::project::load_unparsed(Some(&workspace.file("main.bh"))).unwrap();
    assert!(loaded.program.functions.is_empty());
    let combined = bhasa::project::flattened_source(&loaded.files);
    assert!(!combined.contains("import"));
    assert!(combined.contains("\r\n"));
    assert!(bhasa::check(&combined).is_ok());
    for file in &loaded.files {
        let local = file.source.find("def").unwrap();
        assert_eq!(&combined[file.base + local..file.base + local + 3], "def");
        let diagnostic = bhasa::Diagnostic::new(
            "test",
            "position",
            bhasa::ast::Span::new(file.base + local, file.base + local + 3),
        );
        let located = bhasa::project::locate(&loaded.files, &diagnostic);
        assert_eq!(located.path, file.path);
        assert_eq!(located.diagnostic.span.start, local);
    }
}
