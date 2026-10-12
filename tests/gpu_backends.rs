#![cfg(unix)]
mod common;
use common::*;
use std::path::Path;
use std::process::Command;

const HARNESS: &str = r#"
#include "metal.h"
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void require(int condition, const char *message) {
    if (!condition) { fprintf(stderr, "MOCK PROTOCOL TEST FAILED: %s\n", message); exit(1); }
}
int main(int argc, char **argv) {
    require(argc == 3, "expected mock library and status");
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    require(library != NULL, "mock must load");
    int (*live)(void);
    void *symbol = dlsym(library, "mock_live_count");
    require(symbol != NULL, "mock counter export");
    memcpy(&live, &symbol, sizeof(live));
    int64_t expected_status = strtol(argv[2], NULL, 10);
    int cpu = getenv("BHASA_MOCK_CPU") != NULL;
    require(bhasa_rt_gpu_available() == !cpu, "availability checks hardware class");
    require(live() == 0, "availability must release all resources");
    unsigned char bytes[513];
    for (size_t i = 0; i < sizeof(bytes); ++i) bytes[i] = (unsigned char)i;
    bhasa_buffer data = {513, 513, bytes};
    char entry_text[] = "transform";
    bhasa_buffer entry = {9, 9, (unsigned char *)entry_text};
#ifdef TEST_CUDA
    char source_text[] = "extern \"C\" __global__ void transform(unsigned char *data, unsigned int count) {}";
    bhasa_buffer source = {(int64_t)strlen(source_text), (int64_t)strlen(source_text), (unsigned char *)source_text};
#else
    /* A mock-only SPIR-V envelope. The fake driver does not compile this. */
    uint32_t words[] = {0x07230203, 0x00010000, 0, 100, 0,
        (2u << 16) | 17, 1,
        (6u << 16) | 15, 5, 1, 0x6e617274, 0x726f6673, 0x0000006d,
        (6u << 16) | 16, 1, 17, 1, 1, 1};
    bhasa_buffer source = {sizeof(words), sizeof(words), (unsigned char *)words};
#endif
    require(bhasa_rt_gpu_dispatch(NULL, &entry, &data) == BHASA_GPU_INVALID_ARGUMENT, "null source rejects");
    int64_t status = bhasa_rt_gpu_dispatch(&source, &entry, &data);
    require(status == expected_status, "dispatch status");
    require(live() == 0, "dispatch must release every resource on success and failure");
    for (size_t i = 0; i < sizeof(bytes); ++i) {
        unsigned char expected = status == 0 ? (unsigned char)(((unsigned char)i * 3u + 7u) & 255u) : (unsigned char)i;
        require(bytes[i] == expected, "result or failure preservation");
    }
#ifdef TEST_CUDA
    int (*restored)(void);
    symbol = dlsym(library, "mock_context_restored");
    require(symbol != NULL, "context check export");
    memcpy(&restored, &symbol, sizeof(restored));
    require(restored(), "CUDA caller context restored");
#else
    if (getenv("BHASA_MOCK_PROPERTIES2_MISSING")) {
        int (*attempts)(void);
        symbol = dlsym(library, "mock_device_create_attempts");
        require(symbol != NULL, "device creation counter export");
        memcpy(&attempts, &symbol, sizeof(attempts));
        require(attempts() == 0, "missing instance dependency must prevent invalid device creation");
    }
#endif
    dlclose(library);
    puts("mock host protocol and cleanup verified; no GPU executed");
    return 0;
}
"#;

fn compile_mock_and_harness(
    workspace: &Workspace,
    backend: &str,
) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let clang = clang()?;
    let library = workspace.file(if cfg!(target_os = "macos") {
        "mock.dylib"
    } else {
        "mock.so"
    });
    let mut command = Command::new(&clang);
    command
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fPIC", "-I"])
        .arg(fixture("runtime"))
        .arg(if cfg!(target_os = "macos") {
            "-dynamiclib"
        } else {
            "-shared"
        })
        .arg(fixture(&format!("tests/fixtures/gpu/mock_{backend}.c")))
        .arg("-o")
        .arg(&library);
    succeeds(&command.output().unwrap());
    let harness = workspace.write("harness.c", HARNESS);
    let executable = workspace.file("harness");
    let mut command = Command::new(clang);
    command
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(fixture("runtime"))
        .arg(harness)
        .arg(fixture(&format!("runtime/{backend}.c")))
        .arg("-o")
        .arg(&executable);
    if backend == "cuda" {
        command.arg("-DTEST_CUDA");
    }
    if cfg!(target_os = "linux") {
        command.arg("-ldl");
    }
    succeeds(&command.output().unwrap());
    Some((library, executable))
}

fn mock_command(executable: &Path, library: &Path, expected: i32) -> Command {
    let mut command = Command::new(executable);
    command
        .arg(library)
        .arg(expected.to_string())
        .env("BHASA_CUDA_DRIVER", library)
        .env("BHASA_NVRTC", library)
        .env("BHASA_VULKAN_LIBRARY", library)
        .env_remove("BHASA_MOCK_FAIL")
        .env_remove("BHASA_MOCK_CPU")
        .env_remove("BHASA_MOCK_PORTABILITY")
        .env_remove("BHASA_MOCK_PROPERTIES2_MISSING");
    command
}

