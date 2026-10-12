//! Explicit offline lock creation/update and read-only verification.
use crate::json::Value;
use crate::package_lock::Snapshot;
use crate::project::{self, LocatedDiagnostic};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Create,
    Update,
    Check,
}
impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Check => "check",
        }
    }
}
#[derive(Debug)]
pub(crate) struct Options {
    pub action: Action,
    pub source: Option<PathBuf>,
    pub json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Options, String> {
    let action = match args.first().map(String::as_str) {
        Some("create") => Action::Create,
        Some("update") => Action::Update,
        Some("check") => Action::Check,
        _ => {
            return Err(
                "lock requires create, update, or check; use lock MODE [FILE] [--json]".into(),
            )
        }
    };
    let mut options = Options {
        action,
        source: None,
        json: false,
    };
    let mut positional_only = false;
    for value in &args[1..] {
        if !positional_only && value == "--" {
            positional_only = true;
        } else if !positional_only && value == "--json" && !options.json {
            options.json = true;
        } else if !value.is_empty()
            && (positional_only || !value.starts_with('-'))
            && options.source.is_none()
        {
            options.source = Some(value.into());
        } else {
            return Err(format!(
                "unexpected lock argument {value:?}; use lock MODE [FILE] [--json]"
            ));
        }
    }
    Ok(options)
}

fn emit(value: Value, code: i32) -> i32 {
    match std::io::stdout()
        .lock()
        .write_all(format!("{value}\n").as_bytes())
    {
        Ok(()) => code,
        Err(error) => {
            eprintln!("error: cannot write lock output: {error}");
            1
        }
    }
}
fn failure(errors: &[LocatedDiagnostic], json: bool, code: i32) -> i32 {
    if json {
        let diagnostics = errors
            .iter()
            .map(|error| {
                crate::json::parse(
                    &error
                        .diagnostic
                        .to_json(&error.source, &error.path.display().to_string()),
                )
                .expect("diagnostics are JSON")
            })
            .collect();
        emit(
            Value::object([
                ("schema_version", 1_i64.into()),
                ("ok", false.into()),
                ("diagnostics", Value::Array(diagnostics)),
            ]),
            code,
        )
    } else {
        for error in errors {
            eprintln!(
                "error: {}: {}",
                error.path.display(),
                error.diagnostic.message
            );
        }
        code
    }
}
fn error(path: &Path, message: impl Into<String>) -> LocatedDiagnostic {
    LocatedDiagnostic {
        path: path.to_owned(),
        source: String::new(),
        diagnostic: crate::Diagnostic::new("K001", message, crate::ast::Span::default()),
    }
}
pub(crate) fn usage_error(message: &str) -> i32 {
    failure(&[error(Path::new("<command line>"), message)], true, 2)
}

pub(crate) fn run(options: Options) -> i32 {
    let loaded = match options.action {
        Action::Check => project::load_unparsed(options.source.as_deref()),
        Action::Create | Action::Update => {
            project::load_unparsed_for_lock_update(options.source.as_deref())
        }
    };
    let loaded = match loaded {
        Ok(value) => value,
        Err(errors) => return failure(&errors, options.json, 1),
    };
    let Some(root) = crate::package_lock::root_manifest(&loaded) else {
        return failure(
            &[error(
                &loaded.entry,
                "lock commands require a bhasa.toml package",
            )],
            options.json,
            1,
        );
    };
    let lock = root.root.join("bhasa.lock");
    let operation = || -> Result<bool, String> {
        let snapshot = Snapshot::capture(&loaded, root)?;
        match options.action {
            Action::Check => {
                let existing = crate::package_lock::read_if_present(&root.root)?
                    .ok_or("bhasa.lock does not exist; use lock create")?;
                existing.verify(&snapshot)?;
                Ok(false)
            }
            Action::Create => publish(&lock, snapshot.encode().as_bytes(), false),
            Action::Update => publish(&lock, snapshot.encode().as_bytes(), true),
        }
    };
    match operation() {
        Ok(changed) if options.json => emit(
            Value::object([
                ("schema_version", 1_i64.into()),
                ("ok", true.into()),
                ("command", format!("lock {}", options.action.name()).into()),
                ("lock", lock.display().to_string().into()),
                ("entry", loaded.entry.display().to_string().into()),
                ("changed", changed.into()),
                ("diagnostics", Value::Array(Vec::new())),
            ]),
            0,
        ),
        Ok(changed) => {
            let verb = match options.action {
                Action::Create => "Created",
                Action::Update if changed => "Updated",
                Action::Update => "Unchanged",
                Action::Check => "Verified",
            };
            match std::io::stdout()
                .lock()
                .write_all(format!("{verb} {}\n", lock.display()).as_bytes())
            {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("error: cannot write lock output: {error}");
                    1
                }
            }
        }
        Err(message) => failure(&[error(&lock, message)], options.json, 1),
    }
}

