# Backend development milestone

The user approved this bounded milestone on October 11, 2026. The preceding
methods milestone is complete; its contract is in [methods-milestone.md](methods-milestone.md).

Deliver enums and exhaustive match in both frontends, concrete optional/result
types, UTF-8 text, typed string collections, strict bounded JSON, network
deadlines and a working Bhasa HTTP service. Preserve `.bh`, exactly lowercase
`bhasa.toml`, deterministic ownership, the Rust host and persistent native/Cargo
caches. Do not install SDKs, clean caches, commit, reset or publish.

## Contracts

```bhasa
enum OptionInt { none; some(value: int) }
def value_or_zero(option: OptionInt) -> int {
    match option {
        case OptionInt.none { return 0 }
        case OptionInt.some(value) { return value }
    }
}
```

Constructors require parentheses. Match statements bind all payloads and cover
each variant exactly once. Evaluate the scrutinee once and copy its active
payload into case-local bindings. Cloning/freeing touches only the active
payload. Recursive schemas and task payloads are rejected. Methods and match
bodies remain part of transitive worker-to-extern call graph checks. Enum ABI
layouts are private and cannot cross the scalar C ABI.

`std` is a local package with concrete OptionInt, OptionText, ResultInt,
ResultText and ResultBuffer types. Text wraps an owned buffer with strict Unicode
scalar validation/count/index/slice/append. StringList, StringIntMap and StringMap
use owned copies, checked indexing and insertion-order linear lookup. Generic
types, hashing and Unicode normalization are outside this milestone.

JSON validates Unicode, escapes, surrogate pairs and number syntax. It retains
number spelling, checks int64 conversions, and limits input to 1 MiB, nesting to
64 and nodes to 65536. HTTP supports one HTTP/1.0 or HTTP/1.1 request per connection,
16 KiB headers, 100 fields and 1 MiB bodies. Reject ambiguous Content-Length,
Transfer-Encoding, invalid host authorities and malformed URI targets. Response
length counts bytes. A failed client must not terminate the example listener.
TLS, chunking, keepalive, HTTP/2 and asynchronous execution are outside this slice.

Timed TCP calls share monotonic absolute deadlines across retries; zero means an
immediate nonblocking attempt. Timeouts are 0..2147483647 milliseconds. Timed
connect uses numeric IPv4/IPv6 so name resolution cannot exceed a deadline.
Legacy calls retain synchronous behavior. Runtime descriptors stay nonblocking,
including clones, with poll providing waits. Tests bind only loopback/ephemeral
ports.

## Ownership and acceptance

| Lane | Owned implementation |
| --- | --- |
| frontend_parity | Rust enum AST/parser/checker/LLVM and enum tests; independent review and HTTP grammar repair |
| rust_cache | Bhasa enum lexer/emitter/driver/rebuild gate and selfhost tests; user docs after source freeze |
| packages | Native UTF-8/TCP deadlines, Text/collections, focused runtime tests |
| integration | Builtin/driver linkage, scanner mapping, LSP/formatter/editor grammar, Result/JSON/HTTP, service, combined tests and status |

Use focused checks while sources evolve. Root serializes complete reference and
hybrid suites after source freeze, with two Cargo build jobs and one test thread.
Acceptance requires both frontends executing the same library/service cases;
owned payload/text/map/JSON cleanup at O0/O2 under sanitizers; malformed input
rejection; live request deadlines and client-disconnect recovery; UTF-16 enum
editor navigation; refreshed release binaries; A/B/C self-rebuild; unchanged
native and Rust adapter reuse. Report hardware skips and experimental limits.

The old methods heartbeat stays paused. Internal agents perform this approved
work in the current turn; do not restart the six completed chat assignments.
