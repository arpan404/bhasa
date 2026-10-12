//! Local packages and deterministic source loading. No registry or network access.
use crate::ast::{Program, Span};
use crate::diagnostic::Diagnostic;
use crate::lexer::{Token, TokenKind};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

const MAX_FILES: usize = 1024;
const MAX_DEPTH: usize = 128;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

// Bound allocation even if a regular file grows after the metadata check.
fn read_text_file(path: &Path, limit: usize) -> io::Result<String> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular UTF-8 file",
        ));
    }
    if metadata.len() > limit as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds the {limit} byte size limit"),
        ));
    }
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular UTF-8 file",
        ));
    }
    let mut source = String::new();
    file.take(limit as u64 + 1).read_to_string(&mut source)?;
    if source.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds the {limit} byte size limit"),
        ));
    }
    Ok(source)
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub path: PathBuf,
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub entry: PathBuf,
    pub dependencies: BTreeMap<String, String>,
    pub rust_dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
    pub base: usize,
    pub import_spans: Vec<Span>,
    pub imports: Vec<ModuleImport>,
}

/// A source import and its canonical destination. `parse_imports` leaves target
/// empty; the project loader resolves it before module lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleImport {
    pub path: String,
    pub alias: Option<String>,
    pub span: Span,
    pub target: PathBuf,
}

#[derive(Debug)]
pub struct LocatedDiagnostic {
    pub path: PathBuf,
    pub source: String,
    pub diagnostic: Diagnostic,
}

impl LocatedDiagnostic {
    fn new(path: &Path, source: &str, message: impl Into<String>, span: Span) -> Self {
        Self {
            path: path.to_owned(),
            source: source.to_owned(),
            diagnostic: Diagnostic::new("M001", message, span),
        }
    }
}

#[derive(Debug)]
pub struct LoadedProgram {
    pub program: Program,
    pub files: Vec<SourceFile>,
    pub manifests: Vec<Manifest>,
    pub entry: PathBuf,
    /// Present only for an invocation that omitted a source file.
    pub project: Option<Manifest>,
}

/// Convert virtual token offsets back to the file and byte positions that produced them.
pub fn locate(files: &[SourceFile], diagnostic: &Diagnostic) -> LocatedDiagnostic {
    let file = files
        .iter()
        .rev()
        .find(|file| diagnostic.span.start >= file.base)
        .expect("loaded programs contain at least their entry source");
    let mut local = diagnostic.clone();
    local.span = Span::new(
        diagnostic
            .span
            .start
            .saturating_sub(file.base)
            .min(file.source.len()),
        diagnostic
            .span
            .end
            .saturating_sub(file.base)
            .min(file.source.len()),
    );
    LocatedDiagnostic {
        path: file.path.clone(),
        source: file.source.clone(),
        diagnostic: local,
    }
}

