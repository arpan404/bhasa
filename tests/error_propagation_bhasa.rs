mod common;
use common::*;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

const PROGRAM: &str = r#"
struct Counter { value:int }
struct Failure { message:buffer; details:List[buffer] }
struct Payload { data:buffer; table:Map[str,buffer] }
enum Input { error(reason:Failure); ok(value:Payload) }
enum Output { ok(value:int); error(problem:Failure) }
enum Bytes { ok(value:buffer); error(reason:Failure) }
def failure()->Failure {var rows=List[buffer]();rows.push(buffer("detail"));return Failure(buffer("error"),rows)}
def produce(counter:Counter,bad:bool)->Input {
    counter.value=counter.value+1
    if bad {return Input.error(failure())}
    var table=Map[str,buffer]();table.put("saved",buffer("payload"))
    return Input.ok(Payload(buffer("abc"),table))
}
def take(source:Input)->Output {
    var try=buffer("contextual name does not collide")
    var value:Payload=try source
    set(value.data,0,88);value.table.clear()
    if not same(try,"contextual name does not collide") {return Output.ok(-1)}
    return Output.ok(len(value.data))
}
def call_once(counter:Counter,bad:bool)->Output {
    var before=buffer("released on error")
    var value=try produce(counter,bad)
    var again=try produce(counter,false)
    return Output.ok(len(value.data)+len(again.data))
}
def worker(bad:bool)->Input {var counter=Counter(0);return produce(counter,bad)}
def joined(bad:bool)->Output {
    var job=spawn(worker(bad))
    var value=try join(job)
    return Output.ok(len(value.data))
}
def awaited(bad:bool)->Output {
    var job=async_spawn(worker(bad))
    var value=try await(job)
    return Output.ok(len(value.data))
}
def nested(source:Input)->Output {
    var maps=List[Map[str,buffer]]();var table=Map[str,buffer]();table.put("key",buffer("owned"));maps.push(table)
    for map in maps {
        maps.clear()
        for key in map {
            map.clear()
            match source {
                case Input.error(error) {
                    var local=buffer("match and loop cleanup")
                    var value=try source
                    return Output.ok(len(value.data))
                }
                case Input.ok(payload) {
                    for byte in payload.data {
                        var value=try source
                        return Output.ok(len(value.data))
                    }
                }
            }
        }
    }
    return Output.ok(-1)
}
def as_bytes(source:Input)->Bytes {var value=try source;return Bytes.ok(value.data)}
def read_output(value:Output)->int {
    match value {
        case Output.ok(number) {return number}
        case Output.error(error) {if not same(error.message,"error") or error.details.count()!=1 {return -99};return -1}
    }
}
def try(value:int)->int {return value+1}
def main()->int {
    var named=try (41)
    if named!=42 {return 1}
    var counter=Counter(0)
    var good=produce(counter,false);var bad=produce(counter,true)
    if read_output(take(good))!=3 or read_output(take(bad))!= -1 {return 2}
    match good {case Input.ok(value) {if not same(value.data,"abc") or value.table.count()!=1 {return 3}} case Input.error(error) {return 4}}
    match bad {case Input.error(error) {if not same(error.message,"error") or error.details.count()!=1 {return 5}} case Input.ok(value) {return 6}}
    counter.value=0
    if read_output(call_once(counter,true))!= -1 or counter.value!=1 {return 7}
    if read_output(call_once(counter,false))!=6 or counter.value!=3 {return 8}
    for i in range(12) {
        if read_output(joined(false))!=3 or read_output(joined(true))!= -1 {return 9}
        if read_output(awaited(false))!=3 or read_output(awaited(true))!= -1 {return 10}
        if read_output(nested(good))!=3 or read_output(nested(bad))!= -1 {return 11}
        var worker_job=spawn(joined(true));var result=join(worker_job)
        if read_output(result)!= -1 {return 12}
    }
    var bytes=as_bytes(good)
    match bytes {case Bytes.ok(value) {set(value,0,90);if not same(value,"Zbc") {return 13}} case Bytes.error(error) {return 14}}
    var error_bytes=as_bytes(bad)
    match error_bytes {case Bytes.ok(value) {return 15} case Bytes.error(error) {append(error.message,"!");error.details.clear()}}
    match bad {case Input.error(error) {if not same(error.message,"error") or error.details.count()!=1 {return 16}} case Input.ok(value) {return 17}}
    var try=Counter(7)
    if try.value!=7 {return 18}
    if true {var try=4;var ordinary=try+1;var bare=try;if ordinary!=5 or bare!=4 {return 19}}
    print("typed error propagation ownership ok")
    return 0
}
"#;

