//! Cargo owns Rust dependency freshness. Bhasa only chooses persistent output
//! directories and reads the actual static-library artifacts Cargo reports.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default)]
pub struct RustBuild {
    pub archives: Vec<PathBuf>,
    /// True only when Cargo reports every requested static library fresh.
    pub fresh: bool,
    pub diagnostics: String,
    pub protected_roots: Vec<PathBuf>,
    pub link_args: Vec<String>,
}

/// Build local Cargo static-library adapters declared by a Bhasa manifest.
/// `target` is a resolved LLVM triple, or None for Cargo's native host target.
/// Cargo runs offline but may create or update Cargo.lock when manifests change.
pub fn build(
    manifest_path: &Path,
    dependencies: &BTreeMap<String, String>,
    target: Option<&str>,
    release: bool,
    cache_dir: &Path,
) -> Result<RustBuild, String> {
    let mut result = RustBuild {
        fresh: true,
        ..RustBuild::default()
    };
    if dependencies.is_empty() {
        return Ok(result);
    }
    let target = target.map(rust_target).transpose()?;
    let manifest_path = fs::canonicalize(manifest_path)
        .map_err(|error| format!("cannot read '{}': {error}", manifest_path.display()))?;
    let package_root = manifest_path
        .parent()
        .ok_or("manifest has no parent directory")?;
    fs::create_dir_all(cache_dir).map_err(|error| {
        format!(
            "cannot create Rust cache '{}': {error}",
            cache_dir.display()
        )
    })?;
    let cache_dir = fs::canonicalize(cache_dir).map_err(|error| {
        format!(
            "cannot resolve Rust cache '{}': {error}",
            cache_dir.display()
        )
    })?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut built_manifests = BTreeSet::new();

    for (name, dependency) in dependencies {
        let requested = package_root.join(dependency);
        let requested = if requested.is_dir() {
            requested.join("Cargo.toml")
        } else {
            requested
        };
        let cargo_manifest = fs::canonicalize(&requested).map_err(|error| {
            format!(
                "Rust dependency {name:?}: cannot read '{}': {error}",
                requested.display()
            )
        })?;
        if !cargo_manifest.is_file()
            || cargo_manifest
                .file_name()
                .is_none_or(|file| file != "Cargo.toml")
        {
            return Err(format!("Rust dependency {name:?} must name a directory containing Cargo.toml or a Cargo.toml file"));
        }
        if !built_manifests.insert(cargo_manifest.clone()) {
            continue;
        }
        let crate_root = cargo_manifest
            .parent()
            .ok_or("Cargo manifest has no parent")?;
        // Separate adapter roots avoid collisions between two crates exporting
        // the same library filename. Cargo separates targets and profiles below.
        let cache = cache_dir.join("rust").join(cache_key(&cargo_manifest));
        fs::create_dir_all(&cache)
            .map_err(|error| format!("cannot create '{}': {error}", cache.display()))?;
        check_cache_identity(&cache, &cargo_manifest)?;
        let mut command = Command::new(&cargo);
        command
            .current_dir(crate_root)
            .arg("build")
            .arg("--offline")
            .arg("--lib")
            .arg("--manifest-path")
            .arg(&cargo_manifest)
            .arg("--target-dir")
            .arg(cache.join("target"))
            .arg("--message-format=json-render-diagnostics");
        if let Some(target) = &target {
            command.arg("--target").arg(target);
        }
        if release {
            command.arg("--release");
        }
        let output = command.output().map_err(|error| {
            format!("Rust dependency {name:?}: cannot start Cargo: {error}. Install Cargo to build Rust adapters; an already linked executable does not need Cargo.")
        })?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(format!("Rust dependency {name:?}: Cargo failed ({}). Dependencies must already be available locally; Bhasa does not download crates.\n{stderr}", output.status));
        }
        result.diagnostics.push_str(&stderr);
        let stdout = std::str::from_utf8(&output.stdout)
            .map_err(|_| format!("Rust dependency {name:?}: Cargo emitted non-UTF-8 JSON"))?;
        let mut found = false;
        for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
            let message = Json::parse(line).map_err(|error| {
                format!("Rust dependency {name:?}: invalid Cargo JSON: {error}")
            })?;
            if message.field("reason").and_then(Json::string) != Some("compiler-artifact") {
                continue;
            }
            let Some(path) = message.field("manifest_path").and_then(Json::string) else {
                return Err(format!(
                    "Rust dependency {name:?}: Cargo artifact is missing manifest_path"
                ));
            };
            let path = fs::canonicalize(path).map_err(|error| {
                format!("Rust dependency {name:?}: cannot resolve reported manifest: {error}")
            })?;
            if let Some(root) = path.parent() {
                if !result.protected_roots.iter().any(|old| old == root) {
                    result.protected_roots.push(root.to_owned());
                }
            }
            if path != cargo_manifest {
                continue;
            }
            let staticlib = message
                .field("target")
                .and_then(|value| value.field("crate_types"))
                .and_then(Json::array)
                .is_some_and(|types| {
                    types
                        .iter()
                        .any(|value| value.string() == Some("staticlib"))
                });
            if !staticlib {
                continue;
            }
            let fresh = message
                .field("fresh")
                .and_then(Json::boolean)
                .ok_or_else(|| {
                    format!("Rust dependency {name:?}: Cargo artifact is missing freshness")
                })?;
            let files = message
                .field("filenames")
                .and_then(Json::array)
                .ok_or_else(|| {
                    format!("Rust dependency {name:?}: Cargo artifact is missing filenames")
                })?;
            for file in files {
                let file = file
                    .string()
                    .ok_or("Cargo artifact filename is not a string")?;
                let file = Path::new(file);
                if !matches!(
                    file.extension().and_then(|value| value.to_str()),
                    Some("a" | "lib")
                ) {
                    continue;
                }
                let file = fs::canonicalize(file).map_err(|error| {
                    format!(
                        "Rust dependency {name:?}: cannot read reported archive '{}': {error}",
                        file.display()
                    )
                })?;
                if !file.is_file() {
                    return Err(format!(
                        "Cargo static library '{}' is not a file",
                        file.display()
                    ));
                }
                if !result.archives.contains(&file) {
                    result.archives.push(file);
                }
                result.fresh &= fresh;
                found = true;
            }
        }
        if !found {
            return Err(format!("Rust dependency {name:?} did not produce a static library; add [lib] crate-type = [\"staticlib\"] to '{}' and export compatible C ABI functions", cargo_manifest.display()));
        }
    }
    if target
        .as_deref()
        .is_some_and(|value| value.contains("-linux-"))
        || (target.is_none() && cfg!(target_os = "linux"))
    {
        result.link_args = ["-ldl", "-lpthread", "-lm", "-lrt", "-lutil"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
    }
    Ok(result)
}

