//! The bootstrap's explicit target set and native target resolution.
//!
//! These are code generation candidates. Their presence does not imply that
//! Clang, a linker, or the target's libraries and SDK are installed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Native,
    LinuxX64,
    LinuxArm64,
    WindowsX64,
    MacosArm64,
    MacosX64,
    Wasm32,
}

impl Target {
    /// Accept only documented aliases and their exact LLVM triples.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "native" => Ok(Self::Native),
            "linux-x64" | "x86_64-unknown-linux-gnu" => Ok(Self::LinuxX64),
            "linux-arm64" | "aarch64-unknown-linux-gnu" => Ok(Self::LinuxArm64),
            "windows-x64" | "x86_64-pc-windows-msvc" => Ok(Self::WindowsX64),
            "macos-arm64" | "arm64-apple-macosx" => Ok(Self::MacosArm64),
            "macos-x64" | "x86_64-apple-macosx" => Ok(Self::MacosX64),
            "wasm32" | "wasm32-unknown-unknown" => Ok(Self::Wasm32),
            _ => Err(format!(
                "unsupported target {value:?}; run `bhasa targets` for the supported aliases and exact triples"
            )),
        }
    }

    /// An explicit target remains explicit even when it matches the host.
    pub fn is_native(&self) -> bool {
        *self == Self::Native
    }

    pub fn alias(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::WindowsX64 => "windows-x64",
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX64 => "macos-x64",
            Self::Wasm32 => "wasm32",
        }
    }

    /// Resolve IR emission without invoking Clang. Native builds instead use
    /// the configured Clang frontend's resolved LLVM target.
    pub fn triple_for_ir(&self) -> Result<String, String> {
        let triple = match self {
            Self::Native => native_triple(
                std::env::consts::ARCH,
                std::env::consts::OS,
                host_environment(),
            )?,
            Self::LinuxX64 => "x86_64-unknown-linux-gnu",
            Self::LinuxArm64 => "aarch64-unknown-linux-gnu",
            Self::WindowsX64 => "x86_64-pc-windows-msvc",
            Self::MacosArm64 => "arm64-apple-macosx",
            Self::MacosX64 => "x86_64-apple-macosx",
            Self::Wasm32 => "wasm32-unknown-unknown",
        };
        Ok(triple.to_owned())
    }

    /// Return an extension without a leading dot. An empty extension denotes
    /// the usual Unix executable name. Inspect the resolved triple so native
    /// Clang variants such as `x86_64-w64-windows-gnu` get the correct suffix.
    pub fn output_extension(&self, object: bool, resolved_triple: &str) -> &'static str {
        let windows = resolved_triple
            .split('-')
            .any(|part| part == "windows" || part.starts_with("mingw"));
        if object {
            if windows {
                "obj"
            } else {
                "o"
            }
        } else if windows {
            "exe"
        } else if resolved_triple.starts_with("wasm32-") || resolved_triple.starts_with("wasm64-") {
            "wasm"
        } else {
            ""
        }
    }
}

fn host_environment() -> &'static str {
    if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else {
        ""
    }
}

fn native_triple(arch: &str, os: &str, environment: &str) -> Result<&'static str, String> {
    match (arch, os, environment) {
        ("x86_64", "linux", "gnu") => Ok("x86_64-unknown-linux-gnu"),
        ("aarch64", "linux", "gnu") => Ok("aarch64-unknown-linux-gnu"),
        ("x86_64", "linux", "musl") => Ok("x86_64-unknown-linux-musl"),
        ("aarch64", "linux", "musl") => Ok("aarch64-unknown-linux-musl"),
        ("x86_64", "macos", "") => Ok("x86_64-apple-macosx"),
        ("aarch64", "macos", "") => Ok("arm64-apple-macosx"),
        ("x86_64", "windows", "msvc") => Ok("x86_64-pc-windows-msvc"),
        ("aarch64", "windows", "msvc") => Ok("aarch64-pc-windows-msvc"),
        ("x86_64", "windows", "gnu") => Ok("x86_64-w64-windows-gnu"),
        _ => Err(format!(
            "native IR target is not mapped for host {arch}-{os}-{environment}; pass an explicit --target from `bhasa targets`"
        )),
    }
}

/// Reject a configured Clang's cross target before `run` executes its output.
/// Matching architecture and OS does not certify installed runtime libraries.
pub fn host_can_run(triple: &str) -> bool {
    host_matches(triple, std::env::consts::ARCH, std::env::consts::OS)
}

