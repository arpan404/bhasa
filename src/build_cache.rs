//! Content-addressed final native artifacts. Frontend work still runs on every build.
//! Unsupported native tool plans disable reuse rather than guessing their inputs.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FORMAT: &str = "bhasa-native-artifact-v1";
const HEADER_LEN: usize = 256;
const MAX_PLAN: usize = 8 * 1024 * 1024;
static UNIQUE: AtomicU64 = AtomicU64::new(0);

pub struct CacheEntry {
    directory: Directory,
    key: String,
    command: Invocation,
    artifact: PathBuf,
    temporary: PathBuf,
}

/// Call after Cargo freshness checks, native command assembly and output validation.
/// `cache_root` is the entry project's build directory. A None result is an ordinary
/// uncached build, including unsupported platforms/tool plans or unsafe cache paths.
pub fn prepare(
    command: &Command,
    artifact: &Path,
    temporary: &Path,
    cache_root: &Path,
) -> Result<Option<CacheEntry>, String> {
    let attempt = || -> Result<CacheEntry, String> {
        let command = Invocation::capture(command)?;
        let key = fingerprint(&command, artifact, temporary)?;
        let directory = Directory::root(cache_root)?.child("native-artifacts-v1")?;
        Ok(CacheEntry {
            directory,
            key,
            command,
            artifact: artifact.to_owned(),
            temporary: temporary.to_owned(),
        })
    };
    match attempt() {
        Ok(entry) => Ok(Some(entry)),
        Err(error) => {
            if std::env::var_os("BHASA_CACHE_DEBUG").is_some() {
                eprintln!("Native cache disabled: {error}");
            }
            Ok(None)
        }
    }
}

