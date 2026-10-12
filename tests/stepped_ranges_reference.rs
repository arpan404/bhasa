mod common;
use bhasa::ast::{ExprKind, ForSource, StmtKind, Type};
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
            let output = child.wait_with_output().unwrap();
            panic!("stepped range exceeded deadline: {}", describe(&output));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn optional_steps_parse_and_all_expression_types_are_checked() {
    for (source, explicit) in [
        ("range(4)", false),
        ("range(1,4,)", false),
        ("(range(4,1,-1,))", true),
    ] {
        let checked = bhasa::check(&format!(
            "def main() {{ for i in {source} {{ print(i) }} }}"
        ))
        .unwrap();
        let StmtKind::For {
            source: ForSource::Range { start, end, step },
            element_type,
            ..
        } = &checked.program.functions[0].body.as_ref().unwrap()[0].kind
        else {
            panic!()
        };
        assert_eq!(step.is_some(), explicit);
        assert_eq!(*element_type, Some(Type::Int));
        assert_eq!(start.ty, Some(Type::Int));
        assert_eq!(end.ty, Some(Type::Int));
        if let Some(step) = step {
            assert_eq!(step.ty, Some(Type::Int));
        }
    }
    for range in [
        "range()",
        "range(1,2,3,4)",
        "range(1,2,true)",
        "range(1,2,1.0)",
        "range(1,2,buffer())",
        "range(1,2,print(1))",
    ] {
        assert!(
            bhasa::check(&format!("def main() {{ for i in {range} {{}} }}")).is_err(),
            "{range}"
        );
    }
    // Zero step is a runtime trap, including a statically empty range.
    bhasa::check("def main() { for i in range(1,1,0) {} }").unwrap();
    let mut checked = bhasa::check("def main() { for i in range(1,4,2) {} }").unwrap();
    let StmtKind::For {
        source: ForSource::Range {
            step: Some(step), ..
        },
        ..
    } = &mut checked.program.functions[0].body.as_mut().unwrap()[0].kind
    else {
        panic!()
    };
    step.ty = Some(Type::Bool);
    assert!(bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err());
}

