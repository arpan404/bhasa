mod common;
use common::*;
use std::{fs, path::Path, process::Command};

const PROGRAM: &str = r#"
struct Row { text:buffer; values:ints }
enum Reply { none; row(value:Row) }
def keep[T](value:T)->T { return value }
def increment(value:int)->int {return value+1}
def flip(value:bool)->bool {return not value}
def half(value:float)->float {return value/2.0}
def bytes(value:buffer)->buffer {set(value,0,88);return value}
def numbers(value:ints)->ints {push(value,9);return value}
def record(value:Row)->Row {set(value.text,0,82);push(value.values,8);return value}
def choice(value:Reply)->Reply {
    match value {case Reply.none {return Reply.none()} case Reply.row(row) {return Reply.row(record(row))}}
}
def list(value:List[Row])->List[Row] {value.push(Row(buffer("new"),ints()));return value}
def map(value:Map[str,Row])->Map[str,Row] {value.put("new",Row(buffer("map"),ints()));return value}
def discard(value:buffer) {set(value,0,68)}
def string(value:str)->str {return value}
def convert(value:int)->buffer {return to_buffer(value)}
def int_void(value:int) {}
def int_bool(value:int)->bool {return value==42}
def float_int(value:float)->int {return int(value)}
def nested(value:buffer)->buffer {var inner=spawn(bytes(value));var result=join(inner);return result}
def main()->int {
    var scalar=spawn(increment(41));var integer=await(scalar)
    var boolean_task=async_spawn(flip(false));var boolean=join(boolean_task)
    var float_task=spawn(half(5.0));var floating=join(float_task)
    var string_task=async_spawn(string("literal"));var text=await(string_task)
    if integer!=42 or not boolean or floating!=2.5 {return 1}
    if not same(buffer(text),"literal") {return 2}
    var source=buffer("old")
    var buffer_task=spawn(bytes(source));set(source,1,65);var result=join(buffer_task)
    if not same(result,"Xld") or not same(source,"oAd") {return 3}
    var input=ints();push(input,7)
    var ints_task=async_spawn(numbers(input));push(input,6);var output=await(ints_task)
    if len(output)!=2 or get(output,1)!=9 or get(input,1)!=6 {return 4}
    var row=Row(buffer("old"),input)
    var row_task=spawn(record(row));row.text=buffer("caller");var row_result=join(row_task)
    if not same(row_result.text,"Rld") or len(row_result.values)!=3 or not same(row.text,"caller") {return 5}
    var enum_task=async_spawn(choice(Reply.row(Row(buffer("old"),ints()))))
    var reply=await(enum_task)
    match reply {case Reply.none {return 6} case Reply.row(value) {if not same(value.text,"Rld") or len(value.values)!=1 {return 7}}}
    var items=List[Row]();items.push(Row(buffer("saved"),ints()))
    var list_task=spawn(list(items));items.clear();var list_result=join(list_task)
    if list_result.count()!=2 or not same(list_result.get(0).text,"saved") or items.count()!=0 {return 8}
    var entries=Map[str,Row]();entries.put("saved",Row(buffer("entry"),ints()))
    var map_task=async_spawn(map(entries));entries.clear();var map_result=await(map_task)
    if map_result.count()!=2 or not same(map_result.get("saved").text,"entry") or entries.count()!=0 {return 9}
    var generic_task=spawn(keep[List[Row]](list_result));list_result.clear();var generic_result=join(generic_task)
    if generic_result.count()!=2 {return 10}
    var nested_task=async_spawn(nested(buffer("old")));var nested_result=await(nested_task)
    if not same(nested_result,"Xld") {return 11}
    var void_int_task=spawn(int_void(1));join(void_int_task)
    var bool_int_task=spawn(int_bool(42));var from_int=join(bool_int_task)
    var int_float_task=async_spawn(float_int(42.5));var from_float=await(int_float_task)
    if not from_int or from_float!=42 {return 14}
    var temporary_task=spawn(bytes(Row(buffer("old"),ints()).text));var temporary=join(temporary_task)
    if not same(temporary,"Xld") {return 15}
    var convert_task=spawn(convert(42));var converted=join(convert_task)
    if not same(converted,"42") {return 12}
    for i in range(24) {
        var void_task=async_spawn(discard(source));await(void_task)
        var ignored=spawn(bytes(buffer("temporary")));join(ignored)
        var copied=async_spawn(keep[Row](row));await(copied)
        var kept=spawn(keep[bool](true));var flag=join(kept)
        if not flag or not same(source,"oAd") {return 13}
    }
    print("typed tasks ownership ok")
    return 0
}
"#;

