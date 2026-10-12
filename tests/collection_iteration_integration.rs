mod common;
use common::*;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn frontends() -> Vec<&'static str> {
    if cfg!(feature = "bhasa-frontend") {
        vec!["rust", "bhasa"]
    } else {
        vec!["rust"]
    }
}

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        err.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut timeout = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timeout = true;
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
        !timeout,
        "collection fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

const LIBRARY: &str = r#"
struct Shelf[T] { items:T; def view(self)->T { return self.items } }
def sum[T](items:T)->int {
    var total=0
    for item in Shelf[T](items).view() { total=total+item;item=0 }
    return total
}
def source(value:Map[int,List[buffer]], note:buffer)->Map[int,List[buffer]] {
    append(note,"K")
    return value
}
struct Work { values:Map[int,List[buffer]]; note:buffer }
def process(input:Work)->buffer {
    var value=input.values
    var note=input.note
    var total=0
    var seen=0
    for key in source(value,note) {
        var retained=List[buffer]();retained.push(buffer("loop-owned"))
        value.clear()
        total=total+key
        seen=seen+1
        key=999
        for byte in buffer("AB") {
            var scratch=Map[str,buffer]();scratch.put("inner",buffer("owned"))
            if byte==65 {continue}
            var job=spawn(sum[buffer](buffer("AB")));var answer=join(job)
            if answer!=131 {return buffer("bad task")}
            break
        }
        if seen==1 {continue}
    }
    if seen!=2 or total!=19 {return buffer("bad map")}
    append(note,to_buffer(total))
    return note
}
"#;

const PROGRAM: &str = r#"
import "library.bh" as lib
def check()->bool {
    var values=Map[int,List[buffer]]()
    var parts=List[buffer]();parts.push(buffer("payload"))
    values.put(7,parts);values.put(12,parts);values.put(0,parts)
    var removed=values.remove(0)
    values.put(7,parts)
    var note=buffer()
    var job=async_spawn(lib.process(lib.Work(values,note)))
    values.put(99,parts)
    append(note,"caller")
    var result=await(job)
    if not same(result,"K19") or not same(note,"caller") or values.count()!=3 {return false}
    var numbers=ints();push(numbers,-9223372036854775808);push(numbers,9223372036854775807)
    if lib.sum[ints](numbers)!=-1 {return false}
    var bytes=buffer();push(bytes,0);push(bytes,128);push(bytes,255)
    if lib.sum[buffer](bytes)!=383 {return false}
    var keys=Map[str,buffer]();keys.put("x",buffer("saved"));keys.put("y",buffer("saved"))
    var count=0
    for key in lib.Shelf[Map[str,buffer]](keys).view() {
        if not same(buffer(key),"x") and not same(buffer(key),"y") {return false}
        keys.clear();count=count+1;key="changed"
    }
    return count==2
}
def main()->int {
    for round in range(12,0,-1) {if not check() {return 10}}
    print("collection/module/task integration ok")
    return 0
}
"#;

#[test]
fn imported_generic_collection_sources_and_owned_task_loops_release_allocations_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-owned-integration");
    workspace.write("library.bh", LIBRARY);
    let source = workspace.write("main.bh", PROGRAM);
    let tracker = workspace.write("tracker.c", r#"
#include <stdlib.h>
#include <stdio.h>
#include <stdatomic.h>
#include <pthread.h>
static _Atomic long live;
static pthread_once_t once=PTHREAD_ONCE_INIT;
static void check_live(void) { long n=atomic_load(&live);if(n) {fprintf(stderr,"live allocations: %ld\n",n);_Exit(99);} }
static void register_check(void) { if(atexit(check_live)!=0) abort(); }
void *tracked_malloc(size_t n) {pthread_once(&once,register_check);void *p=malloc(n);if(p) atomic_fetch_add(&live,1);return p;}
void *tracked_realloc(void *p,size_t n) {int empty=p==NULL;void *q=realloc(p,n);if(q&&empty) atomic_fetch_add(&live,1);return q;}
void tracked_free(void *p) {if(p) atomic_fetch_sub(&live,1);free(p);}
"#);
    workspace.write("tracked.h", "#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
    let runtime:Vec<_> = ["core","collections","concurrency"].into_iter().map(|unit| {
        workspace.write(&format!("{unit}.c"),&format!("#define _POSIX_C_SOURCE 200809L\n#if defined(__APPLE__)\n#define _DARWIN_C_SOURCE\n#endif\n#include \"tracked.h\"\n#include \"{}\"\n",fixture(&format!("runtime/{unit}.c")).display()))
    }).collect();
    for frontend in frontends() {
        let emitted = bounded(
            workspace
                .command()
                .arg("emit-llvm")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&emitted);
        let ir = stdout(&emitted)
            .replace("@malloc(", "@tracked_malloc(")
            .replace("@free(", "@tracked_free(");
        let ir = ir
            .lines()
            .map(|line| {
                if line.starts_with("define ") && line.ends_with(" {") {
                    format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let ir = workspace.write("program.ll", &ir);
        for optimization in ["-O0", "-O2"] {
            let executable = workspace.file("program");
            succeeds(&bounded(
                Command::new(&clang)
                    .args([
                        optimization,
                        "-fsanitize=address,undefined",
                        "-fno-sanitize-recover=all",
                        "-pthread",
                    ])
                    .arg(&ir)
                    .arg(&tracker)
                    .args(&runtime)
                    .arg("-o")
                    .arg(&executable),
            ));
            let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
            succeeds(&output);
            assert_eq!(stdout(&output), "collection/module/task integration ok\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn collection_sources_keep_transitive_worker_restrictions_and_module_privacy() {
    let workspace = Workspace::new("collection-source-safety");
    for library in [
        "extern def foreign(value:int)->int\nstruct Source{def values(self)->Map[int,buffer]{var code=foreign(1);return Map[int,buffer]()}}\ndef worker(input:int)->int{for key in Source().values(){}return input}",
        "def child()->int{return 1}\nstruct Source{def values(self)->buffer{var job=async_spawn(child());var result=await(job);return buffer()}}\ndef worker(input:int)->int{for key in Source().values(){}return input}",
    ] {
        workspace.write("library.bh",library);
        let main=workspace.write("main.bh","import \"library.bh\" as lib\ndef main(){var job=spawn(lib.worker(0));var result=join(job)}");
        for frontend in frontends() {
            let output=bounded(workspace.command().arg("check").arg(&main).args(["--frontend",frontend,"--json"]));
            fails(&output);
            assert!(stdout(&output).contains(if frontend=="rust" {"S020"} else {"BH209"}),"{}",describe(&output));
        }
    }
    workspace.write("library.bh","struct Source[T]{private values:T}\ndef make()->Source[buffer]{return Source[buffer](buffer())}");
    let main = workspace.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){for byte in lib.make().values{}}",
    );
    for frontend in frontends() {
        let output = bounded(workspace.command().arg("check").arg(&main).args([
            "--frontend",
            frontend,
            "--json",
        ]));
        fails(&output);
        assert!(stdout(&output).contains("private"), "{}", describe(&output));
    }
}
