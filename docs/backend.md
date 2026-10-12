# Backend libraries

The repository's `std` package provides concrete text and collection types,
concrete and generic Result/Option enums, JSON parsing and a bounded HTTP server
library. These are Bhasa source files using the native macOS/Linux runtime. Both compiler
frontends support their language features. Values use owned copies and
deterministic cleanup with no garbage collector.

## Import the local package

The HTTP example uses this lowercase `examples/http/bhasa.toml`:

```toml
[package]
name = "bhasa-http-example"
version = "0.1.0"
entry = "main.bh"

[dependencies]
std = "../../std"
```

Import it with `import "@std"`. For another project, adjust the dependency path
relative to its manifest. The compiler loads the local package; it does not
download a registry package. Unaliased imports expose public names; aliased imports provide qualified access.

```bhasa
import "@std"

def main() {
    var names = string_list()
    names.push(text("Bhasa"))
    names.push(text("भाषा"))
    print(names.get(1).bytes())

    var counts = string_int_map()
    counts.put(text("requests"), 42)
    match counts.get(text("requests")) {
        case OptionInt.none { print("missing") }
        case OptionInt.some(value) { print(value) }
    }
}
```

## Text, collections and results

| Type or factory | Operations and behavior |
| --- | --- |
| `text(str) -> Text` | Wraps an owned UTF-8 byte buffer |
| `text_from_bytes(buffer) -> Text` | Checks UTF-8 and returns an owned copy; invalid input traps |
| `text_decode(buffer) -> ResultText` | Validates UTF-8 before returning an owned `Text` |
| `Text` | `bytes`, `byte_count`, `valid`, `count`, `codepoint(index)`, `part(start, count)`, `equals`, `append`, `append_codepoint`, `replace`, `clear` |
| `string_list() -> StringList` | `count`, `push(Text)`, `get(index)`, `set(index, Text)`, `find(Text)`, `clear`, `valid` |
| `string_int_map() -> StringIntMap` | Text keys and integer values; `get` returns `OptionInt` |
| `string_map() -> StringMap` | Text keys and text values; `get` returns `OptionText` |

Both maps provide `count`, `find`, `has`, `put`, `key_at`, `keys_snapshot`,
`value_at`, `clear` and `valid`. `key_at` and `value_at` use insertion-order indices;
`keys_snapshot` returns an independent owned StringList. Lookup is
linear. Maps preserve insertion order; replacing a value keeps its position.
`find` returns `-1` when absent. Indexed list/map access uses checked storage
operations and traps on an invalid index. Maps have no removal operation yet.

Use the collection factories to establish storage invariants. Packed bytes,
offsets and key/value storage are private, as are positional constructors outside
the defining module. Use get/set/push/put/clear and owned key snapshots rather
than raw field access. Existing getter signatures and insertion order remain.
`valid()` checks storage shape; checked Text inputs and the collection methods
preserve the UTF-8 of each stored value.

Text indices count Unicode scalars, not grapheme clusters or display cells.
`count` and `codepoint` return `-1` for invalid input; `codepoint` also returns
`-1` outside the scalar range. `part` traps on invalid UTF-8 or an invalid range.
`append_codepoint` returns zero or an errno and preserves the destination when
the scalar is invalid. Text storage is private. `bytes()` returns an independent
owned buffer; edits to that buffer cannot change the Text. Use `append`,
`append_codepoint`, `replace` and `clear` to mutate Text while preserving UTF-8.
Use `text_decode` at untrusted byte boundaries. Direct `Text(buffer)` construction
and `.bytes` field access are no longer public. Replace validated construction
with `text_from_bytes`, whose invalid-input trap is a fatal failure, and field
reads with `.bytes()`. There is no normalization or case folding.

The underlying builtins are `utf8_valid`, `utf8_count`, `utf8_get`, `utf8_slice`
and `utf8_append`, all operating on `buffer`. Byte-oriented `len`, `get` and
`slice` retain their existing meanings.

| Concrete enum | Variants |
| --- | --- |
| `OptionInt`, `OptionText` | `none()`, `some(value)` of the corresponding type |
| `ResultInt`, `ResultText`, `ResultBuffer` | `ok(value)` of the corresponding type, `error(message: Text)` |
| `JsonResult` | `ok(value: JsonDoc)`, `error(message: Text)` |
| `HttpResult` | `ok(value: HttpRequest)`, `error(message: Text)` |

Use an exhaustive `match` to handle these variants. The package also provides
generic `Option[T]`, `Result[T]`, `Box[T]` and `option_or[T]`. `Result[T]` has
`ok(value: T)` and `error(message: Text)` variants; its error type is fixed.
Explicit [generic applications](generics.md) are specialized by the Rust host
before either frontend checks them. Explicit `try` propagation is supported only
as an entire `var` or `let` initializer. The enclosing function must return a
result-shaped enum with the same error payload type. See [Error propagation](errors.md)
for the supported forms and cleanup rules.

## JSON

