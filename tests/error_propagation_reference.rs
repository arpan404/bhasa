mod common;
use bhasa::ast::{ExprKind, StmtKind, Type};
use common::*;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "error propagation exceeded deadline: {}",
                describe(&child.wait_with_output().unwrap())
            )
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
const TYPES: &str = "enum Input{error(message:buffer);ok(value:int)} enum Output{ok(value:buffer);error(reason:buffer)}";

#[test]
fn propagation_records_exact_binding_spans_and_validates_checked_metadata() {
    let source=format!("{TYPES} def convert(input:Input)->Output{{var result:int=try input;return Output.ok(to_buffer(result))}}");
    let checked = bhasa::check(&source).unwrap();
    let StmtKind::TryVar {
        name,
        name_span,
        try_span,
        success_type,
        value,
        ..
    } = &checked.program.functions[0].body.as_ref().unwrap()[0].kind
    else {
        panic!()
    };
    assert_eq!(name, "result");
    assert_eq!(&source[name_span.start..name_span.end], "result");
    assert_eq!(&source[try_span.start..try_span.end], "try");
    assert_eq!(*success_type, Some(Type::Int));
    assert!(matches!(value.ty, Some(Type::Enum(_))));
    let shape = checked.program.enums[0].result_shape().unwrap();
    assert_eq!(shape.ok_index, 1);
    assert_eq!(shape.error_index, 0);
    assert_eq!(shape.success, Type::Int);
    assert_eq!(shape.error, Type::Buffer);
    for corrupt in [
        "missing-success",
        "wrong-success",
        "annotation",
        "operand",
        "return",
    ] {
        let mut broken = checked.clone();
        let StmtKind::TryVar {
            success_type,
            annotation,
            value,
            ..
        } = &mut broken.program.functions[0].body.as_mut().unwrap()[0].kind
        else {
            panic!()
        };
        match corrupt {
            "missing-success" => *success_type = None,
            "wrong-success" => *success_type = Some(Type::Bool),
            "annotation" => *annotation = Some(Type::Str),
            "operand" => {
                value.kind = ExprKind::Bool(true);
                value.ty = Some(Type::Bool)
            }
            "return" => broken.program.enums[1].variants[1].fields[0].ty = Type::Str,
            _ => unreachable!(),
        }
        assert!(
            bhasa::codegen::emit_llvm(&broken, "x86_64-unknown-linux-gnu").is_err(),
            "{corrupt}"
        );
    }
    let mut merged = bhasa::ast::Program::default();
    merged.intern_type_name("Earlier");
    merged.merge(checked.program);
    let StmtKind::TryVar {
        value,
        success_type,
        ..
    } = &merged.functions[0].body.as_ref().unwrap()[0].kind
    else {
        panic!()
    };
    assert_eq!(value.ty, Some(Type::Enum(1)));
    assert_eq!(*success_type, Some(Type::Int));
    // The result of the old binding initializes its new, inner success binding.
    bhasa::check(&format!("{TYPES} def convert(value:Input)->Output{{if true{{var value=try value;return Output.ok(to_buffer(value))}}return Output.ok(buffer())}} ")).unwrap();
}

