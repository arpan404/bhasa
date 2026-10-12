mod common;
use common::*;
use std::{fs, path::Path, process::Command};

const PROGRAM: &str = r#"
struct Counter { value:int }
struct Item { text:buffer }
enum Choice { stop; go }
def bound(counter:Counter, expected:int, result:int)->int {
    if counter.value!=expected { return 12345 }
    counter.value=counter.value+1
    return result
}
def worker(value:int)->int { var n=0; for i in range(value) {n=n+i}; return n }
def first(values:List[Item])->buffer {
    for value in values { for i in range(2) { return value.text } }
    return buffer("empty")
}
def main()->int {
    var calls=Counter(0)
    var sum=0
    for i in range(bound(calls,0,-2),bound(calls,1,3)) { sum=sum+i; i=99 }
    if sum!=0 or calls.value!=2 { return 1 }
    for i in ((range(0,))) {return 2}
    for i in range(-3) {return 3}
    for i in range(4,2) {return 4}
    var count=0
    for i in range(-9223372036854775808,-9223372036854775807) {count=count+1}
    for i in range(9223372036854775806,9223372036854775807) {count=count+1}
    if count!=2 {return 5}
    var i=77
    for i in range(3) { var own=buffer("drop"); if i==1 {continue}; count=count+1 }
    if count!=4 or i!=77 {return 6}
    for i in range(3) { break }
    var values=List[Item]()
    values.push(Item(buffer("one")));values.push(Item(buffer("two")));values.push(Item(buffer("three")))
    var saved=first(values)
    count=0
    for value in values {
        values.clear()
        values=List[Item]()
        count=count+1
        if count==1 {value=Item(buffer("changed"));continue}
        if count==2 and not same(value.text,"two") {return 7}
        if count==3 and not same(value.text,"three") {return 8}
    }
    if count!=3 or not same(saved,"one") {return 9}
    values.push(Item(buffer("saved")))
    for value in values { var body=buffer("cleanup"); break }
    if not same(values.get(0).text,"saved") {return 10}
    count=0
    while true {
        var cleanup=buffer("outer")
        for j in range(4) {if j==1 {continue}; if j==2 {break};count=count+1}
        if count==1 {continue}
        break
    }
    if count!=2 {return 11}
    var choice=Choice.stop()
    for n in range(2) {
        match choice {case Choice.stop {break} case Choice.go {continue}}
    }
    for n in range(2) { if n==0 {continue} else {break} }
    var outer=spawn(worker(4))
    for n in range(2) {break}
    var result=join(outer)
    if result!=6 {return 12}
    for n in range(2) {var task=spawn(worker(n));var joined=join(task)}
    print("iteration ownership ok")
    return 0
}
"#;

const INVALID: &[&str] = &[
    "def main(){break}", "def main(){continue}",
    "def main(){while true {break;print(1)}}",
    "def main(){for i in range(2){continue;print(i)}}",
    "def main(){for i in range(2){if true {break}else{continue};print(i)}}",
    "enum E{a;b} def main(){for i in range(2){match E.a(){case E.a{break}case E.b{continue}};print(i)}}",
    "def main()->int{for i in range(1){return 1}}",
    "def main(){for i in range(){}}", "def main(){for i in range(1,2,3){}}",
    "def main(){for i in range(true){}}", "def main(){for i in range(1,2.0){}}",
    "def main(){for i in 3{}}", "def main(){for i in buffer(){}}",
    "def main(){for i in Map[str,int](){}}", "def main(){for i in ints(){}}",
    "def main(){for i in range(1){};print(i)}", "def main(){var range=2}",
    "def range(n:int)->int{return n} def main(){}", "def main(){range(3)}",
    "def main(){for i in range(1){var i=2}}",
    "def worker(v:int)->int{return v} def main(){while true{var t=spawn(worker(0));break}}",
    "def worker(v:int)->int{return v} def main(){for i in range(2){var t=spawn(worker(0));continue}}",
    "def worker(v:int)->int{return v} def main(){var t=spawn(worker(0));for i in range(1){var r=join(t)}}",
    "extern def unsafe(v:int)->int def bound(v:int)->int{return unsafe(v)} def worker(v:int)->int{for i in range(bound(v)){};return 0} def main(){var t=spawn(worker(0));var r=join(t)}",
    "def inner(v:int)->int{return v} def bound(v:int)->int{var t=async_spawn(inner(v));var r=await(t);return r} def worker(v:int)->int{for i in range(bound(v)){};return 0} def main(){var t=spawn(worker(0));var r=join(t)}",
];

