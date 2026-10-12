//! Explicit reviewed scalar adapter contracts. Metadata does not change the C
//! ABI or certify foreign code, transitive dependencies, or worker safety.
use crate::build_cache::sha256;
use crate::json::{self, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorContract {
    Infallible,
    /// Only the stated value is success. Every other i64 is adapter-defined.
    StatusI64 {
        success: i64,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnwindContract {
    Abort,
    Contained,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadContract {
    CallerOnly,
    Reentrant,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub symbol: String,
    pub parameter_count: usize,
    pub returns_i64: bool,
    pub error: ErrorContract,
    pub unwind: UnwindContract,
    pub thread: ThreadContract,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub source_sha256: String,
    pub reviewed_by: String,
    pub functions: Vec<Function>,
}

fn object<'a>(value: &'a Value, fields: &[&str]) -> Result<&'a Value, String> {
    let Value::Object(map) = value else {
        return Err("adapter contract must be an object".into());
    };
    if map.len() != fields.len() || fields.iter().any(|field| !map.contains_key(*field)) {
        return Err(format!("expected exactly fields {fields:?}"));
    }
    Ok(value)
}
fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be a string"))
}
fn symbol(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !value.contains("__")
        && !((value.starts_with("int") || value.starts_with("uint")) && value.ends_with("_t"))
        && !matches!(
            value,
            "linux"
                | "unix"
                | "i386"
                | "sparc"
                | "mips"
                | "sun"
                | "asm"
                | "explicit"
                | "typeof"
                | "typeof_unqual"
                | "std"
                | "size_t"
                | "ptrdiff_t"
                | "wint_t"
                | "sig_atomic_t"
                | "max_align_t"
                | "main"
                | "print"
                | "auto"
                | "break"
                | "case"
                | "char"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extern"
                | "float"
                | "for"
                | "goto"
                | "if"
                | "inline"
                | "int"
                | "long"
                | "register"
                | "restrict"
                | "return"
                | "short"
                | "signed"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "typedef"
                | "union"
                | "unsigned"
                | "void"
                | "volatile"
                | "while"
                | "_Bool"
                | "_Complex"
                | "_Imaginary"
                | "_Atomic"
                | "_Alignas"
                | "_Alignof"
                | "_Generic"
                | "_Noreturn"
                | "_Static_assert"
                | "_Thread_local"
                | "class"
                | "namespace"
                | "template"
                | "new"
                | "delete"
                | "public"
                | "private"
                | "protected"
                | "virtual"
                | "operator"
                | "this"
                | "throw"
                | "try"
                | "catch"
                | "bool"
                | "true"
                | "false"
                | "nullptr"
                | "constexpr"
                | "consteval"
                | "constinit"
                | "typename"
                | "using"
                | "friend"
                | "mutable"
                | "noexcept"
                | "alignas"
                | "alignof"
                | "static_assert"
                | "thread_local"
                | "and"
                | "or"
                | "not"
                | "xor"
                | "bitand"
                | "bitor"
                | "compl"
                | "and_eq"
                | "or_eq"
                | "xor_eq"
                | "not_eq"
                | "concept"
                | "requires"
                | "co_await"
                | "co_return"
                | "co_yield"
                | "decltype"
                | "dynamic_cast"
                | "static_cast"
                | "reinterpret_cast"
                | "const_cast"
                | "typeid"
                | "wchar_t"
                | "char8_t"
                | "char16_t"
                | "char32_t"
                | "export"
                | "int64_t"
        )
}

impl Adapter {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("adapter metadata exceeds 1 MiB".into());
        }
        let value = json::parse(text)?;
        object(
            &value,
            &[
                "format",
                "source_sha256",
                "reviewed_by",
                "runtime",
                "functions",
            ],
        )?;
        if string(&value, "format")? != "bhasa-scalar-adapter-v1" {
            return Err("unsupported adapter metadata version".into());
        }
        if string(&value, "runtime")? != "native-no-gc" {
            return Err("adapter requires a reviewed native no-GC runtime".into());
        }
        let source_sha256 = string(&value, "source_sha256")?;
        if source_sha256.len() != 64
            || !source_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid source SHA-256".into());
        }
        let reviewed_by = string(&value, "reviewed_by")?;
        if reviewed_by.trim().is_empty() || reviewed_by.chars().any(char::is_control) {
            return Err("reviewed_by must name the reviewer".into());
        }
        let values = value
            .get("functions")
            .and_then(Value::as_array)
            .ok_or("functions must be an array")?;
        if values.is_empty() || values.len() > 1024 {
            return Err("invalid function count".into());
        }
        let mut functions = Vec::new();
        let mut symbols = BTreeSet::new();
        for value in values {
            object(
                value,
                &[
                    "symbol",
                    "parameters",
                    "result",
                    "error",
                    "unwind",
                    "thread",
                ],
            )?;
            let name = string(value, "symbol")?;
            if !symbol(name) || !symbols.insert(name.to_owned()) {
                return Err("invalid or duplicate adapter symbol".into());
            }
            let parameters = value
                .get("parameters")
                .and_then(Value::as_array)
                .ok_or("parameters must be an array")?;
            if parameters.len() > 128 || parameters.iter().any(|v| v.as_str() != Some("i64-copy")) {
                return Err("only scalar i64-copy parameters are supported".into());
            }
            let returns_i64 = match string(value, "result")? {
                "i64-copy" => true,
                "void" => false,
                _ => return Err("only i64-copy or void results are supported".into()),
            };
            let error = value.get("error").ok_or("missing error contract")?;
            let error = match string(error, "kind")? {
                "infallible" => {
                    object(error, &["kind"])?;
                    ErrorContract::Infallible
                }
                "status-i64" if returns_i64 => {
                    object(error, &["kind", "success"])?;
                    let success = string(error, "success")?
                        .parse::<i64>()
                        .map_err(|_| "status success must fit i64")?;
                    ErrorContract::StatusI64 { success }
                }
                _ => return Err("invalid error contract for result".into()),
            };
            let unwind = match string(value, "unwind")? {
                "abort" => UnwindContract::Abort,
                "contained" => UnwindContract::Contained,
                _ => return Err("unwinding across C ABI is forbidden".into()),
            };
            let thread = match string(value, "thread")? {
                "caller-only" => ThreadContract::CallerOnly,
                "reentrant" => ThreadContract::Reentrant,
                _ => return Err("unknown thread contract".into()),
            };
            functions.push(Function {
                symbol: name.to_owned(),
                parameter_count: parameters.len(),
                returns_i64,
                error,
                unwind,
                thread,
            });
        }
        functions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        Ok(Self {
            source_sha256: source_sha256.to_owned(),
            reviewed_by: reviewed_by.to_owned(),
            functions,
        })
    }

    /// Deterministic representation of explicit claims, not a safety proof.
    pub fn to_value(&self) -> Value {
        let functions = self
            .functions
            .iter()
            .map(|function| {
                let error = match function.error {
                    ErrorContract::Infallible => Value::object([("kind", "infallible".into())]),
                    ErrorContract::StatusI64 { success } => Value::object([
                        ("kind", "status-i64".into()),
                        ("success", success.to_string().into()),
                    ]),
                };
                Value::object([
                    ("symbol", function.symbol.clone().into()),
                    (
                        "parameters",
                        Value::Array(vec!["i64-copy".into(); function.parameter_count]),
                    ),
                    (
                        "result",
                        if function.returns_i64 {
                            "i64-copy"
                        } else {
                            "void"
                        }
                        .into(),
                    ),
                    ("error", error),
                    (
                        "unwind",
                        match function.unwind {
                            UnwindContract::Abort => "abort",
                            UnwindContract::Contained => "contained",
                        }
                        .into(),
                    ),
                    (
                        "thread",
                        match function.thread {
                            ThreadContract::CallerOnly => "caller-only",
                            ThreadContract::Reentrant => "reentrant",
                        }
                        .into(),
                    ),
                ])
            })
            .collect();
        Value::object([
            ("format", "bhasa-scalar-adapter-v1".into()),
            ("source_sha256", self.source_sha256.clone().into()),
            ("reviewed_by", self.reviewed_by.clone().into()),
            ("runtime", "native-no-gc".into()),
            ("functions", Value::Array(functions)),
        ])
    }

    pub fn verify_source(&self, source: &[u8]) -> Result<(), String> {
        if sha256(source) != self.source_sha256 {
            return Err("adapter source changed after review".into());
        }
        Ok(())
    }

    /// Header generation is deliberately limited to scalar copy ownership. No
    /// pointer, resource, callback, or Rust native ABI signature is guessed.
    pub fn c_header(&self) -> Result<String, String> {
        let mut out = String::from("/* Bhasa scalar C ABI; metadata claims require source review. */\n#include <stdint.h>\n#ifdef __cplusplus\nextern \"C\" {\n#endif\n");
        let mut names = BTreeSet::new();
        for function in &self.functions {
            if !symbol(&function.symbol)
                || !names.insert(&function.symbol)
                || function.parameter_count > 128
            {
                return Err("invalid adapter function".into());
            }
            if !function.returns_i64 && matches!(function.error, ErrorContract::StatusI64 { .. }) {
                return Err("void result cannot carry scalar status".into());
            }
            let result = if function.returns_i64 {
                "int64_t"
            } else {
                "void"
            };
            let params = if function.parameter_count == 0 {
                "void".into()
            } else {
                vec!["int64_t"; function.parameter_count].join(", ")
            };
            out.push_str(&format!("{result} {}({params});\n", function.symbol));
        }
        out.push_str("#ifdef __cplusplus\n}\n#endif\n");
        Ok(out)
    }
}

