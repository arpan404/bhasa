//! Cross-feature ownership/FileHandle source and public checked-AST guards.
use bhasa::ast::*;
use bhasa::project::{ModuleImport, SourceFile};
const TARGET: &str = "x86_64-unknown-linux-gnu";
fn accepted(source: &str) -> CheckedProgram {
    let p = bhasa::check(source).unwrap_or_else(|errors| panic!("{source}\n{errors:?}"));
    bhasa::codegen::emit_llvm(&p, TARGET).unwrap();
    p
}
fn rejected(source: &str, code: &str) {
    let errors = bhasa::check(source).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == code),
        "{source}\n{errors:?}"
    );
}
fn graph(main: &str, library: &str) -> Vec<SourceFile> {
    vec![
        SourceFile {
            path: "/review/main.bh".into(),
            source: main.into(),
            base: 0,
            import_spans: vec![],
            imports: vec![ModuleImport {
                path: "library.bh".into(),
                target: "/review/library.bh".into(),
                alias: Some("lib".into()),
                span: Span::default(),
            }],
        },
        SourceFile {
            path: "/review/library.bh".into(),
            source: library.into(),
            base: main.len() + 1,
            import_spans: vec![],
            imports: vec![],
        },
    ]
}
fn checked_graph(files: &[SourceFile]) -> Result<CheckedProgram, Vec<bhasa::Diagnostic>> {
    let modules = bhasa::modules::expand_files(files)?;
    let generic = bhasa::generics::expand_with_type_roots(
        &modules.source,
        &modules.boundaries,
        &modules.type_roots()?,
    )
    .map_err(|e| modules.map_diagnostics(e))?;
    let mut p =
        bhasa::generics::parse_expansion(&generic).map_err(|e| modules.map_diagnostics(e))?;
    modules.remap_program(&mut p);
    let checked = bhasa::sema::check(p)
        .map_err(|e| modules.display_diagnostics(generic.display_diagnostics(e)))?;
    modules.check_privacy(&checked, |n| generic.original_name(n))?;
    Ok(checked)
}
#[test]
fn canonical_alias_modes_and_ordinary_file_borrows_remain_separate() {
    let p=accepted("type Bytes=buffer;type Numbers=ints;type File=file_handle;struct Holder{value:File;def read(self)->int{return file_status(self.value)}} def consume(owned bytes:Bytes,owned values:Numbers){} def retain(file:File)->File{return file} def generic[T](value:T)->T{return value} def main(){var file=file_open(\"unused\",0);var kept=generic[File](retain(file));var holder=Holder(kept);var status=holder.read();consume(buffer(),ints())}");
    assert_eq!(ParamMode::default(), ParamMode::Borrowed);
    for f in &p.program.functions {
        assert_eq!(
            p.signatures[&f.name].modes,
            f.params.iter().map(|p| p.mode).collect::<Vec<_>>()
        );
        for param in &f.params {
            if param.ty == Type::FileHandle {
                assert_eq!(param.mode, ParamMode::Borrowed);
            }
        }
    }
    for r in &p.program.records {
        for m in &r.methods {
            assert!(m.params.iter().all(|p| p.mode == ParamMode::Borrowed));
        }
    }
    assert!(p
        .signatures
        .values()
        .any(|s| s.modes == [ParamMode::Owned, ParamMode::Owned]));
    rejected("type File=file_handle;def take(owned file:File){}", "S038");
    rejected(
        "type File=file_handle;def main(){var file=file_open(\"unused\",0);var copied=move file}",
        "S038",
    );
    rejected(
        "type File=file_handle;def change(file:File){file=file_open(\"unused\",0)}",
        "S021",
    );
    rejected(
        "struct Holder{file:file_handle} def take(owned holder:Holder){}",
        "S038",
    );
    rejected("struct Holder{def take(self,owned bytes:buffer){}}", "S038");
    rejected("extern def take(owned bytes:buffer)", "S038");
    rejected(
        "def take[T](owned bytes:T){} def main(){take[buffer](buffer())}",
        "S038",
    );
}
#[test]
fn imported_private_aliases_keep_nominal_authority_and_file_identity() {
    let lib="struct Holder{private resource:file_handle;def status(self)->int{return file_status(self.resource)}} type Handle=Holder;private type Secret=file_handle;def make()->Handle{return Holder(file_open(\"unused\",0))}";
    let source="struct Holder{value:int} type Local=Holder;type Remote=lib.Handle;def main(){var local=Local(42);var remote:Remote=lib.make();var status=remote.status();print(local.value)}";
    let p = checked_graph(&graph(source, lib)).unwrap();
    bhasa::codegen::emit_llvm(&p, TARGET).unwrap();
    assert_eq!(
        p.program
            .records
            .iter()
            .filter(|r| p.program.contains_file_handle(Type::Record(
                p.program
                    .type_names
                    .iter()
                    .position(|n| n == &r.name)
                    .unwrap()
            )))
            .count(),
        1
    );
    for source in ["type Hidden=lib.Secret;def main(){}","type Remote=lib.Handle;def main(){var remote:Remote=lib.make();var resource=remote.resource}","type Remote=lib.Handle;def main(){var remote=Remote(file_open(\"unused\",0))}"] {
        let errors=checked_graph(&graph(source,lib)).unwrap_err();assert!(errors.iter().any(|e|e.message.contains("private")),"{source}: {errors:?}");
    }
    let source="type Remote=lib.Handle;def work(value:Remote)->int{return value.status()} def main(){var job=spawn(work(lib.make()));var result=join(job)}";
    let errors = checked_graph(&graph(source, lib)).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "S020"), "{errors:?}");
}
#[test]
fn pending_file_result_aliases_still_require_direct_handling() {
    let prefix="enum Result[T]{ok(value:T);error(code:int)} type Open=Result[file_handle];def make()->Open{return Open.ok(file_open(\"unused\",0))} def observe(value:Open){} ";
    for body in ["var result=make()","var result=make();observe(result)","var result=make();var copied=result;match copied{case Open.ok(file){var status=file_status(file)}case Open.error(code){print(code)}}"] {
        rejected(&format!("{prefix}def main(){{{body}}}"),"S037");
    }
    accepted(&format!("{prefix}def main(){{var result=make();match result{{case Open.ok(file){{var status=file_status(file);var code=file_close(file)}}case Open.error(code){{print(code)}}}}}}"));
    accepted(&format!(
        "{prefix}def forward()->Open{{var result=make();return result}}"
    ));
    accepted(&format!(
        "{prefix}def forward()->Open{{var result=make();var file=try result;return Open.ok(file)}}"
    ));
    rejected(
        &format!("{prefix}def main(){{var result=make();var moved=move result}}"),
        "S038",
    );
    let mut checked=accepted(&format!("{prefix}def main(){{var result=make();match result{{case Open.ok(file){{var status=file_status(file)}}case Open.error(code){{print(code)}}}}}}"));
    let body = checked
        .program
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap()
        .body
        .as_mut()
        .unwrap();
    body.pop();
    let errors = bhasa::codegen::emit_llvm(&checked, TARGET).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "S037"), "{errors:?}");
}
#[test]
fn nested_generic_file_worker_schemas_are_rejected_for_both_submitters() {
    let schemas=[("type File=file_handle;struct Inner[T]{value:T} enum Outer[T]{none;some(value:T)} type Wrapped=Outer[Inner[File]];def work(value:Wrapped)->int{return 1}","Wrapped.none()"),
    ("type File=file_handle;type Files=List[File];def work(value:Files)->int{return 1}","Files()"),
    ("type File=file_handle;type Files=Map[int,File];def work(value:Files)->int{return 1}","Files()"),
    ("struct Box[T]{value:T} type FileBox=Box[file_handle];def work(value:int)->FileBox{return FileBox(file_open(\"unused\",0))}","1")];
    for submit in ["spawn", "async_spawn"] {
        for (defs, arg) in schemas {
            rejected(
                &format!("{defs} def main(){{var job={submit}(work({arg}));var result=join(job)}}"),
                "S020",
            );
        }
    }
    accepted("struct Box[T]{value:T} type IntBox=Box[int];def work(value:IntBox)->int{return value.value} def main(){var job=spawn(work(IntBox(42)));var result=join(job);print(result)}");
}
#[test]
fn nested_file_operations_and_recursive_methods_keep_worker_restrictions() {
    let helpers=[
    "def helper(n:int)->int{if n==0{var file=file_open(\"unused\",0);return file_status(file)}return work(n-1)}def work(n:int)->int{return helper(n)}",
    "def helper(n:int)->int{var file=file_open(\"unused\",0);return file_position(file)}def work(n:int)->int{if helper(n)>0{return 1}return 0}",
    "def helper(n:int)->int{var file=file_open(\"unused\",0);return file_position(file)}def work(n:int)->int{var values=ints();push(values,helper(n));return len(values)}",
    "struct Worker{def read(self,n:int)->int{if n==0{var file=file_open(\"unused\",0);return file_status(file)}return self.next(n-1)}def next(self,n:int)->int{return self.read(n)}}def work(n:int)->int{return Worker().next(n)}"];
    for submit in ["spawn", "async_spawn"] {
        for defs in helpers {
            rejected(
                &format!("{defs}def main(){{var job={submit}(work(1));var result=join(job)}}"),
                "S020",
            );
        }
    }
    accepted("extern def foreign(n:int)->int;struct Worker{def foreign(self,n:int)->int{return n}def read(self,n:int)->int{return self.foreign(n)}}def work(n:int)->int{return Worker().read(n)}def main(){var job=spawn(work(foreign(1)));var result=join(job)}");
    rejected("extern def foreign(n:int)->int;struct Worker{def read(self,n:int)->int{return foreign(n)}}def work(n:int)->int{return Worker().read(n)}def main(){var job=spawn(work(1));var result=join(job)}","S020");
    rejected("def child(n:int)->int{return n} struct Worker{def read(self,n:int)->int{var job=async_spawn(child(n));var value=await(job);return value}}def work(n:int)->int{return Worker().read(n)}def main(){var job=async_spawn(work(1));var result=await(job)}","S020");
}
#[test]
fn public_emitter_rejects_corrupted_file_worker_body_and_method() {
    for submit in ["spawn", "async_spawn"] {
        let mut p=accepted(&format!("def work(n:int)->int{{return n}}def main(){{var job={submit}(work(1));var result=join(job)}}"));
        let hazardous=accepted("def helper(n:int)->int{var file=file_open(\"unused\",0);return file_status(file)}def work(n:int)->int{if helper(n)>0{return 1}return 0}");
        let work = hazardous
            .program
            .functions
            .iter()
            .find(|f| f.name == "work")
            .unwrap()
            .clone();
        p.program.functions[0].body = work.body;
        p.program
            .functions
            .push(hazardous.program.functions[0].clone());
        p.signatures
            .insert("helper".into(), hazardous.signatures["helper"].clone());
        let errors = bhasa::codegen::emit_llvm(&p, TARGET).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("single-thread file resource")),
            "{errors:?}"
        );
        let source=format!("struct Worker{{def read(self,n:int)->int{{return n}}}}def work(n:int)->int{{return Worker().read(n)}}def main(){{var job={submit}(work(1));var result=join(job)}}");
        let mut p = accepted(&source);
        let hazard=accepted("struct Worker{def read(self,n:int)->int{var file=file_open(\"unused\",0);return file_status(file)}}");
        p.program.records[0].methods[0].body = hazard.program.records[0].methods[0].body.clone();
        let errors = bhasa::codegen::emit_llvm(&p, TARGET).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("single-thread file resource")),
            "{errors:?}"
        );
    }
}
#[test]
fn public_emitter_rejects_corrupted_mode_and_nominal_metadata() {
    let source="type File=file_handle;struct Holder{value:File;def status(self)->int{return file_status(self.value)}}def retain(file:File)->File{return file}def main(){var file=retain(file_open(\"unused\",0));var holder=Holder(file);var status=holder.status()}";
    let p = accepted(source);
    let retained_key = p
        .program
        .functions
        .iter()
        .find(|f| {
            f.params.len() == 1
                && f.params[0].ty == Type::FileHandle
                && f.return_type == Type::FileHandle
        })
        .unwrap()
        .name
        .clone();
    let mut corrupt = p.clone();
    corrupt
        .signatures
        .get_mut(&retained_key)
        .unwrap()
        .modes
        .clear();
    assert!(bhasa::codegen::emit_llvm(&corrupt, TARGET)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "S038"));
    let mut corrupt = p.clone();
    let f = corrupt
        .program
        .functions
        .iter_mut()
        .find(|f| f.name == retained_key)
        .unwrap();
    f.params[0].mode = ParamMode::Owned;
    corrupt.signatures.get_mut(&retained_key).unwrap().modes[0] = ParamMode::Owned;
    assert!(bhasa::codegen::emit_llvm(&corrupt, TARGET)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "S038"));
    let mut corrupt = p.clone();
    let method = &mut corrupt.program.records[0].methods[0];
    method.params[0].mode = ParamMode::Owned;
    let key = method_key(&corrupt.program.records[0].name, "status");
    corrupt.signatures.get_mut(&key).unwrap().modes[0] = ParamMode::Owned;
    assert!(bhasa::codegen::emit_llvm(&corrupt, TARGET)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "S038"));
    let mut corrupt = p;
    corrupt.program.records[0].fields[0].ty = Type::Record(usize::MAX);
    let errors = bhasa::codegen::emit_llvm(&corrupt, TARGET).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "C001"), "{errors:?}");
}
#[test]
fn public_emitter_rejects_corrupted_foreign_worker_call() {
    let source="extern def foreign(n:int)->int;def work(n:int)->int{return n}def main(){var job=spawn(work(1));var result=join(job)}";
    let mut p = accepted(source);
    let hazard = accepted("extern def foreign(n:int)->int;def work(n:int)->int{return foreign(n)}");
    p.program
        .functions
        .iter_mut()
        .find(|f| f.name == "work")
        .unwrap()
        .body = hazard
        .program
        .functions
        .iter()
        .find(|f| f.name == "work")
        .unwrap()
        .body
        .clone();
    assert!(
        bhasa::codegen::emit_llvm(&p, TARGET).is_err(),
        "public emission accepted corrupted worker -> extern call graph"
    );
}
#[test]
fn public_emitter_rejects_corrupted_pooled_worker_schedule() {
    let source="def child(n:int)->int{return n}def work(n:int)->int{return n}def main(){var job=async_spawn(work(1));var result=await(job)}";
    let mut p = accepted(source);
    let hazard=accepted("def child(n:int)->int{return n}def work(n:int)->int{var job=async_spawn(child(n));var result=await(job);return result}");
    p.program
        .functions
        .iter_mut()
        .find(|f| f.name == "work")
        .unwrap()
        .body = hazard
        .program
        .functions
        .iter()
        .find(|f| f.name == "work")
        .unwrap()
        .body
        .clone();
    assert!(
        bhasa::codegen::emit_llvm(&p, TARGET).is_err(),
        "public emission accepted corrupted pooled worker -> nested scheduling graph"
    );
}

