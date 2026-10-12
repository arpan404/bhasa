mod common;
use bhasa::json::Value;
use common::*;

fn report(output: &std::process::Output) -> Value {
    bhasa::json::parse(&stdout(output)).unwrap()
}

#[test]
fn inspection_returns_deterministic_checked_apis_and_module_edges_without_build_tools() {
    let workspace = Workspace::new("agent-inspect");
    workspace.write("library.bh", "private def hidden()->int{return 1}\ndef misleading()->int{var private=1;return private}\n// public declaration after a local named private\ndef visible(){}\nstruct Item {private bytes:buffer; count:int; def size(self)->int{return self.count}}\ndef make()->Item{return Item(buffer(\"hello\"),5)}\nenum Reply{ok(value:buffer);empty}\n");
    let source = workspace.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){var item=lib.make();print(item.size())}\n",
    );
    let invoke = || {
        workspace
            .command()
            .arg("inspect")
            .arg(&source)
            .arg("--json")
            .env("BHASA_CLANG", workspace.file("absent-clang"))
            .env("CARGO", workspace.file("absent-cargo"))
            .output()
            .unwrap()
    };
    let output = invoke();
    succeeds(&output);
    assert!(output.stderr.is_empty(), "{}", describe(&output));
    assert_eq!(output.stdout, invoke().stdout);
    let value = report(&output);
    assert_eq!(value.get("schema_version").and_then(Value::as_i64), Some(1));
    assert_eq!(value.get("ok"), Some(&Value::Bool(true)));
    let modules = value.get("modules").unwrap().as_array().unwrap();
    assert_eq!(modules.len(), 2);
    let imports = modules[0].get("imports").unwrap().as_array().unwrap();
    assert_eq!(imports[0].get("alias").and_then(Value::as_str), Some("lib"));
    assert_eq!(
        imports[0].get("file").and_then(Value::as_str),
        Some(as_str(
            &workspace.file("library.bh").canonicalize().unwrap()
        ))
    );
    let declarations = value.get("declarations").unwrap().as_array().unwrap();
    let hidden = declarations
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some("hidden"))
        .unwrap();
    assert_eq!(hidden.get("private"), Some(&Value::Bool(true)));
    let visible = declarations
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some("visible"))
        .unwrap();
    assert_eq!(visible.get("private"), Some(&Value::Bool(false)));
    let item = declarations
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some("Item"))
        .unwrap();
    let fields = item.get("fields").unwrap().as_array().unwrap();
    assert_eq!(fields[0].get("private"), Some(&Value::Bool(true)));
    assert_eq!(fields[1].get("private"), Some(&Value::Bool(false)));
    assert_eq!(
        fields[0].get("type").and_then(Value::as_str),
        Some("buffer")
    );
    let method = &item.get("methods").unwrap().as_array().unwrap()[0];
    assert_eq!(method.get("returns").and_then(Value::as_str), Some("int"));
    assert!(!stdout(&output).contains("__bhm_"));
    assert!(!workspace.file("build").exists());
}