fn host_matches(triple: &str, host_arch: &str, host_os: &str) -> bool {
    let mut parts = triple.split('-');
    let arch = match parts.next() {
        Some("arm64" | "aarch64") => "aarch64",
        Some("x86_64") => "x86_64",
        _ => return false,
    };
    if arch != host_arch {
        return false;
    }

    let mut target_os = None;
    while let Some(part) = parts.next() {
        let os = if part == "linux" {
            // Android and ABIs such as gnux32 also name Linux in the OS
            // field. Only the default ABI, GNU, and musl are accepted here.
            if !matches!(parts.next(), None | Some("gnu" | "musl")) || parts.next().is_some() {
                return false;
            }
            "linux"
        } else if part == "windows" || part == "mingw32" {
            "windows"
        } else if versioned_os(part, "darwin")
            || versioned_os(part, "macosx")
            || versioned_os(part, "macos")
        {
            "macos"
        } else {
            continue;
        };
        // Malformed triples naming multiple OSes must not pass this guard.
        if target_os.is_some() {
            return false;
        }
        target_os = Some(os);
    }
    target_os == Some(host_os)
}

fn versioned_os(value: &str, os: &str) -> bool {
    value.strip_prefix(os).is_some_and(|suffix| {
        suffix.is_empty()
            || (suffix.starts_with(|c: char| c.is_ascii_digit())
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.'))
    })
}

pub fn listing() -> String {
    "Target aliases and LLVM triples:\n\
     native         Rust host target for IR; Clang target probe for builds\n\
     linux-x64      x86_64-unknown-linux-gnu\n\
     linux-arm64    aarch64-unknown-linux-gnu\n\
     windows-x64    x86_64-pc-windows-msvc\n\
     macos-arm64    arm64-apple-macosx\n\
     macos-x64      x86_64-apple-macosx\n\
     wasm32         wasm32-unknown-unknown\n\n\
     The exact triples above are also accepted. These are candidate code generation\n\
     targets; installed Clang backend support is required for object emission.\n\
     Linked executables also need target libraries, an SDK where applicable, and a\n\
     compatible linker. This list does not certify that those tools are installed.\n\
     Cross-target executables are not run by Bhasa.\n\
     wasm32 initially supports objects for pure functions; print, host imports,\n\
     and runnable WASI are not promised.\n"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPLICIT_TARGETS: &[(&str, &str)] = &[
        ("linux-x64", "x86_64-unknown-linux-gnu"),
        ("linux-arm64", "aarch64-unknown-linux-gnu"),
        ("windows-x64", "x86_64-pc-windows-msvc"),
        ("macos-arm64", "arm64-apple-macosx"),
        ("macos-x64", "x86_64-apple-macosx"),
        ("wasm32", "wasm32-unknown-unknown"),
    ];

    #[test]
    fn aliases_and_exact_triples_resolve_identically() {
        for &(alias, triple) in EXPLICIT_TARGETS {
            let target = Target::parse(alias).unwrap();
            assert_eq!(target, Target::parse(triple).unwrap());
            assert_eq!(target.alias(), alias);
            assert_eq!(target.triple_for_ir().unwrap(), triple);
            assert!(!target.is_native());
        }
        assert!(Target::parse("native").unwrap().is_native());
        assert_eq!(Target::Native.alias(), "native");
    }

    #[test]
    fn unknown_targets_are_rejected_without_normalization() {
        for value in [
            "",
            "Native",
            "linux",
            " linux-x64",
            "linux-x64 ",
            "x86_64-unknown-linux-musl",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-gnu",
            "wasm32-wasi",
            "--target=linux-x64",
        ] {
            let message = Target::parse(value).unwrap_err();
            assert!(message.contains("unsupported target"));
            assert!(message.contains("bhasa targets"));
        }
    }

    #[test]
    fn native_host_mapping_preserves_architecture_and_abi() {
        for (arch, os, environment, expected) in [
            ("x86_64", "linux", "gnu", "x86_64-unknown-linux-gnu"),
            ("aarch64", "linux", "gnu", "aarch64-unknown-linux-gnu"),
            ("x86_64", "linux", "musl", "x86_64-unknown-linux-musl"),
            ("aarch64", "linux", "musl", "aarch64-unknown-linux-musl"),
            ("x86_64", "macos", "", "x86_64-apple-macosx"),
            ("aarch64", "macos", "", "arm64-apple-macosx"),
            ("x86_64", "windows", "msvc", "x86_64-pc-windows-msvc"),
            ("aarch64", "windows", "msvc", "aarch64-pc-windows-msvc"),
            ("x86_64", "windows", "gnu", "x86_64-w64-windows-gnu"),
        ] {
            assert_eq!(native_triple(arch, os, environment).unwrap(), expected);
        }
    }

    #[test]
    fn unmapped_hosts_fail_without_a_fallback_target() {
        for (arch, os, environment) in [
            ("riscv64", "linux", "gnu"),
            ("x86_64", "freebsd", ""),
            ("aarch64", "windows", "gnu"),
            ("x86_64", "linux", ""),
        ] {
            let error = native_triple(arch, os, environment).unwrap_err();
            assert!(error.contains("pass an explicit --target"));
            assert!(error.contains(arch));
            assert!(error.contains(os));
        }
    }

    #[test]
    fn native_ir_resolution_uses_current_rust_host() {
        let expected = native_triple(
            std::env::consts::ARCH,
            std::env::consts::OS,
            host_environment(),
        )
        .map(str::to_owned);
        assert_eq!(Target::Native.triple_for_ir(), expected);
    }

    #[test]
    fn extensions_follow_resolved_triple_including_clang_native_variants() {
        for triple in [
            "x86_64-pc-windows-msvc",
            "x86_64-w64-windows-gnu",
            "x86_64-w64-mingw32",
            "aarch64-pc-windows-msvc",
        ] {
            assert_eq!(Target::Native.output_extension(true, triple), "obj");
            assert_eq!(Target::Native.output_extension(false, triple), "exe");
        }
        for triple in [
            "arm64-apple-darwin25.0.0",
            "x86_64-apple-macosx",
            "aarch64-unknown-linux-gnu",
        ] {
            assert_eq!(Target::Native.output_extension(true, triple), "o");
            assert_eq!(Target::Native.output_extension(false, triple), "");
        }
        assert_eq!(
            Target::Wasm32.output_extension(true, "wasm32-unknown-unknown"),
            "o"
        );
        assert_eq!(
            Target::Wasm32.output_extension(false, "wasm32-unknown-unknown"),
            "wasm"
        );
    }

    #[test]
    fn run_guard_accepts_matching_native_clang_variants() {
        for (triple, arch, os) in [
            ("arm64-apple-darwin25.2.0", "aarch64", "macos"),
            ("aarch64-apple-macosx14.0.0", "aarch64", "macos"),
            ("x86_64-apple-macosx", "x86_64", "macos"),
            ("x86_64-pc-linux-gnu", "x86_64", "linux"),
            ("aarch64-unknown-linux-musl", "aarch64", "linux"),
            ("x86_64-pc-windows-msvc19.33", "x86_64", "windows"),
            ("x86_64-w64-mingw32", "x86_64", "windows"),
        ] {
            assert!(host_matches(triple, arch, os), "{triple}");
        }
    }

    #[test]
    fn run_guard_rejects_foreign_architectures_systems_and_malformed_triples() {
        for (triple, arch, os) in [
            ("x86_64-apple-darwin25.2.0", "aarch64", "macos"),
            ("arm64-apple-ios18.0.0", "aarch64", "macos"),
            ("arm64-apple-darwin-invalid", "x86_64", "macos"),
            ("arm64-apple-darwininvalid", "aarch64", "macos"),
            ("aarch64-unknown-linux-gnu", "aarch64", "macos"),
            ("x86_64-pc-windows-msvc", "x86_64", "linux"),
            ("x86_64-unknown-linux-windows", "x86_64", "linux"),
            ("wasm32-unknown-unknown", "aarch64", "macos"),
            ("", "x86_64", "linux"),
            ("x86_64", "x86_64", "linux"),
        ] {
            assert!(!host_matches(triple, arch, os), "{triple}");
        }
    }

    #[test]
    fn linux_run_guard_accepts_conventional_native_triples() {
        for arch in ["x86_64", "aarch64"] {
            for suffix in [
                "unknown-linux-gnu",
                "pc-linux-gnu",
                "linux-gnu",
                "unknown-linux-musl",
                "alpine-linux-musl",
                "linux-musl",
                "unknown-linux",
                "linux",
            ] {
                let triple = format!("{arch}-{suffix}");
                assert!(host_matches(&triple, arch, "linux"), "{triple}");
            }
        }
    }

    #[test]
    fn linux_run_guard_rejects_android_and_unsupported_abis() {
        for arch in ["x86_64", "aarch64"] {
            for suffix in [
                "linux-android",
                "unknown-linux-android24",
                "unknown-linux-androideabi",
                "unknown-linux-gnux32",
                "unknown-linux-gnu_ilp32",
                "unknown-linux-gnueabihf",
                "unknown-linux-musleabihf",
                "unknown-linux-unknown",
                "unknown-linux-gnu-android",
                "unknown-linux-",
            ] {
                let triple = format!("{arch}-{suffix}");
                assert!(!host_matches(&triple, arch, "linux"), "{triple}");
            }
        }
    }

    #[test]
    fn run_guard_matches_supported_native_ir_host() {
        if let Ok(triple) = Target::Native.triple_for_ir() {
            assert!(host_can_run(&triple));
        }
    }

    #[test]
    fn listing_includes_targets_and_linking_limitations() {
        let output = listing();
        assert!(output.contains("native"));
        for &(alias, triple) in EXPLICIT_TARGETS {
            assert!(output.contains(alias));
            assert!(output.contains(triple));
        }
        assert!(output.contains("object emission"));
        assert!(output.contains("Linked executables"));
        assert!(output.contains("does not certify"));
        assert!(output.contains("pure functions"));
        assert!(output.contains("runnable WASI are not promised"));
    }
}