const INVALID: &[(&str, &str)] = &[
    ("enum R{ok(v:int);error(e:int)} def f()->R{var x=try missing;return R.ok(1)}", "BH202"),
    ("enum R{ok(v:int);error(e:int)} def f()->R{var source=1;var x=try source;return R.ok(x)}", "BH218"),
    ("enum R{ok(v:int);error(e:int)} enum Bad{ok;error(e:int)} def f(source:Bad)->R{var x=try source;return R.ok(1)}", "BH218"),
    ("enum R{ok(v:int);error(e:int)} enum Bad{ok(a:int,b:int);error(e:int)} def f(source:Bad)->R{var x=try source;return R.ok(1)}", "BH218"),
    ("enum R{ok(v:int);error(e:int)} enum Bad{ok(v:int);error(e:int);other} def f(source:Bad)->R{var x=try source;return R.ok(1)}", "BH218"),
    ("enum R{ok(v:int);error(e:int)} enum Bad{yes(v:int);error(e:int)} def f(source:Bad)->R{var x=try source;return R.ok(1)}", "BH218"),
    ("enum R{ok(v:int);error(e:int)} def f(source:R)->int{var x=try source;return x}", "BH219"),
    ("enum R{ok(v:int);error(e:int)} enum Bad{ok(v:int);error} def f(source:R)->Bad{var x=try source;return Bad.ok(x)}", "BH219"),
    ("enum R{ok(v:int);error(e:int)} enum S{error(e:bool);ok(v:int)} def f(source:R)->S{var x=try source;return S.ok(x)}", "BH219"),
    ("struct A{x:int} struct B{x:int} enum R{ok(v:int);error(e:A)} enum S{ok(v:int);error(e:B)} def f(source:R)->S{var x=try source;return S.ok(x)}", "BH219"),
    ("enum R{ok(v:int);error(e:int)} def f(source:R)->R{var x:bool=try source;return R.ok(1)}", "BH201"),
    ("enum R{ok(v:int);error(e:int)} def f(source:R)->R{var x=try source}", "BH205"),
    ("enum R{ok(v:int);error(e:int)} def main(){R.ok(1)}", "BH207"),
    ("enum R{error(e:int);ok(v:int)} def f(v:R){v}", "BH207"),
    ("enum R{ok(v:int);error(e:int)} def f()->R{return R.ok(1)} def main(){(f())}", "BH207"),
    ("enum R{ok(v:int);error(e:int)} struct Box{value:R} def f(v:Box){v.value}", "BH207"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->R{return R.ok(v)} def main(){var job=spawn(worker(1));join(job)}", "BH207"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->R{return R.ok(v)} def main(){var job=async_spawn(worker(1));await(job)}", "BH207"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->int{return v} def f(source:R)->R{var job=spawn(worker(1));var x=try source;var r=join(job);return R.ok(x)}", "BH208"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->R{return R.ok(v)} def f()->R{var other=spawn(worker(1));var job=spawn(worker(1));var x=try join(job);var r=join(other);return R.ok(x)}", "BH208"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->int{return v} def f(source:R)->R{var job=spawn(worker(1));for i in range(1){var x=try source};var r=join(job);return R.ok(1)}", "BH208"),
    ("extern def unsafe(v:int)->int\nenum R{ok(v:int);error(e:int)} def source(v:int)->R{return R.ok(unsafe(v))} def worker(v:int)->R{var x=try source(v);return R.ok(x)} def main(){var job=spawn(worker(1));var r=join(job)}", "BH209"),
    ("enum R{ok(v:int);error(e:int)} def source(v:int)->R{return R.ok(v)} def worker(v:int)->R{var job=spawn(source(v));var x=try await(job);return R.ok(x)} def main(){var job=spawn(worker(1));var r=join(job)}", "BH209"),
];

const COMPATIBLE: &[&str] = &[
    "enum R{ok(v:int);error(e:int)} def try[T](value:T)->T{return value} def f(source:R)->R{var x=try source;return R.ok(x)} def main()->int{var named=try[int](41);var result=f(R.ok(named));match result{case R.ok(value){if value==41{return 0};return 1} case R.error(error){return 2}}}",
    "enum R{ok(v:int);error(e:int)} def f()->R{var try=R.ok(42);var value=try try;return R.ok(value)} def main()->int{var try=true;var value=try and false;var other=try or false;if value or not other{return 1};var result=f();match result{case R.ok(value){return value-42} case R.error(error){return 2}}}",
    "enum Empty{ok;error(e:int)} enum Extra{ok(v:int);error(e:int);other} enum Other{yes(v:int);error(e:int)} def worker(v:int)->Empty{return Empty.ok()} def main(){Empty.ok();Extra.ok(1);Other.yes(1);var job=spawn(worker(1));join(job)}",
    "enum R[T]{error(e:int);ok(value:T)} def keep[T](source:R[T])->R[T]{var value=try source;return R[T].ok(value)} def main()->int{var result=keep[buffer](R[buffer].ok(buffer(\"saved\")));match result{case R[buffer].ok(value){if same(value,\"saved\"){return 0};return 1} case R[buffer].error(error){return 2}}}",
];

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        out.read_to_end(&mut b).unwrap();
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        err.read_to_end(&mut b).unwrap();
        b
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut expired = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            expired = true;
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
        !expired,
        "compiler/executable exceeded deadline: {}",
        describe(&output)
    );
    output
}
fn emit(seed: &Path, workspace: &Workspace, source: &str) -> Output {
    let source = workspace.write("input.bh", source);
    bounded(Command::new(seed).args([
        "emit-llvm",
        as_str(&source),
        "--target",
        &bhasa::target::Target::Native.triple_for_ir().unwrap(),
    ]))
}
const TRACKER: &str = r#"
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic long live;
static pthread_once_t once=PTHREAD_ONCE_INIT;
static void check(void){long n=atomic_load(&live);if(n){fprintf(stderr,"live loop allocations: %ld\n",n);abort();}}
static void setup(void){if(atexit(check)!=0)abort();}
void *tracked_malloc(size_t n){if(pthread_once(&once,setup)!=0)abort();void*p=malloc(n);if(p)atomic_fetch_add(&live,1);return p;}
void *tracked_realloc(void*p,size_t n){int empty=p==NULL;void*q=realloc(p,n);if(q&&empty)atomic_fetch_add(&live,1);return q;}
void tracked_free(void*p){if(p)atomic_fetch_sub(&live,1);free(p);}
"#;
const TRACKER_HEADER: &str = r#"
#include <stdlib.h>
void *tracked_malloc(size_t);
void *tracked_realloc(void*,size_t);
void tracked_free(void*);
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
"#;
fn link(
    workspace: &Workspace,
    clang: &std::ffi::OsStr,
    ir: &str,
    name: &str,
    opt: &str,
    track: bool,
) -> std::path::PathBuf {
    let mut ir = ir.to_string();
    let mut command = Command::new(clang);
    command.arg(opt);
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
        let source = workspace.write("tracker.c", TRACKER);
        let object = workspace.file("tracker.o");
        succeeds(&bounded(
            Command::new(clang)
                .args([
                    opt,
                    "-fsanitize=address,undefined",
                    "-fno-omit-frame-pointer",
                    "-c",
                ])
                .arg(source)
                .arg("-o")
                .arg(&object),
        ));
        command
            .arg("-fsanitize=address,undefined")
            .arg("-fno-omit-frame-pointer")
            .arg(object)
            .arg("-include")
            .arg(workspace.write("tracker.h", TRACKER_HEADER));
    }
    let executable = workspace.file(name);
    command
        .arg(workspace.write(&format!("{name}.ll"), &ir))
        .arg(fixture("runtime/core.c"))
        .arg(fixture("runtime/concurrency.c"))
        .arg(fixture("runtime/collections.c"))
        .arg("-pthread")
        .arg("-o")
        .arg(&executable);
    succeeds(&bounded(&mut command));
    executable
}

