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
def mark(counter:Counter, digit:int, value:int)->int {counter.value=counter.value*10+digit;return value}
def bits(value:int)->int {return (value<<1)^0b101}
def first(values:List[buffer])->buffer {
    for value in values {for i in range(3,0,-1) {if i==2 {return value}}}
    return buffer("empty")
}
def main()->int {
    var maximum=0x7fffffffffffffff
    var minimum=-0X8000000000000000
    if maximum!=9223372036854775807 or minimum!= -9223372036854775808 {return 1}
    if 0B111111111111111111111111111111111111111111111111111111111111111!=maximum {return 2}
    if -0b1000000000000000000000000000000000000000000000000000000000000000!=minimum {return 3}
    if 0x00000000000000000000000000000000000000000000000fF!=255 or 0B0000101!=5 or -0x0!=0 {return 4}
    if ~0!= -1 or ~minimum!=maximum or ~~42!=42 {return 5}
    if 0xF & 0b1010 !=10 or 0xF ^ 0b1010 !=5 or 0xF | 0x10 !=31 {return 6}
    if 1 | 2 ^ 3 & 4 !=3 or ~1 & 0xFF !=254 {return 7}
    if 1 << 2 + 1 !=8 or 16 >> 1 + 1 !=4 or 64>>1>>2!=8 {return 8}
    if 1<<63!=minimum or minimum>>63!= -1 or -3>>1!= -2 or maximum<<1!= -2 {return 9}
    if minimum<<1!=0 or -1<<63!=minimum or maximum>>0!=maximum {return 10}
    var counter=Counter(0)
    var sum=0;var count=0
    for i in ((range(mark(counter,1,5),mark(counter,2,-2),mark(counter,3,-2),))) {
        sum=sum+i;count=count+1;i=99
    }
    if sum!=8 or count!=4 or counter.value!=123 {return 11}
    var end=7;var step=2;sum=0
    for i in range(1,end,step) {sum=sum+i;end=0;step=0;i=100}
    if sum!=9 {return 12}
    for i in range(3,0,2) {return 13}
    for i in range(0,3,-1) {return 14}
    for i in range(minimum,minimum,-1) {return 15}
    var edge=ints()
    for i in range(maximum-1,maximum,2) {push(edge,i)}
    for i in range(minimum+1,minimum,-2) {push(edge,i)}
    if len(edge)!=2 or get(edge,0)!=maximum-1 or get(edge,1)!=minimum+1 {return 16}
    edge=ints()
    for i in range(maximum,minimum,minimum) {push(edge,i)}
    if len(edge)!=2 or get(edge,0)!=maximum or get(edge,1)!= -1 {return 17}
    edge=ints()
    for i in range(minimum,maximum,maximum) {push(edge,i)}
    if len(edge)!=3 or get(edge,0)!=minimum or get(edge,1)!= -1 or get(edge,2)!=maximum-1 {return 18}
    var values=List[buffer]();values.push(buffer("saved"))
    var saved=first(values);count=0
    for value in values {
        values.clear()
        for i in range(3,-1,-1) {
            var owned=buffer("loop cleanup")
            if i==3 {continue}
            if i==1 {break}
            var task=spawn(bits(i));var result=join(task)
            if result!=1 {return 19}
            count=count+1
        }
    }
    if count!=1 or not same(saved,"saved") {return 20}
    for i in range(maximum-1,maximum,2) {var owned=buffer("overflow cleanup");continue}
    var outer=spawn(bits(3));for i in range(2,0,-1) {break};var joined=join(outer)
    if joined!=3 {return 21}
    counter.value=0
    var mask=mark(counter,1,7)&mark(counter,2,3)
    if mask!=3 or counter.value!=12 {return 22}
    print("integer and stepped range ownership ok")
    return 0
}
"#;

const INVALID: &[&str] = &[
    "def main(){print(0x8000000000000000)}",
    "def main(){print(0xffffffffffffffff)}",
    "def main(){print(-0x8000000000000001)}",
    "def main(){print(-(0x8000000000000000))}",
    "def main(){print(0b1000000000000000000000000000000000000000000000000000000000000000)}",
    "def main(){print(-0b1000000000000000000000000000000000000000000000000000000000000001)}",
    "def main(){print(0x)}", "def main(){print(0B)}",
    "def main(){print(0x12g)}", "def main(){print(0b102)}",
    "def main(){print(0x1_2)}", "def main(){print(0x1p2)}",
    "def main(){print(true & false)}", "def main(){print(1.0 | 2.0)}",
    "def main(){print(1 ^ true)}", "def main(){print(~false)}",
    "def main(){print(~1.0)}", "def main(){print(1<<2.0)}",
    "def main(){print(true>>1)}", "def main(){for i in range(0,1,true){}}",
    "def main(){for i in range(0,1,2.0){}}", "def main(){for i in range(0,1,2,3){}}",
    "def main(){for i in range(0,1,,){}}",
    "def worker(n:int)->int{return n} def main(){for i in range(3,0,-1){var t=spawn(worker(i));continue}}",
    "extern def unsafe(n:int)->int\ndef worker(n:int)->int{for i in range(0,1,unsafe(n)){};return n} def main(){var t=spawn(worker(1));join(t)}",
    "def identity(n:int)->int{return n} def step(n:int)->int{var t=async_spawn(identity(n));var r=await(t);return r} def worker(n:int)->int{for i in range(0,1,step(n)){};return n} def main(){var t=spawn(worker(1));join(t)}",
];
const TRAPS: &[&str] = &[
    "def main(){print(1<<64)}",
    "def main(){print(1>> -1)}",
    "def count()->int{return 64} def main(){print(1>>count())}",
    "def main(){for i in range(0,0,0){}}",
    "def step()->int{return 0} def main(){for i in range(3,0,step()){}}",
    "def main(){print(0x7fffffffffffffff+1)}",
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
fn fresh_native_integer_and_range_paths_match_reference_and_rebuild() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("systems-native");
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
            assert_eq!(stdout(&result), "integer and stepped range ownership ok\n");
        }
    }
    for source in INVALID {
        assert!(bhasa::check(source).is_err(), "reference accepted {source}");
        let result = emit(&seed, &workspace, source);
        fails(&result);
        assert!(result.stdout.is_empty());
        if source.starts_with("extern ") {
            assert!(stderr(&result).contains("BH209"), "{}", describe(&result));
        }
    }
    for (index, source) in TRAPS.iter().enumerate() {
        let emitted = emit(&seed, &workspace, source);
        succeeds(&emitted);
        for opt in ["-O0", "-O2"] {
            let executable = link(
                &workspace,
                &clang,
                &stdout(&emitted),
                &format!("trap-{index}"),
                opt,
                false,
            );
            let result = bounded(&mut Command::new(executable));
            assert!(!result.status.success(), "trap returned normally: {source}");
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
fn hybrid_radix_operators_and_stepped_ranges_match_native_contract() {
    for source in [
        "0xF 0B101 -0x8000000000000000 & | ^ ~ << >>",
        "0x 0b2 0x1_2",
    ] {
        assert_eq!(bhasa::lexer::lex(source), bhasa::lexer::lex_rust(source));
    }
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    for source in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "{source}"
        );
    }
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("systems-hybrid");
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let result = bounded(&mut Command::new(executable));
    succeeds(&result);
    assert_eq!(stdout(&result), "integer and stepped range ownership ok\n");
}