impl CacheEntry {
    /// Copy and verify into the driver's fresh temporary output; never hard-link a
    /// mutable public output to a cache entry. Missing/corrupt entries are misses.
    pub fn restore(&self, artifact: &Path) -> Result<bool, String> {
        let name = format!("{}.bhc", self.key);
        let mut cached = match self.directory.read(&name) {
            Ok(file) => file,
            Err(_) => return Ok(false),
        };
        let metadata = cached.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Ok(false);
        }
        let mut header = [0u8; HEADER_LEN];
        if cached.read_exact(&mut header).is_err() {
            return Ok(false);
        }
        let Ok(header) = std::str::from_utf8(&header) else {
            return Ok(false);
        };
        let fields: Vec<_> = header.trim().split(' ').collect();
        if fields.len() != 6 || fields[0] != FORMAT || fields[1] != self.key {
            return Ok(false);
        }
        let mut record_hash = Sha256::new();
        record_hash.update(fields[..5].join(" ").as_bytes());
        if record_hash.finish() != fields[5] {
            return Ok(false);
        }
        let Ok(length) = fields[3].parse::<u64>() else {
            return Ok(false);
        };
        let Ok(mode) = u32::from_str_radix(fields[4], 8) else {
            return Ok(false);
        };
        if length == 0
            || length.checked_add(HEADER_LEN as u64) != Some(metadata.len())
            || mode & !0o777 != 0
        {
            return Ok(false);
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(artifact)
            .map_err(|e| format!("cannot restore native artifact: {e}"))?;
        let result = copy_hash(&mut cached, &mut output);
        let valid = matches!(result, Ok((ref hash, size)) if hash == fields[2] && size == length);
        if !valid {
            drop(output);
            let _ = fs::remove_file(artifact);
            return Ok(false);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output
                .set_permissions(fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Invoke only after a successful native compiler exit. Recheck inputs so a
    /// concurrent source/header/tool/library edit cannot publish under the old key.
    pub fn store(&self, artifact: &Path) -> Result<(), String> {
        if fingerprint(&self.command, &self.artifact, &self.temporary)
            .ok()
            .as_deref()
            != Some(&self.key)
        {
            return Ok(());
        }
        let mut input = File::open(artifact).map_err(|e| e.to_string())?;
        let metadata = input.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Ok(());
        }
        let hash = digest_reader(&mut input)?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o755;
        let header = format!("{FORMAT} {} {hash} {} {mode:o}", self.key, metadata.len());
        let mut record_hash = Sha256::new();
        record_hash.update(header.as_bytes());
        let header = format!("{header} {}", record_hash.finish());
        if header.len() >= HEADER_LEN {
            return Ok(());
        }
        let staging = format!(
            ".pending-{}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        );
        let mut output = match self.directory.create(&staging) {
            Ok(file) => file,
            Err(_) => return Ok(()),
        };
        let result = (|| -> Result<(), String> {
            let mut bytes = [b' '; HEADER_LEN];
            bytes[..header.len()].copy_from_slice(header.as_bytes());
            bytes[HEADER_LEN - 1] = b'\n';
            output.write_all(&bytes).map_err(|e| e.to_string())?;
            let mut input = File::open(artifact).map_err(|e| e.to_string())?;
            let (copied_hash, length) = copy_hash(&mut input, &mut output)?;
            if copied_hash != hash || length != metadata.len() {
                return Err("native artifact changed while caching".into());
            }
            output.sync_all().map_err(|e| e.to_string())?;
            self.directory
                .rename(&staging, &format!("{}.bhc", self.key))?;
            Ok(())
        })();
        let _ = self.directory.remove(&staging);
        // A cache write must not turn a successfully built application into a failure.
        if let Err(error) = result {
            if std::env::var_os("BHASA_CACHE_DEBUG").is_some() {
                eprintln!("Native cache store skipped: {error}");
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Invocation {
    program: PathBuf,
    args: Vec<OsString>,
    env: BTreeMap<OsString, OsString>,
    cwd: PathBuf,
}
impl Invocation {
    fn capture(command: &Command) -> Result<Self, String> {
        let mut env: BTreeMap<_, _> = std::env::vars_os().collect();
        for (key, value) in command.get_envs() {
            if let Some(value) = value {
                env.insert(key.to_owned(), value.to_owned());
            } else {
                env.remove(key);
            }
        }
        let cwd = command
            .get_current_dir()
            .map(Path::to_owned)
            .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?);
        let program = resolve_program(command.get_program(), &env, &cwd)?;
        Ok(Self {
            program,
            args: command.get_args().map(OsStr::to_owned).collect(),
            env,
            cwd,
        })
    }
    fn run(&self, args: &[OsString]) -> Result<std::process::Output, String> {
        Command::new(&self.program)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .current_dir(&self.cwd)
            .output()
            .map_err(|e| e.to_string())
    }
}
fn resolve_program(
    program: &OsStr,
    env: &BTreeMap<OsString, OsString>,
    cwd: &Path,
) -> Result<PathBuf, String> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return fs::canonicalize(if path.is_absolute() {
            path.to_owned()
        } else {
            cwd.join(path)
        })
        .map_err(|e| e.to_string());
    }
    for dir in std::env::split_paths(env.get(OsStr::new("PATH")).ok_or("PATH is missing")?) {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return fs::canonicalize(candidate).map_err(|e| e.to_string());
        }
    }
    Err("native tool is not resolvable".into())
}

fn fingerprint(command: &Invocation, artifact: &Path, temporary: &Path) -> Result<String, String> {
    let args: Vec<_> = command
        .args
        .iter()
        .map(|v| {
            v.to_str()
                .map(str::to_owned)
                .ok_or("non-UTF-8 native argument")
        })
        .collect::<Result<_, _>>()?;
    let temp = temporary.to_str().ok_or("non-UTF-8 temporary path")?;
    let out = artifact.to_str().ok_or("non-UTF-8 output path")?;
    let mut hash = Sha256::new();
    field(&mut hash, FORMAT.as_bytes());
    field(&mut hash, env!("CARGO_PKG_VERSION").as_bytes());
    field(&mut hash, command.cwd.as_os_str().as_encoded_bytes());
    for (key, value) in &command.env {
        // Shell bookkeeping changes between otherwise identical invocations.
        if key == "_" || key == "SHLVL" {
            continue;
        }
        field(&mut hash, key.as_encoded_bytes());
        field(&mut hash, value.as_encoded_bytes());
    }
    for arg in &args {
        field(&mut hash, normalize(arg, temp, out).as_bytes());
    }
    let mut files = BTreeSet::new();
    add_file(
        &mut hash,
        &mut files,
        &std::env::current_exe().map_err(|e| e.to_string())?,
        temp,
    )?;
    add_file(&mut hash, &mut files, &command.program, temp)?;
    let version = command.run(&["--version".into()])?;
    if !version.status.success() {
        return Err("native compiler version probe failed".into());
    }
    field(&mut hash, &version.stdout);
    field(&mut hash, &version.stderr);
    let mut plan_args = command.args.clone();
    plan_args.push("-###".into());
    let plan = command.run(&plan_args)?;
    if !plan.status.success() || plan.stderr.len() > MAX_PLAN {
        return Err("native tool plan probe failed".into());
    }
    let plan = std::str::from_utf8(&plan.stderr).map_err(|_| "non-UTF-8 tool plan")?;
    let commands: Vec<Vec<String>> = plan
        .lines()
        .filter(|line| line.trim_start().starts_with('"'))
        .map(words)
        .collect::<Result<_, _>>()?;
    if commands.is_empty() {
        return Err("native tool did not disclose subprocess inputs".into());
    }
    let mut produced = BTreeMap::new();
    for (index, row) in commands.iter().enumerate() {
        for pair in row.windows(2) {
            if pair[0] == "-o" {
                produced.insert(pair[1].clone(), format!("$OUTPUT_{index}"));
            }
        }
    }
    for row in &commands {
        let tool = resolve_program(
            OsStr::new(row.first().ok_or("empty subprocess plan")?),
            &command.env,
            &command.cwd,
        )?;
        add_file(&mut hash, &mut files, &tool, temp)?;
        for token in row {
            let value = produced
                .get(token)
                .cloned()
                .unwrap_or_else(|| normalize(token, temp, out));
            field(&mut hash, value.as_bytes());
            if !produced.contains_key(token) && Path::new(token).is_file() {
                if row.iter().any(|v| matches!(v.as_str(), "-cc1" | "-cc1as")) {
                    add_file(&mut hash, &mut files, Path::new(token), temp)?;
                } else {
                    add_link_file(&mut hash, &mut files, Path::new(token), temp, 0)?;
                }
            }
        }
        if !row.iter().any(|v| matches!(v.as_str(), "-cc1" | "-cc1as")) {
            link_inputs(command, row, &mut hash, &mut files, temp)?;
        }
    }
    let mut flags = Vec::new();
    let mut sources = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        match value.as_str() {
            "-o" | "-x" | "-Xlinker" | "-framework" | "-L" | "-F" => index += 1,
            "-target" | "-isysroot" | "--sysroot" | "-I" | "-isystem" | "-iquote" | "-include"
            | "-D" | "-U" => {
                flags.push(value.clone().into());
                index += 1;
                flags.push(
                    args.get(index)
                        .ok_or("missing compiler argument")?
                        .clone()
                        .into(),
                );
            }
            "-c" => {}
            _ if value.starts_with("-l")
                || value.starts_with("-L")
                || value.starts_with("-F")
                || value.starts_with("-Wl,") => {}
            _ if value.starts_with('-') => flags.push(value.clone().into()),
            _ => {
                let path = Path::new(value);
                if !path.is_file() {
                    return Err(format!("unknown compiler input {value}"));
                }
                add_file(&mut hash, &mut files, path, temp)?;
                if matches!(path.extension().and_then(OsStr::to_str), Some("c" | "m")) {
                    sources.push(path.to_owned());
                }
            }
        }
        index += 1;
    }
    for source in sources {
        let mut probe = flags.clone();
        probe.extend([
            OsString::from("-E"),
            "-P".into(),
            "-x".into(),
            if source.extension() == Some(OsStr::new("m")) {
                "objective-c".into()
            } else {
                "c".into()
            },
            source.as_os_str().to_owned(),
        ]);
        let result = command.run(&probe)?;
        if !result.status.success() {
            return Err("runtime preprocessing failed".into());
        }
        field(&mut hash, &result.stdout);
        field(&mut hash, &result.stderr);
    }
    Ok(hash.finish())
}
fn normalize(value: &str, temporary: &str, artifact: &str) -> String {
    value
        .replace(artifact, "$ARTIFACT")
        .replace(temporary, "$TEMP")
}
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn add_file(
    hash: &mut Sha256,
    seen: &mut BTreeSet<PathBuf>,
    path: &Path,
    temp: &str,
) -> Result<(), String> {
    let resolved = fs::canonicalize(path)
        .map_err(|e| format!("cannot resolve cache input {}: {e}", path.display()))?;
    if !seen.insert(resolved.clone()) {
        return Ok(());
    }
    let mut file = File::open(&resolved).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("native cache input is not a regular file".into());
    }
    field(
        hash,
        resolved.to_string_lossy().replace(temp, "$TEMP").as_bytes(),
    );
    field(hash, &meta.len().to_le_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        field(hash, &meta.mode().to_le_bytes());
    }
    field(hash, digest_reader(&mut file)?.as_bytes());
    Ok(())
}

/// Parse Clang's quoted -### output, never execute its text through a shell.
fn words(line: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut word = String::new();
    let mut quote = false;
    let mut escape = false;
    let mut active = false;
    for ch in line.chars() {
        if escape {
            word.push(ch);
            escape = false;
            active = true;
        } else if ch == '\\' {
            escape = true;
        } else if ch == '"' {
            quote = !quote;
            active = true;
        } else if ch.is_whitespace() && !quote {
            if active {
                result.push(std::mem::take(&mut word));
                active = false;
            }
        } else {
            word.push(ch);
            active = true;
        }
    }
    if escape || quote {
        return Err("unrecognized native tool plan quoting".into());
    }
    if active {
        result.push(word);
    }
    Ok(result)
}

fn link_inputs(
    command: &Invocation,
    row: &[String],
    hash: &mut Sha256,
    files: &mut BTreeSet<PathBuf>,
    temp: &str,
) -> Result<(), String> {
    let mut directories = Vec::new();
    let mut frameworks = Vec::new();
    let mut libraries = Vec::new();
    let mut framework_names = Vec::new();
    let mut sysroot = None;
    let mut index = 1;
    while index < row.len() {
        let value = &row[index];
        if matches!(
            value.as_str(),
            "-L" | "-F" | "-syslibroot" | "--sysroot" | "-framework"
        ) {
            index += 1;
            let next = row.get(index).ok_or("truncated linker plan")?;
            match value.as_str() {
                "-L" => directories.push(PathBuf::from(next)),
                "-F" => frameworks.push(PathBuf::from(next)),
                "-framework" => framework_names.push(next.clone()),
                _ => sysroot = Some(PathBuf::from(next)),
            }
        } else if let Some(path) = value.strip_prefix("-L") {
            directories.push(PathBuf::from(path));
        } else if let Some(path) = value.strip_prefix("-F") {
            frameworks.push(PathBuf::from(path));
        } else if let Some(name) = value.strip_prefix("-l") {
            if !name.is_empty() && !matches!(value.as_str(), "-lto_library") {
                libraries.push(name.to_owned());
            }
        } else if value.starts_with('@')
            || matches!(value.as_str(), "-T" | "--script" | "-filelist")
        {
            return Err("custom linker response files/scripts are not cached".into());
        }
        index += 1;
    }
    if let Some(root) = &sysroot {
        directories.push(root.join("usr/lib"));
        frameworks.push(root.join("System/Library/Frameworks"));
    }
    let apple = row.iter().any(|v| v == "-platform_version");
    for library in libraries {
        let names: Vec<String> = if let Some(exact) = library.strip_prefix(':') {
            vec![exact.to_owned()]
        } else if apple {
            vec![
                format!("lib{library}.tbd"),
                format!("lib{library}.dylib"),
                format!("lib{library}.a"),
            ]
        } else {
            vec![format!("lib{library}.so"), format!("lib{library}.a")]
        };
        let mut found = None;
        for directory in &directories {
            for name in &names {
                let path = directory.join(name);
                if path.is_file() {
                    found = Some(path);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        if found.is_none() && !apple {
            for name in &names {
                let mut flags: Vec<_> = command
                    .args
                    .windows(2)
                    .filter(|pair| pair[0] == "-target")
                    .flat_map(|pair| pair.to_vec())
                    .collect();
                flags.push(format!("-print-file-name={name}").into());
                let output = command.run(&flags)?;
                let value =
                    String::from_utf8(output.stdout).map_err(|_| "non-UTF-8 library path")?;
                let path = PathBuf::from(value.trim());
                if output.status.success() && path.is_absolute() && path.is_file() {
                    found = Some(path);
                    break;
                }
            }
        }
        let path = found.ok_or_else(|| format!("cannot resolve native link library {library}"))?;
        add_link_file(hash, files, &path, temp, 0)?;
    }
    for name in framework_names {
        let path = frameworks
            .iter()
            .map(|dir| dir.join(format!("{name}.framework/{name}.tbd")))
            .find(|p| p.is_file())
            .ok_or_else(|| format!("cannot resolve framework {name}"))?;
        add_link_file(hash, files, &path, temp, 0)?;
    }
    Ok(())
}
fn add_link_file(
    hash: &mut Sha256,
    files: &mut BTreeSet<PathBuf>,
    path: &Path,
    temp: &str,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("native linker script nesting exceeds cache limit".into());
    }
    add_file(hash, files, path, temp)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut prefix = [0u8; 256];
    let count = file.read(&mut prefix).map_err(|e| e.to_string())?;
    let prefix = &prefix[..count];
    if prefix.starts_with(b"!<thin>\n") {
        return Err("thin archives need external member tracking and are not cached".into());
    }
    if path.extension() == Some(OsStr::new("tbd")) {
        // Apple SDK stubs may embed their reexports, or refer to another stub.
        // Hash both forms, including reexport changes outside the initial file.
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let embedded: BTreeSet<_> = text
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("install-name:")
                    .map(|name| name.trim().trim_matches(['\'', '"']))
            })
            .collect();
        let path_text = path.to_string_lossy();
        let root = path_text
            .find("/usr/lib/")
            .or_else(|| path_text.find("/System/Library/"))
            .map(|index| PathBuf::from(&path_text[..index]));
        for token in text.split(|ch: char| ch.is_whitespace() || "'\"[],".contains(ch)) {
            if !(token.starts_with("/usr/lib/") || token.starts_with("/System/Library/"))
                || embedded.contains(token)
            {
                continue;
            }
            let root = root
                .as_ref()
                .ok_or("cannot resolve SDK stub reexport root")?;
            let candidate = root.join(token.trim_start_matches('/'));
            let stub = if candidate.extension() == Some(OsStr::new("dylib")) {
                candidate.with_extension("tbd")
            } else {
                PathBuf::from(format!("{}.tbd", candidate.display()))
            };
            let selected = if stub.is_file() {
                stub
            } else if candidate.is_file() {
                candidate
            } else {
                return Err(format!("cannot resolve SDK reexport {token}"));
            };
            let canonical = fs::canonicalize(&selected).map_err(|e| e.to_string())?;
            if !files.contains(&canonical) {
                add_link_file(hash, files, &selected, temp, depth + 1)?;
            }
        }
        return Ok(());
    }
    let binary = prefix.starts_with(b"!<arch>\n")
        || prefix.starts_with(b"\x7fELF")
        || prefix.starts_with(b"\xcf\xfa\xed\xfe")
        || prefix.starts_with(b"\xce\xfa\xed\xfe")
        || prefix.starts_with(b"\xfe\xed\xfa\xcf")
        || prefix.starts_with(b"\xfe\xed\xfa\xce")
        || prefix.starts_with(b"\xca\xfe\xba\xbe")
        || prefix.starts_with(b"\xbe\xba\xfe\xca")
        || prefix.starts_with(b"\x00asm")
        || prefix.starts_with(b"MZ");
    if binary {
        return Ok(());
    }
    if !prefix.starts_with(b"/* GNU ld script") {
        return Err(format!("unrecognized linker input {}", path.display()));
    }
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PLAN as u64 {
        return Err("native linker script exceeds cache limit".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "invalid linker script")?;
    // Default GNU system .so scripts use absolute INPUT/GROUP dependencies.
    // Relative or -l references are deliberately uncached until fully resolved.
    let start = text
        .find("GROUP")
        .or_else(|| text.find("INPUT"))
        .ok_or("unknown linker script")?;
    for word in text[start..].split(|ch: char| ch.is_whitespace() || "(),;".contains(ch)) {
        if word.starts_with('/') {
            add_link_file(hash, files, Path::new(word), temp, depth + 1)?;
        } else if !word.is_empty() && !matches!(word, "GROUP" | "INPUT" | "AS_NEEDED") {
            return Err("unresolved linker script input".into());
        }
    }
    Ok(())
}

fn copy_hash(input: &mut impl Read, output: &mut impl Write) -> Result<(String, u64), String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut length = 0u64;
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        hash.update(&buffer[..count]);
        length += count as u64;
    }
    Ok((hash.finish(), length))
}
fn digest_reader(input: &mut impl Read) -> Result<String, String> {
    copy_hash(input, &mut std::io::sink()).map(|(hash, _)| hash)
}

