mod common;
use bhasa::ast::{ExprKind, StmtKind, Type};
use common::*;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(40);
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
        "typed tasks exceeded deadline: {}",
        describe(&output)
    );
    output
}

const OWNERSHIP: &str = r#"
struct Packet { label: buffer; values: ints; tags: List[buffer]; score: float }
enum Message { packet(value: Packet); empty }
def change_buffer(value: buffer) -> buffer { append(value, " worker"); return value }
def change_ints(value: ints) -> ints { push(value, 7); return value }
def change_packet(value: Packet) -> Packet {
    append(value.label, " worker"); push(value.values, 7)
    value.tags.push(buffer("worker")); value.score = value.score + 0.5
    return value
}
def change_message(value: Message) -> Message {
    match value {
        case Message.packet(packet) { return Message.packet(change_packet(packet)) }
        case Message.empty { return Message.empty() }
    }
}
def change_list(value: List[Packet]) -> List[Packet] {
    var first = value.get(0); first = change_packet(first); value.set(0, first)
    value.push(first); return value
}
def change_map(value: Map[str, List[buffer]]) -> Map[str, List[buffer]] {
    var found = value.get("key"); found.push(buffer("worker")); value.put("key", found)
    return value
}
def discard(value: buffer) { append(value, " discarded") }
def identity[T](value: T) -> T { return value }
def truth(value: int) -> bool { return value == 42 }
def scalar(value: int) -> int { return value + 1 }
def fraction(value: float) -> float { return value + 0.25 }
def flag(value: bool) -> bool { return not value }
def string(value: str) -> str { return value }
def make_buffer(value: int) -> buffer { return buffer("allocated result") }
def nested(value: buffer) -> buffer {
    var child = spawn(change_buffer(value)); var answer = join(child); return answer
}
def packet() -> Packet {
    var numbers = ints(); push(numbers, 1)
    var tags = List[buffer](); tags.push(buffer("original"))
    return Packet(buffer("original"), numbers, tags, 1.0)
}
def exercise() -> bool {
    var original = buffer("original")
    var first = spawn(change_buffer(original))
    var second = async_spawn(change_buffer(original))
    append(original, " caller")
    var a = join(first); var b = await(second)
    if not same(original, "original caller") or not same(a, "original worker") or not same(b, "original worker") { return false }
    var numbers = ints(); push(numbers, 1)
    var n = async_spawn(change_ints(numbers)); push(numbers, 9)
    var changed = join(n)
    if len(numbers) != 2 or get(numbers, 1) != 9 or len(changed) != 2 or get(changed, 1) != 7 { return false }
    var source = packet(); var p = spawn(change_packet(source))
    source.label = buffer("caller"); source.tags.clear(); source.score = 8.0
    var modified = await(p)
    if not same(modified.label, "original worker") or modified.tags.count() != 2 or modified.score != 1.5 or source.score != 8.0 { return false }
    var message = Message.packet(packet()); var e = async_spawn(change_message(message))
    message = Message.empty(); var response = await(e)
    match response {
        case Message.packet(value) { if not same(value.label, "original worker") { return false } }
        case Message.empty { return false }
    }
    var items = List[Packet](); items.push(packet())
    var l = spawn(change_list(items)); items.clear(); var list = join(l)
    if items.count() != 0 or list.count() != 2 or not same(list.get(0).label, "original worker") { return false }
    var groups = Map[str, List[buffer]](); var labels = List[buffer](); labels.push(buffer("original")); groups.put("key", labels)
    var m = async_spawn(change_map(groups)); groups.clear(); var map = await(m)
    if groups.count() != 0 or map.get("key").count() != 2 or not same(map.get("key").get(0), "original") { return false }
    var d = spawn(discard(original)); join(d)
    var dp = async_spawn(discard(buffer("temporary"))); await(dp)
    var drop_result = spawn(change_buffer(buffer("temporary"))); join(drop_result)
    var drop_result_pool = async_spawn(change_list(list)); await(drop_result_pool)
    var generic = async_spawn(identity[Packet](packet())); var copy = await(generic)
    if not same(copy.label, "original") { return false }
    var scoped = async_spawn(nested(buffer("nested"))); var nested_result = await(scoped)
    if not same(nested_result, "nested worker") { return false }
    var field_task = spawn(change_buffer(packet().label)); var field_result = join(field_task)
    if not same(field_result, "original worker") { return false }
    var bi = async_spawn(truth(42)); var bo = spawn(flag(false)); var fl = async_spawn(fraction(0.5)); var st = spawn(string("static"))
    var boolean = await(bi); var inverted = join(bo); var floating = await(fl); var text = join(st)
    if not boolean or not inverted or floating != 0.75 or not same(buffer(text), "static") { return false }
    var si = spawn(scalar(41)); var sp = async_spawn(scalar(41)); var one = join(si); var two = await(sp)
    if one != 42 or two != 42 { return false }
    var allocated = spawn(make_buffer(1)); var output = join(allocated)
    if not same(output, "allocated result") { return false }
    for index in range(3) { var loop_task = spawn(change_buffer(original)); var value = join(loop_task); if index == 1 { continue }; if index == 2 { break } }
    return true
}
def main() -> int {
    for round in range(12) { if not exercise() { return 1 } }
    print("typed task ownership ok")
    return 0
}
"#;

