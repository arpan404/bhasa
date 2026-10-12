# Native tests

Create a `.bh` file under your package's `tests/` folder. Tests are functions
whose names start with `test_`, take no arguments and return `bool`:

```bh
import "../src/math.bh" as math

def test_double() -> bool { return math.double(21) == 42 }
```

Run `bhasa test` from the package. You can also pass a file or directory:

```sh
bhasa test tests/math.bh
bhasa test --filter double --release
bhasa test --json --timeout 5
bhasa test --frontend bhasa
```

The runner compiles one harness per test file and starts a separate native process
for each test. Returning false, trapping, or exceeding the deadline fails that
test; remaining tests still execute. The default deadline is ten seconds.
No test source should define main; imported helpers and private local helpers
work. Generic test functions are unsupported. Helper-only files are skipped.

Exit status is 0 when all selected tests pass, 1 for test failures, and 2 for
usage, discovery or compilation errors. JSON reports include schema_version,
passed, failed, tests and error. Each test contains name, file, status, duration_ms,
stdout, stderr and output_truncated. Completed results remain in the report
if a later file fails to compile. Captured output is limited to 64 KiB per
stream per test (before invalid UTF-8 replacement); compiler messages go to stderr. Tests are compiled with the
ordinary runtime, FFI and cache rules. Pipe collection finishes even when a detached descendant retains a pipe.
Process groups clean ordinary descendants; process isolation is not a sandbox.

Discovery is deterministic, refuses symlinks, and limits file count to 256,
test count to 1024 and directory nesting to 32. `--timeout` accepts 1..3600 seconds.
See examples/testing for a runnable package. Assertion libraries, coverage,
parallel execution and dedicated property-testing support remain future work.
