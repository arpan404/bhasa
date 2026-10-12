use bhasa::ast::{ParamMode, Span, Type};
use bhasa::project::{ModuleImport, SourceFile};

const TARGET: &str = "arm64-apple-macosx26.0.0";

fn graph(main: &str, changed_standard: Option<String>) -> Vec<SourceFile> {
    let path = std::fs::canonicalize(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("std/channels.bh"),
    )
    .unwrap();
    let standard = changed_standard.unwrap_or_else(|| std::fs::read_to_string(&path).unwrap());
    let end = main.find('\n').unwrap();
    let span = Span::new(0, end);
    vec![
        SourceFile {
            path: "channels-source-test.bh".into(),
            source: main.into(),
            base: 0,
            import_spans: vec![span],
            imports: vec![ModuleImport {
                path: "channels.bh".into(),
                alias: Some("ch".into()),
                span,
                target: path.clone(),
            }],
        },
        SourceFile {
            path,
            source: standard,
            base: main.len() + 1,
            import_spans: vec![],
            imports: vec![],
        },
    ]
}
fn checked(main: &str) -> bhasa::ast::CheckedProgram {
    let files = graph(main, None);
    let program = bhasa::parse_files(&files).unwrap_or_else(|e| panic!("{main}: {e:?}"));
    let checked = bhasa::sema::check(program).unwrap_or_else(|e| panic!("{main}: {e:?}"));
    bhasa::codegen::emit_llvm(&checked, TARGET).unwrap();
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_files_with_bhasa(&files, TARGET).unwrap();
    checked
}
fn rejects(main: &str, code: &str) {
    let files = graph(main, None);
    let errors = bhasa::parse_files(&files)
        .and_then(bhasa::sema::check)
        .unwrap_err();
    assert!(errors.iter().any(|e| e.code == code), "{main}: {errors:?}");
    #[cfg(feature = "bhasa-frontend")]
    assert!(bhasa::compile_files_with_bhasa(&files, TARGET).is_err());
}
fn body(text: &str) -> String {
    format!("import \"channels.bh\" as ch\ndef main(){{{text}}}\n")
}

#[test]
fn canonical_alias_roots_and_owned_modes_survive_channel_binding() {
    let aliases="import \"channels.bh\" as ch\ntype Count=int\ntype Input=ch.Sender[Count]\ntype Output=ch.Receiver[Count]\ndef keep(s:Input)->ch.Sender[int]{return s}\ndef main(){let pair=ch.channel[int](1);let s=keep(pair.sender());s.close()}\n";
    let program = checked(aliases);
    assert_eq!(
        program.program.channels.as_ref().unwrap().pairs().count(),
        1
    );
    checked("import \"channels.bh\" as ch\ndef take(owned value:buffer)->buffer{return move value}\ndef main(){let value=buffer();let next=take(move value);print(len(next))}\n");
    rejects(&body("let pair=ch.channel[buffer](1);let sender=pair.sender();let b=buffer();let pending=sender.try_send(move b)"), "S038");
}

#[test]
fn file_resources_are_denied_directly_through_aliases_and_aggregates() {
    for declaration in [
        "type Payload=file_handle",
        "type Handle=file_handle\ntype Payload=Handle",
        "struct Payload{handle:file_handle}",
        "enum Payload{empty;owned(handle:file_handle)}",
    ] {
        rejects(&format!("import \"channels.bh\" as ch\n{declaration}\ndef main(){{let pair=ch.channel[Payload](1)}}\n"), "C101");
    }
    rejects(&body("let pair=ch.channel[file_handle](1)"), "C101");
    rejects("import \"channels.bh\" as ch\ndef worker(f:file_handle)->int{return 1}\ndef main(){let f=file_open(buffer(\"unused-path\"),1);var task=spawn(worker(f));print(join(task))}\n", "S020");
}

#[test]
fn intrinsic_storage_constructor_and_result_obligations_stay_checked() {
    rejects(&body("let pair=ch.Channel[int](1)"), "S029");
    rejects(
        &body("let pair=ch.channel[int](1);let sender=pair.sender();print(sender.__payload)"),
        "S026",
    );
    rejects(
        &body("let pair=ch.channel[int](1);let sender=pair.sender();sender.try_send(1)"),
        "S022",
    );
    rejects(&body("let pair=ch.channel[int](1);let receiver=pair.receiver();let pending=receiver.try_recv()"), "S037");
}

#[test]
fn modified_registered_bytes_and_factory_modes_revoke_authority() {
    let main = body("let pair=ch.channel[int](1)");
    let mut files = graph(&main, None);
    files[1].source.push_str("\n# edited standard module\n");
    assert!(bhasa::parse_files(&files)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "C101"));
    let mut program = checked(&main);
    let authority = program.program.channels.clone().unwrap();
    let pair_ids: Vec<_> = authority.pairs().collect();
    let factory = program
        .program
        .functions
        .iter_mut()
        .find(|f| matches!(f.return_type, Type::Record(id) if pair_ids.contains(&id)))
        .unwrap();
    factory.params[0].mode = ParamMode::Owned;
    assert!(authority.validate(&program.program).is_err());
    assert!(bhasa::codegen::emit_llvm(&program, TARGET).is_err());
}

#[test]
fn ownership_and_declaration_guards_keep_precedence_over_mutated_blocking_worker_calls() {
    let main="import \"channels.bh\" as ch\ndef worker(s:ch.Sender[int])->ch.SendResult[int]{return s.try_send(1)}\ndef main(){let pair=ch.channel[int](1);var task=spawn(worker(pair.sender()));let result=join(task);match result{case ch.SendResult[int].ok(status){}case ch.SendResult[int].error(failure){}}}\n";
    let mut program = checked(main);
    let authority = program.program.channels.clone().unwrap();
    let worker = program
        .program
        .functions
        .iter_mut()
        .find(|f| f.params.len() == 1 && authority.schema(f.params[0].ty).is_some())
        .unwrap();
    let key = worker.name.clone();
    if let bhasa::ast::StmtKind::Return(Some(value)) = &mut worker.body.as_mut().unwrap()[0].kind {
        if let bhasa::ast::ExprKind::Call { name, .. } = &mut value.kind {
            *name = name.replace(".try_send", ".send");
        }
    }
    program.signatures.get_mut(&key).unwrap().modes[0] = ParamMode::Owned;
    assert_eq!(
        bhasa::codegen::emit_llvm(&program, TARGET).unwrap_err()[0].code,
        "S038"
    );
    program.signatures.get_mut(&key).unwrap().modes[0] = ParamMode::Borrowed;
    program.signatures.get_mut(&key).unwrap().params[0] = Type::Bool;
    assert_eq!(
        bhasa::codegen::emit_llvm(&program, TARGET).unwrap_err()[0].code,
        "C001"
    );
}
