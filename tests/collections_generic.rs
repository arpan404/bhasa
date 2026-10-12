mod common;
use common::*;
use std::{fs, path::Path, process::Command};

const PROGRAM: &str = r#"
struct Item { bytes:buffer; number:int }
enum Value { none; item(value:Item) }
def count_borrowed(values:List[int])->int { return values.count() }
def worker(value:int)->int { var values=List[int](); values.push(value); return values.get(0) }
def main()->int {
    var values=List[int]()
    var i=0
    while i<1000 { values.push(i); i=i+1 }
    if count_borrowed(values)!=1000 or values.get(999)!=999 { return 1 }
    values.set(0,42)
    var copy=values
    values.clear()
    if values.count()!=0 or copy.get(0)!=42 or copy.count()!=1000 { return 2 }
    var source=Item(buffer("old"),7)
    var owned=List[Item]()
    owned.push(source)
    set(source.bytes,0,88)
    var out=owned.get(0)
    owned.set(0,Item(buffer("new"),8))
    owned.clear()
    if not same(out.bytes,"old") or out.number!=7 { return 3 }
    var nested=List[List[int]]()
    nested.push(copy)
    copy.clear()
    if nested.get(0).get(0)!=42 { return 4 }
    var enums=List[Value]()
    enums.push(Value.item(out))
    match enums.get(0) {
        case Value.none { return 5 }
        case Value.item(snapshot) { if not same(snapshot.bytes,"old") { return 6 } }
    }
    var dictionary=Map[str,Item]()
    dictionary.put("key",out)
    dictionary.put("key",Item(buffer("replacement"),9))
    var saved=dictionary.get("key")
    var duplicate=dictionary
    if dictionary.count()!=1 or not dictionary.contains("key") { return 7 }
    if not dictionary.remove("key") or dictionary.remove("key") { return 8 }
    if dictionary.contains("key") or dictionary.count()!=0 { return 9 }
    duplicate.clear()
    if not same(saved.bytes,"replacement") { return 10 }
    var integers=Map[int,List[int]]()
    i=0
    while i<1000 { integers.put(i,nested.get(0)); i=i+1 }
    if integers.count()!=1000 or integers.get(999).get(0)!=42 { return 11 }
    integers.clear()
    var floats=List[float](); floats.push(0.25); floats.set(0,1.5)
    var bools=Map[str,bool](); bools.put("yes",true)
    if floats.get(0)!=1.5 or not bools.get("yes") { return 12 }
    var text=List[str](); text.push("static")
    if List[int]().count()!=0 or text.count()!=1 { return 13 }
    var task=spawn(worker(23)); var result=join(task)
    if result!=23 { return 14 }
    print("typed collections ok")
    return 0
}
"#;

const INVALID: &[&str] = &[
    "def main(){var x=List[int](1)}",
    "def main(){var x=Map[str,int](1,2)}",
    "def main(){var x=List[int](); x.push(true)}",
    "def main(){var x=List[int](); x.set(0,true)}",
    "def main(){var x=List[int](); print(x.get(false))}",
    "def main(){var x=List[int](); print(x.__element)}",
    "def main(){var x=List[int](); x.__element=3}",
    "def main(){var x=Map[str,int](); print(x.__key)}",
    "def main(){var x=Map[str,int](); x.put(2,3)}",
    "def main(){var x=Map[str,int](); x.put(\"key\",false)}",
    "def main(){var x=Map[buffer,int]()}" ,
    "def main(){var x=Map[float,int]()}" ,
    "def main(){var x=List[task]()}" ,
    "def main(){var x=List[void]()}" ,
    "def main(){var x=List[int](); x.missing()}" ,
    "def takes(x:List[bool]){} def main(){takes(List[int]())}",
    "struct Node { children:List[Node] } def main(){}",
    "struct Owner { values:List[int] } def replace(owner:Owner)->int { owner.values=List[int]();return 0 } def main(){var owner=Owner(List[int]());owner.values.set(replace(owner),1)}",
    "extern def unsafe(value:int)->int def worker(value:int)->int { var x=List[int](); x.push(unsafe(value)); return x.get(0) } def main(){var t=spawn(worker(0));var r=join(t)}",
    "def nested(value:int)->int{return value} def worker(value:int)->int { var x=List[int]();var t=async_spawn(nested(value));var r=await(t);x.push(r);return x.get(0) } def main(){var t=spawn(worker(0));var r=join(t)}",
];