#[test]
fn steps_participate_in_worker_graph_and_lexical_task_checks() {
    for definitions in [
        "extern def foreign(value:int)->int\ndef step()->int {return foreign(1)} def worker(value:int)->int {for i in range(0,3,step()) {} return value}",
        "extern def foreign(value:int)->int\nstruct Step { def value(self)->int {return foreign(1)} } def worker(value:int)->int {for i in range(0,3,Step().value()) {} return value}",
        "extern def foreign(value:int)->int\ndef step[T](value:T)->int {return foreign(1)} def worker(value:int)->int {for i in range(0,3,step[int](1)) {} return value}",
        "def pure(value:int)->int{return value} def step()->int{var task=async_spawn(pure(1));var result=join(task);return result} def worker(value:int)->int{for i in range(0,3,step()) {} return value}",
    ] {
        for submit in ["spawn", "async_spawn"] {
            let source = format!("{definitions} def main() {{var task={submit}(worker(1));join(task)}}");
            let errors = bhasa::check(&source).unwrap_err();
            assert!(errors.iter().any(|error| error.code == "S020"), "{source}: {errors:?}");
        }
    }
    for body in [
        "var task=spawn(worker(i));continue",
        "var task=async_spawn(worker(i));break",
        "var task=spawn(worker(i));return",
        "var task=spawn(worker(i));if true {join(task)}",
    ] {
        let source=format!("def worker(value:int)->buffer{{return buffer()}} def main(){{for i in range(3,0,-1) {{{body}}}}}");
        assert!(bhasa::check(&source).is_err(), "{source}");
    }
    bhasa::check("def worker(value:int)->buffer{return buffer()} def main(){for i in range(3,0,-1) {var task=async_spawn(worker(i));var result=join(task);if i==2 {continue};break}} ").unwrap();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_ranges_match_signed_reference_and_zero_step_traps_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("stepped-range-boundaries");
    let mut source = String::from("def main() {\n");
    let mut expected = String::new();
    for (index, (start, end, step)) in [
        (0, 10, 3),
        (5, -5, -3),
        (-5, 5, 4),
        (6, 1, 1),
        (1, 6, -1),
        (i64::MIN, i64::MIN + 4, 2),
        (i64::MAX - 2, i64::MAX, 2),
        (i64::MAX - 1, i64::MAX, i64::MAX),
        (i64::MIN + 1, i64::MIN, i64::MIN),
        (i64::MAX, i64::MIN, i64::MIN),
        (i64::MIN, i64::MAX, i64::MAX),
        (0, 0, i64::MIN),
        (i64::MAX, i64::MAX, 1),
    ]
    .into_iter()
    .enumerate()
    {
        source.push_str(&format!(
            "print(\"range {index}\"); for i in range({start},{end},{step}) {{print(i)}}\n"
        ));
        expected.push_str(&format!("range {index}\n"));
        let mut cursor = start;
        while if step > 0 { cursor < end } else { cursor > end } {
            expected.push_str(&format!("{cursor}\n"));
            let Some(next) = cursor.checked_add(step) else {
                break;
            };
            cursor = next;
        }
    }
    source.push_str("}\n");
    let triple = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let ir = bhasa::compile_to_llvm(&source, &triple).unwrap();
    for optimization in ["-O0", "-O2"] {
        let file = workspace.write("ranges.ll", &ir);
        let executable = workspace.file("ranges");
        succeeds(
            &Command::new(&clang)
                .arg(optimization)
                .arg(&file)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(&mut Command::new(&executable));
        succeeds(&output);
        assert_eq!(stdout(&output), expected);
        for arguments in [
            "0,0,0",
            "5,2,0",
            "0,2,0",
            "-9223372036854775808,9223372036854775807,0",
        ] {
            let source = format!(
                "def main() {{for i in range({arguments}) {{print(i)}} print(\"missed trap\")}}"
            );
            let ir = bhasa::compile_to_llvm(&source, &triple).unwrap();
            let file = workspace.write("zero.ll", &ir);
            succeeds(
                &Command::new(&clang)
                    .arg(optimization)
                    .arg(file)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap(),
            );
            assert!(
                !bounded(&mut Command::new(&executable)).status.success(),
                "zero step accepted for {arguments} {optimization}"
            );
        }
    }
}

const OWNERSHIP: &str = r#"
struct Counter { trace:int; def mark(self,label:int,value:int)->int{self.trace=self.trace*10+label;return value} }
struct Payload { text:buffer; values:List[buffer] }
enum Choice { value(payload:Payload); empty }
def worker(value:Payload)->Payload{append(value.text," worker");return value}
def early()->buffer {
    for i in range(3,-1,-1) {
        var text=buffer("early")
        var task=spawn(worker(Payload(text,List[buffer]())));var result=join(task)
        if i==2 {return result.text}
    }
    return buffer("missing")
}
def exercise()->bool {
    var counter=Counter(0);var sum=0;var count=0
    for i in (range(counter.mark(1,6),counter.mark(2,-1),counter.mark(3,-2),)) {
        sum=sum+i;count=count+1;i=999
        var values=List[buffer]();values.push(buffer("original"))
        var task=async_spawn(worker(Payload(buffer("owned"),values)));var answer=await(task)
        if not same(answer.text,"owned worker") {return false}
        match Choice.value(answer) {
            case Choice.value(payload) {var local=buffer("arm");if count<4 {continue};break}
            case Choice.empty {return false}
        }
    }
    if counter.trace!=123 or sum!=12 or count!=4 {return false}
    counter.trace=0
    for i in range(counter.mark(1,0),counter.mark(2,0),counter.mark(3,-1)) {return false}
    if counter.trace!=123 {return false}
    var start=1;var end=7;var step=2;sum=0
    for i in range(start,end,step){sum=sum+i;start=99;end=0;step=-1}
    if sum!=9 {return false}
    var iterations=0
    for outer in range(5,-1,-2) {
        var outer_value=buffer("outer")
        for inner in range(-2,3,2) {
            var inner_value=List[buffer]();inner_value.push(outer_value)
            if inner==0 {continue}
            iterations=iterations+1
        }
        if outer==3 {break}
    }
    if iterations!=4 {return false}
    if not same(early(),"early worker") {return false}
    return true
}
def main()->int {for round in range(8,0,-1){if not exercise(){return 1}} print("stepped range ownership ok");return 0}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn stepped_control_flow_preserves_owned_values_and_tasks_under_sanitizers() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("stepped-range-ownership");
    let tracker = workspace.write("tracker.c", r#"
#include <stdatomic.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic long live;
static pthread_once_t registered=PTHREAD_ONCE_INIT;
static void check_live(void){long count=atomic_load(&live);if(count){fprintf(stderr,"live allocations: %ld\n",count);_Exit(99);}}
static void register_check(void){atexit(check_live);}
void *tracked_malloc(size_t size){pthread_once(&registered,register_check);void *value=malloc(size);if(value)atomic_fetch_add(&live,1);return value;}
void *tracked_realloc(void *value,size_t size){int empty=value==NULL;void *result=realloc(value,size);if(result&&empty)atomic_fetch_add(&live,1);return result;}
void tracked_free(void *value){if(value)atomic_fetch_sub(&live,1);free(value);}
"#);
    workspace.write("tracked.h", "#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
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
    let file = workspace.write("ownership.ll", &ir);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("ownership");
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
        assert_eq!(stdout(&output), "stepped range ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

#[test]
fn step_spans_and_nested_types_survive_program_merge() {
    let source="struct Step {value:int;def amount(self)->int{return self.value}} def main(){for i in range(0,3,Step(1).amount()) {}}";
    let checked = bhasa::check(source).unwrap();
    let StmtKind::For {
        source: ForSource::Range {
            step: Some(step), ..
        },
        ..
    } = &checked.program.functions[0].body.as_ref().unwrap()[0].kind
    else {
        panic!()
    };
    assert_eq!(&source[step.span.start..step.span.end], "Step(1).amount()");
    assert_eq!(step.ty, Some(Type::Int));
    assert!(matches!(step.kind, ExprKind::Call { .. }));
    let mut combined = bhasa::ast::Program::default();
    combined.intern_type_name("EarlierModule");
    combined.merge(checked.program);
    let StmtKind::For {
        source: ForSource::Range {
            step: Some(step), ..
        },
        ..
    } = &combined.functions[0].body.as_ref().unwrap()[0].kind
    else {
        panic!()
    };
    assert_eq!(step.ty, Some(Type::Int));
    let ExprKind::Call { args, .. } = &step.kind else {
        panic!()
    };
    assert_eq!(args[0].ty, Some(Type::Record(1)));
}