mod additional_worker_guards {
    use bhasa::ast::CheckedProgram;
    const TARGET: &str = "x86_64-unknown-linux-gnu";
    fn checked(source: &str) -> CheckedProgram {
        bhasa::check(source).unwrap_or_else(|e| panic!("{source}\n{e:?}"))
    }
    fn error(p: &CheckedProgram, needle: &str) {
        let e = bhasa::codegen::emit_llvm(p, TARGET).unwrap_err();
        assert!(
            e.iter()
                .any(|e| e.code == "C001" && e.message.contains(needle)),
            "{e:?}"
        );
    }
    #[test]
    fn corrupted_recursive_methods_reject_foreign_and_pool_without_name_confusion() {
        for (hazard,needle) in [
 ("if n==0{return foreign(n)}return self.next(n-1)","extern function 'foreign'"),
 ("if n==0{var job=async_spawn(child(n));var result=await(job);return result}return self.next(n-1)","worker reaches '")]{
  for submit in ["spawn","async_spawn"]{
   let prefix="extern def foreign(n:int)->int;def child(n:int)->int{return n}";
   let good_methods="struct Worker{def read(self,n:int)->int{if n==0{return n}return self.next(n-1)}def next(self,n:int)->int{return self.read(n)}}";
   let suffix=format!("def work(n:int)->int{{return Worker().next(n)}}def main(){{var job={submit}(work(1));var result=join(job)}}");
   let bad_methods=format!("struct Worker{{def read(self,n:int)->int{{{hazard}}}def next(self,n:int)->int{{return self.read(n)}}}}");
   let mut p=checked(&format!("{prefix}{good_methods}{suffix}"));
   let bad=checked(&format!("{prefix}{bad_methods}"));
   let submitted=bhasa::check(&format!("{prefix}{bad_methods}{suffix}")).unwrap_err();assert!(submitted.iter().any(|e|e.code=="S020"),"{submitted:?}");
   p.program.records[0].methods[0].body=bad.program.records[0].methods[0].body.clone();error(&p,needle);
  }
 }
    }
    #[test]
    fn caller_foreign_argument_and_same_spelled_safe_method_remain_legal() {
        for submit in ["spawn", "async_spawn"] {
            let p=checked(&format!("extern def foreign(n:int)->int;struct Worker{{def foreign(self,n:int)->int{{return n}}def read(self,n:int)->int{{return self.foreign(n)}}}}def work(n:int)->int{{return Worker().read(n)}}def main(){{var job={submit}(work(foreign(1)));var result=join(job)}}"));
            bhasa::codegen::emit_llvm(&p, TARGET).unwrap();
        }
    }
}