const INVALID: &[&str] = &[
    "def invalid()->task{return 1} def main(){var job=invalid();join(job)}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(3));join(t)}",
    "def worker(v:buffer){} def main(){var t=spawn(worker(buffer()));var r=join(t)}",
    "def worker(v:buffer){} def main(){var t=async_spawn(worker(buffer()));var r=await(t)}",
    "def worker(v:bool)->bool{return v} def main(){var t=spawn(worker(true));var r:int=join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));join(t);join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()))}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));if true{join(t)};join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));var x=t;join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));t=spawn(worker(buffer()));join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){while true{var t=spawn(worker(buffer()));break}}",
    "def worker(v:buffer)->buffer{return v} def main(){for i in range(1){var t=spawn(worker(buffer()));continue}}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));return}",
    "def worker(v:buffer)->buffer{return v} def main(){var t=spawn(worker(buffer()));if true{var t=1};join(t)}",
    "def worker(v:buffer)->buffer{return v} def main(){var worker=1;var t=spawn(worker(buffer()));join(t)}",
    "def worker()->bool{return true} def main(){var t=spawn(worker());join(t)}",
    "def worker(a:buffer,b:int)->buffer{return a} def main(){var t=spawn(worker(buffer(),1));join(t)}",
    "struct S{v:int;def worker(self,b:buffer)->buffer{return b}} def main(){var s=S(1);var t=spawn(s.worker(buffer()));join(t)}",
    "extern def unsafe(v:int)->int\ndef helper(v:buffer)->buffer{var r=unsafe(1);return v} def worker(v:buffer)->buffer{return helper(v)} def main(){var t=spawn(worker(buffer()));join(t)}",
    "extern def unsafe(v:int)->int\ndef helper[T](v:T)->T{var r=unsafe(1);return v} def main(){var t=spawn(helper[buffer](buffer()));join(t)}",
    "def inner(v:buffer)->buffer{return v} def worker(v:buffer)->buffer{var t=async_spawn(inner(v));var r=await(t);return r} def main(){var t=spawn(worker(buffer()));join(t)}",
    "def inner(v:buffer)->buffer{return v} def worker(v:buffer)->buffer{var t=spawn(inner(v));var r=await(t);return r} def main(){var t=async_spawn(worker(buffer()));await(t)}",
];

fn seed_source() -> String {
    ["lexer.bh", "emitter.bh", "driver.bh"]
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .join("\n")
}
fn emit(seed: &Path, workspace: &Workspace, source: &str) -> std::process::Output {
    let source = workspace.write("input.bh", source);
    Command::new(seed)
        .args([
            "emit-llvm",
            as_str(&source),
            "--target",
            &bhasa::target::Target::Native.triple_for_ir().unwrap(),
        ])
        .output()
        .unwrap()
}

// Every generated aggregate and runtime allocation is counted across worker
// threads. This detects leaks on platforms where ASan lacks LeakSanitizer.
const TRACKER: &str = r#"
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic int64_t live;
static pthread_once_t once = PTHREAD_ONCE_INIT;
static void check(void) {
    int64_t count = atomic_load(&live);
    if (count != 0) {fprintf(stderr,"live task allocations: %lld\n",(long long)count);abort();}
}
static void setup(void) {if(atexit(check)!=0)abort();}
void *tracked_malloc(size_t size) {
    if(pthread_once(&once,setup)!=0)abort();
    void *p=malloc(size);if(p)atomic_fetch_add(&live,1);return p;
}
void *tracked_realloc(void *old,size_t size) {
    if(pthread_once(&once,setup)!=0)abort();
    int empty=old==NULL;void *p=realloc(old,size);if(p && empty)atomic_fetch_add(&live,1);return p;
}
void tracked_free(void *p) {if(p)atomic_fetch_sub(&live,1);free(p);}
"#;
const TRACKER_HEADER: &str = r#"
#include <stdlib.h>
void *tracked_malloc(size_t);
void *tracked_realloc(void *,size_t);
void tracked_free(void *);
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
"#;