#[test]
fn contextual_try_preserves_existing_names_and_rejects_unsupported_positions() {
    for source in [
        "def try(value:int)->int{return value} def main(){var value=try(1);print(value)}",
        "def try[T](value:T)->T{return value} def main(){var value=try[int](1);print(value)}",
        "struct Item{value:int} def main(){var try=Item(1);var value=try.value;print(value)}",
        "def main(){var try=1;var one=try;var two=try+1;var three=try-1;print(one);print(two);print(three)}",
        "def main(){var try=true;var value=try and false;print(value)}",
        "enum R{ok(value:int);error(message:buffer)}def convert(try:R)->R{var value=try try;return R.ok(value)}",
        "enum R{ok(value:int);error(message:buffer)}def convert(input:R)->R{var try=try input;var value=try+1;return R.ok(value)}",
        "enum R{ok(value:int);error(message:buffer)}def try(value:R)->R{return value}def convert(input:R)->R{var value=try try(input);return R.ok(value)}",
        "def action(){} def main(){var try=1;var value=try # ordinary initializer\n action();print(value)}",
        "enum try{ok(value:int);error(message:buffer)} def main(){var value=try.ok(1);match value{case try.ok(x){print(x)}case try.error(message){print(message)}}}",
    ] { bhasa::check(source).unwrap_or_else(|errors|panic!("{source}: {errors:?}")); }
    let valid=format!("{TYPES} def convert(input:Input)->Output{{var result=try input;return Output.ok(to_buffer(result))}}");
    for malformed in [
        "var result=(try input)",
        "var result=try (input)",
        "var result=try try input",
        "var result=try\ninput",
        "var result=try # no continuation\ninput",
        "var result=to_buffer(try input)",
        "var result=0;result=try input",
        "return try input",
        "try input",
    ] {
        let source = valid.replace("var result=try input", malformed);
        assert!(bhasa::check(&source).is_err(), "{source}");
    }
    for (enums, body) in [
        (
            "enum Input{ok(value:int);error(message:buffer);pending}",
            "var x=try input;return input",
        ),
        (
            "enum Input{ok;error(message:buffer)}",
            "var x=try input;return input",
        ),
        (
            "enum Input{ok(value:int);error}",
            "var x=try input;return input",
        ),
        (
            "enum Input{ok(a:int,b:int);error(message:buffer)}",
            "var x=try input;return input",
        ),
        (
            "enum Input{ok(value:int);error(a:int,b:int)}",
            "var x=try input;return input",
        ),
        (
            "enum Input{yes(value:int);error(message:buffer)}",
            "var x=try input;return input",
        ),
    ] {
        let source = format!("{enums} def convert(input:Input)->Input{{{body}}}");
        assert!(
            bhasa::check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S035"),
            "{source}"
        );
    }
    for source in [
        format!("{TYPES} def bad(input:Input)->int{{var value=try input;return value}}"),
        format!("{TYPES} def bad(input:Input)->Output{{var value:bool=try input;return Output.ok(buffer())}}"),
        "enum A{ok(value:int);error(value:buffer)}enum B{ok(value:int);error(value:str)}def bad(input:A)->B{var value=try input;return B.ok(value)}".into(),
        "struct A{value:int}struct B{value:int}enum R{ok(value:int);error(value:A)}enum S{ok(value:int);error(value:B)}def bad(input:R)->S{var value=try input;return S.ok(value)}".into(),
        format!("{TYPES} def bad(input:Input)->Output{{var value=try input}}"),
        format!("{TYPES} def bad(input:Input)->Output{{var value=try 1;return Output.ok(buffer())}}"),
    ] {assert!(bhasa::check(&source).is_err(),"{source}");}
}

#[test]
fn bare_and_bound_result_discard_require_handling() {
    let definitions="enum R{error(message:buffer);ok(value:int)}def make()->R{return R.ok(1)}struct Holder{value:R;def read(self)->R{return self.value}}def worker(value:int)->R{return R.ok(value)}";
    for statement in [
        "make()",
        "R.error(buffer())",
        "var value=make();value",
        "Holder(make()).read()",
        "Holder(make()).value",
        "var values=List[R]();values.push(make());values.get(0)",
        "var values=Map[int,R]();values.put(0,make());values.get(0)",
        "var task=spawn(worker(1));join(task)",
        "var task=async_spawn(worker(1));await(task)",
    ] {
        let source = format!("{definitions} def main(){{{statement}}}");
        let errors = bhasa::check(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == "S022"),
            "{source}: {errors:?}"
        );
    }
    assert!(bhasa::check(&format!("{definitions} def main(){{var unused=make();var task=spawn(worker(1));var ignored=join(task)}}")).unwrap_err().iter().any(|error| error.code == "S037"));
    bhasa::check("enum Option{none;some(value:int)}def main(){Option.none()}").unwrap();
    bhasa::check("enum Other{ok(value:int);error(value:int);pending}def main(){Other.pending()}")
        .unwrap();
}