fn seed_source() -> String {
    ["lexer.bh", "emitter.bh", "driver.bh"]
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .join("\n")
}
fn emit(seed: &Path, workspace: &Workspace, source: &str) -> std::process::Output {
    let source = workspace.write("input.bh", source);
    Command::new(seed)
        .args([
            "emit-llvm",
            as_str(&source),
            "--target",
            &bhasa::target::Target::Native.triple_for_ir().unwrap(),
        ])
        .output()
        .unwrap()
}
fn link(
    workspace: &Workspace,
    clang: &std::ffi::OsStr,
    ir: &str,
    name: &str,
    optimization: &str,
    sanitize: bool,
) -> std::path::PathBuf {
    let source = workspace.write(&format!("{name}.ll"), ir);
    let executable = workspace.file(name);
    let mut command = Command::new(clang);
    command
        .arg(optimization)
        .arg(source)
        .arg(fixture("runtime/core.c"))
        .arg(fixture("runtime/collections.c"))
        .arg(fixture("runtime/concurrency.c"))
        .arg("-pthread")
        .arg("-o")
        .arg(&executable);
    if sanitize {
        command
            .arg("-fsanitize=address,undefined")
            .arg("-fno-omit-frame-pointer");
    }
    succeeds(&command.output().unwrap());
    executable
}
#[test]
fn fresh_bhasa_seed_matches_iteration_cleanup_rejection_and_self_rebuild() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("iteration-native");
    let seed_text = seed_source();
    let source = workspace.write("seed.bh", &seed_text);
    let built = build(&workspace, &source, true);
    let seed = workspace.file("stage-a");
    fs::rename(built, &seed).unwrap();
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let reference = bhasa::compile_to_llvm(PROGRAM, &target).unwrap();
    let expanded = bhasa::generics::expand(PROGRAM, &[]).unwrap();
    let emitted = emit(&seed, &workspace, &expanded.source);
    succeeds(&emitted);
    for (frontend, ir) in [("reference", reference), ("bhasa", stdout(&emitted))] {
        for optimization in ["-O0", "-O2"] {
            let executable = link(&workspace, &clang, &ir, frontend, optimization, true);
            let output = execute(&executable, &workspace);
            succeeds(&output);
            assert_eq!(stdout(&output), "iteration ownership ok\n");
        }
    }
    for source in INVALID {
        assert!(bhasa::check(source).is_err(), "reference accepted {source}");
        if let Ok(expanded) = bhasa::generics::expand(source, &[]) {
            let result = emit(&seed, &workspace, &expanded.source);
            assert!(
                !result.status.success(),
                "seed accepted {source}: {}",
                describe(&result)
            );
            assert!(result.stdout.is_empty());
        }
    }
    // The fresh compiler rebuilds its complete scanner/parser/checker/emitter.
    let compiler_ir = emit(&seed, &workspace, &seed_text);
    succeeds(&compiler_ir);
    let stage_b = link(
        &workspace,
        &clang,
        &stdout(&compiler_ir),
        "stage-b",
        "-O2",
        false,
    );
    let stage_b_ir = emit(&stage_b, &workspace, &seed_text);
    succeeds(&stage_b_ir);
    assert_eq!(stdout(&compiler_ir), stdout(&stage_b_ir));
    let replay = emit(&stage_b, &workspace, &expanded.source);
    succeeds(&replay);
    assert_eq!(stdout(&emitted), stdout(&replay));
}

#[cfg(feature = "bhasa-frontend")]
#[test]
fn hybrid_keyword_and_iteration_frontend_parity() {
    let keywords = "for in break continue float range";
    assert_eq!(
        bhasa::lexer::lex(keywords),
        bhasa::lexer::lex_rust(keywords)
    );
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("iteration-hybrid");
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let ir = bhasa::compile_with_bhasa(PROGRAM, &target).unwrap();
    let executable = link(&workspace, &clang, &ir, "hybrid", "-O2", true);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "iteration ownership ok\n");
    for source in INVALID {
        assert!(
            bhasa::compile_with_bhasa(source, &target).is_err(),
            "{source}"
        );
    }
}
