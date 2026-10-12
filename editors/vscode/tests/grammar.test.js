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