/// Content digest shared by package locks. No process or network access.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finish()
}

// macOS supplies an accelerated SHA-256 implementation in libSystem. This does
// not add an OpenSSL requirement. Other hosts use the dependency-free fallback.
#[cfg(target_os = "macos")]
#[repr(C)]
struct Sha256 {
    count: [u32; 2],
    hash: [u32; 8],
    words: [u32; 16],
}
#[cfg(target_os = "macos")]
#[link(name = "System")]
unsafe extern "C" {
    fn CC_SHA256_Init(context: *mut Sha256) -> std::ffi::c_int;
    fn CC_SHA256_Update(
        context: *mut Sha256,
        data: *const std::ffi::c_void,
        length: u32,
    ) -> std::ffi::c_int;
    fn CC_SHA256_Final(output: *mut u8, context: *mut Sha256) -> std::ffi::c_int;
}
#[cfg(target_os = "macos")]
impl Sha256 {
    fn new() -> Self {
        let mut value = Self {
            count: [0; 2],
            hash: [0; 8],
            words: [0; 16],
        };
        // The repr(C) context matches the installed CommonCrypto/CommonDigest.h
        // CC_SHA256_CTX: two count words, eight digest words, sixteen block words.
        assert_eq!(unsafe { CC_SHA256_Init(&mut value) }, 1);
        value
    }
    fn update(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(u32::MAX as usize) {
            assert_eq!(
                unsafe { CC_SHA256_Update(self, chunk.as_ptr().cast(), chunk.len() as u32) },
                1
            );
        }
    }
    fn finish(mut self) -> String {
        let mut output = [0u8; 32];
        assert_eq!(
            unsafe { CC_SHA256_Final(output.as_mut_ptr(), &mut self) },
            1
        );
        output.iter().map(|v| format!("{v:02x}")).collect()
    }
}
#[cfg(not(target_os = "macos"))]
type Sha256 = PortableSha256;

