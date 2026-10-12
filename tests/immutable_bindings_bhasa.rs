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
struct Cell {
    value:int
    data:buffer
    def mutate(self,value:int) {self.value=value;append(self.data,"!")}
}
struct Holder { cell:Cell }
enum Reply { error(message:buffer); ok(cell:Cell) }
enum Count { ok(value:int); error(message:buffer) }
def mutate(value:Cell) {value.value=value.value+1;append(value.data,"?")}
def parameter(value:int)->int {value=value+1;return value}
def worker(bad:bool)->Reply {if bad{return Reply.error(buffer("failure"))};return Reply.ok(Cell(1,buffer("task")))}
def forward(source:Reply)->Count {
    let local=buffer("cleanup on error")
    let value=try source
    value.mutate(4)
    return Count.ok(value.value)
}
def joined(bad:bool)->Count {
    let local=Holder(Cell(1,buffer("caller")))
    let job=spawn(worker(bad))
    let value=try join(job)
    value.data=buffer("changed")
    return Count.ok(len(value.data))
}
def awaited(bad:bool)->Count {let job=async_spawn(worker(bad));let value=try await(job);return Count.ok(value.value)}
def count(value:Count)->int {match value {case Count.ok(n){n=n+1;return n-1} case Count.error(message){if not same(message,"failure"){return -2};return -1}}}
def let(value:int)->int {return value+1}
def main()->int {
    let from_call=let (41)
    if from_call!=42 or parameter(3)!=4 {return 1}
    let scalar:int=7
    if scalar!=7 {return 2}
    let bytes=buffer("abc");append(bytes,"!");set(bytes,0,65);push(bytes,63)
    if not same(bytes,"Abc!?") {return 3}
    let numbers=ints();push(numbers,1);set(numbers,0,3)
    if get(numbers,0)!=3 {return 4}
    let cell=Cell(1,buffer("owned"));cell.value=2;cell.data=buffer("field")
    cell.mutate(3);mutate(cell)
    if cell.value!=4 or not same(cell.data,"field!?") {return 5}
    let holder=Holder(cell);holder.cell=Cell(8,buffer("replacement"));holder.cell.data=buffer("nested")
    if cell.value!=4 or not same(cell.data,"field!?") {return 6}
    let copy=holder;holder.cell.mutate(9)
    if copy.cell.value!=8 or not same(copy.cell.data,"nested") {return 7}
    let values=List[buffer]();values.push(buffer("first"));values.set(0,buffer("second"))
    let table=Map[str,Cell]();table.put("key",cell)
    if not same(values.get(0),"second") or table.get("key").value!=4 {return 8}
    table.remove("key");table.put("new",cell);table.clear();values.clear()
    let original=buffer("outer")
    if true {var original=original;append(original,"!");original=buffer("inner")}
    if not same(original,"outer") {return 9}
    var number=1
    if true {let number=number+1;if number!=2{return 10}}
    number=3;if number!=3{return 11}
    let same_name=buffer("old")
    if true {let same_name=same_name;append(same_name,"copy");if not same(same_name,"oldcopy"){return 12}}
    if not same(same_name,"old") {return 13}
    var total=0
    for value in numbers {value=value+1;total=total+value}
    for index in range(3) {index=5;total=total+index}
    if total!=19 {return 14}
    let success=worker(false)
    let success_count=count(forward(success))
    match success {case Reply.ok(value){if value.value!=1 or not same(value.data,"task"){return 16}} case Reply.error(message){return 17}}
    let failure=worker(true)
    let failure_count=count(forward(failure))
    match failure {case Reply.error(message){if not same(message,"failure"){return 15}} case Reply.ok(value){return 15}}
    if success_count!=4 or failure_count!= -1 {return 15}
    for index in range(8) {
        let owned=Cell(index,buffer("loop"))
        if count(joined(false))!=7 or count(joined(true))!= -1 {return 18}
        if count(awaited(false))!=1 or count(awaited(true))!= -1 {return 19}
        let job=spawn(worker(false));let result=join(job)
        match result {case Reply.ok(value){value.value=9} case Reply.error(message){return 20}}
        if index==0 {continue}
        if index==4 {break}
    }
    var let=1;let=2;var ordinary=let+1;if ordinary!=3{return 21}
    if true {let let=Cell(3,buffer("name"));let.value=4;let.mutate(5);if let.value!=5{return 22}}
    print("immutable binding ownership ok")
    return 0
}
"#;