/// Convert the bootstrap's LLVM target spelling to Cargo's built-in target.
pub fn rust_target(llvm: &str) -> Result<String, String> {
    let value = if apple_target(llvm, "arm64-apple-macosx")
        || apple_target(llvm, "aarch64-apple-macosx")
        || llvm == "aarch64-apple-darwin"
    {
        "aarch64-apple-darwin"
    } else if apple_target(llvm, "x86_64-apple-macosx") || llvm == "x86_64-apple-darwin" {
        "x86_64-apple-darwin"
    } else {
        match llvm {
            "x86_64-unknown-linux-gnu" | "aarch64-unknown-linux-gnu"
            | "x86_64-unknown-linux-musl" | "aarch64-unknown-linux-musl"
            | "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"
            | "x86_64-pc-windows-gnu" | "wasm32-unknown-unknown" => llvm,
            _ => return Err(format!("Rust dependency target is not mapped for LLVM triple {llvm:?}; select an explicit supported --target")),
        }
    };
    Ok(value.to_owned())
}

fn apple_target(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.is_empty()
            || (suffix.as_bytes()[0].is_ascii_digit()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.'))
    })
}

fn cache_key(manifest: &Path) -> String {
    // This only names directories, not build freshness. Store and check the
    // original identity to turn any hash collision into an error, never reuse.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in manifest.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn check_cache_identity(cache: &Path, manifest: &Path) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(0);
    let identity = cache.join("manifest.identity");
    let expected = manifest.as_os_str().as_encoded_bytes();
    if !identity.exists() {
        // Publish only a fully written identity, so concurrent first builds
        // cannot observe an empty or partial file before Cargo takes its lock.
        let unique = NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed);
        let temporary = cache.join(format!(".identity-{}-{unique}", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("cannot create '{}': {error}", temporary.display()))?;
        let publication = file
            .write_all(expected)
            .and_then(|()| fs::hard_link(&temporary, &identity));
        let _ = fs::remove_file(&temporary);
        match publication {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("cannot publish '{}': {error}", identity.display())),
        }
    }
    let actual = fs::read(&identity)
        .map_err(|error| format!("cannot read '{}': {error}", identity.display()))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Rust cache identity mismatch at '{}'; choose a different build directory",
            cache.display()
        ))
    }
}

