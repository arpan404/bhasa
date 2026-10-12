"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { PassThrough } = require("node:stream");
const { RpcConnection, MAX_BODY, MAX_HEADER } = require("../transport");

function frame(value) {
  const body = Buffer.from(JSON.stringify(value));
  return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]);
}

function connection(t) {
  const input = new PassThrough();
  const output = new PassThrough();
  const rpc = new RpcConnection(input, output);
  const sent = [];
  const errors = [];
  output.on("data", (data) => sent.push(data));
  rpc.on("failure", (error) => errors.push(error));
  t.after(() => rpc.close());
  return { input, output, rpc, sent, errors };
}

test("reads fragmented UTF-8 and multiple messages using byte Content-Length", (t) => {
  const { input, rpc } = connection(t);
  const messages = [];
  rpc.on("notification", (method, params) => messages.push([method, params]));
  const first = frame({ jsonrpc: "2.0", method: "note", params: { text: "é🌍 नेपाली" } });
  for (const byte of first) input.write(Buffer.from([byte]));
  input.write(Buffer.concat([
    frame({ jsonrpc: "2.0", method: "second", params: 2 }),
    frame({ jsonrpc: "2.0", method: "third", params: 3 })
  ]));
  assert.deepEqual(messages, [["note", { text: "é🌍 नेपाली" }], ["second", 2], ["third", 3]]);
});

test("writes UTF-8 byte length and correlates out-of-order responses", async (t) => {
  const { input, rpc, sent } = connection(t);
  const first = rpc.request("first", { text: "🌍" });
  const second = rpc.request("second", {});
  const split = sent[0].indexOf("\r\n\r\n");
  assert.equal(sent[0].subarray(0, split).toString(), `Content-Length: ${sent[0].length - split - 4}`);
  input.write(frame({ jsonrpc: "2.0", id: 2, result: "two" }));
  input.write(frame({ jsonrpc: "2.0", id: 1, result: "one" }));
  assert.equal(await first, "one");
  assert.equal(await second, "two");
});

test("enforces the server's 4 MiB body limit before writing", async (t) => {
  const { rpc, sent } = connection(t);
  assert.equal(MAX_BODY, 4 * 1024 * 1024);
  const overhead = Buffer.byteLength(JSON.stringify({ jsonrpc: "2.0", method: "note", params: { text: "" } }));
  const text = "x".repeat(MAX_BODY - overhead);
  rpc.notify("note", { text });
  const headerEnd = sent[0].indexOf("\r\n\r\n");
  assert.equal(sent[0].length - headerEnd - 4, MAX_BODY);
  assert.throws(() => rpc.notify("note", { text: text + "x" }), /4 MiB limit/);
  await assert.rejects(rpc.request("note", { text }), /4 MiB limit/);
  assert.equal(sent.length, 1);
  assert.equal(rpc.pending.size, 0);
  assert.equal(rpc.closed, false);
});

test("accepts case-insensitive length plus Content-Type", (t) => {
  const { input, rpc } = connection(t);
  let count = 0;
  rpc.on("notification", () => count++);
  const body = Buffer.from('{"jsonrpc":"2.0","method":"hello"}');
  input.write(Buffer.concat([Buffer.from(`content-length: ${body.length}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n`), body]));
  assert.equal(count, 1);
});

for (const [name, wire] of [
  ["negative length", Buffer.from("Content-Length: -1\r\n\r\n")],
  ["duplicate length", Buffer.from("Content-Length: 1\r\nContent-Length: 1\r\n\r\n")],
  ["missing length", Buffer.from("Other: value\r\n\r\n")],
  ["oversized length", Buffer.from(`Content-Length: ${MAX_BODY + 1}\r\n\r\n`)],
  ["oversized header", Buffer.alloc(MAX_HEADER + 1, 65)],
  ["invalid JSON", Buffer.from("Content-Length: 1\r\n\r\n{")],
  ["invalid UTF-8", Buffer.concat([Buffer.from("Content-Length: 1\r\n\r\n"), Buffer.from([255])])],
  ["batch message", frame([])],
  ["wrong protocol version", frame({ jsonrpc: "1.0", method: "hello" })]
]) {
  test(`rejects ${name} and rejects pending requests`, async (t) => {
    const { input, rpc, errors } = connection(t);
    const pending = rpc.request("waiting", {});
    input.write(wire);
    await assert.rejects(pending);
    assert.equal(rpc.closed, true);
    assert.equal(errors.length, 1);
    assert.equal(rpc.pending.size, 0);
  });
}

test("EOF mid-body rejects requests without hanging", async (t) => {
  const { input, rpc } = connection(t);
  const pending = rpc.request("waiting", {});
  input.end("Content-Length: 100\r\n\r\n{}");
  await assert.rejects(pending, /during a message/);
});

test("request errors preserve server code", async (t) => {
  const { input, rpc } = connection(t);
  const pending = rpc.request("format", {});
  input.write(frame({ jsonrpc: "2.0", id: 1, error: { code: -32602, message: "Cannot format" } }));
  await assert.rejects(pending, { code: -32602, message: "Cannot format" });
});

test("cancellation and timeouts release requests and notify server", async (t) => {
  const { input, rpc, sent } = connection(t);
  let cancel;
  let disposed = false;
  const token = {
    isCancellationRequested: false,
    onCancellationRequested(callback) { cancel = callback; return { dispose() { disposed = true; } }; }
  };
  const pending = rpc.request("format", {}, token);
  cancel();
  await assert.rejects(pending, { code: -32800 });
  assert.equal(disposed, true);
  assert.match(sent[1].toString(), /\$\/cancelRequest/);
  input.write(frame({ jsonrpc: "2.0", id: 1, result: null }));
  assert.equal(rpc.closed, false);
  await assert.rejects(rpc.request("slow", {}, undefined, 5), /timed out/);
  assert.equal(rpc.pending.size, 0);
});

test("unknown server requests receive method-not-found", (t) => {
  const { input, sent } = connection(t);
  input.write(frame({ jsonrpc: "2.0", id: "server-1", method: "unexpected", params: {} }));
  const body = JSON.parse(sent[0].subarray(sent[0].indexOf("\r\n\r\n") + 4));
  assert.equal(body.id, "server-1");
  assert.equal(body.error.code, -32601);
});
