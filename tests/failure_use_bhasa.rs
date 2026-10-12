#![cfg(feature = "bhasa-frontend")]

// Native emission must enforce failure state itself; this source has no private
// members that would cause host checking to run before the native frontend.
fn check(source: &str) -> Result<(), Vec<bhasa::Diagnostic>> {
    let reference = bhasa::check(source);
    let native = bhasa::compile_with_bhasa(source, "x86_64-unknown-linux-gnu");
    assert_eq!(
        reference.is_ok(),
        native.is_ok(),
        "{source}: reference={reference:?}, native={native:?}"
    );
    if reference
        .as_ref()
        .err()
        .is_some_and(|errors| errors.iter().any(|e| e.code == "S037"))
    {
        assert!(
            native
                .as_ref()
                .err()
                .is_some_and(|errors| errors.iter().any(|e| e.code == "BH221")),
            "{source}: {native:?}"
        );
    }
    reference.map(|_| ())
}

const PRELUDE: &str =
    "enum R{ok(value:int);error(code:int)}def make()->R{return R.ok(1)}def observe(value:R){}";
const HANDLE: &str = "match r{case R.ok(n){}case R.error(e){}}";

fn accepted(body: &str) {
    let source = format!("{PRELUDE}def main(){{{body}}}");
    check(&source).unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
}
fn rejected(body: &str) {
    let source = format!("{PRELUDE}def main(){{{body}}}");
    let errors = check(&source).expect_err(&source);
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
    check("enum O{none;some(value:int)}def main(){var unused=O.none()}").unwrap();
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
    let errors = check(&format!("{PRELUDE}def main(){{return;var r=make()}}")).unwrap_err();
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
        check(&format!("{PRELUDE}def forward()->R{{{body}}}")).unwrap();
    }
    for body in ["var r=make();return make()", "var r=make();var n=try make();return R.ok(n)",
        "var r=make();var other=make();var n=try r;match other{case R.ok(n){}case R.error(e){}};return R.ok(n)"] {
        let source = format!("{PRELUDE}def forward()->R{{{body}}}");
        assert!(check(&source).unwrap_err().iter().any(|error| error.code == "S037"), "{source}");
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
            check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S037"),
            "{source}"
        );
    }
    check(&format!("{prelude}def forward(input:Nested)->Nested{{var r=try input;match r{{case R.ok(n){{}}case R.error(e){{}}}};return input}}")).unwrap();
}

#[test]
fn concrete_generics_methods_and_joined_results_follow_the_same_rule() {
    for definition in [
        "enum Result[T]{ok(value:T);error(message:buffer)}def make()->Result[int]{return Result[int].ok(1)}",
        "struct Error{code:int}enum IoResult[T]{error(error:Error);ok(value:T)}def make()->IoResult[int]{return IoResult[int].ok(1)}",
    ] {
        let source=format!("{definition}def main(){{var ignored=make()}}");
        assert!(check(&source).unwrap_err().iter().any(|error|error.code=="S037"),"{source}");
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
            check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S037"),
            "{source}"
        );
        check(&format!(
            "{definitions}def main(){{{task}var r={initializer};{HANDLE}}}"
        ))
        .unwrap();
    }
}

#[test]
fn immutable_assignment_keeps_its_distinct_diagnostic() {
    let source = "enum R{ok(value:int);error(code:int)}def main(){let r=R.ok(1);r=R.ok(2)}";
    let reference = bhasa::check(source).unwrap_err();
    let native = bhasa::compile_with_bhasa(source, "x86_64-unknown-linux-gnu").unwrap_err();
    assert!(reference.iter().any(|e| e.code == "S036"), "{reference:?}");
    assert!(native.iter().any(|e| e.code == "BH220"), "{native:?}");
}

#[test]
fn parenthesized_names_and_field_subjects_keep_lexical_identity() {
    check("enum R{ok(value:int);error(code:int)}def forward()->R{let r=R.ok(1);return (r)}")
        .unwrap();
    // Parameter obligations stay outside this slice, including owned parameters.
    check("enum R{ok(value:int);error(code:int)}def observe(r:R){}").unwrap();
    let source="enum R{ok(value:int);error(code:int)}struct Box{value:R}def main(){var r=R.ok(1);var box=Box(r);match box.value{case R.ok(n){}case R.error(e){}}}";
    assert!(check(source).unwrap_err().iter().any(|e| e.code == "S037"));
}

#[test]
fn result_for_binders_and_match_branch_union_keep_obligations() {
    let definitions = "enum R{ok(value:int);error(code:int)}";
    let bad = format!("{definitions}def main(){{var values=List[R]();for r in values{{}}}}");
    assert!(check(&bad).unwrap_err().iter().any(|e| e.code == "S037"));
    let good = format!("{definitions}def main(){{var values=List[R]();for r in values{{match r{{case R.ok(n){{}}case R.error(e){{}}}}}}}}");
    check(&good).unwrap();
    let bad = format!("{PRELUDE}def main(){{var r=make();match make(){{case R.ok(n){{{HANDLE}}}case R.error(e){{}}}}}}");
    assert!(check(&bad).unwrap_err().iter().any(|e| e.code == "S037"));
    let good = format!("{PRELUDE}def main(){{var r=make();match make(){{case R.ok(n){{{HANDLE}}}case R.error(e){{{HANDLE}}}}}}}");
    check(&good).unwrap();
}

#[test]
fn imported_generic_pending_diagnostics_keep_original_locations() {
    use bhasa::project::{ModuleImport, SourceFile};
    use std::path::PathBuf;
    // Virtual file bases are explicit to exercise the composed module/generic
    // map without native build tools or filesystem project loading.
    let library = "# λ 🙂\r\nenum R[T]{error(code:int);ok(value:T)}\r\ndef ignore[T](value:T){let pending=R[T].ok(value)}\r\n";
    let main = "import \"library.bh\" as lib\ndef main(){lib.ignore[int](42)}\n";
    let library_path = PathBuf::from("/pending-parity/library.bh");
    let import_span = bhasa::ast::Span::new(0, main.find('\n').unwrap());
    let files = vec![
        SourceFile {
            path: library_path.clone(),
            source: library.into(),
            base: 0,
            import_spans: vec![],
            imports: vec![],
        },
        SourceFile {
            path: PathBuf::from("/pending-parity/main.bh"),
            source: main.into(),
            base: library.len() + 1,
            import_spans: vec![import_span],
            imports: vec![ModuleImport {
                path: "library.bh".into(),
                alias: Some("lib".into()),
                span: import_span,
                target: library_path,
            }],
        },
    ];
    let errors = bhasa::compile_files_with_bhasa(&files, "x86_64-unknown-linux-gnu").unwrap_err();
    let error = errors
        .iter()
        .find(|e| e.code == "BH221")
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(error.span.start, library.rfind('}').unwrap(), "{errors:?}");
    assert_eq!(error.span.end, error.span.start + 1);
}
