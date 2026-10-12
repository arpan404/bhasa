#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;
use bhasa::ast::{ExprKind, StmtKind};
use common::*;
#[path = "fixtures/channels/bounded.rs"]
mod channel_process;
use channel_process::{bounded, configured_clang};
use std::process::{Command, Stdio};

fn project(source: &str) -> (Workspace, Vec<bhasa::project::SourceFile>) {
    let workspace = Workspace::new("channels-project");
    std::os::unix::fs::symlink(fixture("std"), workspace.file("std")).unwrap();
    workspace.write("bhasa.toml", "[package]\nname=\"channel-test\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[dependencies]\nstd=\"std\"\n");
    let path = workspace.write("main.bh", source);
    let files = bhasa::project::load_unparsed(Some(&path)).unwrap().files;
    (workspace, files)
}

fn check(
    files: &[bhasa::project::SourceFile],
) -> Result<bhasa::ast::CheckedProgram, Vec<bhasa::Diagnostic>> {
    bhasa::sema::check(bhasa::parse_files(files)?)
}

#[test]
fn real_loader_binds_canonical_families_and_rejects_modified_or_copied_module() {
    let source = "import \"@std\" as ch\ntype Number=int\ndef keep(s:ch.Sender[Number])->ch.Sender[int]{return s}\ndef main(){let pair=ch.channel[Number](1);let sender=keep(pair.sender());sender.close()}\n";
    let (workspace, mut files) = project(source);
    let checked = check(&files).unwrap();
    assert_eq!(
        checked.program.channels.as_ref().unwrap().pairs().count(),
        1
    );
    bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").unwrap();
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_files_with_bhasa(&files, "x86_64-unknown-linux-gnu").unwrap();
    let standard = files
        .iter_mut()
        .find(|f| f.path.ends_with("std/channels.bh"))
        .unwrap();
    standard.source.push_str("\n# provisional editor overlay\n");
    assert!(bhasa::parse_files(&files)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "C101"));
    #[cfg(feature = "bhasa-frontend")]
    assert!(
        bhasa::compile_files_with_bhasa(&files, "x86_64-unknown-linux-gnu")
            .unwrap_err()
            .iter()
            .any(|e| e.code == "C101")
    );
    // Bytes alone cannot confer the registered module's canonical identity.
    workspace.write("copy.bh", include_str!("../std/channels.bh"));
    let copied = workspace.write(
        "copy-main.bh",
        "import \"copy.bh\" as ch\ndef main(){let pair=ch.channel[int](1)}\n",
    );
    let files = bhasa::project::load_unparsed(Some(&copied)).unwrap().files;
    assert!(check(&files).is_err());
}

#[test]
fn channel_refusals_keep_payload_privacy_results_and_worker_rules_distinct() {
    let cases = [
        ("let value=ch.Sender[int](1)", "endpoint constructors"),
        ("let value=ch.Channel[int](1)", "registered factory"),
        (
            "let pair=ch.channel[int](1);print(pair.sender().__payload)",
            "has no field",
        ),
        (
            "let pair=ch.channel[int](1);let r=pair.sender().try_send(false)",
            "requires int",
        ),
        (
            "let pair=ch.channel[int](1);pair.sender().try_send(1)",
            "S022",
        ),
        (
            "let pair=ch.channel[int](1);let r=pair.sender().try_send(1)",
            "S037",
        ),
        ("let pair=ch.channel[file_handle](1)", "C101"),
        ("let pair=ch.channel[task](1)", "G003"),
        ("let pair=ch.channel[ch.Sender[int]](1)", "C101"),
    ];
    for (body, expected) in cases {
        let source = format!("import \"@std\" as ch\ndef main(){{{body}}}\n");
        let (_, files) = project(&source);
        let errors = check(&files).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == expected || e.message.contains(expected)),
            "{body}: {errors:?}"
        );
        #[cfg(feature = "bhasa-frontend")]
        assert!(
            bhasa::compile_files_with_bhasa(&files, "x86_64-unknown-linux-gnu").is_err(),
            "{body}"
        );
    }
    for name in [
        "worker-blocking",
        "worker-method-blocking",
        "pending",
        "immutable",
    ] {
        let source =
            std::fs::read_to_string(fixture(&format!("tests/fixtures/channels/{name}.bh")))
                .unwrap();
        let (_, files) = project(&source);
        let code = match name {
            "pending" => "S037",
            "immutable" => "S036",
            _ => "S020",
        };
        assert!(
            check(&files).unwrap_err().iter().any(|e| e.code == code),
            "{name}"
        );
        #[cfg(feature = "bhasa-frontend")]
        assert!(
            bhasa::compile_files_with_bhasa(&files, "x86_64-unknown-linux-gnu").is_err(),
            "{name}"
        );
    }
    let (_, files) = project("import \"@std\" as ch\nstruct State{file:file_handle}\ndef main(){let pair=ch.channel[State](1)}\n");
    assert!(check(&files).is_err());
}

