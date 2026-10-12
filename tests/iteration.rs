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
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
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
        !timed_out,
        "iteration fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

fn check(workspace: &Workspace, source: &str, accepted: bool) -> Vec<Output> {
    let mut outputs = Vec::new();
    let path = workspace.write("check.bh", source);
    for frontend in frontends() {
        let output = bounded(workspace.command().arg("check").arg(&path).args([
            "--frontend",
            frontend,
            "--json",
        ]));
        assert_eq!(
            output.status.success(),
            accepted,
            "{frontend}: {source}\n{}",
            describe(&output)
        );
        assert!(
            !stderr(&output).contains("panicked"),
            "{}",
            describe(&output)
        );
        if !accepted {
            assert!(stdout(&output).contains("error"), "{}", describe(&output));
        }
        outputs.push(output);
    }
    outputs
}
fn rejected_with_codes(workspace: &Workspace, source: &str, rust_code: &str, bhasa_code: &str) {
    for (frontend, output) in frontends().iter().zip(check(workspace, source, false)) {
        let code = if *frontend == "rust" {
            rust_code
        } else {
            bhasa_code
        };
        assert!(
            stdout(&output).contains(code),
            "{frontend}: {source}\n{}",
            describe(&output)
        );
    }
}

const OWNERSHIP: &str = r#"
struct Counter {
    trace:int
    def mark(self,value:int)->int { self.trace=self.trace*10+value; return value }
}
struct Item { number:int; text:buffer; nested:List[buffer]; fraction:float }
enum Choice { value(item:Item); none }
struct Holder {
    values:List[Item]; calls:int
    def snapshot(self)->List[Item] { self.calls=self.calls+1; return self.values }
}
def items(start:int)->List[Item] {
    var result=List[Item]()
    for i in range(start,start+4) {
        var nested=List[buffer](); nested.push(buffer("inside"))
        result.push(Item(i,buffer("original"),nested,0.25))
    }
    return result
}
def select(groups:List[List[Item]])->buffer {
    for group in groups {
        var outer=buffer("outer scope")
        for item in group {
            var inner=List[buffer](); inner.push(outer)
            match Choice.value(item) {
                case Choice.value(value) {
                    var arm=buffer("match scope")
                    if value.number==1 { var branch=buffer("if scope"); return value.text }
                }
                case Choice.none {}
            }
        }
    }
    return buffer("missing")
}
def ranges()->bool {
    var counter=Counter(0); var sum=0; var count=0
    for i in range(counter.mark(2),counter.mark(5)) { sum=sum+i; count=count+1; i=999 }
    if counter.trace!=25 or sum!=9 or count!=3 { return false }
    counter.trace=0
    for i in range(counter.mark(5),counter.mark(2)) { return false }
    if counter.trace!=52 { return false }
    sum=0
    for i in (range(4)) { sum=sum+i }
    if sum!=6 { return false }
    sum=0
    for i in range(-3,0) { sum=sum+i }
    if sum!=-6 { return false }
    for i in range(-1) { return false }
    for i in range(0) { return false }
    for i in range(7,7) { return false }
    count=0
    for i in range(-9223372036854775808,-9223372036854775806) { count=count+1 }
    for i in range(9223372036854775805,9223372036854775807) { count=count+1 }
    if count!=4 { return false }
    for i in range(9223372036854775807,9223372036854775807) { return false }
    for i in range(-9223372036854775808,-9223372036854775808) { return false }
    for i in range(9223372036854775807,-9223372036854775808) { return false }
    count=0
    for outer in range(3) {
        for inner in range(5) {
            if inner==1 { continue }
            if inner==3 { break }
            count=count+1
        }
    }
    if count!=6 { return false }
    var outer=0; count=0
    while outer<3 {
        outer=outer+1
        for inner in range(4) { if inner==1 { break }; count=count+1 }
        if outer==2 { continue }
        var inner=0
        while true { inner=inner+1; if inner==1 { continue }; break }
        count=count+inner
    }
    return count==7
}
def snapshots()->bool {
    var source=items(0); var holder=Holder(source,0); var seen=0
    for item in holder.snapshot() {
        if seen==0 { holder.values.clear(); holder.values=items(40) }
        if item.number!=seen or not same(item.text,"original") or item.nested.count()!=1 { return false }
        set(item.text,0,88); item.nested.clear()
        item=Item(99,buffer("rebound"),List[buffer](),2.5)
        seen=seen+1
    }
    if holder.calls!=1 or seen!=4 or holder.values.get(0).number!=40 { return false }
    for item in source { set(item.text,0,89); item.nested.clear() }
    if not same(source.get(0).text,"original") or source.get(0).nested.count()!=1 { return false }
    var fractions=List[float](); fractions.push(0.25); fractions.push(1.5); fractions.push(2.0)
    var sum=0.0
    for value in fractions { sum=sum+value; value=99.0 }
    if sum!=3.75 or fractions.get(0)!=0.25 { return false }
    var owned=List[buffer](); owned.push(buffer("one")); owned.push(buffer("two"))
    var item=buffer("outside")
    for item in owned { append(item," changed") }
    if not same(item,"outside") or not same(owned.get(1),"two") { return false }
    var count=0
    for value in source {
        var body=buffer("body scope")
        match Choice.value(value) {
            case Choice.value(payload) {
                var arm=List[buffer](); arm.push(payload.text)
                if payload.number==1 { var branch=buffer("continue scope"); continue }
                if payload.number==2 { var branch=buffer("break scope"); break }
            }
            case Choice.none {}
        }
        count=count+1
    }
    if count!=1 { return false }
    var iteration=0
    while true {
        var body=buffer("while body"); iteration=iteration+1
        match Choice.value(source.get(0)) {
            case Choice.value(payload) {
                if iteration<3 { var branch=buffer("while continue"); continue }
                var branch=buffer("while break"); break
            }
            case Choice.none { break }
        }
    }
    if iteration!=3 { return false }
    var choices=List[Choice](); choices.push(Choice.value(source.get(0))); choices.push(Choice.none())
    count=0
    for choice in choices {
        choices.clear()
        match choice {
            case Choice.value(value) { set(value.text,0,90); count=count+1; continue }
            case Choice.none { count=count+1; break }
        }
    }
    if count!=2 or not same(source.get(0).text,"original") { return false }
    var groups=List[List[Item]](); groups.push(source)
    var chosen=select(groups); source.clear(); groups.clear()
    if not same(chosen,"original") { return false }
    for value in List[buffer]() { return false }
    for value in items(10) { if value.number==11 { break } }
    return true
}
def main()->int {
    if not ranges() { return 1 }
    for round in range(40) { if not snapshots() { return 2 } }
    print("iteration ownership ok")
    return 0
}
"#;