#[test]
fn propagation_consumes_its_join_first_and_rejects_every_other_live_task() {
    let definitions =
        "enum R{ok(value:int);error(message:buffer)}def worker(value:int)->R{return R.ok(value)}";
    for join in ["join", "await"] {
        bhasa::check(&format!("{definitions} def convert()->R{{var task=async_spawn(worker(1));var value=try {join}(task);return R.ok(value)}}")).unwrap();
        let source=format!("{definitions} def convert()->R{{var other=spawn(worker(1));var task=spawn(worker(1));var value=try {join}(task);var done=join(other);return R.ok(value)}}");
        assert!(bhasa::check(&source)
            .unwrap_err()
            .iter()
            .any(|error| error.code == "S019"));
    }
    for body in [
        "var value=try R.ok(1);var done=join(task);return R.ok(value)",
        "if true{var value=try R.ok(1)}var done=join(task);return R.ok(1)",
        "for byte in buffer(\"a\"){var value=try R.ok(byte)}var done=join(task);return R.ok(1)",
    ] {
        let source = format!("{definitions} def convert()->R{{var task=spawn(worker(1));{body}}}");
        assert!(
            bhasa::check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S019"),
            "{source}"
        );
    }
    for definitions in [
        "extern def foreign(value:int)->int\ndef result()->R{return R.ok(foreign(1))}def work(value:int)->R{var n=try result();return R.ok(n)}",
        "def pure(value:int)->R{return R.ok(value)}def result()->R{var task=async_spawn(pure(1));var n=try await(task);return R.ok(n)}def work(value:int)->R{var n=try result();return R.ok(n)}",
    ] {
        let source=format!("enum R{{ok(value:int);error(message:buffer)}}{definitions}def main(){{var task=spawn(work(1));var result=join(task)}}");
        assert!(bhasa::check(&source).unwrap_err().iter().any(|error|error.code=="S020"),"{source}");
    }
}