#[test]
fn public_emitter_rechecks_factory_identity_and_worker_effects() {
    let source = "import \"@std\" as ch\ndef helper(s:ch.Sender[int])->ch.SendResult[int]{return s.try_send(1)}\ndef worker(s:ch.Sender[int])->ch.SendResult[int]{return helper(s)}\ndef main(){let pair=ch.channel[int](1);var job=spawn(worker(pair.sender()));let result=join(job);match result{case ch.SendResult[int].ok(status){} case ch.SendResult[int].error(reason){}}}\n";
    let (_, files) = project(source);
    let checked = check(&files).unwrap();
    let target = "x86_64-unknown-linux-gnu";
    bhasa::codegen::emit_llvm(&checked, target).unwrap();
    let mut changed = checked.clone();
    let authority = changed.program.channels.clone().unwrap();
    let factory = changed
        .program
        .functions
        .iter_mut()
        .find(|f| {
            authority
                .pairs()
                .any(|id| f.return_type == bhasa::ast::Type::Record(id))
        })
        .unwrap();
    factory.body = Some(vec![]);
    assert!(bhasa::codegen::emit_llvm(&changed, target).is_err());
    let mut changed = checked.clone();
    changed.program.records[0].span.end += 1;
    assert!(bhasa::codegen::emit_llvm(&changed, target).is_err());
    let mut changed = checked.clone();
    let mut edits = 0;
    for function in &mut changed.program.functions {
        for statement in function.body.as_mut().into_iter().flatten() {
            if let StmtKind::Return(Some(expression)) = &mut statement.kind {
                if let ExprKind::Call { name, .. } = &mut expression.kind {
                    if name.ends_with(".try_send") {
                        *name = name.replace(".try_send", ".send");
                        edits += 1;
                    }
                }
            }
        }
    }
    assert_eq!(edits, 1);
    assert!(bhasa::codegen::emit_llvm(&changed, target)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "S020"));
    assert!(bhasa::codegen::emit_llvm(&checked, "wasm32-unknown-unknown").is_err());

    // The emitter recomputes intrinsic result types even if a caller falsifies
    // the public checked AST's expression cache and removes result handling.
    let (_, files) = project("import \"@std\" as ch\ndef main(){let pair=ch.channel[int](1);let pending=pair.sender().try_send(1);match pending{case ch.SendResult[int].ok(status){} case ch.SendResult[int].error(reason){}}}\n");
    let mut changed = check(&files).unwrap();
    let main = changed
        .program
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let body = main.body.as_mut().unwrap();
    body.retain(|statement| !matches!(statement.kind, StmtKind::Match { .. }));
    let value = body
        .iter_mut()
        .find_map(|statement| match &mut statement.kind {
            StmtKind::Var { name, value, .. } if name == "pending" => Some(value),
            _ => None,
        })
        .unwrap();
    value.ty = Some(bhasa::ast::Type::Int);
    assert!(bhasa::codegen::emit_llvm(&changed, target).is_err());
}

