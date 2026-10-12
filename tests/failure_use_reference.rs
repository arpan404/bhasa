const PRELUDE: &str =
    "enum R{ok(value:int);error(code:int)}def make()->R{return R.ok(1)}def observe(value:R){}";
const HANDLE: &str = "match r{case R.ok(n){}case R.error(e){}}";

fn accepted(body: &str) {
    let source = format!("{PRELUDE}def main(){{{body}}}");
    bhasa::check(&source).unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
}
fn rejected(body: &str) {
    let source = format!("{PRELUDE}def main(){{{body}}}");
    let errors = bhasa::check(&source).expect_err(&source);
    assert!(
        errors.iter().any(|error| error.code == "S037"),
        "{source}: {errors:?}"
    );
}

#[test]
fn local_results_require_direct_handling_and_reject_overwrite() {
    for body in [
        "var r=make()",
        "var r=make();observe(r)",
        "var r=make();r=make()",
        "var r=make();var copy=r;match copy{case R.ok(n){}case R.error(e){}}",
        "if true{var r=make()}",
        "var r=make();return",
    ] {
        rejected(body);
    }
    accepted(&format!("var r=make();{HANDLE}"));
    accepted(&format!("var r=make();{HANDLE};r=make();{HANDLE}"));
    accepted(&format!(
        "var r=make();var copy=r;{HANDLE};match copy{{case R.ok(n){{}}case R.error(e){{}}}}"
    ));
    // Option-shaped enums remain outside the rule.
    bhasa::check("enum O{none;some(value:int)}def main(){var unused=O.none()}").unwrap();
}

#[test]
fn branch_join_shadowing_and_early_exits_keep_obligations() {
    rejected(&format!("var r=make();if true{{{HANDLE}}}"));
    rejected(&format!("var r=make();if true{{{HANDLE}}}else{{return}}"));
    accepted(&format!("var r=make();if true{{{HANDLE}}}else{{{HANDLE}}}"));
    accepted(&format!(
        "var r=make();if true{{{HANDLE};return}}else{{{HANDLE}}}"
    ));
    rejected(&format!("var r=make();if true{{var r=make();{HANDLE}}}"));
    rejected("var r=make();if true{var r=1;print(r)}");
    accepted(&format!(
        "var r=make();if true{{var r=make();{HANDLE}}};{HANDLE}"
    ));
    // Unreachable declarations cannot create an obligation on a reachable path.
    let errors = bhasa::check(&format!("{PRELUDE}def main(){{return;var r=make()}}")).unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S015"));
}

#[test]
fn loop_obligations_cover_zero_iterations_break_continue_and_assignment() {
    rejected(&format!("var r=make();while false{{{HANDLE}}}"));
    for exit in ["break", "continue"] {
        rejected(&format!("while true{{var r=make();{exit}}}"));
        accepted(&format!("while true{{var r=make();{HANDLE};{exit}}}"));
    }
    rejected("for i in range(2){var r=make()}");
    accepted(&format!("for i in range(2){{var r=make();{HANDLE}}}"));
    rejected(&format!(
        "var r=make();{HANDLE};while false{{r=make();{HANDLE}}}"
    ));
    rejected(&format!(
        "while true{{var r=make();{HANDLE};while false{{r=make();{HANDLE}}};break}}"
    ));
    accepted(&format!("var r=make();while false{{{HANDLE}}};{HANDLE}"));
}

#[test]
fn return_and_try_discharge_only_the_direct_operand() {
    for body in [
        "var r=make();return r",
        "var r=make();var n=try r;return R.ok(n)",
    ] {
        bhasa::check(&format!("{PRELUDE}def forward()->R{{{body}}}")).unwrap();
    }
    for body in ["var r=make();return make()", "var r=make();var n=try make();return R.ok(n)",
        "var r=make();var other=make();var n=try r;match other{case R.ok(n){}case R.error(e){}};return R.ok(n)"] {
        let source = format!("{PRELUDE}def forward()->R{{{body}}}");
        assert!(bhasa::check(&source).unwrap_err().iter().any(|error| error.code == "S037"), "{source}");
    }
}

