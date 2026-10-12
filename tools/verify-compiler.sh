#!/bin/sh
# Offline verification. Cargo and the hybrid builder keep their existing caches.
set -eu

verify_python=${PYTHON:-python3}
if ! command -v "$verify_python" >/dev/null 2>&1; then
    printf '%s\n' "Python 3 is required for process timeouts: $verify_python" >&2
    exit 127
fi
exec "$verify_python" - "$0" "$@" <<'PYTHON'
import argparse
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys


parser = argparse.ArgumentParser(
    prog="sh tools/verify-compiler.sh",
    description="Run offline compiler tests with a timeout for each selected mode.",
    epilog="""Examples:
  sh tools/verify-compiler.sh
  sh tools/verify-compiler.sh --hybrid-frontend
  sh tools/verify-compiler.sh --reference --hybrid-lexer --timeout 1200
  sh tools/verify-compiler.sh --all

Requires Python 3, Cargo, rustc, and Clang on macOS or Linux. BHASA_CLANG
selects a Clang executable; PYTHON selects Python 3. Existing Cargo target
directories and hybrid archives are reused. No cache is cleaned. Cargo is
offline and locked; missing cached dependencies fail the selected check.

The default runs the Rust reference suite. Explicit mode flags select only
those modes. Each hybrid builder already runs its whole suite, including
feature-gated tests, so this script does not repeat that suite afterward.

Tests print hardware results and SKIP reasons. CUDA/Vulkan mock protocol
tests do not establish hardware execution. --require-gpu forbids GPU hardware
skips for every backend tested on this host: Metal on macOS, and CUDA/Vulkan
on macOS/Linux. BHASA_REQUIRE_CUDA=1 and BHASA_REQUIRE_VULKAN=1 can require
one backend instead. Unavailable optional target backends may still skip.
Passing finite tests does not establish production safety or full parity.
""",
    formatter_class=argparse.RawDescriptionHelpFormatter,
)
parser.add_argument("--reference", action="store_true", help="run the Rust reference suite")
parser.add_argument("--hybrid-lexer", action="store_true", help="build and test the Bhasa lexer host")
parser.add_argument("--hybrid-frontend", action="store_true", help="build and test the full Bhasa frontend host")
parser.add_argument("--all", action="store_true", help="run reference, hybrid lexer, and hybrid frontend")
parser.add_argument("--timeout", type=int, default=900, metavar="SECONDS",
                    help="time limit per selected mode, including builds (default: 900)")
parser.add_argument("--require-gpu", action="store_true", help="fail when any applicable GPU hardware test skips")
args = parser.parse_args(sys.argv[2:])
if not 1 <= args.timeout <= 86400:
    parser.error("--timeout must be an integer from 1 to 86400")
if sys.platform not in ("darwin", "linux"):
    parser.error("this verification entrypoint requires macOS or Linux")

repo = Path(sys.argv[1]).resolve().parent.parent
os.chdir(repo)
environment = os.environ.copy()
environment["CARGO_NET_OFFLINE"] = "true"
# The hybrid builder runs cargo test itself. Keep successful tests' SKIP and
# actual-hardware messages visible without invoking its suite a second time.
environment["RUST_TEST_NOCAPTURE"] = "1"
if args.require_gpu:
    environment["BHASA_REQUIRE_GPU"] = "1"


class Interrupted(BaseException):
    def __init__(self, signum):
        self.signum = signum


def interrupt(signum, _frame):
    raise Interrupted(signum)


signal.signal(signal.SIGINT, interrupt)
signal.signal(signal.SIGTERM, interrupt)


def stop_group(process):
    # Cargo, test binaries, generated programs, and their children share this
    # new process group. Killing only Cargo could leave a hung test running.
    for signum in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, signum)
        except ProcessLookupError:
            pass
        if signum == signal.SIGTERM:
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
    process.wait()


def run(label, command, timeout):
    print("\nVERIFY " + label, flush=True)
    print("$ " + " ".join(shlex.quote(str(part)) for part in command), flush=True)
    try:
        process = subprocess.Popen(command, env=environment, start_new_session=True)
    except OSError as error:
        print("FAIL " + label + ": " + str(error), file=sys.stderr, flush=True)
        raise SystemExit(127)
    try:
        status = process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        stop_group(process)
        print(f"TIMEOUT {label} after {timeout}s; terminated its process group",
              file=sys.stderr, flush=True)
        raise SystemExit(124)
    except Interrupted as error:
        stop_group(process)
        print("INTERRUPTED " + label + "; terminated its process group",
              file=sys.stderr, flush=True)
        raise SystemExit(128 + error.signum)
    if status:
        print(f"FAIL {label}: command exit status {status}", file=sys.stderr, flush=True)
        raise SystemExit(status if status > 0 else 128 - status)
    print("PASS " + label, flush=True)


selected = []
if args.all or args.reference or not (args.hybrid_lexer or args.hybrid_frontend):
    selected.append(("Rust reference suite", [
        "cargo", "test", "--offline", "--locked", "--no-default-features", "--", "--nocapture",
    ]))
if args.all or args.hybrid_lexer:
    selected.append(("hybrid lexer build and suite", ["sh", "bootstrap/build-hybrid.sh"]))
if args.all or args.hybrid_frontend:
    selected.append(("hybrid frontend build and suite", ["sh", "bootstrap/build-hybrid.sh", "--frontend"]))

print("Repository: " + str(repo), flush=True)
print(f"Each selected mode has a {args.timeout}s limit; timeout exits with status 124.", flush=True)
print("Hardware skips remain visible; mock driver tests are not hardware evidence.", flush=True)
requirements = [name for name in ("BHASA_REQUIRE_GPU", "BHASA_REQUIRE_CUDA", "BHASA_REQUIRE_VULKAN")
                if name in environment]
if requirements:
    print("Required hardware settings: " + ", ".join(requirements), flush=True)
else:
    print("GPU hardware is optional; only explicit execution messages confirm it ran.", flush=True)

try:
    for name, command in (
        ("Cargo prerequisite", ["cargo", "--version"]),
        ("Rust prerequisite", ["rustc", "--version"]),
        ("Clang prerequisite", [environment.get("BHASA_CLANG", "clang"), "--version"]),
    ):
        run(name, command, min(args.timeout, 30))
    for label, command in selected:
        run(label, command, args.timeout)
except Interrupted as error:
    raise SystemExit(128 + error.signum)

print("\nSelected compiler checks passed. Review any SKIP lines for unverified hardware or targets.", flush=True)
PYTHON
