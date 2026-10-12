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
struct Payload { data:buffer; numbers:ints }
enum Envelope { empty; item(value:Payload) }
def bytes(counter:Counter)->buffer {counter.value=counter.value+1;return buffer("ab")}
def numbers(counter:Counter)->ints {counter.value=counter.value+1;var values=ints();push(values,3);push(values,4);return values}
def keys(counter:Counter)->Map[int,buffer] {counter.value=counter.value+1;var values=Map[int,buffer]();values.put(5,buffer("owned"));values.put(7,buffer("owned"));return values}
def echo(value:buffer)->buffer {append(value,"!");return value}
def sum_numbers(values:ints)->int {var total=0;for value in values {total=total+value};return total}
def map_worker(values:Map[str,Payload])->int {
    var total=0
    for key in values {
        var payload=values.get(key)
        for byte in payload.data {total=total+byte}
        var job=spawn(sum_numbers(payload.numbers));var result=join(job);total=total+result
    }
    return total
}
def first_byte(values:buffer)->int {for byte in values {return byte};return -1}
def first_number(values:ints)->int {for value in values {return value};return -1}
def first_key(values:Map[str,Envelope])->buffer {for key in values {return buffer(key)};return buffer("empty")}
def main()->int {
    for byte in buffer() {return 1}
    for value in ints() {return 2}
    for key in Map[int,buffer]() {return 3}
    for key in Map[str,Envelope]() {return 4}
    var data=buffer();push(data,0);push(data,127);push(data,128);push(data,255)
    var index=0;var total=0
    for byte in data {
        if index==0 and byte!=0 {return 5}
        if index==1 and byte!=127 {return 6}
        if index==2 and byte!=128 {return 7}
        if index==3 and byte!=255 {return 8}
        total=total+byte;index=index+1
        set(data,0,99);push(data,42);byte=42
    }
    if index!=4 or total!=510 or len(data)!=8 {return 9}
    data=buffer("abc");total=0
    for byte in data {total=total+byte;data=buffer("rebound");continue}
    if total!=294 {return 10}
    var values=ints();push(values,-9223372036854775808);push(values,0);push(values,9223372036854775807)
    index=0
    for value in values {
        if index==0 and value!= -9223372036854775808 {return 11}
        if index==1 and value!=0 {return 12}
        if index==2 and value!=9223372036854775807 {return 13}
        set(values,0,5);push(values,99);value=99;index=index+1
    }
    if index!=3 or len(values)!=6 {return 14}
    values=ints();push(values,10);push(values,20);total=0
    for value in values {total=total+value;values=ints();continue}
    if total!=30 {return 15}
    var counter=Counter(0);total=0
    for byte in bytes(counter) {total=total+byte}
    for value in numbers(counter) {total=total+value}
    for key in keys(counter) {total=total+key}
    if counter.value!=3 or total!=214 {return 16}
    var table=Map[int,buffer]();index=0
    while index<100 {table.put(index,buffer("owned map payload"));index=index+1}
    index=0
    while index<100 {table.remove(index);index=index+2}
    total=0;index=0
    for key in table {
        if key%2!=1 or key<0 or key>=100 {return 17}
        total=total+key;index=index+1;key=999
        table.clear();table=Map[int,buffer]();table.put(1000,buffer("replacement"));continue
    }
    if total!=2500 or index!=50 or table.count()!=1 {return 18}
    var edge=Map[int,buffer]();edge.put(-9223372036854775808,buffer());edge.put(9223372036854775807,buffer())
    var minimum=false;var maximum=false;index=0
    for key in edge {
        if key== -9223372036854775808 {minimum=true}
        if key==9223372036854775807 {maximum=true}
        index=index+1;edge.remove(key)
    }
    if not minimum or not maximum or index!=2 or edge.count()!=0 {return 19}
    values=ints();push(values,1);push(values,2)
    var payload=Payload(buffer("ab"),values)
    var strings=Map[str,Payload]();strings.put("alpha",payload);strings.put("beta",payload);strings.put("gamma",payload)
    var alpha=false;var beta=false;var gamma=false;index=0
    for key in strings {
        if same(buffer(key),"alpha") {alpha=true}
        if same(buffer(key),"beta") {beta=true}
        if same(buffer(key),"gamma") {gamma=true}
        index=index+1;strings.remove(key);strings.put("new",payload);key="changed"
    }
    if not alpha or not beta or not gamma or index!=3 or strings.count()!=1 {return 20}
    strings=Map[str,Payload]();strings.put("alpha",payload);strings.put("beta",payload)
    var worker=async_spawn(map_worker(strings));strings.clear();var worker_result=await(worker)
    if worker_result!=396 {return 21}
    var envelopes=Map[str,Envelope]();envelopes.put("saved",Envelope.item(payload))
    if not same(first_key(envelopes),"saved") {return 22}
    index=0
    for key in envelopes {var temporary=envelopes.get(key);envelopes.clear();index=index+1;break}
    if index!=1 {return 23}
    if first_byte(buffer("z"))!=122 or first_number(values)!=1 {return 24}
    var returned=List[buffer]();returned.push(buffer("owned"));index=0
    for item in returned {
        returned.clear()
        for byte in item {
            var copy=buffer("discarded on continue")
            if byte==111 {continue}
            for key in keys(counter) {var job=spawn(echo(buffer("task")));var result=join(job);if not same(result,"task!") {return 25};break}
            index=index+1;break
        }
    }
    if index!=1 {return 26}
    var outer=spawn(echo(buffer("outer")))
    for byte in buffer("x") {break}
    for value in values {var local=buffer("break cleanup");break}
    var joined=join(outer);if not same(joined,"outer!") {return 27}
    total=0
    for byte in payload.data {payload.data=buffer("rebound field");total=total+byte}
    if total!=195 {return 28}
    var nested=List[Map[str,Envelope]]();envelopes.put("nested",Envelope.item(payload));nested.push(envelopes)
    for map in nested {nested.clear();for key in map {map.clear();if not same(buffer(key),"nested") {return 29}}}
    print("collection snapshot iteration ownership ok")
    return 0
}
"#;