#[test]
fn nested_result_success_and_match_payloads_create_obligations() {
    let prelude = "enum R{ok(value:int);error(code:int)}enum Nested{ok(value:R);error(code:int)}";
    for body in [
        "var r=try input;return Nested.ok(R.ok(1))",
        "match input{case Nested.ok(r){}case Nested.error(e){}};return input",
    ] {
        let source = format!("{prelude}def forward(input:Nested)->Nested{{{body}}}");
        assert!(
            bhasa::check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S037"),
            "{source}"
        );
    }
    bhasa::check(&format!("{prelude}def forward(input:Nested)->Nested{{var r=try input;match r{{case R.ok(n){{}}case R.error(e){{}}}};return input}}")).unwrap();
}

#[test]
fn emitter_rechecks_pending_results_after_checked_ast_mutation() {
    let source = format!("{PRELUDE}def main(){{var r=make();{HANDLE}}}");
    let mut checked = bhasa::check(&source).unwrap();
    let body = checked
        .program
        .functions
        .last_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.pop();
    let errors = bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S037"));
}

#[test]
fn concrete_generics_methods_and_joined_results_follow_the_same_rule() {
    for definition in [
        "enum Result[T]{ok(value:T);error(message:buffer)}def make()->Result[int]{return Result[int].ok(1)}",
        "struct Error{code:int}enum IoResult[T]{error(error:Error);ok(value:T)}def make()->IoResult[int]{return IoResult[int].ok(1)}",
    ] {
        let source=format!("{definition}def main(){{var ignored=make()}}");
        assert!(bhasa::check(&source).unwrap_err().iter().any(|error|error.code=="S037"),"{source}");
    }
    let definitions="enum R{ok(value:int);error(code:int)}struct Source{value:int;def get(self)->R{return R.ok(self.value)}}def worker(value:int)->R{return R.ok(value)}";
    for initializer in ["Source(1).get()", "join(task)"] {
        let task = if initializer == "join(task)" {
            "var task=spawn(worker(1));"
        } else {
            ""
        };
        let source = format!("{definitions}def main(){{{task}var r={initializer}}}");
        assert!(
            bhasa::check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S037"),
            "{source}"
        );
        bhasa::check(&format!(
            "{definitions}def main(){{{task}var r={initializer};{HANDLE}}}"
        ))
        .unwrap();
    }
}

#[test]
fn imported_generic_failure_diagnostic_maps_to_original_file_without_text_offsets() {
    use bhasa::{generics, modules, project};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "bhasa-failure-location-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _remove = Remove(directory.clone());
    let library = "# Original locations: λ 🙂\nenum Result[T]{ok(value:T);error(code:int)}\ndef ignored[T](input:T)->int{\nvar r=Result[T].ok(input);\nreturn 0\n}\n".replace('\n', "\r\n");
    let library_path = directory.join("library.bh");
    fs::write(&library_path, &library).unwrap();
    let entry = directory.join("main.bh");
    fs::write(
        &entry,
        "import \"library.bh\" as lib\ndef main(){print(lib.ignored[int](7))}\n",
    )
    .unwrap();
    let loaded = project::load_unparsed(Some(&entry)).unwrap();
    let modules = modules::expand_files(&loaded.files).unwrap();
    let generics = generics::expand(&modules.source, &modules.boundaries).unwrap();
    let mut program = generics::parse_expansion(&generics).unwrap();
    modules.remap_program(&mut program);
    // Match the public host pipeline: parser maps specialization spans before
    // sema, then diagnostics display names and project::locate maps file bases.
    let errors = bhasa::sema::check(program).unwrap_err();
    let errors = modules.display_diagnostics(generics.display_diagnostics(errors));
    assert_eq!(errors.len(), 1, "{errors:?}");
    let located = project::locate(&loaded.files, &errors[0]);
    assert_eq!(located.path, library_path.canonicalize().unwrap());
    assert_eq!(located.source, library);
    assert_eq!(located.diagnostic.code, "S037");
    assert_eq!(
        located.diagnostic.span.start,
        library.find("return 0").unwrap()
    );
    assert_eq!(
        &library[located.diagnostic.span.start..located.diagnostic.span.end],
        "return 0"
    );
    assert_eq!(located.diagnostic.message, "result binding 'r' is pending at return; match, return or propagate it before discarding it");
    let json = located
        .diagnostic
        .to_json(&located.source, located.path.to_str().unwrap());
    assert!(json.contains("\"line\":5"), "{json}");
    assert!(json.contains("\"column\":1"), "{json}");
}
