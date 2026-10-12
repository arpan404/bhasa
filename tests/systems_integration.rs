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
        "systems fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

const LIBRARY: &str = r#"
struct Plan[T] { step:T; def amount(self)->T { return self.step } }
def keep[T](value:T)->T { return value }
def mark(value:buffer, label:str, number:int)->int { append(value,label); return number }
def process(value:buffer)->buffer {
    var plan=Plan[int](2)
    for item in range(mark(value,"A",0),mark(value,"B",5),mark(value,"C",keep[int](plan.amount()))) {
        var scratch=List[buffer](); scratch.push(buffer("temporary"))
        if item==2 { continue }
        append(value,to_buffer(item ^ 0x1))
    }
    return value
}
def ranges[T](value:T)->T {
    for item in range(3,0,-0b1) { var scratch=buffer("cleanup"); if item==2 { continue } }
    return value
}
"#;

const PROGRAM: &str = r#"
import "library.bh" as lib
def check()->bool {
    var source=buffer()
    var job=async_spawn(lib.process(source))
    append(source,"caller")
    var answer=await(job)
    if not same(source,"caller") or not same(answer,"ABC15") { return false }
    var items=List[buffer]();items.push(buffer("saved"))
    for entry in items {
        for index in range(3,-1,-1) {
            var task=spawn(lib.keep[buffer](entry));var result=join(task)
            if not same(result,"saved") { return false }
            if index==3 { continue }
            if index==2 { break }
        }
    }
    var count=0
    for number in range(0x7ffffffffffffffe,0x7fffffffffffffff,0b10) { count=count+1;number=0 }
    for number in range(-0x7fffffffffffffff,-0x8000000000000000,-2) { count=count+1;number=0 }
    if count!=2 or (1 << 63)!=-0x8000000000000000 or (-8 >> 2)!=-2 { return false }
    if (0xA & 0B11 | 1 ^ 2)!=3 or ~(0b11)!=-4 or 1 << 2+1!=8 { return false }
    var returned=lib.ranges[buffer](buffer("return"))
    return same(returned,"return")
}
def main()->int {
    for round in range(20,0,-1) { if not check() { return 10 } }
    print("integer/range/task integration ok")
    return 0
}
"#;

#[test]
fn imported_generic_steps_and_owned_task_loops_release_allocations_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("systems-owned-integration");
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
            assert_eq!(stdout(&output), "integer/range/task integration ok\n");
            assert!(output.stderr.is_empty(), "{}", describe(&output));
        }
    }
}

#[test]
fn step_calls_obey_worker_graph_privacy_and_original_file_diagnostics() {
    let workspace = Workspace::new("systems-step-safety");
    for library in [
        "extern def foreign(value:int)->int\ndef step()->int{return foreign(1)}\ndef worker(value:buffer)->buffer{for n in range(0,3,step()){append(value,\"x\")};return value}",
        "def identity(value:int)->int{return value}\ndef step()->int{var task=async_spawn(identity(1));var result=await(task);return result}\ndef worker(value:buffer)->buffer{for n in range(0,3,step()){append(value,\"x\")};return value}",
    ] {
        workspace.write("library.bh",library);
        let main=workspace.write("main.bh","import \"library.bh\" as lib\ndef main(){var task=spawn(lib.worker(buffer()));var result=join(task)}");
        for frontend in frontends() {
            let output=bounded(workspace.command().arg("check").arg(&main).args(["--frontend",frontend,"--json"]));
            fails(&output);
            assert!(stdout(&output).contains(if frontend=="rust" {"S020"} else {"BH209"}),"{}",describe(&output));
        }
    }
    workspace.write(
        "library.bh",
        "struct Plan[T]{private step:T}\ndef plan()->Plan[int]{return Plan[int](1)}",
    );
    let main=workspace.write("main.bh","import \"library.bh\" as lib\ndef main(){var plan=lib.plan();for n in range(0,3,plan.step){}}");
    for frontend in frontends() {
        let output = bounded(workspace.command().arg("check").arg(&main).args([
            "--frontend",
            frontend,
            "--json",
        ]));
        fails(&output);
        assert!(stdout(&output).contains("private"), "{}", describe(&output));
    }
    let library = "def wrong[T](value:T)->int{\n for n in range(0,3,value){}\n return 0\n}\n";
    workspace.write("library.bh", library);
    let main = workspace.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){print(lib.wrong[bool](true))}",
    );
    for frontend in frontends() {
        let output = bounded(workspace.command().arg("check").arg(&main).args([
            "--frontend",
            frontend,
            "--json",
        ]));
        fails(&output);
        let json = bhasa::json::parse(&stdout(&output)).unwrap();
        let diagnostics = json.as_array().unwrap();
        assert!(!diagnostics.is_empty());
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic
                    .get("file")
                    .and_then(|value| value.as_str())
                    .is_some_and(|file| file.ends_with("library.bh"))
            })
            .unwrap();
        let start = diagnostic
            .get("span")
            .unwrap()
            .get("start")
            .unwrap()
            .as_i64()
            .unwrap() as usize;
        let end = diagnostic
            .get("span")
            .unwrap()
            .get("end")
            .unwrap()
            .as_i64()
            .unwrap() as usize;
        // The reference checker points to the offending expression; the native
        // checker reports at the token reached after consuming that expression.
        // Both must retain their exact original-library location after lowering.
        let expected = library.rfind("value").unwrap();
        let (expected, token) = if frontend == "rust" {
            (expected, "value")
        } else {
            (expected + "value".len(), ")")
        };
        assert_eq!(start, expected, "{}", describe(&output));
        assert_eq!(
            library.get(start..end),
            Some(token),
            "{}",
            describe(&output)
        );
        assert_eq!(diagnostic.get("line").unwrap().as_i64(), Some(2));
        assert_eq!(
            diagnostic.get("code").unwrap().as_str(),
            Some(if frontend == "rust" { "S010" } else { "BH201" })
        );
    }
}

#[test]
fn formatting_retains_radix_spellings_shift_precedence_and_step_arguments() {
    let workspace = Workspace::new("systems-format");
    let original="def main(){var mask=~(0Xf|0B1);print(mask & 0xff != 0);for n in range(4,0,-2){print(n<<1+1)}}\n";
    let source = workspace.write("main.bh", original);
    succeeds(&workspace.run(&["fmt", as_str(&source)]));
    let formatted = std::fs::read_to_string(&source).unwrap();
    assert!(
        formatted.contains("~(0Xf | 0B1)") && formatted.contains("n << 1 + 1"),
        "{formatted}"
    );
    succeeds(&workspace.run(&["fmt", as_str(&source), "--check"]));
    for frontend in frontends() {
        let output = bounded(
            workspace
                .command()
                .arg("run")
                .arg(&source)
                .args(["--frontend", frontend]),
        );
        succeeds(&output);
        assert_eq!(stdout(&output), "true\n16\n8\n");
    }
}
