mod common;
use common::*;

#[test]
fn bound_opaque_results_reject_while_inline_module_consumption_remains_available() {
    let workspace = Workspace::new("failure-use-opaque-contract");
    workspace.write("library.bh", "private enum Hidden{ok(value:int);error(code:int)}def make()->Hidden{return Hidden.ok(7)}def consume(value:Hidden)->int{match value{case Hidden.ok(n){return n}case Hidden.error(e){return -1}}}\n");
    let main = workspace.write("main.bh", "import \"library.bh\" as lib\ndef main(){let value=lib.make();print(lib.consume(value))}\n");
    let output = workspace
        .command()
        .arg("check")
        .arg(&main)
        .arg("--json")
        .output()
        .unwrap();
    fails(&output);
    let json = bhasa::json::parse(&stdout(&output)).unwrap();
    let errors = json.as_array().unwrap();
    assert_eq!(errors.len(), 1, "{}", describe(&output));
    assert_eq!(errors[0].get("code").unwrap().as_str(), Some("S037"));
    assert!(errors[0]
        .get("message")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("value"));
    // The private type cannot be unpacked in this caller. The conservative
    // local rule does not recognize handling inside consume as a discharge.
    workspace.write(
        "main.bh",
        "import \"library.bh\" as lib\ndef main(){print(lib.consume(lib.make()))}\n",
    );
    succeeds(
        &workspace
            .command()
            .arg("check")
            .arg(&main)
            .arg("--json")
            .output()
            .unwrap(),
    );
}
