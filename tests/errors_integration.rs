mod common;
use common::*;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn frontends() -> Vec<&'static str> {
    if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    }
}

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        err.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut timeout = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timeout = true;
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
        !timeout,
        "error integration fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

#[test]
fn imported_generic_propagation_contextual_names_and_formatter_keep_behavior() {
    let ws = Workspace::new("errors-host-integration");
    ws.write(
        "library.bh",
        r#"
        enum Source[T]{error(problem:buffer);ok(value:T)}
        enum Reply[T]{ok(value:T);error(problem:buffer)}
        def try[T](value:T)->T{return value}
        def read[T](value:T)->Source[T]{return Source[T].ok(value)}
        def fail()->Source[int]{return Source[int].error(buffer("missing"))}
        def propagate[T](source:Source[T])->Reply[T]{var value=try source;return Reply[T].ok(value)}
    "#,
    );
    let source=ws.write("main.bh",r#"
        import "library.bh"
        enum Local{error(problem:buffer);ok(value:buffer)}
        def success()->Local{
            var value=try propagate[int](read[int](try[int](42)))
            return Local.ok(to_buffer(value))
        }
        def multiline()->Local{
            var numbers:List[
                int
            ] = try read[List[int]](List[int]())
            return Local.ok(to_buffer(numbers.count()))
        }
        def failure()->Local{
            var values=Map[str,List[buffer]]();var parts=List[buffer]();parts.push(buffer("owned"));values.put("key",parts)
            for key in values {
                match Source[int].ok(1) {
                    case Source[int].ok(number){var value=try fail();return Local.ok(to_buffer(value))}
                    case Source[int].error(problem){return Local.error(problem)}
                }
            }
            return Local.ok(buffer("wrong"))
        }
        def main()->int{
            match success(){case Local.ok(value){if not same(value,"42"){return 1}}case Local.error(problem){return 2}}
            match multiline(){case Local.ok(value){if not same(value,"0"){return 6}}case Local.error(problem){return 7}}
            match failure(){case Local.ok(value){return 3}case Local.error(problem){if not same(problem,"missing"){return 4}}}
            var try=Source[int].ok(7)
            var propagated=0;var failed=false
            match propagate[int](try){case Reply[int].ok(value){propagated=value}case Reply[int].error(problem){failed=true}}
            match try{case Source[int].ok(value){if value!=7{return 8}}case Source[int].error(problem){return 9}}
            if failed{return 5}
            print(propagated)
            return 0
        }
    "#);
    succeeds(&bounded(ws.command().arg("fmt").arg(&source)));
    succeeds(&bounded(
        ws.command().arg("fmt").arg(&source).arg("--check"),
    ));
    for frontend in frontends() {
        let output = bounded(
            ws.command()
                .arg("run")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "7\n");
    }
}

#[test]
fn inferred_private_result_enums_cannot_be_implicitly_unpacked_across_modules() {
    let ws = Workspace::new("errors-private-enum");
    // Define a valid same-module caller result; the private implicit check is
    // exercised solely by the importer, not an unrelated return-shape failure.
    ws.write(
        "library.bh",
        r#"
        private enum Hidden[T]{ok(value:T);error(problem:buffer)}
        enum Reply{ok(value:int);error(problem:buffer)}
        def hidden()->Hidden[int]{return Hidden[int].ok(42)}
        def consume()->Reply{var result=try hidden();return Reply.ok(result)}
    "#,
    );
    let text="import \"library.bh\" as lib\nenum Reply{ok(value:int);error(problem:buffer)}\ndef run()->Reply{var value=try lib.hidden();return Reply.ok(value)}\ndef main(){}\n";
    let main = ws.write("main.bh", text);
    for frontend in frontends() {
        let output =
            bounded(
                ws.command()
                    .arg("check")
                    .arg(&main)
                    .args(["--frontend", frontend, "--json"]),
            );
        fails(&output);
        let json = bhasa::json::parse(&stdout(&output)).unwrap();
        let diagnostics = json.as_array().unwrap();
        let error = diagnostics
            .iter()
            .find(|value| value.get("code").and_then(|v| v.as_str()) == Some("M005"))
            .unwrap_or_else(|| panic!("{}", describe(&output)));
        assert_eq!(
            error.get("span").unwrap().get("start").unwrap().as_i64(),
            Some(text.find("try lib.hidden").unwrap() as i64)
        );
        assert!(error
            .get("file")
            .unwrap()
            .as_str()
            .unwrap()
            .ends_with("main.bh"));
    }
    let main=ws.write("main.bh","import \"library.bh\" as lib\ndef main(){var result=lib.consume();match result{case lib.Reply.ok(value){print(value)}case lib.Reply.error(problem){print(problem)}}}");
    for frontend in frontends() {
        let output = bounded(
            ws.command()
                .arg("run")
                .arg(&main)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "42\n");
    }
}

#[test]
fn imported_generic_try_diagnostics_keep_original_file_and_exact_spans() {
    for (case, source_error, annotation, rust_code, native_code) in [
        ("error-payload", "buffer", "", "S035", "BH219"),
        ("success-annotation", "int", ":bool", "S010", "BH201"),
    ] {
        let ws = Workspace::new(case);
        // The multibyte comment and CRLFs distinguish original byte offsets
        // from positions in the generated specialization or importing file.
        let library = format!(
            "# Imported generic diagnostics: λ 🙂\n\
             enum Source[T]{{ok(value:T);error(problem:{source_error})}}\n\
             enum Reply[T]{{error(problem:int);ok(value:T)}}\n\
             def convert[T](source:Source[T])->Reply[T]{{\n\
                 var value{annotation}=try source;\n\
                 return Reply[T].error(0)\n\
             }}\n"
        )
        .replace('\n', "\r\n");
        let library_path = ws.write("library.bh", &library).canonicalize().unwrap();
        let main = ws.write(
            "main.bh",
            "import \"library.bh\" as lib\ndef main(){var ignored=lib.convert[int](lib.Source[int].ok(7))}\n",
        );
        for frontend in frontends() {
            let output = bounded(ws.command().arg("check").arg(&main).args([
                "--frontend",
                frontend,
                "--json",
            ]));
            fails(&output);
            let json = bhasa::json::parse(&stdout(&output)).unwrap();
            let diagnostics = json.as_array().unwrap();
            assert_eq!(diagnostics.len(), 1, "{}", describe(&output));
            let diagnostic = &diagnostics[0];
            assert_eq!(
                diagnostic.get("code").unwrap().as_str(),
                Some(if frontend == "rust" {
                    rust_code
                } else {
                    native_code
                }),
                "{}",
                describe(&output)
            );
            assert_eq!(
                diagnostic.get("file").unwrap().as_str(),
                library_path.to_str(),
                "{}",
                describe(&output)
            );
            let marker = library.find("try source;").unwrap();
            let (expected_start, token) = if case == "error-payload" {
                (marker, "try")
            } else if frontend == "rust" {
                (marker + "try ".len(), "source")
            } else {
                // The native annotation check runs after consuming the value.
                (marker + "try source".len(), ";")
            };
            let span = diagnostic.get("span").unwrap();
            assert_eq!(
                span.get("start").unwrap().as_i64(),
                Some(expected_start as i64),
                "{}",
                describe(&output)
            );
            assert_eq!(
                span.get("end").unwrap().as_i64(),
                Some((expected_start + token.len()) as i64),
                "{}",
                describe(&output)
            );
            assert_eq!(
                diagnostic.get("line").unwrap().as_i64(),
                Some(5),
                "{}",
                describe(&output)
            );
            assert!(!stdout(&output).contains("__bhg_"));
            assert!(!stdout(&output).contains("__bhm_"));
        }
    }
}

#[test]
fn typed_file_errors_propagate_through_import_aliases_and_different_success_types() {
    let ws = Workspace::new("errors-io-propagation");
    std::fs::create_dir(ws.file("std")).unwrap();
    for entry in std::fs::read_dir(fixture("std")).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        if source.is_file()
            && (source.extension().is_some_and(|ext| ext == "bh")
                || entry.file_name() == "bhasa.toml")
        {
            std::fs::copy(source, ws.file("std").join(entry.file_name())).unwrap();
        }
    }
    ws.write("bhasa.toml","[package]\nname=\"error-propagation-test\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n[dependencies]\nstd=\"./std\"\n");
    let module = "@std";
    let source=ws.write("main.bh",&format!(r#"
        import "{module}" as io
        def count(path:str)->io.IoResult[int]{{
            var values=Map[str,buffer]();values.put("scope",buffer("owned"))
            for key in values {{var bytes=try io.file_read_str(path);return io.IoResult[int].ok(len(bytes))}}
            return io.IoResult[int].ok(0)
        }}
        def main()->int{{
            var status=write_file("data.bin",buffer("abc"))
            if status!=0{{return 1}}
            match count("data.bin"){{case io.IoResult[int].ok(size){{print(size)}}case io.IoResult[int].error(problem){{return 2}}}}
            var destination=buffer("unchanged")
            var expected=read_file("missing.bin",destination)
            match count("missing.bin"){{
                case io.IoResult[int].ok(size){{return 3}}
                case io.IoResult[int].error(problem){{
                    if problem.code()!=expected or expected<=0 or not same(buffer(problem.operation()),"read_file"){{return 4}}
                    var message=problem.message();message.clear()
                    if not same(problem.message().bytes(),"file read failed") or not same(destination,"unchanged"){{return 5}}
                    match problem.domain(){{case io.ErrorDomain.os{{print(true)}}}}
                }}
            }}
            return 0
        }}
    "#));
    for frontend in frontends() {
        let output = bounded(
            ws.command()
                .arg("run")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "3\ntrue\n");
    }
}