#[test]
fn fresh_native_error_propagation_matches_reference_and_rebuilds() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("error-propagation-native");
    let compiler = ["lexer.bh", "emitter.bh", "driver.bh"]
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .join("\n");
    let seed_source = workspace.write("seed.bh", &compiler);
    let built = build(&workspace, &seed_source, true);
    let seed = workspace.file("stage-a");
    fs::rename(built, &seed).unwrap();
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let expanded = bhasa::generics::expand(PROGRAM, &[]).unwrap();
    let native = emit(&seed, &workspace, &expanded.source);
    succeeds(&native);
    let reference = bhasa::compile_to_llvm(PROGRAM, &target).unwrap();
    for (name, ir) in [("native", stdout(&native)), ("reference", reference)] {
        for opt in ["-O0", "-O2"] {
            let executable = link(&workspace, &clang, &ir, name, opt, true);
            let result = bounded(&mut Command::new(executable));
            succeeds(&result);
            assert_eq!(stdout(&result), "typed error propagation ownership ok\n");
        }
    }
    for (source, diagnostic) in INVALID {
        assert!(bhasa::check(source).is_err(), "reference accepted {source}");
        let expanded = bhasa::generics::expand(source, &[]).unwrap();
        let result = emit(&seed, &workspace, &expanded.source);
        fails(&result);
        assert!(result.stdout.is_empty());
        assert!(
            stderr(&result).contains(diagnostic),
            "{source}: {}",
            describe(&result)
        );
    }
    for (index, source) in COMPATIBLE.iter().enumerate() {
        let expanded = bhasa::generics::expand(source, &[]).unwrap();
        let emitted = emit(&seed, &workspace, &expanded.source);
        succeeds(&emitted);
        let reference = bhasa::compile_to_llvm(source, &target).unwrap();
        for (kind, ir) in [("native", stdout(&emitted)), ("reference", reference)] {
            let executable = link(
                &workspace,
                &clang,
                &ir,
                &format!("compatible-{index}-{kind}"),
                "-O2",
                true,
            );
            succeeds(&bounded(&mut Command::new(executable)));
        }
    }
    let stage_b_ir = emit(&seed, &workspace, &compiler);
    succeeds(&stage_b_ir);
    let stage_b = link(
        &workspace,
        &clang,
        &stdout(&stage_b_ir),
        "stage-b",
        "-O2",
        false,
    );
    let stage_c_ir = emit(&stage_b, &workspace, &compiler);
    succeeds(&stage_c_ir);
    assert_eq!(stdout(&stage_b_ir), stdout(&stage_c_ir));
    assert_eq!(
        stdout(&native),
        stdout(&emit(&stage_b, &workspace, &expanded.source))
    );
}

#[cfg(feature = "bhasa-frontend")]
#[test]
fn hybrid_error_propagation_preserves_ownership_and_rejections() {
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    for (source, _) in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "{source}"
        );
    }
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("error-propagation-hybrid");
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let result = bounded(&mut Command::new(executable));
    succeeds(&result);
    assert_eq!(stdout(&result), "typed error propagation ownership ok\n");
}
