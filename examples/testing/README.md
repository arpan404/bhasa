# Native tests

From the repository root:

```sh
./target/release/bhasa test examples/testing/tests --json
```

Or run `bhasa test` inside this package. The five tests cover imported arithmetic, range control, nested breaks and List
snapshots. Their imports use the
same namespace and package-root rules as application code. See docs/testing.md
for filters, deadlines, reports and failure behavior.
