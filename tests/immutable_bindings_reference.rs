mod common;
use bhasa::ast::StmtKind;
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
    let deadline = Instant::now() + Duration::from_secs(30);
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
        "immutable binding fixture timed out: {}",
        describe(&output)
    );
    output
}

#[test]
fn contextual_let_parses_binding_flags_and_preserves_ordinary_names() {
    let source = "enum R{ok(value:int);error(code:int)}def f(source:R)->R{let first:int=1;var second=2;let third=try source;var fourth=try source;return R.ok(first+second+third+fourth)}";
    let program = bhasa::parser::parse(source).unwrap();
    let body = program.functions[0].body.as_ref().unwrap();
    for (statement, expected) in body.iter().take(4).zip([false, true, false, true]) {
        let (StmtKind::Var { reassignable, .. } | StmtKind::TryVar { reassignable, .. }) =
            &statement.kind
        else {
            panic!()
        };
        assert_eq!(*reassignable, expected);
    }
    for (text, expected) in [
        ("let value=1", true),
        ("let let=1", true),
        ("let\nvalue=1", false),
        ("let # comment\nvalue=1", false),
        ("let(value)", false),
        ("let[int](value)", false),
        ("let.field", false),
        ("let=1", false),
        ("var value=1", false),
    ] {
        let tokens = bhasa::lexer::lex(text).unwrap();
        assert_eq!(
            bhasa::parser::starts_let_declaration(&tokens, 0),
            expected,
            "{text}"
        );
        assert!(!bhasa::parser::starts_let_declaration(&tokens, usize::MAX));
    }
    for source in [
        "def let(value:int)->int{return value+1}def main(){let result=let(41);print(result)}",
        "def let[T](value:T)->T{return value}def main(){let result=let[int](42);print(result)}",
        "struct Item{value:int;def let(self)->int{return self.value}}def main(){let item=Item(42);print(item.let())}",
        "struct Item{value:int}def main(){var let=Item(1);let.value=2;let value=let.value;print(value)}",
        "def main(){var let=1;let=2;let value=let+1;print(value)}",
        "def main(){let let=1;print(let)}",
        "enum R{ok(value:int);error(code:int)}def f(source:R)->R{let let=try source;return R.ok(let)}",
        "def f[let](value:let)->let{return value}def main(){let result=f[int](42);print(result)}",
    ] {bhasa::check(source).unwrap_or_else(|errors|panic!("{source}: {errors:?}"));}
    for source in [
        "def main(){let value}",
        "def main(){let value:void=print(1)}",
        "def main(){let value=1;let value=2}",
    ] {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
}

#[test]
fn reassignments_reject_the_exact_immutable_binding_in_every_scope() {
    for (definitions, initializer, replacement) in [
        ("", "1", "2"),
        ("", "1.0", "2.0"),
        ("", "true", "false"),
        ("", "\"a\"", "\"b\""),
        ("", "buffer(\"a\")", "buffer(\"b\")"),
        ("", "ints()", "ints()"),
        ("struct Item{value:int}", "Item(1)", "Item(2)"),
        (
            "enum R{ok(value:int);error(code:int)}",
            "R.ok(1)",
            "R.error(2)",
        ),
        ("", "List[buffer]()", "List[buffer]()"),
        ("", "Map[str,buffer]()", "Map[str,buffer]()"),
        (
            "",
            "tcp_listen_on(\"127.0.0.1\",0)",
            "tcp_listen_on(\"127.0.0.1\",0)",
        ),
    ] {
        let assignment = format!("value={replacement}");
        for body in [
            format!("let value={initializer};{assignment}"),
            format!("let value={initializer};if true{{{assignment}}}"),
            format!("let value={initializer};for n in range(1){{{assignment}}}"),
        ] {
            let source = format!("{definitions}def main(){{{body}}}");
            let errors = bhasa::check(&source).unwrap_err();
            let error = errors
                .iter()
                .find(|error| error.code == "S036")
                .unwrap_or_else(|| panic!("{source}: {errors:?}"));
            assert_eq!(
                &source[error.span.start..error.span.end],
                assignment,
                "{source}"
            );
        }
    }
    for source in [
        "def main(){var value=1;if true{let value=2;value=3}}",
        "def main(){let value=1;if true{var value=2;value=3}value=4}",
        "enum R{ok(value:int);error(code:int)}def f(source:R)->R{let value=try source;value=2;return R.ok(value)}",
    ] {assert!(bhasa::check(source).unwrap_err().iter().any(|error|error.code=="S036"),"{source}");}
}

#[test]
fn field_mutation_borrows_and_existing_parameter_loop_match_rules_remain_valid() {
    for source in [
        "def main(){let value=buffer();append(value,\"x\");set(value,0,121);let numbers=ints();push(numbers,1);set(numbers,0,2)}",
        "struct Item{value:buffer;def change(self){self.value=buffer(\"method\")}}def mutate(item:Item){item.value=buffer(\"call\")}def main(){let item=Item(buffer());item.value=buffer(\"field\");append(item.value,\"nested\");item.change();mutate(item)}",
        "def main(){let list=List[buffer]();list.push(buffer());list.set(0,buffer(\"x\"));list.clear();let map=Map[str,int]();map.put(\"a\",1);let removed=map.remove(\"a\");map.clear()}",
        "def main(){let value=1;if true{var value=value+1;value=3;print(value)}print(value)}",
        "def main(){var value=1;if true{let value=value+1;print(value)}value=3;print(value)}",
        "def add(value:int)->int{value=value+1;return value}def main(){let value=1;print(add(value))}",
        "enum E{some(value:int)}def main(){let value=1;for value in range(2){value=9;print(value)}match E.some(2){case E.some(value){value=3;print(value)}}print(value)}",
    ] {bhasa::check(source).unwrap_or_else(|errors|panic!("{source}: {errors:?}"));}
    let errors = bhasa::check(
        "def rebind(value:buffer){value=buffer()}def main(){let value=buffer();rebind(value)}",
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S021"));
    let errors =
        bhasa::check("struct Item{value:int}def f(item:Item){let item=Item(1)}").unwrap_err();
    assert!(errors.iter().any(|error| error.code == "S007"));
}

#[test]
fn corrupted_checked_ast_cannot_reassign_var_or_try_bindings_marked_immutable() {
    for source in [
        "def main(){var value=1;value=2}",
        "def main(){var value=buffer(\"a\");value=buffer(\"b\")}",
        "enum R{ok(value:buffer);error(code:int)}def f(source:R)->R{var value=try source;value=buffer(\"b\");return R.ok(value)}",
    ] {
        let mut checked=bhasa::check(source).unwrap();
        let body=checked.program.functions[0].body.as_mut().unwrap();
        let (StmtKind::Var{reassignable,..}|StmtKind::TryVar{reassignable,..})=&mut body[0].kind else {panic!()};
        *reassignable=false;
        let errors=bhasa::codegen::emit_llvm(&checked,"x86_64-unknown-linux-gnu").unwrap_err();
        assert!(errors.iter().any(|error|error.code=="C001"&&error.message.contains("immutable binding")),"{source}: {errors:?}");
    }
    // Inner mutable declarations keep their own flags when they shadow a let.
    let checked=bhasa::check("def main(){let value=buffer(\"a\");if true{var value=value;value=buffer(\"b\")}append(value,\"c\")}").unwrap();
    bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").unwrap();
}

#[test]
fn immutable_task_bindings_keep_consumption_and_transitive_worker_checks() {
    let definitions =
        "enum R{ok(value:buffer);error(code:int)}def worker(value:buffer)->R{return R.ok(value)}";
    for submit in ["spawn", "async_spawn"] {
        for join in ["join", "await"] {
            bhasa::check(&format!("{definitions}def f()->R{{let task={submit}(worker(buffer()));let value=try {join}(task);append(value,\"x\");return R.ok(value)}}")).unwrap();
        }
    }
    bhasa::check("def worker(value:int){}def main(){let task=spawn(worker(1));join(task)}")
        .unwrap();
    for body in [
        "let task=spawn(worker(buffer()))",
        "let task=spawn(worker(buffer()));let result=join(task);let again=join(task)",
        "let task=spawn(worker(buffer()));if true{var task=1};let result=join(task)",
        "let task=spawn(worker(buffer()));let other=spawn(worker(buffer()));let result=try join(task);let done=join(other);return R.ok(result)",
    ] {
        let source=format!("{definitions}def f()->R{{{body}}}");
        assert!(bhasa::check(&source).unwrap_err().iter().any(|error|error.code=="S019"),"{source}");
    }
    let source="extern def unsafe(value:int)->int\ndef worker(value:int)->int{let copied=unsafe(value);return copied}def main(){let task=spawn(worker(1));let answer=join(task)}";
    assert!(bhasa::check(source)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "S020"));
}

const OWNERSHIP: &str = r#"
struct Item{bytes:buffer;def add(self){append(self.bytes,"!")}}
enum Reply[T]{error(problem:buffer);ok(value:T)}
def mutate(value:Item){append(value.bytes,"?")}
def worker(value:Item)->Reply[Item]{value.add();return Reply[Item].ok(value)}
def joined(value:Item)->Reply[int]{
    let scratch=buffer("scope")
    let task=spawn(worker(value))
    let result=try join(task)
    result.add()
    return Reply[int].ok(len(result.bytes))
}
def propagate(source:Reply[buffer])->Reply[int]{
    let values=Map[str,buffer]();values.put("key",buffer("snapshot"))
    for key in values{
        match Reply[int].ok(1){
            case Reply[int].ok(number){
                let scratch=Item(buffer("arm"))
                let result=try source
                append(result,"!")
                return Reply[int].ok(len(result))
            }
            case Reply[int].error(problem){return Reply[int].error(problem)}
        }
    }
    return Reply[int].error(buffer("unreachable"))
}
def run()->int{
    let original=Item(buffer("a"));original.add();mutate(original)
    let copied=original;original.bytes=buffer("changed")
    if not same(copied.bytes,"a!?"){return 1}
    let parts=List[buffer]();parts.push(copied.bytes)
    let part=parts.get(0);append(part,"owned")
    if not same(parts.get(0),"a!?"){return 2}
    for part in parts{part=buffer("iteration");append(part,"!")}
    let items=Map[str,Item]();items.put("key",copied)
    let item=items.get("key");item.bytes=buffer("detached")
    if not same(items.get("key").bytes,"a!?"){return 3}
    let numbers=ints();push(numbers,42);set(numbers,0,43)
    if get(numbers,0)!=43{return 4}
    let success=Reply[buffer].ok(buffer("x"))
    match propagate(success){case Reply[int].ok(value){if value!=2{return 5}}case Reply[int].error(problem){return 6}}
    match success{case Reply[buffer].ok(value){if not same(value,"x"){return 7}}case Reply[buffer].error(problem){return 8}}
    let failure=Reply[buffer].error(buffer("failure"))
    match propagate(failure){case Reply[int].ok(value){return 9}case Reply[int].error(problem){append(problem,"changed")}}
    match failure{case Reply[buffer].ok(value){return 10}case Reply[buffer].error(problem){if not same(problem,"failure"){return 11}}}
    match joined(copied){case Reply[int].ok(value){if value!=5{return 12}}case Reply[int].error(problem){return 13}}
    let task=async_spawn(worker(copied));let result=await(task)
    match result{case Reply[Item].ok(value){if not same(value.bytes,"a!?!"){return 14}}case Reply[Item].error(problem){return 15}}
    if not same(copied.bytes,"a!?"){return 16}
    let number=1;if true{var number=number+1;number=3;if number!=3{return 17}}if number!=1{return 18}
    parts.clear();items.clear()
    return 0
}
def main()->int{for iteration in range(12){let status=run();if status!=0{return status}}print("immutable binding ownership ok");return 0}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn immutable_bindings_preserve_owned_cleanup_and_mutation_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("immutable-binding-ownership");
    let tracker=workspace.write("tracker.c",r#"
#include <stdatomic.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic long live;static pthread_once_t registered=PTHREAD_ONCE_INIT;
static void check_live(void){long count=atomic_load(&live);if(count){fprintf(stderr,"live allocations: %ld\n",count);_Exit(99);}}
static void register_check(void){atexit(check_live);}
void *tracked_malloc(size_t size){pthread_once(&registered,register_check);void *value=malloc(size);if(value)atomic_fetch_add(&live,1);return value;}
void *tracked_realloc(void *value,size_t size){int empty=value==NULL;void *result=realloc(value,size);if(result&&empty)atomic_fetch_add(&live,1);return result;}
void tracked_free(void *value){if(value)atomic_fetch_sub(&live,1);free(value);}
"#);
    workspace.write("tracked.h","#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
    let runtime=["core","collections","concurrency"].map(|unit|workspace.write(&format!("{unit}-tracked.c"),&format!("#define _POSIX_C_SOURCE 200809L\n#if defined(__APPLE__)\n#define _DARWIN_C_SOURCE\n#endif\n#include \"tracked.h\"\n#include \"{}\"\n",fixture(&format!("runtime/{unit}.c")).display())));
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
            if line.starts_with("define ") {
                format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    let file = workspace.write("owned.ll", &ir);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("owned");
        succeeds(
            &Command::new(&clang)
                .args([
                    optimization,
                    "-pthread",
                    "-fsanitize=address,undefined",
                    "-fno-sanitize-recover=all",
                    "-fno-omit-frame-pointer",
                ])
                .arg(&file)
                .arg(&tracker)
                .args(&runtime)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(Command::new(&executable).env("UBSAN_OPTIONS", "halt_on_error=1"));
        succeeds(&output);
        assert_eq!(stdout(&output), "immutable binding ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}
