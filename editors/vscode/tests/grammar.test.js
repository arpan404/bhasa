"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const grammar = require("../syntaxes/bhasa.tmLanguage.json");

// Check the simple regular expressions without adding a TextMate engine.
// These checks do not claim to exercise VS Code's scope/tokenization engine.
test("grammar recognizes struct and method declarations and dotted method calls", () => {
  const source = "struct Point { x: int; def total(self) -> int { return self.x } }\npoint.total()";
  const declaration = grammar.patterns.find((pattern) => pattern.captures?.[2]?.name === "entity.name.function.bhasa");
  const record = grammar.patterns.find((pattern) => pattern.captures?.[2]?.name === "entity.name.type.bhasa");
  const call = grammar.patterns.find((pattern) => pattern.name === "entity.name.function.call.bhasa");
  assert.deepEqual(Array.from(source.match(new RegExp(record.match))).slice(1), ["struct", "Point"]);
  assert.deepEqual(Array.from(source.match(new RegExp(declaration.match))).slice(1), ["def", "total"]);
  assert.equal("point.total()".match(new RegExp(call.match))[0], "total");
});

test("grammar highlights the self receiver without matching longer identifiers", () => {
  const receiver = grammar.patterns.find((pattern) => pattern.name === "variable.language.bhasa");
  const expression = new RegExp(receiver.match, "g");
  assert.deepEqual("def total(self) { return self.x } selfish myself self2 _self".match(expression), ["self", "self"]);
});

test("grammar recognizes enum declarations and exhaustive match keywords", () => {
  const declaration = grammar.patterns.find((pattern) => pattern.captures?.[2]?.name === "entity.name.type.bhasa");
  const control = grammar.patterns.find((pattern) => pattern.name === "keyword.control.bhasa");
  assert.deepEqual(Array.from("enum OptionInt { none; some(value: int) }".match(new RegExp(declaration.match))).slice(1), ["enum", "OptionInt"]);
  assert.deepEqual("match value { case OptionInt.none {} } matcher casework".match(new RegExp(control.match, "g")), ["match", "case"]);
});


test("grammar recognizes privacy, alias imports, numeric and collection types", () => {
  const control = grammar.patterns.find((pattern) => pattern.name === "keyword.control.bhasa");
  const type = grammar.patterns.find((pattern) => pattern.name === "storage.type.bhasa");
  const number = grammar.patterns.find((pattern) => pattern.name === "constant.numeric.float.bhasa");
  assert.deepEqual("private def test() { } import \"math.bh\" as math".match(new RegExp(control.match, "g")), ["private", "import", "as"]);
  assert.deepEqual("float List Map floaty ListExtra Mapper".match(new RegExp(type.match, "g")), ["float", "List", "Map"]);
  assert.deepEqual("file_handle file_handler file_handles".match(new RegExp(type.match, "g")), ["file_handle"]);
  assert.deepEqual("1.25 2e3 3.0E-2 4 1.member".match(new RegExp(number.match, "g")), ["1.25", "2e3", "3.0E-2"]);
});


test("grammar recognizes iteration keywords without matching identifier substrings", () => {
  const control = grammar.patterns.find((pattern) => pattern.name === "keyword.control.bhasa");
  assert.deepEqual("for item in items { break; continue } before inner breakage continued".match(new RegExp(control.match, "g")), ["for", "in", "break", "continue"]);
});


test("grammar highlights contextual initializer propagation while keeping ordinary try names", () => {
  const pattern = grammar.patterns.find((pattern) => pattern.captures?.[1]?.name === "keyword.control.bhasa");
  const expression = new RegExp(pattern.match);
  assert.equal("var value = try read()".match(expression)[1], "try");
  assert.equal("var value=\ttry\tresult".match(expression)[1], "try");
  for (const source of ["var value=try(1)", "var value=try[int](1)", "var value=try.member", "var value=try and flag", "var value=try or flag", "return try read()", "var value=try\nread()", "var value=try # comment\nread()"] ) {
    assert.equal(expression.test(source), false, source);
  }
});

test("grammar highlights statement let bindings while preserving ordinary let names", () => {
  const pattern = grammar.patterns.find((pattern) => pattern.match?.includes("(let)"));
  const expression = new RegExp(pattern.match, "m");
  for (const source of ["let value=1", "  let let=1", "def main(){let value=1}", "var x=1;let result=try read()", "# comment\nlet item=buffer(\"x\")"]) {
    assert.equal(source.match(expression)[1], "let", source);
  }
  for (const source of ["def let(value:int){}", "let(1)", "let[int](1)", "let.field", "let=2", "let\nvalue=1", "var value=let and flag", "let # comment\nvalue=1"]) {
    assert.equal(expression.test(source), false, source);
  }
});


test("grammar keeps owned and move contextual rather than globally reserved", () => {
  const owned = grammar.patterns.find((p) => p.captures?.[1]?.name === "storage.modifier.ownership.bhasa");
  const move = grammar.patterns.find((p) => p.captures?.[1]?.name === "keyword.operator.transfer.bhasa");
  for (const source of ["def take(owned value: buffer)", "def take(x:int, owned owned:ints)"]) {
    assert.equal(source.match(new RegExp(owned.match))[1], "owned", source);
  }
  for (const source of ["def owned(value: buffer)", "def ordinary(owned:buffer)", "owned(x)", "var owned=buffer()", "item.owned()"]) {
    assert.equal(new RegExp(owned.match).test(source), false, source);
  }
  for (const source of ["return move value", "take(move move)", "let next=move value"]) {
    assert.equal(source.match(new RegExp(move.match))[1], "move", source);
  }
  for (const source of ["def move(value:buffer)", "move(value)", "move[buffer](value)", "var move=buffer()", "move and flag", "move or flag", "move not flag", "move true", "move false", "move\nvalue"]) {
    assert.equal(new RegExp(move.match).test(source), false, source);
  }
  const control = grammar.patterns.find((p) => p.name === "keyword.control.bhasa");
  assert.equal(new RegExp(control.match).test("owned move"), false);
});
