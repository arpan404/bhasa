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
        "error integration fixture exceeded deadline: {}",
        describe(&output)
    );
    output
}

#[test]
fn imported_generic_let_bindings_keep_contextual_names_and_owned_behavior() {
    let ws = Workspace::new("immutable-host");
    ws.write("library.bh",r#"
        struct Holder[T]{value:T;def get(self)->T{return self.value}}
        enum Outcome[T]{error(problem:buffer);ok(payload:T)}
        def let[T](value:T)->T{return value}
        def worker(value:buffer)->buffer{append(value," worker");return value}
        def success[T](value:T)->Outcome[T]{return Outcome[T].ok(value)}
        def pick[let](value:let)->let{let result=value;return result}
        def propagate[T](value:T)->Outcome[T]{let result=try success[T](value);return Outcome[T].ok(result)}
    "#);
    let source = ws.write(
        "main.bh",
        r#"
        import "library.bh" as lib
        def main()->int{
            let holder=lib.Holder[buffer](buffer("original"))
            holder.value=buffer("changed")
            append(holder.value,"!")
            let copy=lib.let[buffer](holder.get())
            append(copy," copy")
            if not same(holder.value,"changed!") or not same(copy,"changed! copy"){return 1}
            let items=List[buffer]();items.push(copy)
            for item in items{let count=len(item);if count!=13{return 2}}
            match lib.propagate[buffer](lib.pick[buffer](copy)){
                case lib.Outcome[buffer].ok(payload){if not same(payload,"changed! copy"){return 3}}
                case lib.Outcome[buffer].error(problem){return 4}
            }
            let job=spawn(lib.worker(buffer("job")))
            let reply=join(job)
            if not same(reply,"job worker"){return 5}
            var let=6;let=let+1
            if true{var holder=1;holder=2;if holder!=2{return 6}}
            print(let)
            return 0
        }
    "#,
    );
    succeeds(&bounded(ws.command().arg("fmt").arg(&source)));
    succeeds(&bounded(ws.command().args(["fmt", "--check"]).arg(&source)));
    for frontend in frontends() {
        let output = bounded(ws.command().arg("run").arg(&source).args([
            "--frontend",
            frontend,
            "--release",
        ]));
        succeeds(&output);
        assert_eq!(stdout(&output), "7\n");
    }
}

#[test]
fn imported_generic_immutable_assignment_diagnostics_keep_source_identity() {
    let ws = Workspace::new("immutable-host-diagnostic");
    let text =
        "# UTF-8 भाषा\r\ndef bad[T](input:T)->T{let value=input;value=input;return value}\r\n";
    let library = ws.write("library.bh", text);
    let source = ws.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){print(lib.bad[int](1))}\n",
    );
    let start = text.find("value=input;return").unwrap();
    for frontend in frontends() {
        let output = bounded(ws.command().arg("check").arg(&source).args([
            "--frontend",
            frontend,
            "--json",
        ]));
        fails(&output);
        let diagnostics = bhasa::json::parse(&stdout(&output)).unwrap();
        let diagnostics = diagnostics.as_array().unwrap();
        assert_eq!(diagnostics.len(), 1, "{}", describe(&output));
        let diagnostic = &diagnostics[0];
        assert_eq!(
            diagnostic.get("code").and_then(|v| v.as_str()),
            Some(if frontend == "rust" { "S036" } else { "BH220" })
        );
        assert_eq!(
            diagnostic.get("file").and_then(|v| v.as_str()),
            library.canonicalize().unwrap().to_str()
        );
        assert_eq!(
            diagnostic
                .get("span")
                .unwrap()
                .get("start")
                .and_then(|v| v.as_i64()),
            Some(start as i64)
        );
        assert!(!stdout(&output).contains("__bhm_") && !stdout(&output).contains("__bhg_"));
    }
}