/// Input for a whole-program frontend, with imports blanked and original byte
/// offsets retained. Diagnostic offsets still map through `locate`.
pub fn flattened_source(files: &[SourceFile]) -> String {
    let mut output = Vec::new();
    for file in files {
        debug_assert_eq!(output.len(), file.base);
        let mut bytes = file.source.as_bytes().to_vec();
        for span in &file.import_spans {
            for byte in &mut bytes[span.start..span.end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }
        output.extend(bytes);
        // Avoid a CRLF token that straddles the frontend's module boundary.
        output.push(if file.source.ends_with('\r') {
            b' '
        } else {
            b'\n'
        });
    }
    String::from_utf8(output).expect("blanking complete UTF-8 import spans preserves UTF-8")
}

pub fn load(source: Option<&Path>) -> Result<LoadedProgram, Vec<LocatedDiagnostic>> {
    load_inner(source, true).map_err(|error| vec![error])
}

/// Load and resolve imports while leaving declarations to an alternate frontend.
pub fn load_unparsed(source: Option<&Path>) -> Result<LoadedProgram, Vec<LocatedDiagnostic>> {
    load_inner(source, false).map_err(|error| vec![error])
}

fn load_inner(source: Option<&Path>, parse: bool) -> Result<LoadedProgram, LocatedDiagnostic> {
    let cwd = std::env::current_dir().map_err(|error| {
        LocatedDiagnostic::new(
            Path::new("."),
            "",
            format!("cannot read current directory: {error}"),
            Span::default(),
        )
    })?;
    let explicit = source.is_some();
    let (entry, manifest) = if let Some(path) = source {
        let path = fs::canonicalize(path).map_err(|error| {
            LocatedDiagnostic::new(
                path,
                "",
                format!("cannot read source '{}': {error}", path.display()),
                Span::default(),
            )
        })?;
        let manifest = find_manifest(path.parent().unwrap_or(&cwd))
            .map(|path| read_manifest(&path))
            .transpose()?;
        (path, manifest)
    } else {
        let path = find_manifest(&cwd).ok_or_else(|| {
            LocatedDiagnostic::new(&cwd.join("bhasa.toml"), "", "no bhasa.toml found; provide a source file or create a project with bhasa new NAME", Span::default())
        })?;
        let manifest = read_manifest(&path)?;
        (manifest.entry.clone(), Some(manifest))
    };
    let root = manifest
        .as_ref()
        .map(|m| m.root.clone())
        .unwrap_or_else(|| entry.parent().unwrap_or(&cwd).to_owned());
    let mut loader = Loader {
        files: Vec::new(),
        manifests: BTreeMap::new(),
        active: BTreeSet::new(),
        loaded: BTreeMap::new(),
        next_base: 0,
        program: Program::default(),
        parse: false,
    };
    if let Some(manifest) = &manifest {
        loader
            .manifests
            .insert(manifest.path.clone(), manifest.clone());
    }
    loader.visit(&entry, &root, manifest.as_ref(), 0)?;
    if parse {
        loader.program = crate::parse_files(&loader.files)
            .map_err(|errors| locate(&loader.files, &errors[0]))?;
    }
    Ok(LoadedProgram {
        program: loader.program,
        files: loader.files,
        manifests: loader.manifests.into_values().collect(),
        entry,
        project: if explicit { None } else { manifest },
    })
}

fn find_manifest(directory: &Path) -> Option<PathBuf> {
    directory.ancestors().find_map(exact_manifest)
}

// Check the actual directory entry, including on case-insensitive filesystems.
// A differently cased filename must not silently establish a package root.
fn exact_manifest(directory: &Path) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| entry.file_name() == "bhasa.toml")
        .map(|entry| entry.path())
}

struct Loader {
    files: Vec<SourceFile>,
    manifests: BTreeMap<PathBuf, Manifest>,
    active: BTreeSet<PathBuf>,
    loaded: BTreeMap<PathBuf, PathBuf>,
    next_base: usize,
    program: Program,
    parse: bool,
}