// Standard SHA-256 fallback. It remains covered on macOS by known-vector tests.
#[cfg(any(not(target_os = "macos"), test))]
struct PortableSha256 {
    state: [u32; 8],
    block: [u8; 64],
    used: usize,
    length: u64,
}
#[cfg(any(not(target_os = "macos"), test))]
impl PortableSha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            used: 0,
            length: 0,
        }
    }
    fn update(&mut self, mut bytes: &[u8]) {
        self.length += bytes.len() as u64;
        while !bytes.is_empty() {
            let n = (64 - self.used).min(bytes.len());
            self.block[self.used..self.used + n].copy_from_slice(&bytes[..n]);
            self.used += n;
            bytes = &bytes[n..];
            if self.used == 64 {
                self.compress();
                self.used = 0;
            }
        }
    }
    fn compress(&mut self) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (i, word) in self.block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let a = w[i - 15];
            let b = w[i - 2];
            w[i] = w[i - 16]
                .wrapping_add(a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3))
                .wrapping_add(w[i - 7])
                .wrapping_add(b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10));
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let t = h
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let u = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t);
            d = c;
            c = b;
            b = a;
            a = t.wrapping_add(u);
        }
        for (state, v) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(v);
        }
    }
    fn finish(mut self) -> String {
        let bits = self.length * 8;
        self.update(&[0x80]);
        while self.used != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        self.state.iter().map(|v| format!("{v:08x}")).collect()
    }
}