const INVALID: &[(&str, &str)] = &[
    ("def main(){let value=1;value=2}", "BH220"),
    ("def main(){let value=buffer();value=buffer(\"new\")}", "BH220"),
    ("def main(){let value=ints();value=ints()}", "BH220"),
    ("struct Cell{v:int} def main(){let value=Cell(1);value=Cell(2)}", "BH220"),
    ("enum Choice{some(v:int);none} def main(){let value=Choice.none();value=Choice.some(2)}", "BH220"),
    ("def main(){let value=List[buffer]();value=List[buffer]()}", "BH220"),
    ("def main(){let value=Map[str,int]();value=Map[str,int]()}", "BH220"),
    ("def main(){let value=1;if true{value=2}}", "BH220"),
    ("def main(){let value=1;for index in range(1){value=2}}", "BH220"),
    ("def main(){let let=1;let=2}", "BH220"),
    ("enum R{ok(v:int);error(e:int)} def f(source:R)->R{let value=try source;value=2;return R.ok(value)}", "BH220"),
    ("def main(){let value:int=true}", "BH201"),
    ("def main(){let value=1;let value=2}", "BH202"),
    ("def worker(v:int)->int{return v} def main(){let job=spawn(worker(1))}", "BH208"),
    ("def worker(v:int)->int{return v} def main(){let job=spawn(worker(1));join(job);join(job)}", "BH208"),
    ("def worker(v:int)->int{return v} def main(){let job=spawn(worker(1));job=spawn(worker(2));join(job)}", "BH208"),
    ("def worker(v:int)->int{return v} def main(){let job=spawn(worker(1));if true{let job=1};join(job)}", "BH208"),
    ("enum R{ok(v:int);error(e:int)} def worker(v:int)->int{return v} def f(source:R)->R{let job=spawn(worker(1));let value=try source;let done=join(job);return R.ok(value)}", "BH208"),
];
const COMPATIBLE: &[&str] = &[
    "def let[T](value:T)->T{return value} def main()->int{let value=let[int](42);return value-42}",
    "enum R[T]{ok(v:T);error(e:int)} def let[T](value:T)->T{return value} def f(source:R[int])->R[int]{let value=try source;return R[int].ok(value)} def main()->int{let result=f(R[int].ok(let[int](42)));match result{case R[int].ok(v){return v-42}case R[int].error(e){return 1}}}",
    "struct let{value:int} def main()->int{let item=let(42);item.value=43;return item.value-43}",
    "def action(){} def main()->int{var let=1;let value=let # ordinary identifier initializer\n action();return value-1}",
    "enum R{ok(v:int);error(e:int)} def f()->R{let try=R.ok(42);let value=try try;return R.ok(value)} def main()->int{let result=f();match result{case R.ok(v){return v-42}case R.error(e){return 1}}}",
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
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                    "-c",
                ])
                .arg(source)
                .arg("-o")
                .arg(&object),
        ));
        command
            .arg("-fsanitize=address,undefined")
            .arg("-fno-sanitize-recover=all")
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
fn fresh_native_immutable_bindings_match_reference_and_rebuild() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("immutable-binding-native");
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
            assert_eq!(stdout(&result), "immutable binding ownership ok\n");
        }
    }
    for (source, diagnostic) in INVALID {
        assert!(bhasa::check(source).is_err(), "reference accepted {source}");
        let expanded = bhasa::generics::expand(source, &[]).unwrap();
        let result = emit(&seed, &workspace, &expanded.source);
        fails(&result);
        assert!(result.stdout.is_empty());
        if *diagnostic == "BH220" {
            // No generics precede the assignment in the scalar span fixture.
            if *source == "def main(){let value=1;value=2}" {
                let start = source.rfind("value=").unwrap();
                assert!(stderr(&result).contains(&format!("at byte {start}..{}", start + 5)));
            }
        }
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
fn hybrid_immutable_bindings_preserve_ownership_and_rejections() {
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    for (source, _) in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "{source}"
        );
    }
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("immutable-binding-hybrid");
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let result = bounded(&mut Command::new(executable));
    succeeds(&result);
    assert_eq!(stdout(&result), "immutable binding ownership ok\n");
}