impl Loader {
    fn visit(
        &mut self,
        path: &Path,
        root: &Path,
        manifest: Option<&Manifest>,
        depth: usize,
    ) -> Result<(), LocatedDiagnostic> {
        let path = fs::canonicalize(path).map_err(|error| {
            LocatedDiagnostic::new(
                path,
                "",
                format!("cannot read module '{}': {error}", path.display()),
                Span::default(),
            )
        })?;
        if !path.starts_with(root) {
            return Err(LocatedDiagnostic::new(
                &path,
                "",
                "module path escapes its package root",
                Span::default(),
            ));
        }
        if self.active.contains(&path) {
            return Err(LocatedDiagnostic::new(
                &path,
                "",
                "import cycle detected",
                Span::default(),
            ));
        }
        if let Some(existing_root) = self.loaded.get(&path) {
            if existing_root != root {
                return Err(LocatedDiagnostic::new(&path, "", "module was imported under two different package roots; use one declared dependency", Span::default()));
            }
            return Ok(());
        }
        if self.files.len() >= MAX_FILES || depth >= MAX_DEPTH {
            return Err(LocatedDiagnostic::new(
                &path,
                "",
                "module graph exceeds the limit of 1024 files or 128 import levels",
                Span::default(),
            ));
        }
        let source = read_text_file(&path, MAX_SOURCE_BYTES.saturating_sub(self.next_base + 1))
            .map_err(|error| {
                LocatedDiagnostic::new(
                    &path,
                    "",
                    format!("cannot read module: {error}"),
                    Span::default(),
                )
            })?;
        if self.next_base.saturating_add(source.len() + 1) > MAX_SOURCE_BYTES {
            return Err(LocatedDiagnostic::new(
                &path,
                "",
                "module graph exceeds the 64 MiB source limit",
                Span::default(),
            ));
        }
        let base = self.next_base;
        let file_index = self.files.len();
        self.next_base += source.len() + 1;
        self.files.push(SourceFile {
            path: path.clone(),
            source: source.clone(),
            base,
            import_spans: Vec::new(),
            imports: Vec::new(),
        });
        self.active.insert(path.clone());
        let tokens = crate::lexer::lex(&source).map_err(|errors| LocatedDiagnostic {
            path: path.clone(),
            source: source.clone(),
            diagnostic: errors.into_iter().next().expect("lexer error"),
        })?;
        let (imports, mut tokens) =
            split_imports(tokens).map_err(|diagnostic| LocatedDiagnostic {
                path: path.clone(),
                source: source.clone(),
                diagnostic,
            })?;
        self.files[file_index].import_spans = imports.iter().map(|import| import.span).collect();
        for mut import in imports {
            let span = import.span;
            let resolved = if let Some(alias) = import.path.strip_prefix('@') {
                let Some(manifest) = manifest else {
                    return Err(LocatedDiagnostic::new(
                        &path,
                        &source,
                        "package imports require a bhasa.toml manifest",
                        span,
                    ));
                };
                let Some(dependency) = manifest.dependencies.get(alias) else {
                    return Err(LocatedDiagnostic::new(
                        &path,
                        &source,
                        format!("unknown local dependency '{alias}'"),
                        span,
                    ));
                };
                let dependency_path = manifest.root.join(dependency).join("bhasa.toml");
                let dependency_manifest = read_manifest(&dependency_path)?;
                self.manifests.insert(
                    dependency_manifest.path.clone(),
                    dependency_manifest.clone(),
                );
                import.target = dependency_manifest.entry.clone();
                self.visit(
                    &dependency_manifest.entry,
                    &dependency_manifest.root,
                    Some(&dependency_manifest),
                    depth + 1,
                )
            } else {
                if !safe_module_relative(&import.path)
                    || Path::new(&import.path)
                        .extension()
                        .and_then(|ext| ext.to_str())
                        != Some("bh")
                {
                    return Err(LocatedDiagnostic::new(&path, &source, "module imports must use relative .bh paths inside their package root; use @name for a declared dependency", span));
                }
                import.target = path.parent().unwrap().join(&import.path);
                self.visit(&import.target, root, manifest, depth + 1)
            };
            if let Err(mut error) = resolved {
                // Missing paths, cycles and root escapes belong at the import that caused them.
                if error.source.is_empty() {
                    error.path = path.clone();
                    error.source = source.clone();
                    error.diagnostic.span = span;
                }
                return Err(error);
            }
            // visit has validated existence, confinement and package identity.
            import.target = fs::canonicalize(&import.target).map_err(|error| {
                LocatedDiagnostic::new(
                    &path,
                    &source,
                    format!("cannot resolve imported module: {error}"),
                    span,
                )
            })?;
            self.files[file_index].imports.push(import);
        }
        for token in &mut tokens {
            token.span.start += base;
            token.span.end += base;
        }
        if self.parse {
            let program = crate::parser::parse_tokens(tokens)
                .map_err(|errors| locate(&self.files, &errors[0]))?;
            self.program.merge(program);
        }
        self.active.remove(&path);
        self.loaded.insert(path, root.to_owned());
        Ok(())
    }
}

type Imports = Vec<ModuleImport>;

pub fn parse_imports(source: &str) -> Result<Vec<ModuleImport>, Vec<Diagnostic>> {
    split_imports(crate::lexer::lex(source)?)
        .map(|(imports, _)| imports)
        .map_err(|error| vec![error])
}

