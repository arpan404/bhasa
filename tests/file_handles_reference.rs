use bhasa::ast::Type;

const TARGET: &str = "x86_64-unknown-linux-gnu";

#[test]
fn file_type_signatures_and_aggregate_cleanup_use_additive_native_abi() {
    let checked = bhasa::check(
        r#"
struct Holder { resource: file_handle }
enum Opened { ok(value: Holder); error(code: int) }
def retain(value: Holder) -> Holder { return value }
def wrap(value: Holder) -> Opened { return Opened.ok(value) }
def probe(path: buffer) -> int {
    var file = file_open(path, 0)
    var copied = retain(Holder(file))
    var result = wrap(copied)
    match result {
        case Opened.ok(owned) {
            var retained = retain(owned)
            var status = file_status(retained.resource)
        }
        case Opened.error(code) { return code }
    }
    var bytes = buffer()
    var read_status = file_read_chunk(file, bytes, 10)
    var write_status = file_write_chunk(file, bytes)
    var seek_status = file_seek(file, 0)
    var position = file_position(file)
    var flush_status = file_flush(file)
    var close_status = file_close(file)
    return file_status(file)
}
def other() -> file_handle { return file_open("unused", 0) }
"#,
    )
    .unwrap();
    assert!(Type::FileHandle.is_owned());
    assert_eq!(Type::FileHandle.to_string(), "file_handle");
    assert!(checked
        .program
        .contains_file_handle(checked.program.functions[0].params[0].ty));
    let ir = bhasa::codegen::emit_llvm(&checked, TARGET).unwrap();
    for expected in [
        "; bhasa runtime: file",
        "@bhasa_rt_file_open(ptr, i64)",
        "@bhasa_rt_file_open_str(ptr, i64)",
        "@bhasa_rt_file_clone(ptr)",
        "@bhasa_rt_file_free(ptr)",
        "@bhasa_rt_file_read_chunk(ptr, ptr, i64)",
        "@bhasa_rt_file_write_chunk(ptr, ptr)",
        "@bhasa_rt_file_seek(ptr, i64)",
        "@bhasa_rt_file_position(ptr)",
        "@bhasa_rt_file_flush(ptr)",
        "@bhasa_rt_file_close(ptr)",
    ] {
        assert!(ir.contains(expected), "missing {expected}");
    }
}

#[test]
fn file_statuses_and_wrong_types_are_checked() {
    for call in [
        "file_open(\"unused\", 0)",
        "file_status(file)",
        "file_read_chunk(file, buffer(), 1)",
        "file_write_chunk(file, buffer())",
        "file_seek(file, 0)",
        "file_position(file)",
        "file_flush(file)",
        "file_close(file)",
    ] {
        let source = format!("def main(){{var file=file_open(\"unused\",0); {call}}}");
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "S022"),
            "{source}: {errors:?}"
        );
    }
    for call in [
        "file_open(1, 0)",
        "file_open(\"unused\", true)",
        "file_status(buffer())",
        "file_read_chunk(file, 1, 1)",
        "file_write_chunk(file, \"bad\")",
        "file_seek(file, false)",
        "file_position(1)",
        "file_flush(true)",
        "file_close(1)",
    ] {
        let source = format!("def main(){{var file=file_open(\"unused\",0); var status={call}}}");
        assert!(bhasa::check(&source).is_err(), "{source}");
    }
    for declaration in ["struct file_handle {n:int}", "enum file_handle {none}"] {
        assert!(bhasa::check(declaration).is_err());
    }
}

#[test]
fn workers_reject_file_state_and_transitive_operations_for_spawn_and_pool() {
    for submit in ["spawn", "async_spawn"] {
        for definitions in [
            "def work(n:int)->int{var f=file_open(\"unused\",0);return file_status(f)}",
            "def helper(n:int)->int{var f=file_open(\"unused\",0);return file_status(f)} def work(n:int)->int{return helper(n)}",
            "struct Host {n:int; def use(self)->int{var f=file_open(\"unused\",0);return file_status(f)}} def work(n:int)->int{return Host(n).use()}",
            "def work(n:int)->file_handle{return file_open(\"unused\",0)}",
        ] {
            let wait = if submit == "spawn" { "join" } else { "await" };
            let source = format!("{definitions} def main(){{var job={submit}(work(1));var result={wait}(job)}}");
            let errors = bhasa::check(&source).unwrap_err();
            assert!(errors.iter().any(|error| error.code == "S020"), "{source}: {errors:?}");
        }
        for (definitions, argument) in [
            (
                "def work(f:file_handle)->int{return 1}",
                "file_open(\"unused\",0)",
            ),
            (
                "struct Box {f:file_handle} def work(f:Box)->int{return 1}",
                "Box(file_open(\"unused\",0))",
            ),
            (
                "enum Box {none; some(f:file_handle)} def work(f:Box)->int{return 1}",
                "Box.none()",
            ),
            (
                "def work(f:List[file_handle])->int{return 1}",
                "List[file_handle]()",
            ),
            (
                "def work(f:Map[int,file_handle])->int{return 1}",
                "Map[int,file_handle]()",
            ),
        ] {
            let wait = if submit == "spawn" { "join" } else { "await" };
            let source = format!("{definitions} def main(){{var job={submit}(work({argument}));var result={wait}(job)}}");
            let errors = bhasa::check(&source).unwrap_err();
            assert!(
                errors.iter().any(|error| error.code == "S020"),
                "{source}: {errors:?}"
            );
        }
    }
}

#[test]
fn corrupt_checked_worker_body_cannot_access_file_resources() {
    let mut checked = bhasa::check(
        "def work(n:int)->int{return n} def main(){var job=spawn(work(1));var result=join(job)}",
    )
    .unwrap();
    let mut hazardous =
        bhasa::check("def work(n:int)->int{var f=file_open(\"unused\",0);return file_status(f)}")
            .unwrap();
    checked.program.functions[0].body = hazardous.program.functions.remove(0).body;
    let errors = bhasa::codegen::emit_llvm(&checked, TARGET).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("single-thread file resource")),
        "{errors:?}"
    );
}

#[test]
fn file_aliases_keep_borrowed_signatures_and_reject_owned_transfer() {
    let checked = bhasa::check("type File=file_handle\ndef identity(value:File)->File{return value} def main(){var file=file_open(\"unused\",0);var copy=identity(file);var status=file_status(copy)}").unwrap();
    let identity = checked
        .program
        .functions
        .iter()
        .find(|function| function.params.len() == 1)
        .unwrap();
    let signature = &checked.signatures[&identity.name];
    assert_eq!(signature.modes, vec![bhasa::ast::ParamMode::Borrowed]);
    assert_eq!(signature.params, vec![Type::FileHandle]);
    for source in [
        "type File=file_handle\ndef take(owned file:File){}def main(){}",
        "type File=file_handle\ndef main(){var file=file_open(\"unused\",0);var copy=move file}",
    ] {
        let errors = bhasa::check(source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "S038"),
            "{errors:?}"
        );
    }
}
