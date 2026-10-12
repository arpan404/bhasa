//! Native Bhasa tests. Each boolean test executes in an isolated process.
use crate::ast::Type;
use crate::json::Value;
use crate::project;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_TEST_FILES: usize = 256;
const MAX_TESTS: usize = 1024;
const MAX_OUTPUT: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) struct Options {
    pub source: Option<PathBuf>,
    pub filter: Option<String>,
    pub json: bool,
    pub release: bool,
    pub frontend: String,
    pub timeout: Duration,
}
pub(crate) fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        source: None,
        filter: None,
        json: false,
        release: false,
        frontend: "rust".into(),
        timeout: Duration::from_secs(10),
    };
    let mut frontend = false;
    let mut timeout = false;
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        match value.as_str() {
            "--json" if !options.json => options.json = true,
            "--release" if !options.release => options.release = true,
            "--filter" | "--frontend" | "--timeout" => {
                let next = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{value} requires a value"))?;
                if next.is_empty() {
                    return Err(format!("{value} requires a nonempty value"));
                }
                match value.as_str() {
                    "--filter" if options.filter.is_none() => options.filter = Some(next.clone()),
                    "--frontend" if !frontend && matches!(next.as_str(), "rust" | "bhasa") => {
                        options.frontend = next.clone();
                        frontend = true;
                    }
                    "--timeout" if !timeout => {
                        let seconds: u64 = next
                            .parse()
                            .map_err(|_| "--timeout requires seconds in 1..3600")?;
                        if !(1..=3600).contains(&seconds) {
                            return Err("--timeout requires seconds in 1..3600".into());
                        }
                        options.timeout = Duration::from_secs(seconds);
                        timeout = true;
                    }
                    _ => return Err(format!("invalid or repeated {value}")),
                }
                index += 1;
            }
            _ if !value.starts_with('-') && !value.is_empty() && options.source.is_none() => {
                options.source = Some(PathBuf::from(value));
            }
            _ => return Err(format!("unexpected test argument '{value}'")),
        }
        index += 1;
    }
    Ok(options)
}