`json_parse(buffer) -> JsonResult` accepts UTF-8 JSON with null, booleans,
numbers, strings, arrays and objects. It rejects malformed input and trailing
content. Limits are 1 MiB of source, 65,536 nodes and a parser depth guard of
64 below the root. The document owns private bytes and a flat node table. `json_parse` is its public
constructor. Parser mutation methods, json_hex and json_unquote are private.
`document.bytes()` returns an independent owned source copy; mutating it cannot
change the parsed document. Existing count/kind/raw/string/integer/find/item
accessors retain their signatures and behavior.

The root node is index zero. `document.find(node, text(key))` and
`document.item(node, position)` return `OptionInt` node indices. `raw(node)`
returns an owned copy of a value's original JSON bytes. `string(node)` returns
`ResultText` with decoded escapes, and `integer(node)` returns `ResultInt` for
an integer spelling within the signed 64-bit range. Fraction and exponent
spellings are preserved by `raw`; JSON helpers preserve numbers as text even though the language now has `float`.
For duplicate object keys, `find` returns the first occurrence.

`json_quote(Text) -> ResultText` validates and escapes a string as a JSON string
token. There is no automatic record serializer, schema system or generic JSON
value type. Document storage and parser state are private. Use the public
`bytes`, `raw`, `string`, `integer`, `find`, `item`, `kind` and `count` methods;
returned buffers and Text values are independent owned copies.

## HTTP

`http_read(socket, timeout_ms) -> HttpResult` reads one HTTP/1.0 or HTTP/1.1
request. `HttpRequest` contains `method: Text`, `path: Text`,
`headers: StringMap` with lowercase names, and `body: buffer`.

The parser accepts origin-form paths beginning with `/`. It requires CRLF
lines and an HTTP/1.1 Host header, rejects duplicate headers and all
Transfer-Encoding headers, and supports bodies delimited by Content-Length.
Headers are limited to 16 KiB and 100 fields, and a body to 1 MiB. Header values
are ASCII. Targets use ASCII URI path/query characters and valid `%HH` escapes;
Unicode paths must be percent-encoded. Fragments and malformed escapes fail.
Host authorities support DNS labels, strict IPv4, bracketed IPv6 and an optional
decimal port in 0..65535. Zone IDs and IPvFuture are unsupported.
It does not decode URL escapes or parse query
parameters. `http_parse_head(buffer)` exposes the header parser for offline use
and may return only the body prefix already supplied.

`http_read` shares one timeout budget across header and body reads. TCP deadlines
use a monotonic clock; [networking](network.md#deadlines) describes their range
and error semantics. `http_write(socket, status, body: Text, timeout_ms)` returns
`ResultInt`, containing the complete response byte count or an error. A failure
can follow a partial write.

`http_response(status, Text) -> ResultBuffer` constructs a response without
network I/O. Responses use Content-Length, `Connection: close` and
`Content-Type: application/json; charset=utf-8`. The caller supplies JSON text;
the helper validates UTF-8 and the 1 MiB limit, not JSON syntax. Status codes
200, 400, 404, 408, 413 and 431 have explicit reason strings; other codes become
500.

The library handles one request per connection. The caller releases its socket
through its scope. The optional [TLS package](tls.md) provides HTTPS through the
same generic transport helpers; ordinary `std` imports do not require OpenSSL.
Chunking, keepalive, pipelining, WebSockets and event-loop async I/O are not
implemented. The [bounded executor](concurrency.md) runs typed jobs on four
workers with a 64-job queue; submission and awaiting can block. It does not make
these HTTP operations nonblocking. Workers can receive socket handles; copies
retain the same endpoint or serialized TLS session. Callers must coordinate I/O
on those shared connections.

## Run the example

From the repository root, with a built compiler and Clang:

```sh
./target/release/bhasa run examples/http/main.bh -- 8080 3
```

The server prints its bound port, listens on `127.0.0.1`, serves three requests
sequentially, then exits. In another terminal:

```sh
curl -sS http://127.0.0.1:8080/health
curl -sS http://127.0.0.1:8080/counter
curl -sS -H 'Content-Type: application/json' --data '{"hello":"Bhasa"}' http://127.0.0.1:8080/echo
```

The responses are `{"ok":true}`, `{"count":1}` and the supplied JSON object.
Port zero requests an available port; use the printed value in client URLs.
A request count of zero keeps serving and waits for incoming connections.
Request reads and response writes each use two seconds. Client failures end
that request and leave the listener running.

Build once to reuse the executable:

```sh
./target/release/bhasa build examples/http/main.bh --release -o build/http-example
./build/http-example 8080 3
```

The Rust host still handles projects and native linking. Rust adapters retain
their persistent Cargo cache, and unchanged hybrid frontend archives remain
cached. These libraries do not expand GPU portability or Wasm runtime support,
and they do not establish Rust-level memory safety or production HTTP coverage.

See `examples/https/main.bh` and the [TLS guide](tls.md) for the optional HTTPS
example, certificate requirements and TLS session ownership.