#[test]
fn cuda_mock_checks_launch_readback_cleanup_and_context_restoration() {
    let workspace = Workspace::new("cuda-protocol");
    let Some((library, executable)) = compile_mock_and_harness(&workspace, "cuda") else {
        return;
    };
    succeeds(&mock_command(&executable, &library, 0).output().unwrap());
    for (failure, status) in [
        ("compile", 3),
        ("pipeline", 5),
        ("allocation", 7),
        ("launch", 8),
        ("sync", 8),
        ("readback", 8),
    ] {
        let output = mock_command(&executable, &library, status)
            .env("BHASA_MOCK_FAIL", failure)
            .output()
            .unwrap();
        succeeds(&output);
    }
}

#[test]
fn vulkan_mock_checks_dispatch_narrowing_cleanup_and_cpu_device_rejection() {
    let workspace = Workspace::new("vulkan-protocol");
    let Some((library, executable)) = compile_mock_and_harness(&workspace, "vulkan") else {
        return;
    };
    succeeds(&mock_command(&executable, &library, 0).output().unwrap());
    for (failure, status) in [
        ("pipeline", 5),
        ("allocation", 7),
        ("launch", 8),
        ("sync", 8),
        ("range", 8),
    ] {
        succeeds(
            &mock_command(&executable, &library, status)
                .env("BHASA_MOCK_FAIL", failure)
                .output()
                .unwrap(),
        );
    }
    succeeds(
        &mock_command(&executable, &library, 1)
            .env("BHASA_MOCK_CPU", "1")
            .output()
            .unwrap(),
    );
}

#[test]
fn vulkan_portability_subset_enables_its_instance_dependency() {
    let workspace = Workspace::new("vulkan-portability");
    let Some((library, executable)) = compile_mock_and_harness(&workspace, "vulkan") else {
        return;
    };
    succeeds(
        &mock_command(&executable, &library, 0)
            .env("BHASA_MOCK_PORTABILITY", "1")
            .output()
            .unwrap(),
    );
    succeeds(
        &mock_command(&executable, &library, 5)
            .env("BHASA_MOCK_PORTABILITY", "1")
            .env("BHASA_MOCK_PROPERTIES2_MISSING", "1")
            .output()
            .unwrap(),
    );
}

#[test]
fn bhasa_can_select_cuda_and_vulkan_without_installed_sdk() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("gpu-select");
    let source = workspace.write("gpu.bh", "def main() -> int {\n if gpu_available() { return 1 }\n print(\"backend unavailable\")\n return 0\n}\n");
    let absent = workspace.file("missing-driver-library");
    for backend in ["cuda", "vulkan"] {
        let output = workspace
            .command()
            .args(["run", as_str(&source), "--gpu", backend])
            .env("BHASA_CUDA_DRIVER", &absent)
            .env("BHASA_NVRTC", &absent)
            .env("BHASA_VULKAN_LIBRARY", &absent)
            .output()
            .unwrap();
        succeeds(&output);
        assert_eq!(stdout(&output), "backend unavailable\n");
    }
}

fn skip_hardware(backend: &str, reason: &str) {
    let required = std::env::var_os("BHASA_REQUIRE_GPU").is_some()
        || std::env::var_os(format!("BHASA_REQUIRE_{}", backend.to_uppercase())).is_some();
    assert!(
        !required,
        "{backend} hardware validation required: {reason}"
    );
    eprintln!("SKIP {backend} hardware execution: {reason}");
}

fn hardware_dispatch(backend: &str) {
    if clang().is_none() {
        skip_hardware(backend, "Clang is unavailable");
        return;
    }
    let workspace = Workspace::new(&format!("{backend}-hardware"));
    let probe = workspace.write("probe.bh", "def main() { print(gpu_available()) }\n");
    let availability = workspace
        .command()
        .args(["run", as_str(&probe), "--gpu", backend])
        .output()
        .unwrap();
    succeeds(&availability);
    if stdout(&availability) == "false\n" {
        skip_hardware(
            backend,
            "driver/runtime compiler or hardware device unavailable",
        );
        return;
    }
    assert_eq!(stdout(&availability), "true\n");
    let (kernel, entry) = if backend == "cuda" {
        (fixture("examples/gpu/transform.cu"), "transform")
    } else {
        let kernel = workspace.file("transform.spv");
        match Command::new("glslangValidator")
            .args(["-V", "--target-env", "vulkan1.0"])
            .arg(fixture("examples/gpu/transform.comp"))
            .arg("-o")
            .arg(&kernel)
            .output()
        {
            Ok(output) => succeeds(&output),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                skip_hardware(
                    backend,
                    "glslangValidator is needed to compile the test shader",
                );
                return;
            }
            Err(error) => panic!("cannot execute glslangValidator: {error}"),
        }
        (kernel, "main")
    };
    let executable = workspace.file("dispatch");
    for release in [false, true] {
        let mut compiler = workspace.command();
        compiler
            .arg("build")
            .arg(fixture("examples/gpu/dispatch.bh"))
            .args(["--gpu", backend, "-o"])
            .arg(&executable);
        if release {
            compiler.arg("--release");
        }
        succeeds(&compiler.output().unwrap());
        let output = Command::new(&executable)
            .arg(&kernel)
            .arg(entry)
            .output()
            .unwrap();
        succeeds(&output);
        assert_eq!(stdout(&output), "GPU verified 513 bytes\n");
    }
    eprintln!(
        "{backend} hardware execution verified for 513 bytes in development and release modes"
    );
}

#[test]
fn cuda_hardware_dispatch_when_available() {
    hardware_dispatch("cuda");
}

#[test]
fn vulkan_hardware_dispatch_when_available() {
    hardware_dispatch("vulkan");
}