fn files_in(
    directory: &Path,
    root: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > 32 {
        return Err("test folder nesting exceeds 32".into());
    }
    let mut entries = fs::read_dir(directory)
        .map_err(|e| format!("cannot read test folder '{}': {e}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "test discovery refuses symlink '{}'",
                entry.path().display()
            ));
        }
        if kind.is_dir() {
            files_in(&entry.path(), root, depth + 1, output)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "bh")
        {
            let path = fs::canonicalize(entry.path()).map_err(|e| e.to_string())?;
            if !path.starts_with(root) {
                return Err("test file escapes its package root".into());
            }
            output.push(path);
            if output.len() > MAX_TEST_FILES {
                return Err("test discovery exceeds 256 files".into());
            }
        }
    }
    Ok(())
}
fn entries(options: &Options) -> Result<Vec<PathBuf>, String> {
    if let Some(source) = &options.source {
        let path = fs::canonicalize(source)
            .map_err(|e| format!("cannot read '{}': {e}", source.display()))?;
        if path.is_dir() {
            let mut files = Vec::new();
            files_in(&path, &path, 0, &mut files)?;
            return Ok(files);
        }
        return Ok(vec![path]);
    }
    let loaded = project::load_unparsed(None).map_err(located_errors)?;
    let manifest = loaded.project.ok_or("no bhasa.toml found")?;
    let directory = manifest.root.join("tests");
    if directory.exists() {
        if fs::symlink_metadata(&directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("test discovery refuses a symlink tests folder".into());
        }
        let mut files = Vec::new();
        files_in(&directory, &manifest.root, 0, &mut files)?;
        Ok(files)
    } else {
        Ok(vec![manifest.entry])
    }
}
fn located_errors(errors: Vec<project::LocatedDiagnostic>) -> String {
    errors
        .into_iter()
        .map(|error| {
            format!(
                "{}: {}: {}",
                error.path.display(),
                error.diagnostic.code,
                error.diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn source_errors(files: &[project::SourceFile], errors: Vec<crate::Diagnostic>) -> String {
    located_errors(
        errors
            .iter()
            .map(|error| project::locate(files, error))
            .collect(),
    )
}

struct Scratch {
    path: PathBuf,
}
impl Scratch {
    fn new(parent: &Path) -> Result<Self, String> {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        for index in 0..100 {
            let path = parent.join(format!(
                ".bhasa-tests-{}-{}-{index}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("cannot create isolated test directory".into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// A descendant may leave the test's process group while retaining its pipes.
// Polling lets collection finish even when such a process never closes them.
#[cfg(unix)]
fn pipe_ready(fd: std::os::fd::RawFd, stopping: bool) -> std::io::Result<bool> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    type Count = std::ffi::c_ulong;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    type Count = std::ffi::c_uint;
    unsafe extern "C" {
        fn poll(fds: *mut PollFd, count: Count, timeout_ms: i32) -> i32;
    }
    let mut descriptor = PollFd {
        fd,
        events: 1,
        revents: 0,
    };
    let result = unsafe { poll(&mut descriptor, 1, if stopping { 0 } else { 10 }) };
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else if descriptor.revents & 0x20 != 0 {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "invalid test pipe",
        ))
    } else {
        Ok(result > 0)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn nonblocking_pipe(fd: std::os::fd::RawFd) -> Result<(), String> {
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    #[cfg(target_os = "macos")]
    const NONBLOCK: i32 = 4;
    #[cfg(target_os = "linux")]
    const NONBLOCK: i32 = 0x800;
    let flags = unsafe { fcntl(fd, 3) }; // F_GETFL
    if flags < 0 || unsafe { fcntl(fd, 4, flags | NONBLOCK) } < 0 {
        // F_SETFL
        return Err(format!(
            "cannot configure test output pipe: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn drain(
    mut pipe: impl Read,
    #[cfg(unix)] fd: std::os::fd::RawFd,
    stop: Arc<AtomicBool>,
) -> (String, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut block = [0_u8; 8192];
    let mut after_stop = 0;
    loop {
        let stopping = stop.load(Ordering::Acquire);
        // Finish bounded buffered output, even if an escaped descendant writes
        // continuously after the test has already exited.
        if stopping && after_stop > MAX_OUTPUT {
            truncated = true;
            break;
        }
        #[cfg(unix)]
        match pipe_ready(fd, stopping) {
            Ok(true) => {}
            Ok(false) if stopping => break,
            Ok(false) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
        match pipe.read(&mut block) {
            Ok(0) => break,
            Ok(count) => {
                if stopping {
                    after_stop += count;
                }
                let available = MAX_OUTPUT.saturating_sub(kept.len());
                kept.extend_from_slice(&block[..count.min(available)]);
                truncated |= count > available;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stopping {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}

fn execute(
    executable: &Path,
    index: usize,
    name: &str,
    file: &Path,
    timeout: Duration,
) -> Result<Value, String> {
    let mut command = Command::new(executable);
    command
        .arg(index.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let start = Instant::now();
    struct TestProcess(Child);
    impl Drop for TestProcess {
        fn drop(&mut self) {
            terminate_group(&mut self.0);
            let _ = self.0.wait();
        }
    }
    let mut process = TestProcess(
        command
            .spawn()
            .map_err(|e| format!("cannot run test '{name}': {e}"))?,
    );
    let child = &mut process.0;
    let stdout = child.stdout.take().ok_or("test stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("test stderr unavailable")?;
    #[cfg(unix)]
    let (out_fd, err_fd) = {
        use std::os::fd::AsRawFd;
        (stdout.as_raw_fd(), stderr.as_raw_fd())
    };
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        nonblocking_pipe(out_fd)?;
        nonblocking_pipe(err_fd)?;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let out_stop = Arc::clone(&stop);
    let err_stop = Arc::clone(&stop);
    let out = std::thread::spawn(move || {
        drain(
            stdout,
            #[cfg(unix)]
            out_fd,
            out_stop,
        )
    });
    let err = std::thread::spawn(move || {
        drain(
            stderr,
            #[cfg(unix)]
            err_fd,
            err_stop,
        )
    });
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => break Err(error.to_string()),
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            terminate_group(child);
            break child.wait().map_err(|e| e.to_string());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Cleanup also runs on wait failures. Escaped descendants are outside the
    // runner's process group, so stop collecting their inherited output.
    drop(process);
    stop.store(true, Ordering::Release);
    let out = out.join();
    let err = err.join();
    let (stdout, stdout_truncated) = out.map_err(|_| "test stdout reader failed")?;
    let (stderr, stderr_truncated) = err.map_err(|_| "test stderr reader failed")?;
    let status = status?;
    let status = if timed_out {
        "timeout"
    } else if status.success() {
        "passed"
    } else {
        "failed"
    };
    Ok(Value::Object(BTreeMap::from([
        ("name".into(), name.into()),
        ("file".into(), file.to_string_lossy().into_owned().into()),
        ("status".into(), status.into()),
        (
            "duration_ms".into(),
            (start.elapsed().as_millis().min(i64::MAX as u128) as i64).into(),
        ),
        ("stdout".into(), stdout.into()),
        ("stderr".into(), stderr.into()),
        (
            "output_truncated".into(),
            Value::Bool(stdout_truncated || stderr_truncated),
        ),
    ])))
}
fn terminate_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // This process group was created for this single test, including its children.
        unsafe {
            kill(-(child.id() as i32), 9);
        }
    }
    let _ = child.kill();
}

fn run_inner(options: &Options, results: &mut Vec<Value>) -> Result<(), String> {
    let mut total = 0;
    for entry in entries(options)? {
        let mut loaded = project::load_test_graph(&entry).map_err(located_errors)?;
        let expansion = crate::expand_files(&loaded.files)
            .map_err(|errors| source_errors(&loaded.files, errors))?;
        let program = expansion
            .parse()
            .map_err(|errors| source_errors(&loaded.files, errors))?;
        let root = &loaded.files[0];
        let mut templates = crate::generics::template_program(
            &expansion.modules.source,
            &expansion.modules.boundaries,
        )
        .map_err(|errors| {
            source_errors(&loaded.files, expansion.modules.map_diagnostics(errors))
        })?;
        expansion.modules.remap_program(&mut templates);
        for function in &templates.functions {
            let name = expansion.modules.original_name(&function.name);
            if function.span.start >= root.base
                && function.span.start < root.base + root.source.len()
                && (name.starts_with("test_") || name == "main")
            {
                return Err(format!(
                    "test '{name}' must be a non-generic def with no arguments returning bool"
                ));
            }
        }
        let mut names = Vec::new();
        for function in &program.functions {
            if function.span.start < root.base
                || function.span.start >= root.base + root.source.len()
            {
                continue;
            }
            let name = expansion.original_name(&function.name);
            if name == "main" {
                return Err(format!(
                    "test source '{}' must not define main; use a separate test file",
                    entry.display()
                ));
            }
            if !name.starts_with("test_") {
                continue;
            }
            if function.body.is_none()
                || !function.params.is_empty()
                || function.return_type != Type::Bool
            {
                return Err(format!(
                    "test '{name}' must be a non-generic def with no arguments returning bool"
                ));
            }
            if options
                .filter
                .as_ref()
                .is_none_or(|filter| name.contains(filter))
            {
                names.push(name);
            }
        }
        names.sort();
        names.dedup();
        total += names.len();
        if total > MAX_TESTS {
            return Err("test discovery exceeds 1024 tests".into());
        }
        if names.is_empty() {
            continue;
        }
        let mut harness = String::from(
            "\ndef main() -> int {\nif arg_count() != 2 { return 2 }\nvar selected = arg(1)\n",
        );
        for (index, name) in names.iter().enumerate() {
            harness.push_str(&format!(
                "if same(selected, \"{index}\") {{ if {name}() {{ return 0 }} return 1 }}\n"
            ));
        }
        harness.push_str("return 2\n}\n");
        loaded.files[0].source.push_str(&harness);
        let mut base = 0;
        for file in &mut loaded.files {
            file.base = base;
            base += file.source.len() + 1;
        }
        let parent = loaded
            .manifests
            .iter()
            .filter(|manifest| entry.starts_with(&manifest.root))
            .max_by_key(|manifest| manifest.root.components().count())
            .map(|manifest| manifest.root.join("build"))
            .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).join("build"));
        let scratch = Scratch::new(&parent)?;
        let executable = scratch
            .path
            .join(if cfg!(windows) { "tests.exe" } else { "tests" });
        crate::driver::build_test_program(loaded, &executable, &options.frontend, options.release)?;
        for (index, name) in names.iter().enumerate() {
            results.push(execute(&executable, index, name, &entry, options.timeout)?);
        }
    }
    if results.is_empty() {
        return Err(
            "no matching tests found; define def test_name() -> bool in a .bh test file".into(),
        );
    }
    Ok(())
}
pub(crate) fn run(options: Options) -> i32 {
    let mut results = Vec::new();
    let error = run_inner(&options, &mut results).err();
    let passed = results
        .iter()
        .filter(|result| {
            result
                .get("status")
                .is_some_and(|status| status.as_str() == Some("passed"))
        })
        .count();
    let success = error.is_none() && passed == results.len();
    let code = if error.is_some() {
        2
    } else if success {
        0
    } else {
        1
    };
    if options.json {
        let value = Value::Object(BTreeMap::from([
            ("schema_version".into(), 1_i64.into()),
            ("passed".into(), (passed as i64).into()),
            ("failed".into(), ((results.len() - passed) as i64).into()),
            ("tests".into(), Value::Array(results)),
            ("error".into(), error.map_or(Value::Null, Value::String)),
        ]));
        println!("{value}");
    } else {
        for result in &results {
            println!(
                "{} {}",
                result
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("failed"),
                result.get("name").and_then(Value::as_str).unwrap_or("?")
            );
            if result.get("status").and_then(Value::as_str) != Some("passed") {
                for key in ["stdout", "stderr"] {
                    if let Some(text) = result.get(key).and_then(Value::as_str) {
                        if !text.is_empty() {
                            eprint!("{text}");
                        }
                    }
                }
            }
        }
        if let Some(error) = error {
            eprintln!("error: {error}");
        } else {
            println!("{passed} passed; {} failed", results.len() - passed);
        }
    }
    code
}