#[test]
fn inspection_reads_lowercase_package_metadata_and_concrete_generics() {
    let workspace = Workspace::new("agent-inspect-package");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"query\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    workspace.write("main.bh", "struct Box[T]{value:T}\ndef identity[T](value:T)->T{return value}\ndef main(){var box=Box[buffer](buffer(\"query\"));var values=List[Box[buffer]]();values.push(identity[Box[buffer]](box))}\n");
    let output = workspace.run(&["inspect"]);
    succeeds(&output);
    let value = report(&output);
    let packages = value.get("packages").unwrap().as_array().unwrap();
    assert_eq!(
        packages[0].get("name").and_then(Value::as_str),
        Some("query")
    );
    assert!(!stdout(&output).contains("__bhg_"));
    let declarations = value.get("declarations").unwrap().as_array().unwrap();
    assert!(declarations
        .iter()
        .any(|value| value.get("name").and_then(Value::as_str) == Some("Box[buffer]")));
    assert!(declarations
        .iter()
        .any(|value| value.get("returns").and_then(Value::as_str) == Some("Box[buffer]")));
    assert!(!workspace.file("build").exists());
}

#[test]
fn inspection_errors_are_structured_located_and_never_report_unchecked_types() {
    let workspace = Workspace::new("agent-inspect-errors");
    workspace.write("library.bh", "def broken()->int {return missing}\n");
    let source = workspace.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){print(lib.broken())}\n",
    );
    let output = workspace
        .command()
        .arg("inspect")
        .arg(source)
        .output()
        .unwrap();
    fails(&output);
    let value = report(&output);
    assert_eq!(value.get("ok"), Some(&Value::Bool(false)));
    assert!(value.get("declarations").is_none());
    let diagnostics = value.get("diagnostics").unwrap().as_array().unwrap();
    assert_eq!(
        diagnostics[0].get("file").and_then(Value::as_str),
        Some(as_str(
            &workspace.file("library.bh").canonicalize().unwrap()
        ))
    );
    assert_eq!(diagnostics[0].get("line").and_then(Value::as_i64), Some(1));
    for args in [
        &["inspect", "--release"][..],
        &["inspect", "--json", "--json"][..],
        &["inspect", "missing.bh"][..],
    ] {
        let output = workspace.run(args);
        fails(&output);
        assert_eq!(report(&output).get("ok"), Some(&Value::Bool(false)));
    }
}

#[test]
fn inspection_owned_parameter_modes_are_checked_and_preserve_imported_alias_apis() {
    let workspace = Workspace::new("inspect-owned-aliases");
    workspace.write("lib.bh","struct Point{value:int}\nstruct Box[T]{value:T}\ntype Item=Box[Point]\ntype Bytes=buffer\ndef take(owned value:Bytes)->Bytes{return move value}\ndef read(value:Bytes)->int{return len(value)}\ndef owned(owned:int)->int{return owned}\ndef move(value:int)->int{return value}\n");
    let source=workspace.write("main.bh","import \"lib.bh\" as lib\ndef main(){let input=buffer(\"x\");let output=lib.take(move input);print(lib.read(output));print(lib.owned(lib.move(1)))}\n");
    let invoke = || {
        workspace
            .command()
            .arg("inspect")
            .arg(&source)
            .arg("--json")
            .env("BHASA_CLANG", workspace.file("missing-clang"))
            .env("CARGO", workspace.file("missing-cargo"))
            .output()
            .unwrap()
    };
    let output = invoke();
    succeeds(&output);
    assert_eq!(output.stdout, invoke().stdout);
    let value = report(&output);
    let declarations = value.get("declarations").unwrap().as_array().unwrap();
    for (name, mode, ty) in [
        ("take", "owned", "buffer"),
        ("read", "borrowed", "buffer"),
        ("owned", "borrowed", "int"),
        ("move", "borrowed", "int"),
    ] {
        let function = declarations
            .iter()
            .find(|v| v.get("name").and_then(Value::as_str) == Some(name))
            .unwrap();
        let param = &function.get("parameters").unwrap().as_array().unwrap()[0];
        assert_eq!(param.get("mode").and_then(Value::as_str), Some(mode));
        assert_eq!(param.get("type").and_then(Value::as_str), Some(ty));
    }
    assert!(declarations
        .iter()
        .any(|v| v.get("name").and_then(Value::as_str) == Some("Item")
            && v.get("kind").and_then(Value::as_str) == Some("type_alias")));
    assert!(!stdout(&output).contains("__bhm_") && !stdout(&output).contains("__bhg_"));
    assert!(!workspace.file("build").exists());
}
#[test]
fn inspection_owned_rejects_invalid_transfer_before_reporting_api() {
    let workspace = Workspace::new("inspect-owned-reject");
    for source in ["def take(owned value:int){}\ndef main(){}", "def take(owned value:buffer){}\ndef main(){let value=buffer();take(move value);print(value)}"] {
        let file=workspace.write("main.bh",source);let output=workspace.command().arg("inspect").arg(file).output().unwrap();fails(&output);
        let value=report(&output);assert_eq!(value.get("ok"),Some(&Value::Bool(false)));assert!(value.get("declarations").is_none());
        assert!(value.get("diagnostics").unwrap().as_array().unwrap().iter().any(|v|v.get("code").and_then(Value::as_str)==Some("S038")));
    }
}