const INVALID: &[(&str, &str)] = &[
    ("def main(){for value in true{}}", "BH201"),
    ("def main(){for value in 1.0{}}", "BH201"),
    ("def main(){for value in \"text\"{}}", "BH201"),
    ("struct Item{value:int} def main(){for value in Item(1){}}", "BH201"),
    ("def main(){for value in buffer(\"x\"){value=true}}", "BH201"),
    ("def main(){for value in ints(){value=buffer()}}", "BH201"),
    ("def main(){for key in Map[str,int](){key=1}}", "BH201"),
    ("def echo(v:buffer)->buffer{return v} def main(){for byte in buffer(\"x\"){var job=spawn(echo(buffer()));break}}", "BH208"),
    ("def echo(v:buffer)->buffer{return v} def main(){for value in ints(){var job=spawn(echo(buffer()));continue}}", "BH208"),
    ("def echo(v:buffer)->buffer{return v} def main(){for key in Map[str,int](){var job=spawn(echo(buffer()));return}}", "BH208"),
    ("def echo(v:buffer)->buffer{return v} def main(){var job=spawn(echo(buffer()));for key in Map[str,int](){join(job)};join(job)}", "BH208"),
    ("extern def unsafe(v:int)->int\ndef source(v:int)->buffer{unsafe(v);return buffer()} def worker(v:int)->int{for byte in source(v){};return v} def main(){var job=spawn(worker(1));join(job)}", "BH209"),
    ("extern def unsafe(v:int)->int\ndef worker(v:int)->int{for key in Map[int,buffer](){unsafe(key)};return v} def main(){var job=spawn(worker(1));join(job)}", "BH209"),
    ("def echo(v:int)->int{return v} def source(v:int)->ints{var job=async_spawn(echo(v));var result=await(job);return ints()} def worker(v:int)->int{for value in source(v){};return v} def main(){var job=spawn(worker(1));join(job)}", "BH209"),
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
fn fresh_native_collection_iteration_matches_reference_and_rebuilds() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-iteration-native");
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
            assert_eq!(
                stdout(&result),
                "collection snapshot iteration ownership ok\n"
            );
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
fn hybrid_collection_iteration_preserves_snapshots_and_rejections() {
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    for (source, _) in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "{source}"
        );
    }
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-iteration-hybrid");
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let result = bounded(&mut Command::new(executable));
    succeeds(&result);
    assert_eq!(
        stdout(&result),
        "collection snapshot iteration ownership ok\n"
    );
}
