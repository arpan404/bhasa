"use strict";

const { EventEmitter } = require("node:events");
const { TextDecoder } = require("node:util");

const MAX_HEADER = 8192;
const MAX_BODY = 4 * 1024 * 1024;
const MAX_PENDING = 128;
const utf8 = new TextDecoder("utf-8", { fatal: true });

// stdin/stdout JSON-RPC only. The compiler's stderr is handled by the extension.
class RpcConnection extends EventEmitter {
  constructor(readable, writable) {
    super();
    this.readable = readable;
    this.writable = writable;
    this.buffer = Buffer.alloc(0);
    this.bodyLength = null;
    this.nextId = 1;
    this.pending = new Map();
    this.closed = false;
    readable.on("data", (data) => this.receive(data));
    readable.on("error", (error) => this.close(error));
    writable.on("error", (error) => this.close(error));
    readable.on("end", () => this.close(new Error(
      this.buffer.length || this.bodyLength !== null
        ? "Language server ended during a message."
        : "Language server closed its output."
    )));
  }

  receive(data) {
    if (this.closed) return;
    try {
      this.buffer = Buffer.concat([this.buffer, data]);
      for (;;) {
        if (this.bodyLength === null) {
          const end = this.buffer.indexOf("\r\n\r\n");
          if (end < 0) {
            if (this.buffer.length > MAX_HEADER) throw new Error("LSP header is too large.");
            return;
          }
          if (end > MAX_HEADER) throw new Error("LSP header is too large.");
          const header = this.buffer.subarray(0, end);
          if (header.some((byte) => byte > 127)) throw new Error("LSP header must be ASCII.");
          let length;
          for (const line of header.toString("ascii").split("\r\n")) {
            const field = /^([^:]+):[ \t]*(.*)$/.exec(line);
            if (!field) throw new Error("Invalid LSP header.");
            if (field[1].toLowerCase() === "content-length") {
              if (length !== undefined || !/^[0-9]+$/.test(field[2])) {
                throw new Error("Invalid LSP Content-Length.");
              }
              length = Number(field[2]);
            }
          }
          if (!Number.isSafeInteger(length) || length < 1 || length > MAX_BODY) {
            throw new Error("Missing or oversized LSP Content-Length.");
          }
          this.bodyLength = length;
          this.buffer = this.buffer.subarray(end + 4);
        }
        if (this.buffer.length < this.bodyLength) return;
        const body = this.buffer.subarray(0, this.bodyLength);
        this.buffer = this.buffer.subarray(this.bodyLength);
        this.bodyLength = null;
        this.message(JSON.parse(utf8.decode(body)));
        if (this.closed) return;
      }
    } catch (error) {
      this.close(error);
    }
  }

  message(message) {
    if (!message || typeof message !== "object" || Array.isArray(message) || message.jsonrpc !== "2.0") {
      throw new Error("Invalid JSON-RPC message from language server.");
    }
    if (typeof message.method === "string") {
      if (Object.hasOwn(message, "id")) {
        this.send({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "Method not found" } });
      } else {
        this.emit("notification", message.method, message.params);
      }
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) return; // Canceled requests may still receive a response.
    if (Object.hasOwn(message, "result") === Object.hasOwn(message, "error")) {
      throw new Error("Invalid JSON-RPC response from language server.");
    }
    this.pending.delete(message.id);
    pending.cleanup();
    if (message.error) {
      const error = new Error(message.error.message || "Language server request failed.");
      error.code = message.error.code;
      pending.reject(error);
    } else {
      pending.resolve(message.result);
    }
  }

  send(message) {
    if (this.closed) throw new Error("Language server connection is closed.");
    const body = Buffer.from(JSON.stringify(message), "utf8");
    if (body.length > MAX_BODY) throw new Error("LSP message exceeds the 4 MiB limit.");
    if (this.writable.writableLength + body.length > MAX_BODY * 2) {
      throw new Error("Language server is not reading requests.");
    }
    this.writable.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
  }

  notify(method, params) {
    this.send({ jsonrpc: "2.0", method, params });
  }

  request(method, params, token, timeout = 10000) {
    if (this.closed) return Promise.reject(new Error("Language server connection is closed."));
    if (token?.isCancellationRequested) return Promise.reject(canceled());
    if (this.pending.size >= MAX_PENDING) return Promise.reject(new Error("Too many pending LSP requests."));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      let subscription;
      const cancel = (error) => {
        const pending = this.pending.get(id);
        if (!pending) return;
        this.pending.delete(id);
        pending.cleanup();
        try { this.notify("$/cancelRequest", { id }); } catch (_) { /* Already closed. */ }
        reject(error);
      };
      const timer = setTimeout(() => cancel(new Error(`Language server timed out: ${method}`)), timeout);
      const cleanup = () => { clearTimeout(timer); subscription?.dispose(); };
      this.pending.set(id, { resolve, reject, cleanup });
      try {
        this.send({ jsonrpc: "2.0", id, method, params });
        subscription = token?.onCancellationRequested(() => cancel(canceled()));
        if (!this.pending.has(id)) subscription?.dispose();
      } catch (error) {
        this.pending.delete(id);
        cleanup();
        reject(error);
      }
    });
  }

  close(error = new Error("Language server connection disposed.")) {
    if (this.closed) return;
    this.closed = true;
    this.buffer = Buffer.alloc(0);
    for (const pending of this.pending.values()) {
      pending.cleanup();
      pending.reject(error);
    }
    this.pending.clear();
    this.emit("failure", error);
  }
}

function canceled() {
  const error = new Error("Request canceled.");
  error.code = -32800;
  return error;
}

module.exports = { RpcConnection, MAX_BODY, MAX_HEADER };