#[test]
fn task_bindings_preserve_result_types_without_changing_task_type() {
    let source = "def worker(value: buffer) -> bool { return len(value) > 0 } def main() { var job = spawn(worker(buffer(\"x\"))); var result = join(job) }";
    let checked = bhasa::check(source).unwrap();
    let body = checked
        .program
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap()
        .body
        .as_ref()
        .unwrap();
    let StmtKind::Var { value: spawn, .. } = &body[0].kind else {
        panic!("expected binding")
    };
    let StmtKind::Var { value: join, .. } = &body[1].kind else {
        panic!("expected binding")
    };
    assert_eq!(spawn.ty, Some(Type::Task));
    assert_eq!(join.ty, Some(Type::Bool));
    let ExprKind::Call { args, .. } = &spawn.kind else {
        panic!("expected call")
    };
    assert_eq!(args[0].ty, Some(Type::Bool));
    bhasa::check(OWNERSHIP).unwrap();
}

#[test]
fn task_runtime_abi_preserves_scalar_fast_path_and_uses_typed_wrappers() {
    let ir = bhasa::compile_to_llvm(
        OWNERSHIP,
        &bhasa::target::Target::Native.triple_for_ir().unwrap(),
    )
    .unwrap();
    for symbol in [
        "bhasa_rt_spawn_i64",
        "bhasa_rt_async_spawn_i64",
        "bhasa_rt_join_i64",
        "bhasa_rt_await_i64",
        "bhasa_rt_spawn_owned",
        "bhasa_rt_async_spawn_owned",
        "bhasa_rt_join_owned",
        "bhasa_rt_await_owned",
        "bhasa_rt_task_alloc",
        "bhasa_rt_task_free",
    ] {
        assert!(ir.contains(symbol), "missing {symbol}");
    }
    assert!(ir.contains("; bhasa runtime: threads"));
    assert_eq!(
        ir.matches("define internal ptr @bhasa.task.change_buffer(")
            .count(),
        1
    );
    assert!(ir.contains("define internal ptr @bhasa.task.discard("));
    assert!(!ir.contains("@bhasa.task.scalar("));
}

