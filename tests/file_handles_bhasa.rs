#![cfg(feature = "bhasa-frontend")]

fn emit(source: &str) -> String {
    bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu")
        .unwrap_or_else(|errors| panic!("reference rejected {source}: {errors:?}"));
    bhasa::compile_with_bhasa(source, "x86_64-unknown-linux-gnu")
        .unwrap_or_else(|errors| panic!("native rejected {source}: {errors:?}"))
}

fn reject(source: &str, code: &str) {
    assert!(bhasa::check(source).is_err(), "reference accepted {source}");
    let errors = bhasa::compile_with_bhasa(source, "x86_64-unknown-linux-gnu").expect_err(source);
    assert!(
        errors.iter().any(|error| error.code == code),
        "{source}: {errors:?}"
    );
}

#[test]
fn streaming_builtin_signatures_lower_to_existing_integer_status_abi() {
    let ir = emit(
        r#"
        def main()->int {
            var text=file_open("stream",1)
            var bytes=file_open(buffer("stream"),0)
            var destination=buffer()
            var a=file_status(text)
            var b=file_read_chunk(bytes,destination,1048576)
            var c=file_write_chunk(text,destination)
            var d=file_seek(bytes,0)
            var e=file_position(bytes)
            var f=file_flush(text)
            var g=file_close(text)
            return a+b+c+d+e+f+g
        }
    "#,
    );
    assert!(ir.contains("; bhasa runtime: file\n"));
    for symbol in ["file_open", "file_open_str"] {
        assert!(
            ir.contains(&format!("call ptr @bhasa_rt_{symbol}(")),
            "{symbol}"
        );
    }
    for symbol in [
        "file_status",
        "file_read_chunk",
        "file_write_chunk",
        "file_seek",
        "file_position",
        "file_flush",
        "file_close",
    ] {
        assert!(
            ir.contains(&format!("call i64 @bhasa_rt_{symbol}(")),
            "{symbol}"
        );
    }
}

#[test]
fn local_return_record_enum_and_collection_cleanup_acquire_shared_handles() {
    let ir = emit(
        r#"
        struct Box {handle:file_handle}
        enum Open {error(code:int);ok(value:file_handle)}
        def identity(value:file_handle)->file_handle{return value}
        def produce()->Open{return Open.ok(file_open("missing",0))}
        def main(){
            let original=file_open("stream",1)
            var alias=original
            var box=Box(original)
            var box_copy=box
            var returned=identity(alias)
            var values=List[file_handle]();values.push(original)
            var mapped=Map[str,file_handle]();mapped.put("held",original)
            if true {var nested=returned}
            var wrapped=produce()
            match wrapped{case Open.ok(handle){var status=file_close(handle)}case Open.error(code){}}
            var status=file_close(original)
        }
    "#,
    );
    assert!(ir.contains("; bhasa runtime: file\n"));
    assert!(ir.contains("call ptr @bhasa_rt_file_clone(ptr "));
    assert!(ir.contains("call void @bhasa_rt_file_free(ptr "));
}

#[test]
fn every_file_operation_requires_use_and_wrong_signatures_reject() {
    for operation in [
        "file_status(handle)",
        "file_read_chunk(handle,buffer(),1)",
        "file_write_chunk(handle,buffer())",
        "file_seek(handle,0)",
        "file_position(handle)",
        "file_flush(handle)",
        "file_close(handle)",
    ] {
        reject(
            &format!("def main(){{var handle=file_open(\"missing\",0);{operation}}}"),
            "BH207",
        );
    }
    reject("def main(){file_open(\"missing\",0)}", "BH207");
    for operation in [
        "file_open(1,0)",
        "file_open(\"x\",true)",
        "file_status(1)",
        "file_read_chunk(handle,1,1)",
        "file_read_chunk(handle,buffer(),true)",
        "file_write_chunk(handle,1)",
        "file_seek(handle,true)",
        "file_position()",
        "file_flush(1)",
        "file_close(buffer())",
    ] {
        reject(
            &format!("def main(){{var handle=file_open(\"missing\",0);var bad={operation}}}"),
            "BH201",
        );
    }
    reject("struct file_handle{value:int}def main(){}", "BH212");
}

#[test]
fn direct_and_nested_file_argument_and_result_tasks_reject() {
    for source in [
        "def worker(h:file_handle)->int{return 1}def main(){var h=file_open(\"x\",0);var task=spawn(worker(h));var n=join(task)}",
        "def worker(n:int)->file_handle{return file_open(\"x\",0)}def main(){var task=spawn(worker(0));var h=join(task)}",
        "struct Box{h:file_handle}struct Outer{inner:Box}def worker(value:Outer)->int{return 1}def main(){var h=file_open(\"x\",0);var task=spawn(worker(Outer(Box(h))));var n=join(task)}",
        "enum Open{ok(h:file_handle);error(code:int)}def worker(n:int)->Open{return Open.ok(file_open(\"x\",0))}def main(){var task=spawn(worker(0));var r=join(task);match r{case Open.ok(h){}case Open.error(code){}}}",
        "def worker(value:List[file_handle])->int{return 1}def main(){var values=List[file_handle]();var task=spawn(worker(values));var n=join(task)}",
        "def worker(value:Map[str,file_handle])->int{return 1}def main(){var values=Map[str,file_handle]();var task=async_spawn(worker(values));var n=await(task)}",
    ] { reject(source,"BH209"); }
}

#[test]
fn worker_graph_checks_helpers_and_receiver_types_for_file_state() {
    for source in [
        "def helper(n:int)->int{var h=file_open(\"x\",0);return file_status(h)}def worker(n:int)->int{return helper(n)}def main(){var task=spawn(worker(0));var n=join(task)}",
        "def helper(h:file_handle)->int{return 1}def worker(n:int)->int{var h=file_open(\"x\",0);return helper(h)}def main(){var task=spawn(worker(0));var n=join(task)}",
        "struct Box{h:file_handle;def ignored(self)->int{return 1}}def helper(box:Box)->int{return box.ignored()}def worker(n:int)->int{var box=Box(file_open(\"x\",0));return helper(box)}def main(){var task=spawn(worker(0));var n=join(task)}",
        "def worker(n:int)->int{var h=file_open(\"x\",0);return file_close(h)}def main(){var task=async_spawn(worker(0));var n=await(task)}",
    ] { reject(source,"BH209"); }
    // Both direct and nested file-free scalar/owned tasks remain admitted.
    emit("struct Box{data:buffer}def worker(value:Box)->Box{return value}def main(){var task=spawn(worker(Box(buffer(\"x\"))));var result=join(task)}");
}