// Descriptor-relative cache operations prevent symlink swaps from redirecting
// writes or cache reads outside the chosen directory, including during races.
#[cfg(unix)]
struct Directory(File);
#[cfg(unix)]
mod unix {
    use std::ffi::{c_char, c_int, c_uint};
    unsafe extern "C" {
        pub fn openat(fd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
        pub fn mkdirat(fd: c_int, path: *const c_char, mode: c_uint) -> c_int;
        pub fn renameat(a: c_int, p: *const c_char, b: c_int, q: *const c_char) -> c_int;
        pub fn unlinkat(fd: c_int, p: *const c_char, flags: c_int) -> c_int;
    }
    #[cfg(target_os = "macos")]
    pub const SAFE: c_int = 0x100 | 0x1000000 | 0x4;
    #[cfg(not(target_os = "macos"))]
    pub const SAFE: c_int = 0x20000 | 0x80000 | 0x800;
    #[cfg(target_os = "macos")]
    pub const CREATE: c_int = 1 | 0x200 | 0x800;
    #[cfg(not(target_os = "macos"))]
    pub const CREATE: c_int = 1 | 0x40 | 0x80;
}
#[cfg(unix)]
impl Directory {
    fn root(path: &Path) -> Result<Self, String> {
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                return Err("cache root is a symlink".into());
            }
        }
        if !path.exists() {
            let parent = Self::root(path.parent().ok_or("invalid cache parent")?)?;
            return parent.child(
                path.file_name()
                    .and_then(OsStr::to_str)
                    .ok_or("invalid cache directory")?,
            );
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(unix::SAFE)
            .open(path)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_dir() {
            return Err("cache root is not a directory".into());
        }
        Ok(Self(file))
    }
    fn name(value: &str) -> Result<std::ffi::CString, String> {
        if value.is_empty() || value.contains('/') || matches!(value, "." | "..") {
            return Err("invalid cache leaf".into());
        }
        std::ffi::CString::new(value).map_err(|e| e.to_string())
    }
    fn child(&self, name: &str) -> Result<Self, String> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = Self::name(name)?;
        unsafe {
            unix::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o700);
        }
        let fd = unsafe { unix::openat(self.0.as_raw_fd(), name.as_ptr(), unix::SAFE) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if !file.metadata().map_err(|e| e.to_string())?.is_dir() {
            return Err("cache child is not a directory".into());
        }
        Ok(Self(file))
    }
    fn open(&self, name: &str, flags: i32) -> Result<File, String> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = Self::name(name)?;
        let fd = unsafe {
            unix::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                unix::SAFE | flags,
                0o600u32,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn read(&self, name: &str) -> Result<File, String> {
        self.open(name, 0)
    }
    fn create(&self, name: &str) -> Result<File, String> {
        self.open(name, unix::CREATE)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        use std::os::fd::AsRawFd;
        let from = Self::name(from)?;
        let to = Self::name(to)?;
        if unsafe {
            unix::renameat(
                self.0.as_raw_fd(),
                from.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
            )
        } < 0
        {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        use std::os::fd::AsRawFd;
        let name = Self::name(name)?;
        if unsafe { unix::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) } < 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
}
#[cfg(not(unix))]
struct Directory;
#[cfg(not(unix))]
impl Directory {
    fn root(_: &Path) -> Result<Self, String> {
        Err("native cache currently requires descriptor-relative Unix file operations".into())
    }
    fn child(&self, _: &str) -> Result<Self, String> {
        Err("unsupported platform".into())
    }
    fn read(&self, _: &str) -> Result<File, String> {
        Err("unsupported platform".into())
    }
    fn create(&self, _: &str) -> Result<File, String> {
        Err("unsupported platform".into())
    }
    fn rename(&self, _: &str, _: &str) -> Result<(), String> {
        Err("unsupported platform".into())
    }
    fn remove(&self, _: &str) -> Result<(), String> {
        Err("unsupported platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_vectors_and_chunk_boundaries() {
        for (input, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            let mut hash = Sha256::new();
            for b in input.bytes() {
                hash.update(&[b]);
            }
            assert_eq!(hash.finish(), expected);
            let mut portable = PortableSha256::new();
            portable.update(input.as_bytes());
            assert_eq!(portable.finish(), expected);
        }
        let mut hash = Sha256::new();
        for _ in 0..1000 {
            hash.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hash.finish(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
