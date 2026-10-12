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
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let errors = std::thread::spawn(move || {
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
        stdout: reader.join().unwrap(),
        stderr: errors.join().unwrap(),
    };
    assert!(
        !timeout,
        "typed-task fixture exceeded its deadline: {}",
        describe(&output)
    );
    output
}

const OWNERSHIP: &str = r#"
struct Item { bytes:buffer; values:List[buffer]; fraction:float; flag:bool }
enum Choice { value(item:Item); none }
def change(value:buffer)->buffer { append(value," worker"); return value }
def integers(value:ints)->ints { push(value,42); return value }
def list(value:List[buffer])->List[buffer] { value.push(buffer("worker")); return value }
def mapping(value:Map[str,buffer])->Map[str,buffer] { value.put("worker",buffer("added")); return value }
def record(value:Item)->Item { append(value.bytes," worker"); value.values.push(buffer("worker")); return value }
def choice(value:Choice)->Choice {
    match value {
        case Choice.value(item) { append(item.bytes," enum"); return Choice.value(item) }
        case Choice.none { return Choice.none() }
    }
}
def identity[T](value:T)->T { return value }
def truth(value:bool)->bool { return not value }
def number(value:float)->float { return value*2.0 }
def text(value:str)->str { return value }
def consume(value:buffer) { append(value," consumed") }
def legacy(value:int)->int { return value+1 }
def nested(value:buffer)->buffer { var task=spawn(change(value)); var answer=join(task); return answer }
def verify()->bool {
    var original=buffer("source")
    var first=spawn(change(original))
    append(original," caller")
    var changed=join(first)
    if not same(original,"source caller") or not same(changed,"source worker") { return false }
    var vector=ints(); push(vector,7)
    var vector_task=async_spawn(integers(vector)); set(vector,0,99)
    var vector_result=join(vector_task)
    if len(vector_result)!=2 or get(vector_result,0)!=7 or get(vector,0)!=99 { return false }
    var values=List[buffer](); values.push(buffer("entry"))
    var list_task=async_spawn(list(values)); values.clear()
    var list_result=await(list_task)
    if list_result.count()!=2 or not same(list_result.get(0),"entry") { return false }
    var entries=Map[str,buffer](); entries.put("original",buffer("kept"))
    var map_task=spawn(mapping(entries)); entries.clear()
    var map_result=await(map_task)
    if map_result.count()!=2 or not same(map_result.get("original"),"kept") { return false }
    var item=Item(buffer("record"),list_result,0.25,true)
    var record_task=spawn(record(item)); set(item.bytes,0,88); item.values.clear()
    var record_result=join(record_task)
    if not same(record_result.bytes,"record worker") or record_result.values.count()!=3 { return false }
    if record_result.fraction!=0.25 or not record_result.flag or item.values.count()!=0 { return false }
    var enum_task=async_spawn(choice(Choice.value(record_result)))
    var enum_result=await(enum_task)
    match enum_result {
        case Choice.value(payload) { if not same(payload.bytes,"record worker enum") { return false } }
        case Choice.none { return false }
    }
    var generic_task=spawn(identity[List[buffer]](list_result))
    list_result.clear()
    var generic_result=join(generic_task)
    if generic_result.count()!=2 { return false }
    var bool_task=async_spawn(truth(true)); var bool_result=await(bool_task)
    var float_task=spawn(number(1.25)); var float_result=join(float_task)
    var str_task=async_spawn(text("static")); var str_result=await(str_task)
    if bool_result or float_result!=2.5 { return false }
    if not same(buffer(str_result),"static") { return false }
    var void_task=spawn(consume(original)); join(void_task)
    var pool_void=async_spawn(consume(original)); await(pool_void)
    if not same(original,"source caller") { return false }
    var child=spawn(nested(buffer("nested"))); var child_result=join(child)
    if not same(child_result,"nested worker") { return false }
    var scalar=spawn(legacy(41)); var scalar_result=join(scalar)
    if scalar_result!=42 { return false }
    return true
}
def main()->int {
    for round in range(40) { if not verify() { return 10 } }
    print("typed task ownership ok")
    return 0
}
"#;