#[test]
fn typed_tasks_retain_linear_lexical_and_call_graph_restrictions() {
    for (body, expected) in [
        ("var job = spawn(worker(buffer()));", "must be joined"),
        (
            "var job = spawn(worker(buffer())); if true { join(job) }",
            "same lexical block",
        ),
        (
            "var job = spawn(worker(buffer())); join(job); join(job)",
            "already been joined",
        ),
        (
            "var job = spawn(worker(buffer())); var copy = job; join(job)",
            "can only be consumed",
        ),
        (
            "var job = spawn(worker(buffer())); print(join(job))",
            "complete variable initializer",
        ),
        (
            "var job = spawn(worker(buffer())); var value: int = join(job)",
            "requires int",
        ),
        (
            "var job = spawn(worker(buffer())); while true { break }; return",
            "must be joined",
        ),
        (
            "for x in range(2) { var job = spawn(worker(buffer())); continue }",
            "must be joined",
        ),
    ] {
        let source = format!(
            "def worker(value: buffer) -> buffer {{ return value }} def main() {{ {body} }}"
        );
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "{source}\n{errors:?}"
        );
    }
    for source in [
        "def worker(value: buffer) {} def main() { var job = spawn(worker(buffer())); var result = join(job) }",
        "def invalid() -> task { return 1 } def main() { var job = invalid(); join(job) }",
        "def worker(value: buffer, other: int) -> buffer { return value } def main() { var job = spawn(worker(buffer(), 1)); join(job) }",
        "struct Box { value: buffer; def work(self) -> buffer { return self.value } } def main() { var value = Box(buffer()); var job = spawn(value.work()); join(job) }",
        "extern def foreign(value: int) -> int\ndef helper(value: buffer) -> buffer { var result = foreign(1); return value } def worker(value: buffer) -> buffer { return helper(value) } def main() { var job = spawn(worker(buffer())); join(job) }",
        "def leaf(value: buffer) -> buffer { return value } def helper(value: buffer) -> buffer { var job = async_spawn(leaf(value)); var result = await(job); return result } def worker(value: buffer) -> buffer { return helper(value) } def main() { var job = spawn(worker(buffer())); join(job) }",
    ] { assert!(bhasa::check(source).is_err(), "accepted {source}"); }
    bhasa::check("def worker(value: buffer) {} def main() { var job = spawn(worker(buffer())); join(job); var pooled = async_spawn(worker(buffer())); await(pooled) }").unwrap();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn typed_task_frames_and_owned_values_are_isolated_and_freed_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("typed-task-reference");
    let tracker = workspace.write("tracker.c", r#"
#include <stdatomic.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic long live;
static pthread_once_t registered = PTHREAD_ONCE_INIT;
static void check_live(void) { long count = atomic_load(&live); if (count) { fprintf(stderr,"live allocations: %ld\n",count); _Exit(99); } }
static void register_check(void) { atexit(check_live); }
void *tracked_malloc(size_t size) { pthread_once(&registered, register_check); void *value = malloc(size); if(value) atomic_fetch_add(&live,1); return value; }
void *tracked_realloc(void *value,size_t size) { int empty = value == NULL; void *result = realloc(value,size); if(result && empty) atomic_fetch_add(&live,1); return result; }
void tracked_free(void *value) { if(value) atomic_fetch_sub(&live,1); free(value); }
"#);
    workspace.write("tracked.h", "#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
    let runtime = ["core", "collections", "concurrency"].map(|unit| workspace.write(&format!("{unit}-tracked.c"), &format!("#define _POSIX_C_SOURCE 200809L\n#if defined(__APPLE__)\n#define _DARWIN_C_SOURCE\n#endif\n#include \"tracked.h\"\n#include \"{}\"\n", fixture(&format!("runtime/{unit}.c")).display())));
    let ir = bhasa::compile_to_llvm(
        OWNERSHIP,
        &bhasa::target::Target::Native.triple_for_ir().unwrap(),
    )
    .unwrap()
    .replace("@malloc(", "@tracked_malloc(")
    .replace("@free(", "@tracked_free(");
    let ir = ir
        .lines()
        .map(|line| {
            if let Some(header) = line
                .strip_prefix("define ")
                .and_then(|_| line.strip_suffix(" {"))
            {
                format!("{header} sanitize_address {{\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    let ir = workspace.write("typed-tasks.ll", &ir);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("typed-tasks");
        let output = Command::new(&clang)
            .args([
                optimization,
                "-pthread",
                "-fsanitize=address,undefined",
                "-fno-sanitize-recover=all",
                "-fno-omit-frame-pointer",
            ])
            .arg(&ir)
            .arg(&tracker)
            .args(&runtime)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        succeeds(&output);
        let output = bounded(
            Command::new(&executable)
                .current_dir(workspace.path())
                .env("UBSAN_OPTIONS", "halt_on_error=1"),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "typed task ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}