#[test]
fn driver_links_channels_reuses_artifacts_and_refuses_combined_std_runtime_target_early() {
    let Some(_) = configured_clang() else { return };
    let source = include_str!("fixtures/channels/basic.bh");
    let (workspace, files) = project(source);
    // @std also imports the streaming file wrappers. This CLI gate proves
    // combined standard-library refusal before Cargo/output writes; the private
    // driver unit separately attributes admission to the channels-only marker.
    let ir =
        bhasa::codegen::emit_llvm(&check(&files).unwrap(), "x86_64-unknown-linux-gnu").unwrap();
    assert!(ir.contains("; bhasa runtime: channels"));
    assert!(ir.contains("; bhasa runtime: file"));
    let mut frontends = vec!["rust"];
    if cfg!(feature = "bhasa-frontend") {
        frontends.push("bhasa")
    }
    for frontend in frontends {
        for attempt in 0..2 {
            let output =
                bounded(
                    workspace
                        .command()
                        .args(["run", "main.bh", "--frontend", frontend]),
                );
            succeeds(&output);
            assert_eq!(stdout(&output), "42\n");
            if attempt == 1 {
                assert!(
                    stderr(&output).contains("Reused native artifact"),
                    "{}",
                    describe(&output)
                );
            }
        }
    }
    let (workspace, _) = project(source);
    std::fs::create_dir_all(workspace.file("adapter/src")).unwrap();
    workspace.write("adapter/Cargo.toml","[package]\nname=\"slow\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\ncrate-type=[\"staticlib\"]\n");
    workspace.write(
        "adapter/src/lib.rs",
        "#[no_mangle] pub extern \"C\" fn value()->i64{42}\n",
    );
    let manifest = std::fs::read_to_string(workspace.file("bhasa.toml")).unwrap()
        + "[rust-dependencies]\nslow=\"adapter\"\n";
    workspace.write("bhasa.toml", &manifest);
    let output = bounded(workspace.command().args([
        "build",
        "main.bh",
        "--target",
        "windows-x64",
        "-o",
        "app",
    ]));
    fails(&output);
    assert!(
        stderr(&output).contains("unsupported native 64-bit POSIX target"),
        "{}",
        describe(&output)
    );
    assert!(!workspace.file("app").exists());
    assert!(!workspace.file("build").exists());
    assert!(!workspace.file("adapter/Cargo.lock").exists());
    assert!(!workspace.file("adapter/target").exists());
}

#[test]
fn both_production_frontends_execute_typed_frames_with_sanitizers_and_no_live_allocations() {
    let Some(clang) = configured_clang() else {
        return;
    };
    let workspace = Workspace::new("channels-guests");
    // Use the configured frontend's deployment triple, as the production
    // driver does, so strict warnings also cover LLVM/C target agreement.
    let query = bounded(
        Command::new(&clang)
            .args(["-S", "-emit-llvm", "-x", "c", "-", "-o", "-"])
            .stdin(Stdio::null()),
    );
    succeeds(&query);
    let queried_ir = stdout(&query);
    let target = queried_ir
        .lines()
        .find_map(|line| {
            line.strip_prefix("target triple = \"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .unwrap();
    for optimization in ["-O0", "-O2"] {
        let flags = [
            optimization,
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-pthread",
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-fno-omit-frame-pointer",
        ];
        let mut objects = Vec::new();
        for name in [
            "core",
            "concurrency",
            "channel",
            "file",
            "net",
            "text",
            "collections",
        ] {
            let object = workspace.file(&format!("{name}.o"));
            succeeds(&bounded(
                Command::new(&clang)
                    .args(flags)
                    .args([
                        "-Dmalloc=storage_malloc",
                        "-Dcalloc=storage_calloc",
                        "-Drealloc=storage_realloc",
                        "-Dfree=storage_free",
                        "-c",
                    ])
                    .arg(fixture(&format!("runtime/{name}.c")))
                    .arg("-o")
                    .arg(&object),
            ));
            objects.push(object);
        }
        for (name, expected) in [
            ("basic", "42\n"),
            ("blocking", "42\n"),
            ("try", "42\n"),
            ("owned-worker", "42\n"),
            ("ordinary-method", "7\n42\n"),
            ("ints", "9\n"),
            ("record", "record\n"),
            ("enum", "6\n"),
            ("multi-family", "42\n"),
        ] {
            let source =
                std::fs::read_to_string(fixture(&format!("tests/fixtures/channels/{name}.bh")))
                    .unwrap();
            let (_project, files) = project(&source);
            let checked = check(&files).unwrap();
            let mut emissions =
                vec![("rust", bhasa::codegen::emit_llvm(&checked, target).unwrap())];
            #[cfg(feature = "bhasa-frontend")]
            emissions.push((
                "bhasa",
                bhasa::compile_files_with_bhasa(&files, target).unwrap(),
            ));
            for (frontend, ir) in emissions.drain(..) {
                assert!(ir.contains("; bhasa runtime: channels"));
                let ir = ir
                    .replace("@malloc(", "@storage_malloc(")
                    .replace("@free(", "@storage_free(");
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
                let llvm = workspace.write("guest.ll", &ir);
                let executable = workspace.file("guest");
                succeeds(&bounded(
                    Command::new(&clang)
                        .args(flags)
                        .arg(&llvm)
                        .args(&objects)
                        .arg(fixture("tests/fixtures/storage/ownership-tracker.c"))
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
                    expected,
                    "{frontend}/{name}/{optimization}"
                );
                assert!(
                    output.stderr.is_empty(),
                    "{frontend}/{name}/{optimization}: {}",
                    describe(&output)
                );
            }
        }
    }
}