fn split_imports(tokens: Vec<Token>) -> Result<(Imports, Vec<Token>), Diagnostic> {
    let mut cursor = 0;
    let mut imports = Vec::new();
    loop {
        while matches!(
            tokens[cursor].kind,
            TokenKind::Newline | TokenKind::Semicolon
        ) {
            cursor += 1;
        }
        if !matches!(&tokens[cursor].kind, TokenKind::Ident(name) if name == "import") {
            break;
        }
        let start = tokens[cursor].span;
        cursor += 1;
        let TokenKind::Str(path) = &tokens[cursor].kind else {
            return Err(Diagnostic::new(
                "M002",
                "expected a quoted module path after import",
                tokens[cursor].span,
            ));
        };
        let path = path.clone();
        let mut span = start.join(tokens[cursor].span);
        cursor += 1;
        let alias = if matches!(&tokens[cursor].kind, TokenKind::Ident(name) if name == "as") {
            cursor += 1;
            let TokenKind::Ident(alias) = &tokens[cursor].kind else {
                return Err(Diagnostic::new(
                    "M002",
                    "expected a namespace name after 'as'",
                    tokens[cursor].span,
                ));
            };
            let alias = alias.clone();
            span = span.join(tokens[cursor].span);
            cursor += 1;
            Some(alias)
        } else {
            None
        };
        imports.push(ModuleImport {
            path,
            alias,
            span,
            target: PathBuf::new(),
        });
        if !matches!(
            tokens[cursor].kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof
        ) {
            return Err(Diagnostic::new(
                "M002",
                "expected a newline or ';' after import",
                tokens[cursor].span,
            ));
        }
    }
    Ok((imports, tokens.into_iter().skip(cursor).collect()))
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\0', '\\'])
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

fn safe_module_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\0', '\\'])
        && Path::new(value).components().all(|part| {
            matches!(
                part,
                Component::Normal(_) | Component::CurDir | Component::ParentDir
            )
        })
}

fn valid_name(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

pub fn read_manifest(path: &Path) -> Result<Manifest, LocatedDiagnostic> {
    if path.file_name().is_none_or(|name| name != "bhasa.toml")
        || exact_manifest(
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .is_none()
    {
        return Err(LocatedDiagnostic::new(
            path,
            "",
            "package manifest must be named exactly 'bhasa.toml' (lowercase)",
            Span::default(),
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        LocatedDiagnostic::new(
            path,
            "",
            format!("cannot read package manifest '{}': {error}", path.display()),
            Span::default(),
        )
    })?;
    let source = read_text_file(&canonical, MAX_MANIFEST_BYTES).map_err(|error| {
        LocatedDiagnostic::new(
            path,
            "",
            format!("cannot read package manifest: {error}"),
            Span::default(),
        )
    })?;
    let root = canonical.parent().unwrap().to_owned();
    let mut section = "";
    let mut package = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut rust_dependencies = BTreeMap::new();
    let mut sections = BTreeSet::new();
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let span = Span::new(offset, offset + raw.trim_end().len());
        offset += raw.len();
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let error = |message| LocatedDiagnostic::new(&canonical, &source, message, span);
        if line.starts_with('[') {
            section = match line {
                "[package]" => "package", "[dependencies]" => "dependencies", "[rust-dependencies]" => "rust-dependencies",
                _ => return Err(error("unknown manifest section; expected [package], [dependencies], or [rust-dependencies]")),
            };
            if !sections.insert(section) {
                return Err(error("repeated manifest section"));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(error("expected key = \"value\""));
        };
        let key = key.trim();
        let value = value.trim();
        if !valid_name(key) || value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
            return Err(error(
                "manifest keys must be names and values must be double-quoted strings",
            ));
        }
        let value = &value[1..value.len() - 1];
        if value.contains(['"', '\\', '\0', '\r', '\n']) {
            return Err(error(
                "manifest strings cannot contain escapes, quotes, or control newlines",
            ));
        }
        let map = match section {
            "package" => {
                if !matches!(key, "name" | "version" | "entry") {
                    return Err(error(
                        "unknown package key; expected name, version, or entry",
                    ));
                }
                &mut package
            }
            "dependencies" => &mut dependencies,
            "rust-dependencies" => &mut rust_dependencies,
            _ => return Err(error("manifest values must follow a section header")),
        };
        if map.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(error("repeated manifest key"));
        }
    }
    let error = |message| LocatedDiagnostic::new(&canonical, &source, message, Span::default());
    let name = package
        .remove("name")
        .ok_or_else(|| error("package.name is required"))?;
    if !valid_name(&name) {
        return Err(error("package.name must begin with an ASCII letter and contain only letters, digits, '_' or '-'"));
    }
    let version = package
        .remove("version")
        .ok_or_else(|| error("package.version is required"))?;
    if version.split('.').count() != 3
        || !version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(error(
            "package.version must use major.minor.patch numeric form",
        ));
    }
    let entry = package
        .remove("entry")
        .ok_or_else(|| error("package.entry is required"))?;
    if !safe_relative(&entry)
        || Path::new(&entry).extension().and_then(|ext| ext.to_str()) != Some("bh")
    {
        return Err(error(
            "package.entry must be a relative .bh path without '..'",
        ));
    }
    for dependency in dependencies.values().chain(rust_dependencies.values()) {
        if dependency.is_empty()
            || Path::new(dependency).is_absolute()
            || dependency.contains(['\0', '\\'])
        {
            return Err(error(
                "dependencies must be explicit relative local directory paths",
            ));
        }
    }
    Ok(Manifest {
        path: canonical,
        root: root.clone(),
        name,
        version,
        entry: root.join(entry),
        dependencies,
        rust_dependencies,
    })
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}