/// Stage the complete document beside its destination. A create uses hard-link
/// publication for an atomic no-replace operation. Update atomically renames the
/// stage after rejecting symlink/nonregular destinations. Identical updates do
/// no writes. This is not a cross-process compare-and-swap protocol.
fn publish(destination: &Path, bytes: &[u8], update: bool) -> Result<bool, String> {
    let existing = match fs::symlink_metadata(destination) {
        Ok(meta) => {
            if !meta.is_file() {
                return Err("bhasa.lock must be a regular file; symlinks are rejected".into());
            }
            if !update {
                return Err("bhasa.lock already exists; use lock update".into());
            }
            if meta.len() > 1024 * 1024 {
                return Err("existing bhasa.lock exceeds 1 MiB".into());
            }
            let file = fs::File::open(destination).map_err(|error| error.to_string())?;
            if !file
                .metadata()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("bhasa.lock is not regular".into());
            }
            let mut old = Vec::new();
            file.take(1024 * 1024 + 1)
                .read_to_end(&mut old)
                .map_err(|error| error.to_string())?;
            if old.len() > 1024 * 1024 {
                return Err("existing bhasa.lock exceeds 1 MiB".into());
            }
            Some(old)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if update {
                return Err("bhasa.lock does not exist; use lock create".into());
            }
            None
        }
        Err(error) => return Err(error.to_string()),
    };
    if existing.as_deref() == Some(bytes) {
        return Ok(false);
    }
    let parent = destination.parent().ok_or("lock has no parent")?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    for _ in 0..64 {
        let path = parent.join(format!(
            ".bhasa-lock-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot stage lock: {error}")),
        };
        struct Stage(PathBuf);
        impl Drop for Stage {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _stage = Stage(path.clone());
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("cannot write lock: {error}"))?;
        drop(file);
        if update {
            // A concurrently replaced symlink/nonregular file must not be followed.
            if !fs::symlink_metadata(destination)
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("bhasa.lock changed to a nonregular file before publication".into());
            }
            fs::rename(&path, destination)
                .map_err(|error| format!("cannot replace lock: {error}"))?;
        } else {
            fs::hard_link(&path, destination).map_err(|error| {
                format!("cannot create lock without replacing an existing file: {error}")
            })?;
        }
        return Ok(true);
    }
    Err("cannot allocate a unique lock stage".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }
    #[test]
    fn modes_and_argument_boundaries_are_explicit() {
        assert_eq!(parse(&args(&["create"])).unwrap().action, Action::Create);
        assert!(parse(&args(&["check", "--json"])).unwrap().json);
        assert_eq!(
            parse(&args(&["update", "--", "-main.bh"])).unwrap().source,
            Some("-main.bh".into())
        );
        for bad in [
            &[][..],
            &["--check"][..],
            &["create", "--json", "--json"][..],
            &["check", "--release"][..],
            &["update", "a", "b"][..],
            &["check", ""][..],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }
}