#[test]
fn typed_workers_isolate_owned_data_and_release_frames_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-task-ownership");
    let source = workspace.write("main.bh", OWNERSHIP);
    let tracker = workspace.write("tracker.c", r#"
#include <stdio.h>
#include <stdlib.h>
#include <stdatomic.h>
#include <pthread.h>
static _Atomic long live;
static pthread_once_t once = PTHREAD_ONCE_INIT;
static void check_live(void) { long count=atomic_load(&live); if(count) { fprintf(stderr,"live allocations: %ld\n",count); _Exit(99); } }
static void register_check(void) { if(atexit(check_live)!=0) abort(); }
void *tracked_malloc(size_t n) { pthread_once(&once,register_check); void *p=malloc(n); if(p) atomic_fetch_add(&live,1); return p; }
void *tracked_realloc(void *p,size_t n) { int empty=p==NULL; void *q=realloc(p,n); if(q && empty) atomic_fetch_add(&live,1); return q; }
void tracked_free(void *p) { if(p) atomic_fetch_sub(&live,1); free(p); }
"#);
    workspace.write("tracked.h", "#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
    let mut runtime = Vec::new();
    for unit in ["core", "collections", "concurrency"] {
        runtime.push(workspace.write(&format!("{unit}-tracked.c"), &format!(
            "#define _POSIX_C_SOURCE 200809L\n#if defined(__APPLE__)\n#define _DARWIN_C_SOURCE\n#endif\n#include \"tracked.h\"\n#include \"{}\"\n", fixture(&format!("runtime/{unit}.c")).display())));
    }
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
                if let Some(prefix) = line
                    .strip_prefix("define ")
                    .and_then(|_| line.strip_suffix(" {"))
                {
                    format!("{prefix} sanitize_address {{\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let ir = workspace.write("typed.ll", &ir);
        for optimization in ["-O0", "-O2"] {
            let executable = workspace.file("typed");
            let output = Command::new(&clang)
                .args([
                    optimization,
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                    "-pthread",
                ])
                .arg(&ir)
                .arg(&tracker)
                .args(&runtime)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            succeeds(&output);
            let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
            succeeds(&output);
            assert_eq!(stdout(&output), "typed task ownership ok\n");
            assert!(
                output.stderr.is_empty(),
                "{frontend} {optimization}: {}",
                describe(&output)
            );
        }
    }
}

#[test]
fn typed_tasks_preserve_linear_scope_and_transitive_worker_safety() {
    let workspace = Workspace::new("typed-task-rejections");
    let worker = "def worker(value:buffer)->buffer { return value }";
    for body in [
        "var task=spawn(worker(buffer()))",
        "var task=spawn(worker(buffer())); var copy=task; join(task)",
        "var task=spawn(worker(buffer())); join(task); join(task)",
        "var task=spawn(worker(buffer())); if true {join(task)}",
        "var task=spawn(worker(buffer())); print(join(task))",
        "for i in range(1) {var task=spawn(worker(buffer())); break}",
        "for i in range(1) {var task=spawn(worker(buffer())); continue}",
        "var task=spawn(worker(buffer())); var result:int=join(task)",
    ] {
        let source = workspace.write("reject.bh", &format!("{worker}\ndef main(){{{body}}}"));
        for frontend in frontends() {
            let output = bounded(workspace.command().arg("check").arg(&source).args([
                "--frontend",
                frontend,
                "--json",
            ]));
            fails(&output);
            assert!(stdout(&output).contains("error"), "{}", describe(&output));
        }
    }
    for (index, source) in [
        "extern def foreign(n:int)->int\ndef helper()->int{return foreign(1)} def worker(value:buffer)->buffer {var n=helper(); return value} def main(){var task=spawn(worker(buffer())); var result=join(task)}",
        "def pure(value:buffer)->buffer{return value} def worker(value:buffer)->buffer {var task=async_spawn(pure(value)); var result=join(task); return result} def main(){var task=spawn(worker(buffer())); var result=join(task)}",
        "def worker(value:buffer){} def main(){var task=spawn(worker(buffer())); var result=join(task)}",
    ].iter().enumerate() {
        let source = workspace.write("graph.bh", source);
        for frontend in frontends() {
            let output = bounded(workspace.command().arg("check").arg(&source).args(["--frontend",frontend,"--json"]));
            fails(&output);
            if index < 2 {
                let code = if frontend == "rust" { "S020" } else { "BH209" };
                assert!(stdout(&output).contains(code), "{}", describe(&output));
            }
        }
    }
}

#[test]
fn imported_specialized_workers_preserve_original_type_identity() {
    let workspace = Workspace::new("typed-task-imports");
    workspace.write("worker.bh", "struct Message[T] { value:T }\ndef work[T](message:Message[T])->Message[T] {return message}\n");
    let source = workspace.write("main.bh", "import \"worker.bh\" as worker\ndef main(){var message=worker.Message[buffer](buffer(\"imported\"));var task=async_spawn(worker.work[buffer](message));set(message.value,0,88);var result=await(task);print(result.value)}");
    for frontend in frontends() {
        let output = bounded(
            workspace
                .command()
                .arg("run")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "imported\n");
    }
}

#[test]
fn task_resource_copies_preserve_tcp_and_tls_error_handle_statuses() {
    let workspace = Workspace::new("typed-task-resources");
    let source = workspace.write(
        "main.bh",
        r#"
def listener(value:tcp_listener)->tcp_listener{return value}
def socket(value:tcp_socket)->tcp_socket{return value}
def secure(value:tls_socket)->tls_socket{return value}
def main()->int {
    var l=tcp_listen(-1)
    var l_task=spawn(listener(l)); var l_result=join(l_task)
    if tcp_listener_status(l)==0 or tcp_listener_status(l_result)!=tcp_listener_status(l) {return 1}
    var s=tcp_connect("",0)
    var s_task=async_spawn(socket(s)); var s_result=await(s_task)
    if tcp_socket_status(s)==0 or tcp_socket_status(s_result)!=tcp_socket_status(s) {return 2}
    var t=tls_connect("",0,"","",0)
    var t_task=async_spawn(secure(t)); var t_result=join(t_task)
    if tls_status(t)==0 or tls_status(t_result)!=tls_status(t) {return 3}
    print("typed task resources ok")
    return 0
}
"#,
    );
    for frontend in frontends() {
        let output = bounded(
            workspace
                .command()
                .arg("run")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "typed task resources ok\n");
    }
}