/// Scaffold a new package without replacing any existing directory or file.
pub fn create(name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("project name must begin with an ASCII letter and contain only letters, digits, '_' or '-'".into());
    }
    let root = PathBuf::from(name);
    fs::create_dir(&root)
        .map_err(|error| format!("cannot create project '{}': {error}", root.display()))?;
    fs::create_dir(root.join("src"))
        .map_err(|error| format!("cannot create project source directory: {error}"))?;
    fs::write(root.join("bhasa.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"src/main.bh\"\n\n[dependencies]\n"))
        .map_err(|error| format!("cannot write manifest: {error}"))?;
    fs::write(
        root.join("src/main.bh"),
        "def main() {\n    print(\"Hello from Bhasa!\")\n}\n",
    )
    .map_err(|error| format!("cannot write entry source: {error}"))?;
    fs::write(root.join(".gitignore"), "/build/\n")
        .map_err(|error| format!("cannot write .gitignore: {error}"))?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{StmtKind, Type};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn imported_records_share_type_ids_with_entry_annotations() {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "bhasa-record-module-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        fs::write(directory.join("inner.bh"), "struct Inner { value: int }\n").unwrap();
        fs::write(
            directory.join("main.bh"),
            "import \"inner.bh\"\nstruct Outer { inner: Inner }\ndef extract(value: Outer) -> Inner { var copy: Outer = value; return copy.inner }\n",
        )
        .unwrap();
        let loaded = load(Some(&directory.join("main.bh"))).unwrap();
        let program = &loaded.program;
        let inner_id = program
            .type_names
            .iter()
            .position(|name| name == "Inner")
            .unwrap();
        let outer_id = program
            .type_names
            .iter()
            .position(|name| name == "Outer")
            .unwrap();
        assert_eq!(program.records.len(), 2);
        assert_eq!(
            program.record(outer_id).unwrap().fields[0].ty,
            Type::Record(inner_id)
        );
        assert_eq!(program.functions[0].params[0].ty, Type::Record(outer_id));
        assert_eq!(program.functions[0].return_type, Type::Record(inner_id));
        assert!(matches!(
            program.functions[0].body.as_ref().unwrap()[0].kind,
            StmtKind::Var {
                annotation: Some(Type::Record(id)),
                ..
            } if id == outer_id
        ));
        let inner = program.record(inner_id).unwrap();
        let diagnostic = Diagnostic::new("T001", "field location", inner.fields[0].span);
        let located = locate(&loaded.files, &diagnostic);
        assert_eq!(located.path.file_name().unwrap(), "inner.bh");
        assert_eq!(
            &located.source[located.diagnostic.span.start..located.diagnostic.span.end],
            "value: int"
        );
    }
}
