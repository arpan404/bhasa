#!/usr/bin/env python3
"""File-only compiler/runtime gate. No Cargo, SDK, source mutation or network."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--compiler", type=Path)
parser.add_argument("--frontends", nargs="+", choices=["rust", "bhasa"], default=["rust"])
parser.add_argument("--host-api-probe", type=Path)
parser.add_argument("--native-seed", type=Path)
args = parser.parse_args()
repo, output = args.repo.resolve(strict=True), args.output.resolve()
if bool(args.compiler) == bool(args.host_api_probe):
    parser.error("choose a production compiler or an isolated host API probe")
if args.host_api_probe and not args.native_seed:
    parser.error("isolated paired mode requires --native-seed")
if args.native_seed and not args.host_api_probe:
    parser.error("--native-seed is only for isolated paired mode")
if sys.platform not in ["darwin", "linux"]:
    raise SystemExit("this gate requires the reviewed macOS/Linux POSIX regular-file runtime")
clang = shutil.which(os.environ.get("BHASA_CLANG", "clang"))
if not clang:
    raise SystemExit("Clang is required; this gate never installs a toolchain")
output.mkdir(parents=True, exist_ok=True)
records = []
artifacts = {}

def run(argv, expected=0, timeout=40, **kwargs):
    argv = list(map(str, argv))
    env = {**os.environ, "ASAN_OPTIONS": "detect_leaks=0", "UBSAN_OPTIONS": "halt_on_error=1"}
    result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, env=env, **kwargs)
    records.append({"argv": argv, "status": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
    (output / "verification.log").write_text("\n".join("$ " + " ".join(r["argv"]) + "\n" + r["stdout"] + r["stderr"] for r in records))
    if expected == 0:
        assert result.returncode == 0, records[-1]
    else:
        assert result.returncode != 0, records[-1]
    return result

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

# Ask the installed compiler for its actual host triple, rather than guessing.
probe = run([clang, "-S", "-emit-llvm", "-x", "c", "-", "-o", "-"], input="int target_probe;\n")
target = re.search(r'target triple = "([^"]+)"', probe.stdout).group(1)
frontends = ["rust", "native-seed"] if args.host_api_probe else args.frontends
compiler = args.compiler.resolve(strict=True) if args.compiler else None
host = args.host_api_probe.resolve(strict=True) if args.host_api_probe else None
seed = args.native_seed.resolve(strict=True) if args.native_seed else None
for label, path in [("compiler", compiler), ("host_api_probe", host), ("native_seed", seed)]:
    if path: artifacts[label] = {"path": str(path), "sha256": digest(path)}

flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic", "-pthread",
         "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer"]
trace_header = repo / "tests/file_stream_trace.h"
objects = {}
for opt in ["O0", "O2"]:
    # The existing reviewed injected-failure/native-lifetime fixture stays separate.
    unit = output / ("file-runtime-" + opt)
    run([clang, *flags, "-" + opt, "-I", repo / "runtime", repo / "tests/file_stream_runtime.c", "-o", unit])
    assert run([unit]).stdout == "file stream native tests passed\n"
    paths = []
    for source, label in [(repo / "runtime/core.c", "core"), (repo / "runtime/text.c", "text"),
                          (repo / "tests/file_stream_counted_runtime.c", "file")]:
        obj = output / (label + "-" + opt + ".o")
        run([clang, *flags, "-" + opt, "-I", repo / "runtime", "-include", trace_header,
             "-c", source, "-o", obj])
        paths.append(obj)
    objects[opt] = paths

outcomes = []
for name, truth_count in [("file_stream_typed", 9), ("file_stream_cleanup", 8)]:
    project = output / name
    project.mkdir(exist_ok=True)
    for library in ["io.bh", "text.bh"]:
        shutil.copyfile(repo / "std" / library, project / library)
    source = project / "main.bh"
    shutil.copyfile(repo / "tests" / (name + ".bh"), source)
    if host:
        run([host, "check", source])
        lowered = project / "native.bh"
        lowered.write_text(run([host, "native-source", source]).stdout)
    for frontend in frontends:
        ir = project / (frontend + ".ll")
        if frontend == "native-seed":
            # Host loader/privacy/generic lowering above is explicit shared evidence.
            # This frozen seed checks/emits the concrete lowered source itself.
            run([seed, "check", lowered])
            ir.write_text(run([seed, "emit-llvm", lowered, "--target", target]).stdout)
        elif host:
            ir.write_text(run([host, "emit-llvm", source, target]).stdout)
        else:
            run([compiler, "check", source, "--frontend", frontend])
            # Production CLI accepts documented aliases, while standalone seeds
            # accept resolved LLVM triples. Keep those distinct contracts.
            run([compiler, "emit-llvm", source, "--frontend", frontend, "--target", "native", "-o", ir])
        text = ir.read_text()
        assert "; bhasa runtime: file\n" in text
        text = text.replace("@main(", "@stream_guest_main(").replace("@malloc(", "@stream_trace_malloc(").replace("@realloc(", "@stream_trace_realloc(").replace("@free(", "@stream_trace_free(")
        text = "\n".join(line[:-2] + " sanitize_address {" if line.startswith("define ") and line.endswith(" {") and "sanitize_address" not in line else line for line in text.splitlines()) + "\n"
        ir.write_text(text)
        for opt in ["O0", "O2"]:
            binary = project / (frontend + "-" + opt)
            run([clang, *flags, "-Wno-override-module", "-" + opt, ir,
                 repo / "tests/file_stream_trace.c", *objects[opt], "-o", binary])
            data = project / (frontend + "-" + opt + ".data")
            data.write_bytes(b"")
            missing = project / (frontend + "-" + opt + ".missing")
            assert not missing.exists(), "choose a fresh output directory for a genuinely missing-path case"
            result = run([binary, data, missing], timeout=20)
            lines = result.stdout.splitlines()
            assert lines[:-1] == ["true"] * truth_count, records[-1]
            assert result.stderr == "", records[-1]
            counts = re.fullmatch(r"FILE COUNTERS opened=(\d+) retained=(\d+) released=(\d+) closed=(\d+) allocations=0 descriptors=0", lines[-1])
            assert counts, records[-1]
            opened, retained, released, closed = map(int, counts.groups())
            assert opened + retained == released and closed > 0
            outcomes.append({"fixture": name, "frontend": frontend, "optimization": opt,
                             "assertions": truth_count, "opened": opened, "retained": retained,
                             "released": released, "closed": closed, "live_allocations": 0, "live_descriptors": 0,
                             "binary_sha256": digest(binary), "llvm_sha256": digest(ir)})

# Concrete standalone guards establish native diagnostics without laundering an
# imported privacy rejection through the host preparation step.
guards = [
    ("ignored_open", 'def main(){file_open("unused",0)}', "S022", "BH207"),
    ("owned_file", 'def use(owned file:file_handle){}def main(){}', "S038", "BH222"),
    ("move_file", 'def main(){var file=file_open("unused",0);var other=move file}', "S038", "BH222"),
    ("pending_file_result", 'enum Opened{ok(file:file_handle);error(code:int)}def opened()->Opened{return Opened.ok(file_open("unused",0))}def main(){var value=opened()}', "S037", "BH221"),
    ("worker_file", 'def helper(n:int)->int{var file=file_open("unused",0);return file_status(file)}def worker(n:int)->int{return helper(n)}def main(){var job=spawn(worker(1));var result=join(job)}', "S020", "BH209"),
    ("pooled_file", 'def worker(file:file_handle)->int{return 1}def main(){var file=file_open("unused",0);var job=async_spawn(worker(file));var result=await(job)}', "S020", "BH209"),
]
for name, source_text, rust_code, native_code in guards:
    source = output / ("guard-" + name + ".bh")
    source.write_text(source_text)
    for frontend in frontends:
        command = [seed, "check", source] if frontend == "native-seed" else [host, "check", source] if host else [compiler, "check", source, "--frontend", frontend]
        result = run(command, expected=1)
        combined = result.stdout + result.stderr
        # Production hybrid checking can report the shared-host diagnostic first.
        allowed = [native_code] if frontend == "native-seed" else [rust_code, native_code] if frontend == "bhasa" else [rust_code]
        assert any(code in combined for code in allowed), records[-1]

for relative in ["std/io.bh", "std/text.bh", "runtime/core.c", "runtime/core.h", "runtime/text.c", "runtime/text.h", "runtime/file.c", "runtime/file.h"]:
    artifacts[relative] = {"sha256": digest(repo / relative)}
(output / "results.json").write_text(json.dumps({"target": target, "outcomes": outcomes, "source_guards": len(guards) * len(frontends), "artifacts": artifacts,
    "limits": ["ASan generated LLVM and C runtime; UBSan C compilation only", "single-thread file operations", "host module/privacy/generic lowering precedes frozen native seed", "explicit counted allocation/regular-file descriptor balance; no LeakSanitizer claim"]}, indent=2) + "\n")
print("PASS: file stream runtime, typed ownership/try/cursor/close, 8 paired guests and 12 concrete guards" if host else "PASS: file stream runtime and selected production frontend gates")