/// Optional local sidecar. It binds review metadata to src/lib.rs only; it
/// does not inspect Cargo dependencies or infer absence of GC from Rust code.
/// Existing adapters without a sidecar retain their trusted C ABI behavior.
pub fn read_if_present(crate_root: &std::path::Path) -> Result<Option<Adapter>, String> {
    use std::{fs, io::Read};
    let path = crate_root.join("bhasa-adapter.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("adapter sidecar must be a regular file of at most 1 MiB".into());
    }
    fn read(path: &std::path::Path, limit: u64) -> Result<Vec<u8>, String> {
        let file = fs::File::open(path).map_err(|error| error.to_string())?;
        let meta = file.metadata().map_err(|error| error.to_string())?;
        if !meta.is_file() || meta.len() > limit {
            return Err("adapter input is not a bounded regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("adapter input exceeds limit".into());
        }
        Ok(bytes)
    }
    let bytes = read(&path, 1024 * 1024)?;
    let contract =
        Adapter::parse(std::str::from_utf8(&bytes).map_err(|_| "adapter metadata must be UTF-8")?)?;
    let source = crate_root
        .join("src/lib.rs")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let root = crate_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !source.starts_with(root) {
        return Err("reviewed adapter source escapes crate root".into());
    }
    contract.verify_source(&read(&source, 64 * 1024 * 1024)?)?;
    Ok(Some(contract))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> String {
        format!(
            r#"{{"format":"bhasa-scalar-adapter-v1","source_sha256":"{}","reviewed_by":"fixture reviewer","runtime":"native-no-gc","functions":[{{"symbol":"adapter_double","parameters":["i64-copy"],"result":"i64-copy","error":{{"kind":"infallible"}},"unwind":"abort","thread":"caller-only"}}]}}"#,
            sha256(b"source")
        )
    }
    #[test]
    fn source_review_binding_and_header() {
        let adapter = Adapter::parse(&metadata()).unwrap();
        adapter.verify_source(b"source").unwrap();
        assert!(adapter.verify_source(b"modified").is_err());
        assert!(adapter
            .c_header()
            .unwrap()
            .contains("int64_t adapter_double(int64_t);"));
    }
    #[test]
    fn status_codes_require_i64_and_explicit_success() {
        let source = metadata().replace(
            r#""kind":"infallible""#,
            r#""kind":"status-i64","success":"-9223372036854775808""#,
        );
        assert_eq!(
            Adapter::parse(&source).unwrap().functions[0].error,
            ErrorContract::StatusI64 { success: i64::MIN }
        );
        assert!(
            Adapter::parse(&source.replace(r#""result":"i64-copy""#, r#""result":"void""#))
                .is_err()
        );
        assert!(
            Adapter::parse(&source.replace("-9223372036854775808", "9223372036854775808")).is_err()
        );
    }
    #[test]
    fn rejects_gc_unwind_pointer_ownership_and_unknown_fields() {
        for (from, to) in [
            ("native-no-gc", "python"),
            ("native-no-gc", "typescript"),
            ("abort", "unwind"),
            ("i64-copy", "buffer-owned"),
            ("caller-only", "unknown"),
            ("adapter_double", "main"),
            ("adapter_double", "int"),
            ("adapter_double", "class"),
            ("fixture reviewer", " "),
            (
                r#""runtime":"native-no-gc""#,
                r#""runtime":"native-no-gc","extra":true"#,
            ),
        ] {
            assert!(
                Adapter::parse(&metadata().replace(from, to)).is_err(),
                "{to}"
            );
        }
        let mut adapter = Adapter::parse(&metadata()).unwrap();
        adapter.functions[0].symbol = "injected();".into();
        assert!(adapter.c_header().is_err());
    }
    #[test]
    fn optional_sidecar_checks_source_and_rejects_stale_review() {
        use std::{
            fs,
            sync::atomic::{AtomicUsize, Ordering},
        };
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "bhasa-contract-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        assert!(read_if_present(&root).unwrap().is_none());
        fs::write(root.join("src/lib.rs"), b"source").unwrap();
        fs::write(root.join("bhasa-adapter.json"), metadata()).unwrap();
        assert!(read_if_present(&root).unwrap().is_some());
        fs::write(root.join("src/lib.rs"), b"changed").unwrap();
        assert!(read_if_present(&root)
            .unwrap_err()
            .contains("source changed"));
        fs::remove_file(root.join("bhasa-adapter.json")).unwrap();
        fs::create_dir(root.join("bhasa-adapter.json")).unwrap();
        assert!(read_if_present(&root).is_err());
    }
    #[test]
    fn generated_header_names_reject_reserved_typedef_macro_and_compiler_identifiers() {
        for name in [
            "explicit",
            "asm",
            "uint64_t",
            "int32_t",
            "intptr_t",
            "INT64_MAX",
            "_adapter",
            "__has_include",
            "std",
            "typeof",
            "SIZE_MAX",
        ] {
            assert!(
                Adapter::parse(&metadata().replace("adapter_double", name)).is_err(),
                "{name}"
            );
            let mut adapter = Adapter::parse(&metadata()).unwrap();
            adapter.functions[0].symbol = name.into();
            assert!(adapter.c_header().is_err(), "{name}");
        }
        let adapter = Adapter::parse(&metadata()).unwrap();
        assert_eq!(
            Adapter::parse(&adapter.to_value().to_string()).unwrap(),
            adapter
        );
    }
}