fn link(
    workspace: &Workspace,
    clang: &std::ffi::OsStr,
    ir: &str,
    name: &str,
    optimization: &str,
    track: bool,
) -> std::path::PathBuf {
    let mut command = Command::new(clang);
    command.arg(optimization);
    let mut ir = ir.to_string();
    if track {
        ir = ir
            .replace("@malloc(", "@tracked_malloc(")
            .replace("@free(", "@tracked_free(");
        ir = ir
            .lines()
            .map(|line| {
                if line.starts_with("define ") {
                    line.replace(" {", " sanitize_address {")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tracker = workspace.write("tracker.c", TRACKER);
        let object = workspace.file("tracker.o");
        succeeds(
            &Command::new(clang)
                .args([
                    optimization,
                    "-fsanitize=address,undefined",
                    "-fno-omit-frame-pointer",
                    "-c",
                ])
                .arg(tracker)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap(),
        );
        command
            .arg("-fsanitize=address,undefined")
            .arg("-fno-omit-frame-pointer")
            .arg("-include")
            .arg(workspace.write("tracker.h", TRACKER_HEADER))
            .arg(object);
    }
    let executable = workspace.file(name);
    command
        .arg(workspace.write(&format!("{name}.ll"), &ir))
        .arg(fixture("runtime/core.c"))
        .arg(fixture("runtime/collections.c"))
        .arg(fixture("runtime/concurrency.c"))
        .arg("-pthread")
        .arg("-o")
        .arg(&executable);
    succeeds(&command.output().unwrap());
    executable
}

#[test]
fn fresh_bhasa_typed_tasks_match_reference_and_rebuild_with_owned_cleanup() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-tasks-native");
    let compiler_source = seed_source();
    let source = workspace.write("seed.bh", &compiler_source);
    let built = build(&workspace, &source, true);
    let seed = workspace.file("stage-a");
    fs::rename(built, &seed).unwrap();
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let reference = bhasa::compile_to_llvm(PROGRAM, &target).unwrap();
    let expanded = bhasa::generics::expand(PROGRAM, &[]).unwrap();
    let output = emit(&seed, &workspace, &expanded.source);
    succeeds(&output);
    let native = stdout(&output);
    for (frontend, ir) in [("reference", &reference), ("bhasa", &native)] {
        assert!(ir.contains("; bhasa runtime: threads"));
        assert!(ir.contains("call ptr @bhasa_rt_spawn_owned"));
        assert!(ir.contains("call ptr @bhasa_rt_async_spawn_owned"));
        assert!(ir.contains("call ptr @bhasa_rt_spawn_i64"));
        for optimization in ["-O0", "-O2"] {
            let executable = link(&workspace, &clang, ir, frontend, optimization, true);
            let output = execute(&executable, &workspace);
            succeeds(&output);
            assert_eq!(stdout(&output), "typed tasks ownership ok\n");
        }
    }
    for source in INVALID {
        let errors = bhasa::check(source).expect_err(&format!("reference accepted {source}"));
        if source.starts_with("extern ") {
            assert!(
                errors.iter().any(|error| error.code == "S020"),
                "{source}: {errors:?}"
            );
        }
        if let Ok(expanded) = bhasa::generics::expand(source, &[]) {
            let result = emit(&seed, &workspace, &expanded.source);
            assert!(
                !result.status.success(),
                "seed accepted {source}: {}",
                describe(&result)
            );
            assert!(result.stdout.is_empty());
            if source.starts_with("extern ") {
                assert!(
                    stderr(&result).contains("BH209"),
                    "{source}: {}",
                    describe(&result)
                );
            }
        }
    }
    let compiler_ir = emit(&seed, &workspace, &compiler_source);
    succeeds(&compiler_ir);
    let stage_b = link(
        &workspace,
        &clang,
        &stdout(&compiler_ir),
        "stage-b",
        "-O2",
        false,
    );
    let rebuilt = emit(&stage_b, &workspace, &compiler_source);
    succeeds(&rebuilt);
    assert_eq!(stdout(&compiler_ir), stdout(&rebuilt));
    let replay = emit(&stage_b, &workspace, &expanded.source);
    succeeds(&replay);
    assert_eq!(native, stdout(&replay));
}

#[cfg(feature = "bhasa-frontend")]
#[test]
fn hybrid_typed_tasks_preserve_checked_results_and_worker_restrictions() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-tasks-hybrid");
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "typed tasks ownership ok\n");
    for source in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "hybrid accepted {source}"
        );
    }
}
