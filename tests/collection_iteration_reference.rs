mod common;
use bhasa::ast::{ExprKind, ForSource, StmtKind, Type};
use common::*;
use std::collections::BTreeMap;
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
            panic!(
                "collection iteration exceeded deadline: {}",
                describe(&output)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn iterable_types_and_scopes_are_checked_and_corrupt_bindings_rejected() {
    for (source, element) in [
        ("buffer()", Type::Int),
        ("ints()", Type::Int),
        ("Map[int,buffer]()", Type::Int),
        ("Map[str,buffer]()", Type::Str),
        ("List[float]()", Type::Float),
    ] {
        let mut checked = bhasa::check(&format!(
            "def main() {{ for value in ({source}) {{print(value)}} }}"
        ))
        .unwrap();
        let body = checked
            .program
            .functions
            .iter_mut()
            .find(|function| function.name == "main")
            .unwrap()
            .body
            .as_mut()
            .unwrap();
        let StmtKind::For {
            source: ForSource::Iterable(value),
            element_type,
            ..
        } = &mut body[0].kind
        else {
            panic!()
        };
        assert!(value.ty.is_some());
        assert_eq!(*element_type, Some(element));
        *element_type = Some(Type::Bool);
        assert!(
            bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err(),
            "{source}"
        );
    }
    for source in [
        "1",
        "true",
        "2.0",
        "\"text\"",
        "print(1)",
        "Item(1)",
        "Choice.none()",
    ] {
        let source = format!(
            "struct Item {{value:int}} enum Choice{{none}} def main(){{for value in {source} {{}}}}"
        );
        assert!(
            bhasa::check(&source)
                .unwrap_err()
                .iter()
                .any(|error| error.code == "S034"),
            "{source}"
        );
    }
    for source in ["buffer()", "ints()", "Map[str,buffer]()"] {
        assert!(bhasa::check(&format!(
            "def main(){{for value in {source} {{}} print(value)}}"
        ))
        .is_err());
    }
    let mut checked = bhasa::check("def main(){for value in buffer() {}}").unwrap();
    let StmtKind::For {
        source: ForSource::Iterable(value),
        ..
    } = &mut checked.program.functions[0].body.as_mut().unwrap()[0].kind
    else {
        panic!()
    };
    value.kind = ExprKind::Int(1);
    value.ty = Some(Type::Int);
    assert!(bhasa::codegen::emit_llvm(&checked, "x86_64-unknown-linux-gnu").is_err());
    assert!(bhasa::check("def main(){bhasa_rt_map_next_key(1,2,3)}").is_err());
}

#[test]
fn iterable_sources_preserve_worker_graph_and_lexical_task_rules() {
    for definitions in [
        "extern def foreign(value:int)->int\ndef values()->buffer{var n=foreign(1);return buffer()} def work(value:int)->int{for byte in values(){}return value}",
        "extern def foreign(value:int)->int\nstruct Source{def values(self)->ints{var n=foreign(1);return ints()}} def work(value:int)->int{for n in Source().values(){}return value}",
        "extern def foreign(value:int)->int\ndef values[T](value:T)->Map[int,T]{var n=foreign(1);return Map[int,T]()} def work(value:int)->int{for key in values[buffer](buffer()){}return value}",
        "def inner(value:int)->int{return value} def values()->Map[str,int]{var task=async_spawn(inner(1));var n=join(task);return Map[str,int]()} def work(value:int)->int{for key in values(){}return value}",
    ] {
        for submit in ["spawn", "async_spawn"] {
            let source =
                format!("{definitions} def main(){{var task={submit}(work(1));join(task)}}");
            let errors = bhasa::check(&source).unwrap_err();
            assert!(
                errors.iter().any(|error| error.code == "S020"),
                "{source}: {errors:?}"
            );
        }
    }
    for iterable in [
        "buffer(\"abc\")",
        "ints()",
        "Map[int,buffer]()",
        "Map[str,buffer]()",
    ] {
        for exit in ["continue", "break", "return"] {
            let source = format!(
                "def work(value:int)->buffer{{return buffer()}} def main(){{for value in {iterable} {{var task=spawn(work(1));{exit}}}}}"
            );
            assert!(bhasa::check(&source).is_err(), "{source}");
        }
        bhasa::check(&format!("def work(value:int)->buffer{{return buffer()}} def main(){{for value in {iterable} {{var task=async_spawn(work(1));var result=await(task);continue}}}} ")).unwrap();
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn native_iteration_matches_byte_integer_and_key_set_oracles_at_o0_o2() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-iteration-oracle");
    let mut source = String::from("def main(){print(\"@bytes\");var bytes=buffer();");
    let mut expected = BTreeMap::<String, Vec<String>>::new();
    let bytes = [0, 1, 127, 128, 195, 169, 254, 255];
    for value in bytes {
        source.push_str(&format!("push(bytes,{value});"));
    }
    expected.insert(
        "@bytes".into(),
        bytes.map(|value| value.to_string()).to_vec(),
    );
    source.push_str("for byte in bytes{print(byte);set(bytes,0,88);push(bytes,42);byte=999} print(\"@ints\");var numbers=ints();");
    let numbers = [i64::MIN, -256, -1, 0, 1, 255, i64::MAX];
    for value in numbers {
        source.push_str(&format!("push(numbers,{value});"));
    }
    expected.insert(
        "@ints".into(),
        numbers.map(|value| value.to_string()).to_vec(),
    );
    source.push_str("for value in numbers{print(value);set(numbers,0,99);push(numbers,42);value=999} print(\"@map-int\");var table=Map[int,buffer]();");
    let keys: Vec<i64> = (0..40).chain([i64::MIN, i64::MAX]).collect();
    for key in &keys {
        source.push_str(&format!("table.put({key},buffer(\"owned\"));"));
    }
    for key in (0..40).step_by(2) {
        source.push_str(&format!("table.remove({key});"));
    }
    source.push_str("table.put(3,buffer(\"replacement\"));for key in table{print(key);table.clear();table.put(999,buffer());key=999} print(\"@map-str\");var names=Map[str,buffer]();");
    let mut integers: Vec<_> = keys
        .into_iter()
        .filter(|key| *key < 0 || *key == i64::MAX || key % 2 == 1)
        .map(|key| key.to_string())
        .collect();
    integers.sort();
    expected.insert("@map-int".into(), integers);
    let strings = ["", "a", "é", "🌍", "replaced"];
    for key in strings {
        source.push_str(&format!("names.put(\"{key}\",buffer(\"owned\"));"));
    }
    source.push_str("names.put(\"removed\",buffer());names.remove(\"removed\");names.put(\"replaced\",buffer(\"new\"));for key in names{print(key);names.clear();names.put(\"new\",buffer());key=\"mutated\"} print(\"@empty\");for byte in buffer(){print(99)}for value in ints(){print(99)}for key in Map[int,buffer](){print(99)}for key in Map[str,buffer](){print(99)}}");
    let mut strings = strings.map(str::to_owned).to_vec();
    strings.sort();
    expected.insert("@map-str".into(), strings);
    expected.insert("@empty".into(), Vec::new());
    let ir = bhasa::compile_to_llvm(
        &source,
        &bhasa::target::Target::Native.triple_for_ir().unwrap(),
    )
    .unwrap();
    let file = workspace.write("oracle.ll", &ir);
    for optimization in ["-O0", "-O2"] {
        let executable = workspace.file("oracle");
        succeeds(
            &Command::new(&clang)
                .arg(optimization)
                .arg(&file)
                .arg(fixture("runtime/core.c"))
                .arg(fixture("runtime/collections.c"))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let output = bounded(&mut Command::new(&executable));
        succeeds(&output);
        let mut actual = BTreeMap::<String, Vec<String>>::new();
        let mut section = String::new();
        for line in stdout(&output).lines() {
            if line.starts_with('@') {
                section = line.to_owned();
                assert!(actual.insert(section.clone(), Vec::new()).is_none());
            } else {
                actual.get_mut(&section).unwrap().push(line.to_owned());
            }
        }
        for (name, values) in &mut actual {
            if name.starts_with("@map-") {
                values.sort();
            }
        }
        assert_eq!(actual, expected, "{optimization}");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}

const OWNERSHIP: &str = r#"
struct Item { text:buffer; nested:List[buffer] }
enum Choice { some(item:Item); none }
struct Source {
    calls:int; data:buffer; numbers:ints; table:Map[int,Choice]
    def bytes(self)->buffer{self.calls=self.calls+1;return self.data}
    def integers(self)->ints{self.calls=self.calls+1;return self.numbers}
    def keys(self)->Map[int,Choice]{self.calls=self.calls+1;return self.table}
}
def make()->Map[int,Choice]{
    var result=Map[int,Choice]()
    for i in range(6){var nested=List[buffer]();nested.push(buffer("inside"));result.put(i,Choice.some(Item(buffer("owned"),nested)))}
    return result
}
def echo(value:Map[int,Choice])->Map[int,Choice]{return value}
def early(value:Map[int,Choice])->buffer{
    for key in value {
        var local=List[buffer]();local.push(buffer("scope"))
        var task=spawn(echo(value));var result=join(task)
        for byte in buffer("ab") {var temporary=buffer("nested");if byte==97 {continue};return buffer("returned")}
    }
    return buffer("missing")
}
def exercise()->bool{
    var numbers=ints();push(numbers,-1);push(numbers,2);push(numbers,3)
    var source=Source(0,buffer("abc"),numbers,make());var count=0;var sum=0
    for byte in source.bytes(){count=count+1;sum=sum+byte;source.data=buffer("changed");byte=99;if count==1{continue}}
    if source.calls!=1 or count!=3 or sum!=294{return false}
    count=0;sum=0
    for number in source.integers(){count=count+1;sum=sum+number;source.numbers=ints();number=99;if count==2{continue}}
    if source.calls!=2 or count!=3 or sum!=4{return false}
    count=0;sum=0
    for key in source.keys(){
        count=count+1;sum=sum+key;source.table.clear();key=99
        var task=async_spawn(echo(make()));var returned=await(task)
        if returned.count()!=6{return false}
        match returned.get(0){
            case Choice.some(item){var scratch=List[buffer]();scratch.push(item.text);if count<6{continue}}
            case Choice.none{return false}
        }
    }
    if source.calls!=3 or count!=6 or sum!=15{return false}
    for key in source.keys(){return false}
    if source.calls!=4{return false}
    var table=make()
    for key in table{var local=buffer("break");break}
    if not same(early(table),"returned"){return false}
    count=0
    for key in table{
        var outer=buffer("outer")
        for number in numbers{var inner=buffer("inner");if number<0{continue};count=count+1;break}
    }
    if count!=6{return false}
    var texts=Map[str,Item]();texts.put("saved",Item(buffer("text"),List[buffer]()))
    for key in texts {var task=spawn(echo(table));var result=join(task);texts.clear();if not same(buffer(key),"saved"){return false};key="rebound";break}
    return true
}
def main()->int{for round in range(8){if not exercise(){return 1}}print("collection iteration ownership ok");return 0}
"#;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn iteration_snapshots_and_early_exits_release_owned_values_and_typed_tasks() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("collection-iteration-ownership");
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
        assert_eq!(stdout(&output), "collection iteration ownership ok\n");
        assert!(output.stderr.is_empty(), "{}", describe(&output));
    }
}