// Cargo's machine output is parsed rather than searched for filenames. Keep
// this small parser private; it supports JSON strings including surrogate pairs.
#[derive(Debug)]
enum Json {
    Null,
    Boolean(bool),
    Number,
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn parse(input: &str) -> Result<Self, String> {
        let mut parser = JsonParser {
            input: input.as_bytes(),
            at: 0,
        };
        let value = parser.value(0)?;
        parser.space();
        if parser.at != parser.input.len() {
            return Err("trailing data".into());
        }
        Ok(value)
    }
    fn field(&self, key: &str) -> Option<&Self> {
        if let Self::Object(values) = self {
            values.get(key)
        } else {
            None
        }
    }
    fn string(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn array(&self) -> Option<&[Self]> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn boolean(&self) -> Option<bool> {
        if let Self::Boolean(value) = self {
            Some(*value)
        } else {
            None
        }
    }
}

struct JsonParser<'a> {
    input: &'a [u8],
    at: usize,
}

impl JsonParser<'_> {
    fn space(&mut self) {
        while self.input.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn take(&mut self, wanted: u8) -> bool {
        self.space();
        if self.input.get(self.at) == Some(&wanted) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > 128 {
            return Err("JSON nesting exceeds 128".into());
        }
        self.space();
        match self.input.get(self.at).copied() {
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'{') => {
                self.at += 1;
                let mut values = BTreeMap::new();
                if self.take(b'}') {
                    return Ok(Json::Object(values));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if !self.take(b':') {
                        return Err("missing colon".into());
                    }
                    let value = self.value(depth + 1)?;
                    if values.insert(key, value).is_some() {
                        return Err("duplicate object key".into());
                    }
                    if self.take(b'}') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err("missing comma".into());
                    }
                }
                Ok(Json::Object(values))
            }
            Some(b'[') => {
                self.at += 1;
                let mut values = Vec::new();
                if self.take(b']') {
                    return Ok(Json::Array(values));
                }
                loop {
                    values.push(self.value(depth + 1)?);
                    if self.take(b']') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err("missing comma".into());
                    }
                }
                Ok(Json::Array(values))
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Json::Boolean(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Json::Boolean(false))
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Json::Null)
            }
            Some(b'-' | b'0'..=b'9') => {
                self.number()?;
                Ok(Json::Number)
            }
            _ => Err("unexpected JSON token".into()),
        }
    }
    fn literal(&mut self, value: &[u8]) -> Result<(), String> {
        if self.input[self.at..].starts_with(value) {
            self.at += value.len();
            Ok(())
        } else {
            Err("invalid literal".into())
        }
    }
    fn number(&mut self) -> Result<(), String> {
        if self.input.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        if self.input.get(self.at) == Some(&b'0') {
            self.at += 1;
        } else {
            if !self
                .input
                .get(self.at)
                .is_some_and(|b| matches!(b, b'1'..=b'9'))
            {
                return Err("invalid JSON number".into());
            }
            self.digits()?;
        }
        if self.input.get(self.at) == Some(&b'.') {
            self.at += 1;
            self.digits()?;
        }
        if self
            .input
            .get(self.at)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.at += 1;
            if self
                .input
                .get(self.at)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.at += 1;
            }
            self.digits()?;
        }
        Ok(())
    }
    fn digits(&mut self) -> Result<(), String> {
        let start = self.at;
        while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.at == start {
            Err("missing number digits".into())
        } else {
            Ok(())
        }
    }
    fn string(&mut self) -> Result<String, String> {
        if self.input.get(self.at) != Some(&b'"') {
            return Err("expected string".into());
        }
        self.at += 1;
        let mut value = Vec::new();
        loop {
            let byte = *self.input.get(self.at).ok_or("unterminated string")?;
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(value).map_err(|_| "invalid UTF-8".into()),
                b'\\' => {
                    let escaped = *self.input.get(self.at).ok_or("unterminated escape")?;
                    self.at += 1;
                    match escaped {
                        b'"' | b'\\' | b'/' => value.push(escaped),
                        b'b' => value.push(8),
                        b'f' => value.push(12),
                        b'n' => value.push(b'\n'),
                        b'r' => value.push(b'\r'),
                        b't' => value.push(b'\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                self.literal(b"\\u")?;
                                let second = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err("invalid surrogate pair".into());
                                }
                                0x10000 + ((first - 0xd800) << 10) + second - 0xdc00
                            } else {
                                first
                            };
                            let scalar = char::from_u32(scalar).ok_or("invalid Unicode scalar")?;
                            let mut bytes = [0; 4];
                            value.extend_from_slice(scalar.encode_utf8(&mut bytes).as_bytes());
                        }
                        _ => return Err("invalid string escape".into()),
                    }
                }
                0..=31 => return Err("unescaped control character".into()),
                _ => value.push(byte),
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..4 {
            let byte = *self.input.get(self.at).ok_or("short Unicode escape")?;
            self.at += 1;
            value = value * 16
                + (byte as char)
                    .to_digit(16)
                    .ok_or("invalid Unicode escape")?;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_json_handles_paths_quotes_and_unicode() {
        let value = Json::parse(r#"{"filenames":["/a path/\\quoted\"/\u03bb/\ud83d\ude80.a"],"fresh":true,"unused":[null,2,-1.5e2,false]}"#).unwrap();
        assert_eq!(value.field("fresh").and_then(Json::boolean), Some(true));
        assert_eq!(
            value.field("filenames").unwrap().array().unwrap()[0].string(),
            Some("/a path/\\quoted\"/λ/🚀.a")
        );
        for bad in [
            "",
            "{}x",
            "[1,]",
            "01",
            "-.1",
            "1.",
            "1e",
            r#""\ud800""#,
            r#"{"fresh":true,"fresh":false}"#,
        ] {
            assert!(Json::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn llvm_targets_map_to_distinct_cargo_targets() {
        assert_eq!(
            rust_target("arm64-apple-macosx26.0.0").unwrap(),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            rust_target("x86_64-apple-macosx").unwrap(),
            "x86_64-apple-darwin"
        );
        assert_eq!(
            rust_target("x86_64-unknown-linux-gnu").unwrap(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            rust_target("wasm32-unknown-unknown").unwrap(),
            "wasm32-unknown-unknown"
        );
        assert!(rust_target("native").is_err());
        assert!(rust_target("hostile.json").is_err());
        assert!(rust_target("arm64-apple-macosxgarbage").is_err());
    }
}