#[test]
fn range_and_list_iteration_validate_ownership_and_cleanup_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("iteration-owned");
    let source = workspace.write("main.bh", OWNERSHIP);
    let tracker = workspace.write("tracker.c", r#"
#include <stdio.h>
#include <stdlib.h>
static long live;
static int registered;
static void check_live(void) { if (live) { fprintf(stderr,"live allocations: %ld\n",live); _Exit(99); } }
void *tracked_malloc(size_t n) { if (!registered) { registered=1; atexit(check_live); } void *p=malloc(n); if(p) ++live; return p; }
void *tracked_realloc(void *p,size_t n) { int empty=p==NULL; void *q=realloc(p,n); if(q && empty) ++live; return q; }
void tracked_free(void *p) { if(p) --live; free(p); }
"#);
    workspace.write("tracked.h", "#include <stdlib.h>\nvoid *tracked_malloc(size_t);\nvoid *tracked_realloc(void *,size_t);\nvoid tracked_free(void *);\n#define malloc tracked_malloc\n#define realloc tracked_realloc\n#define free tracked_free\n");
    let mut runtime = Vec::new();
    for unit in ["core", "collections"] {
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
                if line.starts_with("define ") {
                    format!("{} sanitize_address {{\n", line.strip_suffix(" {").unwrap())
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let ir = workspace.write("iteration.ll", &ir);
        for optimization in ["-O0", "-O2"] {
            let executable = workspace.file("iteration");
            let output = Command::new(&clang)
                .args([
                    optimization,
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
            assert_eq!(stdout(&output), "iteration ownership ok\n");
            assert!(
                output.stderr.is_empty(),
                "{frontend} {optimization}: {}",
                describe(&output)
            );
        }
    }
}

#[test]
fn loop_types_arity_scope_and_terminal_flow_are_checked() {
    let workspace = Workspace::new("iteration-diagnostics");
    for source in [
        "def main(){break}", "def main(){continue}",
        "def main(){for x in range() {}}", "def main(){for x in range(1,2,3,4) {}}",
        "def main(){for x in range(true) {}}", "def main(){for x in range(1.0) {}}",
        "def main(){for x in range(0,false) {}}", "def main(){for x in range(buffer()) {}}",
        "def main(){for x in 3 {}}", "def main(){for x in true {}}",
        "def main(){for x in 1.0 {}}", "def main(){for x in \"text\" {}}",
        "def main(){var r=range(3)}", "def main(){print(range(3))}",
        "private def range()->int{return 3} def main(){}",
        "def range[T](x:T)->T{return x} def main(){}",
        "def identity[range](x:range)->range{return x} def main(){}",
        "def identity[for](x:for)->for{return x} def main(){}",
        "def main(){for x in range(2) {} print(x)}",
        "def main(){for x in range(2) {var x=1}}",
        "def main(){for x in range(2) {x=true}}",
        "def main(){for x in range(2) {break; print(x)}}",
        "def main(){while true {continue; print(1)}}",
        "def main(){for x in range(2) {if true {break} else {continue}; print(x)}}",
        "enum E { one; two } def main(){for x in range(2) {match E.one() {case E.one {break} case E.two {continue}} print(x)}}",
        "def result()->int {for x in range(1) {return x}} def main(){}",
        "def result()->int {while true {break}} def main(){}",
    ] { check(&workspace,source,false); }
    for source in [
        "def main(){var x=9;for x in range(2) {x=x+1} print(x)}",
        "def result()->int {for x in range(2) {if x==0 {continue} else {return x}} return 3} def main(){print(result())}",
        "def main(){while true {for x in range(2) {break}; break}}",
        "def main(){for x in List[int]() {continue}}",
    ] { check(&workspace,source,true); }
}

#[test]
fn loop_exits_enforce_only_the_task_scopes_they_leave() {
    let workspace = Workspace::new("iteration-tasks");
    let prefix = "enum E { one } def worker(n:int)->int{return n} def main(){";
    for operation in ["spawn", "async_spawn"] {
        for body in [
            "for i in range(2) {var task=OP(worker(i)); break}",
            "for i in range(2) {var task=OP(worker(i)); continue}",
            "for i in range(2) {var task=OP(worker(i)); if true {break}; join(task)}",
            "for i in range(2) {var task=OP(worker(i)); if true {continue}; join(task)}",
            "for i in range(2) {if true {var task=OP(worker(i)); break}}",
            "for i in range(2) {if true {var task=OP(worker(i)); continue}}",
            "for i in range(2) {var task=OP(worker(i)); if true {join(task)}}",
            "for i in range(2) {var task=OP(worker(i)); match E.one() {case E.one {break}}}",
            "for i in range(2) {match E.one() {case E.one {var task=OP(worker(i)); continue}}}",
            "var task=OP(worker(1)); for i in range(2) {join(task)}",
            "var task=OP(worker(1)); for i in range(2) {return}; join(task)",
            "var task=OP(worker(1)); for task in range(2) {}; join(task)",
        ] {
            rejected_with_codes(
                &workspace,
                &format!("{prefix}{} }}", body.replace("OP", operation)),
                "S019",
                "BH208",
            );
        }
        for body in [
            "var task=OP(worker(1)); for i in range(2) {if i==0 {continue}; break}; join(task)",
            "for i in range(2) {var task=OP(worker(i)); join(task); if i==0 {continue}; break}",
            "for i in range(2) {if i==0 {var task=OP(worker(i)); await(task); continue}; break}",
            "for i in range(2) {var task=OP(worker(i)); for j in range(2) {if j==0 {continue}; break}; join(task)}",
            "for i in range(2) {var task=OP(worker(i)); while true {break}; join(task)}",
            "for i in range(2) {match E.one() {case E.one {var task=OP(worker(i)); join(task); continue}}}",
        ] { check(&workspace,&format!("{prefix}{} }}",body.replace("OP",operation)),true); }
    }
}

#[test]
fn worker_safety_includes_bounds_sources_and_loop_bodies() {
    let workspace = Workspace::new("iteration-workers");
    for helper in [
        "extern def foreign(n:int)->int\ndef worker(n:int)->int{for x in range(foreign(n)){break};return n}",
        "extern def foreign(n:int)->int\ndef worker(n:int)->int{for x in range(n,foreign(n)){break};return n}",
        "extern def foreign(n:int)->int\ndef source(n:int)->List[int]{var v=List[int]();v.push(foreign(n));return v} def worker(n:int)->int{for x in source(n){break};return n}",
        "extern def foreign(n:int)->int\ndef worker(n:int)->int{for x in range(n){var value=foreign(x)};return n}",
        "extern def foreign(n:int)->int\nstruct Worker {def run(self,n:int)->int{for x in range(n){return foreign(x)};return n}} def worker(n:int)->int{return Worker().run(n)}",
        "extern def foreign(n:int)->int\ndef generic[T](n:T)->int{for x in range(1){return foreign(1)};return 0} def worker(n:int)->int{return generic[int](n)}",
        "def pure(n:int)->int{return n} def source(n:int)->int{var task=async_spawn(pure(n));var result=join(task);return result} def worker(n:int)->int{for x in range(source(n)){break};return n}",
        "def pure(n:int)->int{return n} def worker(n:int)->int{for x in range(n){var task=spawn(pure(x));var result=await(task)};return n}",
    ] {
        for operation in ["spawn","async_spawn"] {
            rejected_with_codes(&workspace,&format!("{helper} def main(){{var task={operation}(worker(2));var result=join(task)}}"),"S020","BH209");
        }
    }
    check(&workspace,"def worker(n:int)->int{var values=List[int]();values.push(n);var sum=0;for value in values {sum=sum+value};return sum} def main(){var task=async_spawn(worker(2));var result=await(task)}",true);
}

#[test]
fn aliased_generic_loop_sources_and_owned_returns_work_in_both_frontends() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("iteration-imports");
    workspace.write(
        "library.bh",
        r#"
struct Box[T] { value:T }
struct Secret { private value:int; private def hidden(self)->int{return self.value} }
def secrets()->List[Secret]{var values=List[Secret]();values.push(Secret(42));return values}
def singleton[T](value:T)->List[T]{var values=List[T]();values.push(value);return values}
def first[T](values:List[T],fallback:T)->T {for value in values {return value};return fallback}
"#,
    );
    for body in [
        "for item in lib.secrets() {print(item.value)}",
        "for item in lib.secrets() {print(item.hidden())}",
        "for i in range(lib.secrets().get(0).hidden()) {}",
    ] {
        rejected_with_codes(
            &workspace,
            &format!("import \"library.bh\" as lib\ndef main(){{{body}}}"),
            "M005",
            "M005",
        );
    }
    workspace.write(
        "main.bh",
        r#"
import "library.bh" as lib
def main()->int {
    var source=lib.singleton[lib.Box[buffer]](lib.Box[buffer](buffer("hello")))
    for item in source {source.clear();append(item.value," loop");print(item.value)}
    var nested=List[List[buffer]]();nested.push(lib.singleton[buffer](buffer("nested")))
    for inner in nested {nested.clear();for value in inner {inner.clear();print(value)}}
    var result=lib.first[buffer](lib.singleton[buffer](buffer("owned")),buffer("fallback"))
    print(result)
    for lib in lib.singleton[int](42) { print(lib) }
    return 0
}
"#,
    );
    for frontend in frontends() {
        for release in [false, true] {
            let mut command = workspace.command();
            command.args(["run", "main.bh", "--frontend", frontend]);
            if release {
                command.arg("--release");
            }
            let output = bounded(&mut command);
            succeeds(&output);
            assert_eq!(stdout(&output), "hello loop\nnested\nowned\n42\n");
        }
    }
}