#[test]
fn collections_are_concrete_typed_and_storage_is_inaccessible() {
    bhasa::check(PROGRAM).unwrap();
    let shapes = bhasa::generics::template_program(
        "def singleton[T](v:T)->List[T]{var x=List[T]();x.push(v);return x}",
        &[],
    )
    .unwrap();
    assert_eq!(shapes.functions.len(), 1);
    assert_eq!(shapes.functions[0].name, "singleton");
    assert!(shapes.records.is_empty());

    for source in INVALID {
        assert!(bhasa::check(source).is_err(), "{source}");
    }
    for source in [
        "struct __bhg_list_0 { wrong:int } def main(){}",
        "struct __bhg_map_0 { __key:bool; __value:int } def main(){}",
        "struct __bhg_list_0 { __element:int; def count(self)->int{return 0} } def main(){}",
    ] {
        let parsed = bhasa::parser::parse(source).unwrap();
        assert!(bhasa::sema::check(parsed).is_err(), "{source}");
    }
    let mut checked = bhasa::check("def main(){var x=List[int]();x.push(1)}").unwrap();
    checked.program.records[0].fields[0].name = "forged".into();
    assert!(bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err());
}

fn seed(workspace: &Workspace) -> std::path::PathBuf {
    let source = ["lexer.bh", "emitter.bh", "driver.bh"]
        .map(|name| fs::read_to_string(fixture(&format!("bootstrap/compiler/{name}"))).unwrap())
        .join("\n");
    let source = workspace.write("seed.bh", &source);
    let executable = build(workspace, &source, true);
    let saved = workspace.file("seed");
    fs::rename(executable, &saved).unwrap();
    saved
}
fn emit(seed: &Path, workspace: &Workspace, source: &str) -> std::process::Output {
    let source = workspace.write("expanded.bh", source);
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
    optimization: &str,
    sanitize: bool,
) -> std::process::Output {
    let source = workspace.write("collections.ll", ir);
    let executable = workspace.file("collections");
    let mut command = Command::new(clang);
    command
        .arg(optimization)
        .arg(source)
        .arg(fixture("runtime/core.c"))
        .arg(fixture("runtime/concurrency.c"))
        .arg(fixture("runtime/collections.c"))
        .arg("-pthread")
        .arg("-o")
        .arg(&executable);
    if sanitize {
        command
            .arg("-fsanitize=address,undefined")
            .arg("-fno-omit-frame-pointer");
    }
    succeeds(&command.output().unwrap());
    execute(&executable, workspace)
}

#[test]
fn reference_and_bhasa_execute_owned_nested_collections_and_bounds_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collections-generic");
    let seed = seed(&workspace);
    let target = bhasa::target::Target::Native.triple_for_ir().unwrap();
    let reference = bhasa::compile_to_llvm(PROGRAM, &target).unwrap();
    assert!(reference.contains("; bhasa runtime: collections"));
    let expanded = bhasa::generics::expand(PROGRAM, &[]).unwrap();
    let emitted = emit(&seed, &workspace, &expanded.source);
    succeeds(&emitted);
    let bhasa = stdout(&emitted);
    assert!(bhasa.contains("; bhasa runtime: collections"));
    for ir in [&reference, &bhasa] {
        for optimization in ["-O0", "-O2"] {
            let output = link(&workspace, &clang, ir, optimization, true);
            succeeds(&output);
            assert_eq!(stdout(&output), "typed collections ok\n");
        }
    }
    for source in INVALID {
        if let Ok(expanded) = bhasa::generics::expand(source, &[]) {
            let output = emit(&seed, &workspace, &expanded.source);
            assert!(!output.status.success(), "{source}: {}", describe(&output));
            assert!(output.stdout.is_empty());
        }
    }
    for body in [
        "var x=List[int]();print(x.get(0))",
        "var x=List[int]();x.push(1);print(x.get(-1))",
        "var x=List[int]();x.set(0,1)",
        "var x=Map[str,int]();print(x.get(\"missing\"))",
    ] {
        let source = format!("def main(){{{body}}}");
        let reference = bhasa::compile_to_llvm(&source, &target).unwrap();
        let expanded = bhasa::generics::expand(&source, &[]).unwrap();
        let emitted = emit(&seed, &workspace, &expanded.source);
        succeeds(&emitted);
        for ir in [&reference, &stdout(&emitted)] {
            for optimization in ["-O0", "-O2"] {
                let output = link(&workspace, &clang, ir, optimization, false);
                assert!(!output.status.success(), "{body}: {}", describe(&output));
                assert!(output.stdout.is_empty());
            }
        }
    }
}

#[test]
fn imported_generic_collection_functions_work_through_driver() {
    let Some(_) = clang() else { return };
    let workspace = Workspace::new("collections-import");
    workspace.write(
        "library.bh",
        "def singleton[T](value:T)->List[T]{var list=List[T]();list.push(value);return list}",
    );
    let source=workspace.write("main.bh","import \"library.bh\"\ndef main(){var x=singleton[buffer](buffer(\"hello\"));print(x.get(0))}");
    let executable = build(&workspace, &source, false);
    let output = execute(&executable, &workspace);
    succeeds(&output);
    assert_eq!(stdout(&output), "hello\n");
}
