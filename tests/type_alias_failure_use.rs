use bhasa::ast::Span;
use bhasa::project::{ModuleImport, SourceFile};

fn reference(files: &[SourceFile]) -> Result<(), Vec<bhasa::Diagnostic>> {
    let modules = bhasa::modules::expand_files(files)?;
    let generic = bhasa::generics::expand_with_type_roots(
        &modules.source,
        &modules.boundaries,
        &modules.type_roots()?,
    )
    .map_err(|errors| modules.map_diagnostics(errors))?;
    let mut program = bhasa::generics::parse_expansion(&generic)
        .map_err(|errors| modules.map_diagnostics(errors))?;
    modules.remap_program(&mut program);
    let checked = bhasa::sema::check(program)
        .map_err(|errors| modules.display_diagnostics(generic.display_diagnostics(errors)))?;
    modules.check_privacy(&checked, |name| generic.original_name(name))
}
fn files(source: &str) -> Vec<SourceFile> {
    vec![SourceFile {
        path: "/alias-failures/main.bh".into(),
        source: source.into(),
        base: 0,
        import_spans: vec![],
        imports: vec![],
    }]
}
fn graph(main: &str, library: &str) -> Vec<SourceFile> {
    let mut result = files(main);
    result[0].imports.push(ModuleImport {
        path: "library.bh".into(),
        target: "/alias-failures/library.bh".into(),
        alias: Some("lib".into()),
        span: Span::default(),
    });
    result.push(SourceFile {
        path: "/alias-failures/library.bh".into(),
        source: library.into(),
        base: main.len() + 1,
        import_spans: vec![],
        imports: vec![],
    });
    result
}
fn accepted(files: &[SourceFile]) {
    reference(files).unwrap_or_else(|errors| panic!("{errors:?}"));
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_files_with_bhasa(files, "x86_64-unknown-linux-gnu").unwrap();
}
fn rejected(files: &[SourceFile]) {
    let errors = reference(files).unwrap_err();
    assert!(
        errors.iter().any(|error| error.code == "S037"),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| !error.message.contains("__bhm_") && !error.message.contains("__bhg_")),
        "{errors:?}"
    );
    #[cfg(feature = "bhasa-frontend")]
    {
        // No private members: the selected native checker must retain BH221.
        assert!(!bhasa::modules::expand_files(files)
            .unwrap()
            .has_member_privacy());
        let errors =
            bhasa::compile_files_with_bhasa(files, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "BH221"),
            "{errors:?}"
        );
    }
}

#[test]
fn direct_and_generic_result_aliases_keep_pending_bindings_and_copy_obligations() {
    for declarations in [
        "enum Answer{error(code:int);ok(value:int)}\ntype Reply=Answer\n",
        "enum Answer[T]{error(code:int);ok(value:T)}\ntype Reply=Answer[int]\n",
    ] {
        let prelude = format!(
            "{declarations}def make()->Reply{{return Reply.ok(42)}}def observe(value:Reply){{}}"
        );
        for body in [
            "let result:Reply=make()",
            "let result:Reply=make();observe(result)",
            "let result:Reply=make();let copied:Reply=result;match copied{case Reply.ok(value){}case Reply.error(code){}}",
        ] { rejected(&files(&format!("{prelude}def main(){{{body}}}"))); }
        for function in [
            "def main(){let result:Reply=make();match result{case Reply.ok(value){}case Reply.error(code){}}}",
            "def forward()->Reply{let result:Reply=make();return result}",
            "def forward()->Reply{let result:Reply=make();let value=try result;return Reply.ok(value)}",
        ] { accepted(&files(&format!("{prelude}{function}"))); }
    }
}

const LIBRARY: &str="# λ 🙂\r\nenum Answer[T]{error(code:int);ok(value:T)}\r\ntype Reply=Answer[int]\r\ndef make()->Reply{return Reply.ok(42)}\r\n";
#[test]
fn imported_result_aliases_keep_pending_identity_and_direct_handling() {
    let prefix = "type Local=lib.Reply\n";
    for body in [
        "let result:Local=lib.make()",
        "let result:Local=lib.make();let copied:Local=result;match copied{case Local.ok(value){}case Local.error(code){}}",
    ] { rejected(&graph(&format!("{prefix}def main(){{{body}}}"),LIBRARY)); }
    for function in [
        "def main(){let result:Local=lib.make();match result{case Local.ok(value){}case Local.error(code){}}}",
        "def forward()->Local{let result:Local=lib.make();return result}",
        "def forward()->Local{let result:Local=lib.make();let value=try result;return Local.ok(value)}",
    ] { accepted(&graph(&format!("{prefix}{function}"),LIBRARY)); }
}

#[test]
fn aliased_pending_diagnostics_keep_original_imported_return_and_try_sites() {
    for (body, marker) in [
        ("def ignore()->int{\r\nlet pending:Reply=make();\r\nreturn 0\r\n}\r\n", "return 0"),
        ("def ignore(input:Reply)->Reply{\r\nlet pending:Reply=make();\r\nlet value=try input;\r\nmatch pending{case Reply.ok(n){}case Reply.error(e){}}\r\nreturn Reply.ok(value)\r\n}\r\n", "try"),
    ] {
        let library=format!("{LIBRARY}{body}");
        let files=graph("# नेपाली\ndef main(){}\n",&library);
        let errors=reference(&files).unwrap_err();
        let error=errors.iter().find(|error|error.code=="S037").unwrap();
        let located=bhasa::project::locate(&files,error);
        assert_eq!(located.path,files[1].path);
        assert_eq!(located.diagnostic.span.start,library.find(marker).unwrap());
        assert_eq!(&library[located.diagnostic.span.start..located.diagnostic.span.end],marker);
        assert!(located.diagnostic.message.contains("pending"));
        assert!(!located.diagnostic.message.contains("__bh"));
        println!("{}",located.diagnostic.to_json(&located.source,located.path.to_str().unwrap()));
        #[cfg(feature="bhasa-frontend")]
        {
            let errors=bhasa::compile_files_with_bhasa(&files,"x86_64-unknown-linux-gnu").unwrap_err();
            let error=errors.iter().find(|error|error.code=="BH221").unwrap();
            let located=bhasa::project::locate(&files,error);
            assert_eq!(located.path,files[1].path);
            assert_eq!(located.diagnostic.span.start,library.find(marker).unwrap());
        }
    }
}