const OWNERSHIP: &str = r#"
struct Payload{message:buffer;items:List[buffer];table:Map[str,buffer]}
enum Event{some(value:Payload);none}
enum Input[T,E]{error(reason:E);ok(value:T)}
enum Output[T,E]{ok(value:T);error(reason:E)}
def payload()->Payload{var items=List[buffer]();items.push(buffer("inside"));var table=Map[str,buffer]();table.put("key",buffer("map"));return Payload(buffer("error"),items,table)}
def forward[T,E](value:Input[T,E])->Output[T,E]{var result=try value;return Output[T,E].ok(result)}
def measure(value:Input[buffer,Payload])->Output[int,Payload]{var bytes=try value;append(bytes,"!");return Output[int,Payload].ok(len(bytes))}
struct Counter{
 calls:int
 def read(self,fail:bool)->Input[buffer,Payload]{self.calls=self.calls+1;if fail{return Input[buffer,Payload].error(payload())}return Input[buffer,Payload].ok(buffer("read"))}
}
def nested(counter:Counter,fail:bool)->Output[int,Payload]{
 var function_value=payload();var table=Map[int,Payload]();table.put(1,payload());table.put(2,payload())
 for key in table{
  var loop_value=payload()
  match Event.some(payload()){
   case Event.some(value){var arm_value=payload();var bytes=try counter.read(fail);return Output[int,Payload].ok(len(bytes))}
   case Event.none{continue}
  }
 }
 return Output[int,Payload].ok(0)
}
def worker(fail:bool)->Input[buffer,Payload]{if fail{return Input[buffer,Payload].error(payload())}return Input[buffer,Payload].ok(buffer("worker"))}
def joined(fail:bool)->Output[int,Payload]{var local=payload();var task=async_spawn(worker(fail));var bytes=try await(task);return Output[int,Payload].ok(len(bytes))}
def exercise()->bool{
 var counter=Counter(0)
 match nested(counter,false){case Output[int,Payload].ok(n){if n!=4{return false}}case Output[int,Payload].error(e){return false}}
 if counter.calls!=1{return false}
 match nested(counter,true){case Output[int,Payload].ok(n){return false}case Output[int,Payload].error(e){if not same(e.message,"error") or e.items.count()!=1{return false};append(e.message," modified")}}
 if counter.calls!=2{return false}
 var source=Input[buffer,Payload].ok(buffer("original"))
 var measured_ok=false;var measured_size=0
 match measure(source){case Output[int,Payload].ok(n){measured_ok=true;measured_size=n}case Output[int,Payload].error(e){measured_ok=false}}
 match source{case Input[buffer,Payload].ok(bytes){if not same(bytes,"original"){return false}}case Input[buffer,Payload].error(e){return false}}
 if not measured_ok or measured_size!=9{return false}
 source=Input[buffer,Payload].error(payload())
 var measured_error=false
 match measure(source){case Output[int,Payload].ok(n){measured_error=false}case Output[int,Payload].error(e){measured_error=true;append(e.message," changed");e.items.clear();e.table.clear()}}
 match source{case Input[buffer,Payload].ok(bytes){return false}case Input[buffer,Payload].error(e){if not same(e.message,"error") or e.items.count()!=1 or e.table.count()!=1{return false}}}
 if not measured_error{return false}
 match joined(false){case Output[int,Payload].ok(n){if n!=6{return false}}case Output[int,Payload].error(e){return false}}
 match joined(true){case Output[int,Payload].ok(n){return false}case Output[int,Payload].error(e){if not same(e.message,"error"){return false}}}
 var numbers=ints();push(numbers,-7)
 match forward[ints,buffer](Input[ints,buffer].ok(numbers)){case Output[ints,buffer].ok(values){push(values,8);if len(values)!=2{return false}}case Output[ints,buffer].error(e){return false}}
 if len(numbers)!=1{return false}
 match forward[Payload,buffer](Input[Payload,buffer].ok(payload())){case Output[Payload,buffer].ok(value){if value.table.count()!=1{return false}}case Output[Payload,buffer].error(e){return false}}
 match forward[Event,buffer](Input[Event,buffer].ok(Event.some(payload()))){case Output[Event,buffer].ok(event){match event{case Event.some(value){if value.items.count()!=1{return false}}case Event.none{return false}}}case Output[Event,buffer].error(e){return false}}
 match forward[int,Event](Input[int,Event].error(Event.some(payload()))){case Output[int,Event].ok(value){return false}case Output[int,Event].error(event){match event{case Event.some(value){if value.table.count()!=1{return false}}case Event.none{return false}}}}
 var list=List[buffer]();list.push(buffer("list"));var map=Map[str,List[buffer]]();map.put("key",list)
 match forward[Map[str,List[buffer]],int](Input[Map[str,List[buffer]],int].ok(map)){case Output[Map[str,List[buffer]],int].ok(value){if value.get("key").count()!=1{return false};value.clear()}case Output[Map[str,List[buffer]],int].error(e){return false}}
 match forward[int,Map[str,List[buffer]]](Input[int,Map[str,List[buffer]]].error(map)){case Output[int,Map[str,List[buffer]]].ok(value){return false}case Output[int,Map[str,List[buffer]]].error(error){if error.count()!=1{return false};error.clear()}}
 if map.count()!=1{return false}
 return true
}
def main()->int{for i in range(8){if not exercise(){return 1}}print("error propagation ownership ok");return 0}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn propagation_releases_owned_payloads_scopes_and_tasks_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("error-propagation-ownership");
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
        assert_eq!(stdout(&output), "error propagation ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn corrupt_result_tags_trap_before_payload_access() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("error-propagation-tag");
    let source="enum R{ok(value:int);error(code:int)}def read()->R{return R.ok(7)}def forward()->R{var n=try read();return R.ok(n)}def main()->int{match forward(){case R.ok(n){return 0}case R.error(n){return 1}}}";
    let ir = bhasa::compile_to_llvm(
        source,
        &bhasa::target::Target::Native.triple_for_ir().unwrap(),
    )
    .unwrap();
    let broken = ir.replacen(".new(i64 0,", ".new(i64 77,", 1);
    assert_ne!(broken, ir);
    let file = workspace.write("corrupt.ll", &broken);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("corrupt");
        succeeds(
            &Command::new(&clang)
                .arg(optimization)
                .arg(&file)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        assert!(!bounded(&mut Command::new(&executable)).status.success());
    }
}
