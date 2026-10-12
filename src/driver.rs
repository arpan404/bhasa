//! Command-line parsing and the installed Clang toolchain boundary.
use crate::ast::Span;
use crate::diagnostic::Diagnostic;
use crate::target::{self, Target};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HELP: &str = "Bhasa compiler\n\n\
Usage:\n\
  bhasa new NAME\n\
  bhasa fmt FILE [--check]\n\
  bhasa lsp\n\
  bhasa test [FILE|DIRECTORY] [--filter TEXT] [--json] [--release]\n\
             [--frontend rust|bhasa] [--timeout SECONDS]\n\
  bhasa check [FILE] [--json]\n\
  bhasa emit-llvm FILE [--target TARGET] [-o FILE]\n\
  bhasa build [FILE] [-o FILE] [--target TARGET] [--release]\n\
              [--emit object] [--link FILE ...] [--gpu metal|cuda|vulkan]\n\
  bhasa run [FILE] [--release] [--link FILE ...] [--gpu metal|cuda|vulkan]\n\
                [-- ARGS...]\n\
  bhasa targets\n\
  bhasa --help\n\
  bhasa --version\n\n\
Build outputs default to build/<source-name>; objects use .o or .obj.\n\
Without FILE, check/build/run use the nearest bhasa.toml package entry.\n\
Use --frontend rust|bhasa with check/emit-llvm/build/run. The Bhasa frontend\n\
requires a host built with sh bootstrap/build-hybrid.sh --frontend.\n\
emit-llvm writes to stdout unless -o is given. check --json writes a\n\
JSON array of diagnostics to stdout, including [] on success.\n\
Clang is required only for build and run. Set BHASA_CLANG to an executable\n\
path to select it. Cross objects require a target backend; cross executables\n\
also require installed target libraries, an SDK, and a linker. No downloads\n\
are performed. run always targets the current host.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Check,
    EmitLlvm,
    Build,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frontend {
    Rust,
    Bhasa,
}

#[derive(Debug)]
struct Options {
    action: Action,
    source: PathBuf,
    output: Option<PathBuf>,
    target: Target,
    release: bool,
    object: bool,
    json: bool,
    links: Vec<PathBuf>,
    run_args: Vec<String>,
    gpu: Option<String>,
    frontend: Frontend,
    source_files: Vec<crate::project::SourceFile>,
    manifests: Vec<crate::project::Manifest>,
    project_output: Option<PathBuf>,
}

#[derive(Debug)]
enum Invocation {
    Help,
    Version,
    Targets,
    Lsp,
    New(String),
    Format { path: PathBuf, check: bool },
    Test(crate::test_runner::Options),
    Compile(Options),
}

fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(Invocation::Help);
    };
    if matches!(command, "--help" | "-h" | "--version" | "targets" | "lsp") {
        if args.len() != 1 {
            return Err(format!("{command} does not accept additional arguments"));
        }
        return Ok(match command {
            "--version" => Invocation::Version,
            "lsp" => Invocation::Lsp,
            "targets" => Invocation::Targets,
            _ => Invocation::Help,
        });
    }
    if command == "new" {
        if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        if args.len() != 2 || args[1].is_empty() {
            return Err("new requires one project name".into());
        }
        return Ok(Invocation::New(args[1].clone()));
    }
    if command == "fmt" {
        if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        let mut path = None;
        let mut check = false;
        for arg in &args[1..] {
            if arg == "--check" && !check {
                check = true;
            } else if !arg.is_empty() && !arg.starts_with('-') && path.is_none() {
                path = Some(PathBuf::from(arg));
            } else {
                return Err(format!(
                    "unexpected fmt argument '{arg}'; use fmt FILE [--check]"
                ));
            }
        }
        return Ok(Invocation::Format {
            path: path.ok_or("fmt requires a source file")?,
            check,
        });
    }
    if command == "test" {
        if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
            return Ok(Invocation::Help);
        }
        return crate::test_runner::parse(&args[1..]).map(Invocation::Test);
    }
    let action = match command {
        "check" => Action::Check,
        "emit-llvm" => Action::EmitLlvm,
        "build" => Action::Build,
        "run" => Action::Run,
        _ => return Err(format!("unknown command '{command}'; see bhasa --help")),
    };
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Invocation::Help);
    }
    let mut source = None;
    let mut output = None;
    let mut selected_target = None;
    let mut release = false;
    let mut object = false;
    let mut json = false;
    let mut links = Vec::new();
    let mut run_args = Vec::new();
    let mut gpu = None;
    let mut frontend = None;
    let mut index = 1;
    let mut positional_only = false;
    while index < args.len() {
        let arg = &args[index];
        if !positional_only && arg == "--" {
            if action == Action::Run {
                run_args.extend_from_slice(&args[index + 1..]);
                break;
            }
            positional_only = true;
        } else if !positional_only && arg.starts_with('-') {
            match arg.as_str() {
                "--json" if action == Action::Check && !json => json = true,
                "--frontend" if frontend.is_none() => {
                    frontend = Some(match option_value(args, &mut index, "--frontend")? {
                        "rust" => Frontend::Rust,
                        "bhasa" => Frontend::Bhasa,
                        _ => return Err("--frontend requires rust or bhasa".into()),
                    });
                }
                "--release" if matches!(action, Action::Build | Action::Run) && !release => {
                    release = true;
                }
                "-o" if matches!(action, Action::Build | Action::EmitLlvm) && output.is_none() => {
                    output = Some(PathBuf::from(option_value(args, &mut index, "-o")?));
                }
                "--target"
                    if matches!(action, Action::Build | Action::EmitLlvm)
                        && selected_target.is_none() =>
                {
                    selected_target =
                        Some(Target::parse(option_value(args, &mut index, "--target")?)?);
                }
                "--emit" if action == Action::Build && !object => {
                    let value = option_value(args, &mut index, "--emit")?;
                    if value != "object" {
                        return Err(format!("unsupported emission '{value}'; use --emit object"));
                    }
                    object = true;
                }
                "--link" if matches!(action, Action::Build | Action::Run) => {
                    let before = links.len();
                    while let Some(value) = args.get(index + 1) {
                        if value.starts_with('-') {
                            break;
                        }
                        if value.is_empty() {
                            return Err("--link requires a nonempty file path".into());
                        }
                        index += 1;
                        links.push(PathBuf::from(value));
                    }
                    if links.len() == before {
                        return Err("--link requires at least one file path".into());
                    }
                }
                "--gpu" if matches!(action, Action::Build | Action::Run) && gpu.is_none() => {
                    let backend = option_value(args, &mut index, "--gpu")?;
                    if !matches!(backend, "metal" | "cuda" | "vulkan") {
                        return Err("--gpu requires metal, cuda, or vulkan".into());
                    }
                    gpu = Some(backend.to_owned());
                }
                _ => {
                    return Err(format!(
                        "unknown, repeated, or unsupported option '{arg}' for {command}"
                    ))
                }
            }
        } else if source.is_none() && !arg.is_empty() {
            source = Some(PathBuf::from(arg));
        } else {
            return Err(format!(
                "unexpected argument '{arg}'; {command} takes one source file"
            ));
        }
        index += 1;
    }
    if object && !links.is_empty() {
        return Err(
            "--link cannot be used with --emit object; link inputs are only used for executables"
                .into(),
        );
    }
    if object && gpu.is_some() {
        return Err(
            "--gpu selects a runtime for executable linking and cannot be used with --emit object"
                .into(),
        );
    }
    Ok(Invocation::Compile(Options {
        action,
        source: if action == Action::EmitLlvm {
            source.ok_or("emit-llvm requires a source file")?
        } else {
            source.unwrap_or_default()
        },
        output,
        target: selected_target.unwrap_or(Target::parse("native")?),
        release,
        object,
        json,
        links,
        run_args,
        gpu,
        frontend: frontend.unwrap_or(Frontend::Rust),
        source_files: Vec::new(),
        manifests: Vec::new(),
        project_output: None,
    }))
}

fn option_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, String> {
    *index += 1;
    match args.get(*index) {
        Some(value) if !value.is_empty() && !value.starts_with('-') => Ok(value),
        _ => Err(format!(
            "{flag} requires a value; use ./ before a path beginning with '-'"
        )),
    }
}

/// Run the compiler with arguments that exclude the executable name.
pub fn run(args: &[String]) -> i32 {
    let invocation = match parse_args(args) {
        Ok(invocation) => invocation,
        Err(message) => {
            let json = args.first().is_some_and(|value| value == "check")
                && args.iter().any(|value| value == "--json");
            report_driver_error(&message, "", "<command line>", json);
            return 2;
        }
    };
    match invocation {
        Invocation::Help => write_stdout(HELP),
        Invocation::Version => write_stdout(&format!(
            "bhasa {} ({})\n",
            env!("CARGO_PKG_VERSION"),
            if cfg!(feature = "bhasa-frontend") {
                "Rust host; Bhasa frontend available"
            } else if cfg!(feature = "bhasa-lexer") {
                "Bhasa lexer; Rust host"
            } else {
                "Rust bootstrap"
            }
        )),
        Invocation::Targets => write_stdout(&target::listing()),
        Invocation::Lsp => crate::lsp::run(),
        Invocation::New(name) => match crate::project::create(&name) {
            Ok(path) => {
                eprintln!(
                    "Created package '{}'. Run: cd {} && bhasa run",
                    path.display(),
                    path.display()
                );
                0
            }
            Err(message) => {
                eprintln!("error: {message}");
                1
            }
        },
        Invocation::Format { path, check } => crate::format::run(&path, check),
        Invocation::Test(options) => crate::test_runner::run(options),
        Invocation::Compile(options) => compile(options),
    }
}

fn write_stdout(text: &str) -> i32 {
    match io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: cannot write stdout: {error}");
            1
        }
    }
}

fn compile(mut options: Options) -> i32 {
    let source_path = if options.source.as_os_str().is_empty() {
        None
    } else {
        Some(options.source.as_path())
    };
    let loaded = match crate::project::load_unparsed(source_path) {
        Ok(loaded) => loaded,
        Err(diagnostics) => return report_located_diagnostics(&diagnostics, options.json),
    };
    options.source = loaded.entry;
    options.source_files = loaded.files;
    options.manifests = loaded.manifests;
    options.project_output = loaded
        .project
        .map(|manifest| manifest.root.join("build").join(manifest.name));
    let path = options.source.display().to_string();
    let source = options.source_files[0].source.clone();
    let checked = match options.frontend {
        Frontend::Bhasa => None,
        Frontend::Rust => match crate::check_files(&options.source_files) {
            Ok(checked) => Some(checked),
            Err(diagnostics) => {
                return report_source_diagnostics(
                    &options,
                    &diagnostics,
                    &source,
                    &path,
                    options.json,
                )
            }
        },
    };
    if options.action == Action::Check {
        if options.frontend == Frontend::Bhasa {
            let target = match options.target.triple_for_ir() {
                Ok(target) => target,
                Err(message) => {
                    report_driver_error(&message, &source, &path, options.json);
                    return 1;
                }
            };
            if let Err(diagnostics) = emit_ir(&options, checked.as_ref(), &source, &target) {
                return report_source_diagnostics(
                    &options,
                    &diagnostics,
                    &source,
                    &path,
                    options.json,
                );
            }
        }
        return if options.json {
            write_stdout("[]\n")
        } else {
            0
        };
    }
    let result = if options.action == Action::EmitLlvm {
        options.target.triple_for_ir().and_then(|triple| {
            let ir = match emit_ir(&options, checked.as_ref(), &source, &triple) {
                Ok(ir) => ir,
                Err(diagnostics) => {
                    return Ok(report_source_diagnostics(
                        &options,
                        &diagnostics,
                        &source,
                        &path,
                        false,
                    ))
                }
            };
            match &options.output {
                Some(output) => {
                    write_artifact(output, ir.as_bytes(), &source_inputs(&options))?;
                    Ok(0)
                }
                None => Ok(write_stdout(&ir)),
            }
        })
    } else {
        build(&options, checked.as_ref(), &source, &path)
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            report_driver_error(&message, &source, &path, false);
            1
        }
    }
}

fn emit_ir(
    options: &Options,
    checked: Option<&crate::ast::CheckedProgram>,
    source: &str,
    target: &str,
) -> Result<String, Vec<Diagnostic>> {
    match options.frontend {
        Frontend::Rust => {
            crate::codegen::emit_llvm(checked.expect("Rust frontend was checked"), target)
        }
        Frontend::Bhasa => {
            if options.source_files.is_empty() {
                crate::compile_with_bhasa(source, target)
            } else {
                crate::compile_files_with_bhasa(&options.source_files, target)
            }
        }
    }
}

pub(crate) fn build_test_program(
    loaded: crate::project::LoadedProgram,
    executable: &Path,
    frontend: &str,
    release: bool,
) -> Result<(), String> {
    let options = Options {
        action: Action::Build,
        source: loaded.entry,
        output: Some(executable.to_owned()),
        target: Target::Native,
        release,
        object: false,
        json: false,
        links: Vec::new(),
        run_args: Vec::new(),
        gpu: None,
        frontend: if frontend == "bhasa" {
            Frontend::Bhasa
        } else {
            Frontend::Rust
        },
        source_files: loaded.files,
        manifests: loaded.manifests,
        project_output: None,
    };
    let checked = if options.frontend == Frontend::Rust {
        Some(crate::check_files(&options.source_files).map_err(|errors| {
            errors
                .iter()
                .map(|error| {
                    let located = crate::project::locate(&options.source_files, error);
                    format!(
                        "{}: {}: {}",
                        located.path.display(),
                        error.code,
                        error.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?)
    } else {
        None
    };
    let source = &options.source_files[0].source;
    let path = options.source.display().to_string();
    match build(&options, checked.as_ref(), source, &path)? {
        0 => Ok(()),
        _ => Err("test compilation failed".into()),
    }
}

fn source_inputs(options: &Options) -> Vec<&Path> {
    let mut inputs = vec![options.source.as_path()];
    inputs.extend(options.source_files.iter().map(|file| file.path.as_path()));
    inputs.extend(
        options
            .manifests
            .iter()
            .map(|manifest| manifest.path.as_path()),
    );
    inputs
}

fn report_source_diagnostics(
    options: &Options,
    diagnostics: &[Diagnostic],
    source: &str,
    path: &str,
    json: bool,
) -> i32 {
    if options.source_files.is_empty() {
        return report_diagnostics(diagnostics, source, path, json);
    }
    let displayed = if diagnostics
        .iter()
        .any(|error| error.message.contains("__bhg_") || error.message.contains("__bhm_"))
    {
        crate::expand_files(&options.source_files)
            .ok()
            .map(|expanded| expanded.display_diagnostics(diagnostics.to_vec()))
    } else {
        None
    };
    let located = displayed
        .as_deref()
        .unwrap_or(diagnostics)
        .iter()
        .map(|diagnostic| crate::project::locate(&options.source_files, diagnostic))
        .collect::<Vec<_>>();
    report_located_diagnostics(&located, json)
}

fn report_located_diagnostics(
    diagnostics: &[crate::project::LocatedDiagnostic],
    json: bool,
) -> i32 {
    if json {
        let entries = diagnostics
            .iter()
            .map(|error| {
                error
                    .diagnostic
                    .to_json(&error.source, &error.path.display().to_string())
            })
            .collect::<Vec<_>>();
        write_stdout(&format!("[{}]\n", entries.join(",")));
    } else {
        for error in diagnostics {
            eprintln!(
                "{}",
                error
                    .diagnostic
                    .render(&error.source, &error.path.display().to_string())
            );
        }
    }
    1
}

fn report_diagnostics(diagnostics: &[Diagnostic], source: &str, path: &str, json: bool) -> i32 {
    if json {
        let entries: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.to_json(source, path))
            .collect();
        write_stdout(&format!("[{}]\n", entries.join(",")));
    } else {
        for diagnostic in diagnostics {
            eprintln!("{}", diagnostic.render(source, path));
        }
    }
    1
}

fn report_driver_error(message: &str, source: &str, path: &str, json: bool) {
    if json {
        let diagnostic = Diagnostic::new("D001", message, Span::default());
        report_diagnostics(&[diagnostic], source, path, true);
    } else {
        eprintln!("error: {message}");
    }
}

struct Toolchain {
    executable: OsString,
}

impl Toolchain {
    fn discover() -> Result<Self, String> {
        let executable = std::env::var_os("BHASA_CLANG").unwrap_or_else(|| OsString::from("clang"));
        if executable.is_empty() {
            return Err("BHASA_CLANG is empty; set it to a Clang executable path or unset it to use clang from PATH".into());
        }
        Ok(Self { executable })
    }

    fn invoke(&self, command: &mut Command) -> Result<Output, String> {
        command.output().map_err(|error| {
            format!(
                "cannot execute Clang '{}': {error}. Install Clang or set BHASA_CLANG to its executable path. No shell arguments are accepted in BHASA_CLANG",
                self.executable.to_string_lossy()
            )
        })
    }

    fn resolved_triple(&self, target: &Target) -> Result<String, String> {
        // The driver-level triple queries omit some SDK/deployment defaults.
        // Ask Clang's frontend for the target it actually puts in LLVM IR.
        // Empty stdin needs no headers, source files, system libraries or linker.
        let mut command = Command::new(&self.executable);
        if !target.is_native() {
            command.arg("-target").arg(target.triple_for_ir()?);
        }
        command.args(["-S", "-emit-llvm", "-x", "c", "-", "-o", "-"]);
        command.stdin(Stdio::null());
        let output = self.invoke(&mut command)?;
        if !output.status.success() {
            return Err(format!(
                "Clang could not resolve target '{}':\n{}",
                target.alias(),
                tool_output(&output)
            ));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        let module = String::from_utf8(output.stdout)
            .map_err(|_| "Clang returned target information that is not UTF-8".to_string())?;
        parse_resolved_triple(&module)
    }
}

fn parse_resolved_triple(module: &str) -> Result<String, String> {
    let mut triples = module.lines().filter_map(|line| {
        line.trim()
            .strip_prefix("target triple = \"")?
            .strip_suffix('"')
    });
    let triple = triples
        .next()
        .ok_or("Clang returned no LLVM target triple")?;
    if triples.next().is_some() {
        return Err("Clang returned multiple LLVM target triples".into());
    }
    if triple.is_empty()
        || triple.starts_with('-')
        || !triple
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        || !triple.contains('-')
    {
        return Err("Clang returned an invalid LLVM target triple".into());
    }
    Ok(triple.to_string())
}

fn build(
    options: &Options,
    checked: Option<&crate::ast::CheckedProgram>,
    source: &str,
    path: &str,
) -> Result<i32, String> {
    build_with_toolchain(options, checked, source, path, &Toolchain::discover()?)
}

fn build_with_toolchain(
    options: &Options,
    checked: Option<&crate::ast::CheckedProgram>,
    source: &str,
    path: &str,
    toolchain: &Toolchain,
) -> Result<i32, String> {
    if !options.object && checked.is_some_and(|checked| !checked.signatures.contains_key("main")) {
        return Err("cannot link an executable without a main function; add def main() or use --emit object".into());
    }
    let triple = toolchain.resolved_triple(&options.target)?;
    if options.action == Action::Run && !target::host_can_run(&triple) {
        return Err(format!("refusing to run target '{triple}' on this host; select a host Clang with BHASA_CLANG. Use build --emit object for cross compilation"));
    }
    if triple.starts_with("wasm32-") && !options.object {
        return Err("wasm32 currently supports object emission only; use --emit object with pure functions. Runnable WASI and host imports are not supported".into());
    }
    let ir = match emit_ir(options, checked, source, &triple) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return Ok(report_source_diagnostics(
                options,
                &diagnostics,
                source,
                path,
                false,
            ))
        }
    };
    if !options.object && options.frontend == Frontend::Bhasa && !ir.contains("define i32 @main(") {
        return Err("cannot link an executable without a main function; add def main() or use --emit object".into());
    }
    let output = options
        .output
        .clone()
        .unwrap_or_else(|| default_output(options, &triple));
    let mut links = options
        .links
        .iter()
        .map(|link| {
            let absolute = fs::canonicalize(link)
                .map_err(|error| format!("cannot read link input '{}': {error}", link.display()))?;
            if !absolute.is_file() {
                return Err(format!("link input '{}' is not a file", link.display()));
            }
            Ok(absolute)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut rust_link_args = Vec::new();
    let mut rust_roots = Vec::new();
    let rust_cache = options
        .manifests
        .iter()
        .filter(|manifest| options.source.starts_with(&manifest.root))
        .max_by_key(|manifest| manifest.root.components().count())
        .map(|manifest| manifest.root.join("build"))
        .unwrap_or_else(|| {
            options
                .source
                .parent()
                .unwrap_or(Path::new("."))
                .join("build")
        });
    if !options.object {
        for manifest in &options.manifests {
            let built = crate::rust_deps::build(
                &manifest.path,
                &manifest.rust_dependencies,
                Some(&triple),
                options.release,
                &rust_cache,
            )?;
            if !built.diagnostics.is_empty() {
                eprint!("{}", built.diagnostics);
            }
            for archive in built.archives {
                if !links.contains(&archive) {
                    links.push(archive);
                }
            }
            rust_roots.extend(built.protected_roots);
            for argument in built.link_args {
                if !rust_link_args.contains(&argument) {
                    rust_link_args.push(argument);
                }
            }
        }
    }
    let mut inputs = source_inputs(options);
    inputs.extend(links.iter().map(PathBuf::as_path));
    let destination = prepare_output(&output, &inputs)?;
    if rust_roots.iter().any(|root| destination.starts_with(root)) {
        return Err(format!("output '{}' is inside a Rust dependency source directory; choose a build directory outside dependency sources", destination.display()));
    }
    let parent = destination
        .parent()
        .ok_or("output must have a parent directory")?;
    let temporary = TemporaryDirectory::new(parent)?;
    let ir_path = temporary.path.join("program.ll");
    let artifact = temporary
        .path
        .join(if triple.contains("windows") && !options.object {
            "program.exe"
        } else {
            "program.out"
        });
    fs::write(&ir_path, &ir).map_err(|error| format!("cannot write temporary LLVM IR: {error}"))?;
    let mut command = clang_command(toolchain, options, &triple, &ir_path, &artifact, &links);
    command.args(&rust_link_args);
    if !options.object {
        add_native_runtime(
            &mut command,
            &temporary.path,
            &ir,
            &triple,
            options.gpu.as_deref(),
        )?;
    }
    let cache = crate::build_cache::prepare(&command, &artifact, &temporary.path, &rust_cache)?;
    let reused = match &cache {
        Some(cache) => cache.restore(&artifact)?,
        None => false,
    };
    if reused {
        eprintln!("Reused native artifact");
    } else {
        let clang_output = toolchain.invoke(&mut command)?;
        if !clang_output.status.success() {
            let details = tool_output(&clang_output);
            let advice = tool_failure_advice(options.object, &details);
            return Err(format!(
                "Clang failed to {} for '{triple}'. {advice}\n{details}",
                if options.object {
                    "emit an object"
                } else {
                    "link an executable"
                }
            ));
        }
        if !clang_output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&clang_output.stderr));
        }
        if let Some(cache) = &cache {
            cache.store(&artifact)?;
        }
    }
    // Another build may replace the public path before run can execute it.
    let run_artifact = if options.action == Action::Run {
        Some(retain_for_run(&artifact, &temporary.path)?)
    } else {
        None
    };
    publish_artifact(&artifact, &destination, &inputs)?;
    if options.action == Action::Run {
        let run_artifact = run_artifact.ok_or("private run artifact was not created")?;
        let status = Command::new(&run_artifact)
            .args(&options.run_args)
            .status()
            .map_err(|error| format!("cannot run '{}': {error}", destination.display()))?;
        Ok(exit_code(status))
    } else {
        eprintln!(
            "{} '{}' for {triple}",
            if options.object {
                "Emitted object"
            } else {
                "Built executable"
            },
            output.display()
        );
        if !target::host_can_run(&triple) && options.object {
            eprintln!("This is a cross-target object, not a linked executable.");
        }
        if options.object && ir.contains("; bhasa runtime: core") {
            eprintln!(
                "This object requires the Bhasa native runtime when linked into an executable."
            );
        }
        Ok(0)
    }
}

fn add_native_runtime(
    command: &mut Command,
    directory: &Path,
    ir: &str,
    triple: &str,
    gpu: Option<&str>,
) -> Result<(), String> {
    if !ir.contains("; bhasa runtime: core") {
        return Ok(());
    }
    if !triple.contains("linux") && !triple.contains("apple") {
        return Err("the owned-buffer and I/O runtime currently supports macOS and Linux executables; other targets can emit unlinked objects".into());
    }
    let mut files = vec![
        ("core.h", include_str!("../runtime/core.h")),
        ("core.c", include_str!("../runtime/core.c")),
    ];
    let uses_tls = ir.contains("; bhasa runtime: tls");
    let uses_net = ir.contains("; bhasa runtime: net") || uses_tls;
    let uses_text = ir.contains("; bhasa runtime: text");
    let uses_collections = ir.contains("; bhasa runtime: collections");
    if uses_collections {
        files.push(("collections.h", include_str!("../runtime/collections.h")));
        files.push(("collections.c", include_str!("../runtime/collections.c")));
    }
    if uses_net {
        files.push(("net.h", include_str!("../runtime/net.h")));
        files.push(("net.c", include_str!("../runtime/net.c")));
    }
    if uses_text {
        files.push(("text.h", include_str!("../runtime/text.h")));
        files.push(("text.c", include_str!("../runtime/text.c")));
    }
    if uses_tls {
        files.push(("tls.h", include_str!("../runtime/tls.h")));
        files.push(("tls.c", include_str!("../runtime/tls.c")));
    }
    if ir.contains("; bhasa runtime: threads") {
        files.push(("concurrency.h", include_str!("../runtime/concurrency.h")));
        files.push(("concurrency.c", include_str!("../runtime/concurrency.c")));
    }
    let gpu = gpu.unwrap_or(if triple.contains("apple") {
        "metal"
    } else {
        "vulkan"
    });
    if ir.contains("; bhasa runtime: gpu") {
        if gpu == "metal" && !triple.contains("apple") {
            return Err("the GPU runtime currently requires a macOS Metal target".into());
        }
        match gpu {
            "metal" => {
                files.push(("metal.h", include_str!("../runtime/metal.h")));
                files.push(("metal.m", include_str!("../runtime/metal.m")));
            }
            "cuda" => {
                files.push(("metal.h", include_str!("../runtime/metal.h")));
                files.push(("cuda.h", include_str!("../runtime/cuda.h")));
                files.push(("cuda.c", include_str!("../runtime/cuda.c")));
            }
            "vulkan" => {
                files.push(("metal.h", include_str!("../runtime/metal.h")));
                files.push(("vulkan.h", include_str!("../runtime/vulkan.h")));
                files.push(("vulkan.c", include_str!("../runtime/vulkan.c")));
                files.push((
                    "vendor/vulkan_core.h",
                    include_str!("../runtime/vendor/vulkan_core.h"),
                ));
                files.push((
                    "vendor/vk_platform.h",
                    include_str!("../runtime/vendor/vk_platform.h"),
                ));
            }
            _ => return Err(format!("unknown GPU backend '{gpu}'")),
        }
    }
    for (name, contents) in files {
        let path = directory.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot prepare runtime directory: {error}"))?;
        }
        fs::write(path, contents)
            .map_err(|error| format!("cannot prepare native runtime: {error}"))?;
    }
    command
        .args(["-x", "c", "-std=c11"])
        .arg(directory.join("core.c"));
    // Core I/O uses per-thread SIGPIPE masking even for programs without tasks.
    // Older Linux libc releases resolve pthread_sigmask through libpthread.
    if triple.contains("linux") {
        command.arg("-pthread");
    }
    if uses_net {
        command.arg(directory.join("net.c"));
    }
    if uses_text {
        command.arg(directory.join("text.c"));
    }
    if uses_collections {
        command.arg(directory.join("collections.c"));
    }
    if uses_tls {
        command.arg(directory.join("tls.c")).arg("-pthread");
        add_openssl(command)?;
    }
    if ir.contains("; bhasa runtime: threads") {
        command.arg(directory.join("concurrency.c")).arg("-pthread");
    }
    if ir.contains("; bhasa runtime: gpu") && gpu == "metal" {
        command
            .args(["-x", "objective-c", "-fno-objc-arc"])
            .arg(directory.join("metal.m"))
            .args([
                "-framework",
                "Foundation",
                "-framework",
                "Metal",
                "-framework",
                "CoreGraphics",
            ]);
    } else if ir.contains("; bhasa runtime: gpu") {
        command.arg(directory.join(format!("{gpu}.c")));
        if triple.contains("linux") {
            command.arg("-ldl");
        }
    }
    Ok(())
}

// Query directories separately: flags printed by pkg-config are shell syntax,
// and splitting that text would corrupt paths containing spaces.
fn add_openssl(command: &mut Command) -> Result<(), String> {
    let (include, library) = if let Some(prefix) = std::env::var_os("BHASA_OPENSSL_PREFIX") {
        let prefix = PathBuf::from(prefix);
        (prefix.join("include"), prefix.join("lib"))
    } else {
        let version = Command::new("pkg-config")
            .args(["--atleast-version=3.0", "openssl"])
            .output()
            .map_err(|error| format!("TLS requires OpenSSL 3 development libraries and pkg-config: {error}. Set BHASA_OPENSSL_PREFIX to an installed target OpenSSL prefix."))?;
        if !version.status.success() {
            return Err("TLS requires installed OpenSSL 3 development libraries. Configure PKG_CONFIG_PATH or BHASA_OPENSSL_PREFIX for the target toolchain.".into());
        }
        let directory = |variable| -> Result<PathBuf, String> {
            let output = Command::new("pkg-config")
                .args([variable, "openssl"])
                .output()
                .map_err(|error| format!("cannot locate OpenSSL: {error}"))?;
            if !output.status.success() {
                return Err(format!("cannot locate OpenSSL: {}", tool_output(&output)));
            }
            let text = std::str::from_utf8(&output.stdout)
                .map_err(|_| "OpenSSL directory is not UTF-8")?
                .trim_end_matches(['\n', '\r']);
            if text.is_empty() || text.contains(['\0', '\n', '\r']) {
                return Err("pkg-config returned an invalid OpenSSL directory".into());
            }
            Ok(PathBuf::from(text))
        };
        (
            directory("--variable=includedir")?,
            directory("--variable=libdir")?,
        )
    };
    if !include.join("openssl/ssl.h").is_file() || !library.is_dir() {
        return Err("OpenSSL 3 headers or libraries are missing from the configured prefix".into());
    }
    command
        .arg("-I")
        .arg(include)
        .arg("-L")
        .arg(&library)
        .args(["-Xlinker", "-rpath", "-Xlinker"])
        .arg(library)
        .args(["-lssl", "-lcrypto"]);
    Ok(())
}

fn tool_failure_advice(object: bool, details: &str) -> &'static str {
    if object {
        return "Object emission requires a Clang installation with this target backend. Run bhasa targets for the accepted targets.";
    }
    let lowercase = details.to_ascii_lowercase();
    if [
        "undefined symbol",
        "undefined reference",
        "unresolved external",
    ]
    .iter()
    .any(|message| lowercase.contains(message))
    {
        "For an extern function, build its native library and pass its archive or object file with --link FILE to bhasa build or bhasa run. If already linked, check C ABI exports, target architecture and required dependencies."
    } else {
        "Check the linker diagnostics below. Executables require compatible native libraries and a target SDK/linker; use build --emit object if you only need an unlinked object."
    }
}

fn retain_for_run(artifact: &Path, temporary: &Path) -> Result<PathBuf, String> {
    let retained = temporary.join(if cfg!(windows) { "run.exe" } else { "run" });
    fs::copy(artifact, &retained)
        .map_err(|error| format!("cannot retain a private executable for run: {error}"))?;
    Ok(retained)
}

fn clang_command(
    toolchain: &Toolchain,
    options: &Options,
    triple: &str,
    ir: &Path,
    output: &Path,
    links: &[PathBuf],
) -> Command {
    let mut command = Command::new(&toolchain.executable);
    command
        .arg("-target")
        .arg(triple)
        .arg(if options.release { "-O2" } else { "-O0" })
        .arg("-x")
        .arg("ir")
        .arg(ir);
    if options.object {
        command.arg("-c");
    }
    if !links.is_empty() {
        command.arg("-x").arg("none").args(links);
    }
    command.arg("-o").arg(output);
    command
}

fn tool_output(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stderr.is_empty() && stdout.is_empty() {
        format!("process exited with {}", output.status)
    } else {
        format!("{stderr}{stdout}").trim_end().to_string()
    }
}

fn default_output(options: &Options, triple: &str) -> PathBuf {
    if let Some(output) = &options.project_output {
        let extension = options.target.output_extension(options.object, triple);
        return if extension.is_empty() {
            output.clone()
        } else {
            output.with_extension(extension)
        };
    }
    let stem = options
        .source
        .file_stem()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("program"));
    let mut filename = stem.to_os_string();
    let extension = options.target.output_extension(options.object, triple);
    if !extension.is_empty() {
        filename.push(".");
        filename.push(extension);
    }
    PathBuf::from("build").join(filename)
}

fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|signal| 128 + signal).unwrap_or(1)
    }
    #[cfg(not(unix))]
    1
}

/// Keep the output in a canonical parent, and never overwrite an input path.
fn prepare_output(output: &Path, inputs: &[&Path]) -> Result<PathBuf, String> {
    let filename = output
        .file_name()
        .ok_or_else(|| format!("invalid output path '{}'", output.display()))?;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create output directory '{}': {error}",
            parent.display()
        )
    })?;
    let destination = fs::canonicalize(parent)
        .map_err(|error| {
            format!(
                "cannot resolve output directory '{}': {error}",
                parent.display()
            )
        })?
        .join(filename);
    validate_output(&destination, inputs)?;
    Ok(destination)
}

fn validate_output(output: &Path, inputs: &[&Path]) -> Result<(), String> {
    if output.is_dir() {
        return Err(format!("output '{}' is a directory", output.display()));
    }
    for input in inputs {
        if same_file(output, input)? {
            return Err(format!(
                "refusing to overwrite input '{}' with output '{}'",
                input.display(),
                output.display()
            ));
        }
    }
    Ok(())
}

fn same_file(left: &Path, right: &Path) -> Result<bool, String> {
    let right_path = fs::canonicalize(right)
        .map_err(|error| format!("cannot resolve input '{}': {error}", right.display()))?;
    if left == right_path {
        return Ok(true);
    }
    let left_path = match fs::canonicalize(left) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "cannot inspect output '{}': {error}",
                left.display()
            ))
        }
    };
    if left_path == right_path {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let left_meta = fs::metadata(&left_path).map_err(|error| error.to_string())?;
        let right_meta = fs::metadata(&right_path).map_err(|error| error.to_string())?;
        Ok(left_meta.dev() == right_meta.dev() && left_meta.ino() == right_meta.ino())
    }
    #[cfg(not(unix))]
    Ok(false)
}

fn publish_artifact(artifact: &Path, destination: &Path, inputs: &[&Path]) -> Result<(), String> {
    // Check again after Clang finishes, before replacing an existing output.
    validate_output(destination, inputs)?;
    fs::rename(artifact, destination)
        .map_err(|error| format!("cannot save output '{}': {error}", destination.display()))
}

fn write_artifact(output: &Path, bytes: &[u8], inputs: &[&Path]) -> Result<(), String> {
    let destination = prepare_output(output, inputs)?;
    let temporary = TemporaryDirectory::new(
        destination
            .parent()
            .ok_or("output must have a parent directory")?,
    )?;
    let artifact = temporary.path.join("program.ll");
    fs::write(&artifact, bytes).map_err(|error| format!("cannot write LLVM output: {error}"))?;
    publish_artifact(&artifact, &destination, inputs)
}

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(parent: &Path) -> Result<Self, String> {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..100 {
            let sequence = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".bhasa-tmp-{}-{time}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot create temporary directory in '{}': {error}",
                        parent.display()
                    ))
                }
            }
        }
        Err(format!(
            "cannot allocate a unique temporary directory in '{}'",
            parent.display()
        ))
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_linkage_uses_calls_rather_than_unused_builtin_declarations() {
        for (source, net, text) in [
            (
                "def main() { var bytes = buffer(\"hello\"); print(bytes) }",
                false,
                false,
            ),
            ("def main() { print(monotonic_millis()) }", true, false),
            (
                "def main() { print(utf8_count(buffer(\"hello\"))) }",
                false,
                true,
            ),
        ] {
            let mut ir =
                crate::codegen::emit_llvm(&crate::check(source).unwrap(), "aarch64-apple-darwin")
                    .unwrap();
            // The Bhasa emitter declares all builtin signatures even when unused.
            ir.push_str("\ndeclare i64 @bhasa_rt_monotonic_millis()\ndeclare i64 @bhasa_rt_utf8_count(ptr)\ndeclare ptr @bhasa_rt_tls_socket_clone(ptr)\ndeclare ptr @bhasa_rt_tls_connect(ptr, i64, ptr, ptr, i64)\n");
            let directory = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
            let mut command = Command::new("clang");
            add_native_runtime(
                &mut command,
                &directory.path,
                &ir,
                "aarch64-apple-darwin",
                None,
            )
            .unwrap();
            let arguments: Vec<_> = command.get_args().map(Path::new).collect();
            for (file, expected) in [("net.c", net), ("text.c", text), ("tls.c", false)] {
                assert_eq!(
                    directory.path.join(file).exists(),
                    expected,
                    "{source}: {file}"
                );
                assert_eq!(
                    arguments
                        .iter()
                        .any(|arg| arg.file_name().is_some_and(|name| name == file)),
                    expected
                );
            }
        }
    }

    fn parse(args: &[&str]) -> Result<Invocation, String> {
        parse_args(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
    }

    fn options(args: &[&str]) -> Options {
        match parse(args).unwrap() {
            Invocation::Compile(options) => options,
            _ => panic!("expected compile options"),
        }
    }

    #[test]
    fn parse_documented_commands() {
        assert!(matches!(parse(&[]).unwrap(), Invocation::Help));
        assert!(matches!(
            parse(&["--version"]).unwrap(),
            Invocation::Version
        ));
        assert!(matches!(parse(&["targets"]).unwrap(), Invocation::Targets));
        assert!(options(&["check", "a.bh", "--json"]).json);
        assert!(options(&["run", "a.bh", "--release"]).release);
        let opts = options(&[
            "run",
            "a.bh",
            "--gpu",
            "cuda",
            "--",
            "file with spaces",
            "--release",
        ]);
        assert_eq!(opts.gpu.as_deref(), Some("cuda"));
        assert_eq!(opts.run_args, ["file with spaces", "--release"]);
        assert!(!opts.release);
        let opts = options(&["emit-llvm", "a.bh", "--target", "linux-arm64", "-o", "a.ll"]);
        assert_eq!(opts.action, Action::EmitLlvm);
        assert_eq!(opts.output.unwrap(), PathBuf::from("a.ll"));
        for action in ["build", "run"] {
            let opts = options(&[
                action,
                "a.bh",
                "--link",
                "first.a",
                "second.o",
                "--release",
                "--link",
                "third.a",
            ]);
            assert_eq!(opts.links.len(), 3);
            assert!(opts.release);
        }
    }

    #[test]
    fn reject_options_that_would_be_ignored() {
        for args in [
            vec!["check", "a.bh", "--target", "native"],
            vec!["run", "a.bh", "--target", "linux-arm64"],
            vec!["run", "a.bh", "-o", "a"],
            vec!["emit-llvm", "a.bh", "--release"],
            vec!["build", "a.bh", "--emit", "object", "--link", "lib.a"],
            vec!["build", "a.bh", "--json"],
            vec!["check", "a.bh", "--json", "--json"],
            vec!["build", "a.bh", "-o", "a", "-o", "b"],
            vec!["targets", "extra"],
            vec!["check", "a.bh", "--gpu", "metal"],
            vec!["build", "a.bh", "--emit", "object", "--gpu", "metal"],
            vec!["run", "a.bh", "--gpu", "metal", "--gpu", "cuda"],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
    }

    #[test]
    fn reject_missing_values_unknown_targets_and_extra_sources() {
        for args in [
            vec!["build", "a.bh", "-o"],
            vec!["build", "a.bh", "--link"],
            vec!["run", "a.bh", "--link"],
            vec!["run", "a.bh", "--gpu"],
            vec!["build", "a.bh", "--gpu", "unknown"],
            vec!["build", "a.bh", "--target", "plan9"],
            vec!["build", "a.bh", "--emit", "assembly"],
            vec!["build", "a.bh", "b.bh"],
            vec!["build", "a.bh", "--target", "--release"],
            vec!["check", ""],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
        assert_eq!(
            options(&["check", "--", "-source.bh"]).source,
            PathBuf::from("-source.bh")
        );
    }

    #[test]
    fn unresolved_symbols_get_link_input_guidance() {
        for details in [
            "Undefined symbols for architecture arm64:\n  _adapter",
            "ld.lld: error: undefined symbol: adapter",
            "ld: program.o: undefined reference to `adapter'",
            "error LNK2019: unresolved external symbol adapter",
        ] {
            let advice = tool_failure_advice(false, details);
            assert!(advice.contains("--link"), "{advice}");
            assert!(advice.contains("extern"), "{advice}");
        }
        assert!(
            !tool_failure_advice(false, "ld: library not found for -lSystem").contains("--link")
        );
        assert!(!tool_failure_advice(true, "undefined symbol: adapter").contains("--link"));
    }

    #[test]
    fn clang_uses_argument_arrays_and_resets_the_ir_language() {
        let opts = options(&["build", "source.bh", "--release"]);
        let toolchain = Toolchain {
            executable: OsString::from("/tools/clang with spaces"),
        };
        let links = vec![PathBuf::from("/tmp/library $(touch nope).a")];
        let command = clang_command(
            &toolchain,
            &opts,
            "aarch64-unknown-linux-gnu",
            Path::new("/tmp/source file.ll"),
            Path::new("/tmp/output file"),
            &links,
        );
        assert_eq!(command.get_program(), "/tools/clang with spaces");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-target",
                "aarch64-unknown-linux-gnu",
                "-O2",
                "-x",
                "ir",
                "/tmp/source file.ll",
                "-x",
                "none",
                "/tmp/library $(touch nope).a",
                "-o",
                "/tmp/output file"
            ]
        );
        let opts = options(&["build", "source.bh", "--emit", "object"]);
        let command = clang_command(
            &toolchain,
            &opts,
            "wasm32-unknown-unknown",
            Path::new("x.ll"),
            Path::new("x.o"),
            &[],
        );
        assert!(command.get_args().any(|arg| arg == "-c"));
        assert!(command.get_args().any(|arg| arg == "-O0"));
    }

    #[test]
    fn output_names_retain_source_stem() {
        let opts = options(&["build", "dir/my.file.bh"]);
        assert_eq!(
            default_output(&opts, "x86_64-unknown-linux-gnu"),
            PathBuf::from("build/my.file")
        );
        assert_eq!(
            default_output(&opts, "x86_64-pc-windows-msvc"),
            PathBuf::from("build/my.file.exe")
        );
        let opts = options(&["build", "dir/my.file.bh", "--emit", "object"]);
        assert_eq!(
            default_output(&opts, "x86_64-pc-windows-msvc"),
            PathBuf::from("build/my.file.obj")
        );
    }

    #[test]
    fn preserve_source_and_link_inputs_and_replace_only_output() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let source = temporary.path.join("source.bh");
        let link = temporary.path.join("library.a");
        let output = temporary.path.join("nested/result.ll");
        fs::write(&source, "source").unwrap();
        fs::write(&link, "link").unwrap();
        assert!(write_artifact(&source, b"changed", &[&source]).is_err());
        assert!(write_artifact(&link, b"changed", &[&source, &link]).is_err());
        write_artifact(&output, b"first", &[&source, &link]).unwrap();
        write_artifact(&output, b"second", &[&source, &link]).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"second");
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read(&link).unwrap(), b"link");
        assert_eq!(fs::read_dir(output.parent().unwrap()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn preserve_symlink_and_hardlink_aliases_of_inputs() {
        use std::os::unix::fs::symlink;
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let source = temporary.path.join("source.bh");
        let symlink_path = temporary.path.join("symlink.ll");
        let hardlink_path = temporary.path.join("hardlink.ll");
        fs::write(&source, "source").unwrap();
        symlink(&source, &symlink_path).unwrap();
        fs::hard_link(&source, &hardlink_path).unwrap();
        assert!(write_artifact(&symlink_path, b"changed", &[&source]).is_err());
        assert!(write_artifact(&hardlink_path, b"changed", &[&source]).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"source");
    }

    #[test]
    fn temporaries_are_distinct_and_cleanup_only_their_own_directory() {
        let parent = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let retained = parent.path.join("user-file");
        fs::write(&retained, "keep").unwrap();
        let first = TemporaryDirectory::new(&parent.path).unwrap();
        let second = TemporaryDirectory::new(&parent.path).unwrap();
        assert_ne!(first.path, second.path);
        let first_path = first.path.clone();
        drop(first);
        assert!(!first_path.exists());
        assert!(second.path.exists());
        assert_eq!(fs::read_to_string(retained).unwrap(), "keep");
    }

    #[test]
    fn concurrent_output_replacement_cannot_change_the_private_run_artifact() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let artifact = temporary.path.join("program");
        let output = temporary.path.join("public-output");
        fs::write(&artifact, "native program").unwrap();
        let retained = retain_for_run(&artifact, &temporary.path).unwrap();
        publish_artifact(&artifact, &output, &[]).unwrap();
        fs::write(&output, "unrelated cross-target program").unwrap();
        assert_eq!(fs::read_to_string(retained).unwrap(), "native program");
    }

    #[cfg(unix)]
    fn fake_clang(directory: &Path, triple: &str) -> Toolchain {
        use std::os::unix::fs::PermissionsExt;
        let executable = directory.join("fake clang");
        let script = format!(
            "#!/bin/sh\nfor arg in \"$@\"; do\n if [ \"$arg\" = \"-emit-llvm\" ]; then printf '%s\\n' \"$@\" > \"$0.query-args\"; printf 'target triple = \"%s\"\\n' '{triple}'; exit 0; fi\ndone\nprintf compiled > \"$0.compiled\"\nwhile [ \"$#\" -gt 0 ]; do\n if [ \"$1\" = \"-o\" ]; then shift; printf partial > \"$1\"; break; fi\n shift\ndone\nexit 41\n"
        );
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Toolchain {
            executable: executable.into_os_string(),
        }
    }

    #[test]
    fn resolved_target_metadata_is_validated() {
        let triple = "arm64-apple-macosx26.0.0";
        let module =
            format!("; empty module\nsource_filename = \"-\"\ntarget triple = \"{triple}\"\n");
        assert_eq!(parse_resolved_triple(&module).unwrap(), triple);
        for invalid in [
            "",
            "target triple = \"\"",
            "target triple = \"-invalid\"",
            "target triple = \"x86_64 unknown linux\"",
            "target triple = \"no_separator\"",
            "target triple = \"a-b\"\ntarget triple = \"c-d\"",
        ] {
            assert!(parse_resolved_triple(invalid).is_err(), "{invalid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn target_probe_preserves_the_explicit_selection() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let resolved = "x86_64-pc-windows-msvc19.33.0";
        let toolchain = fake_clang(&temporary.path, resolved);
        assert_eq!(
            toolchain.resolved_triple(&Target::WindowsX64).unwrap(),
            resolved
        );
        let args = fs::read_to_string(temporary.path.join("fake clang.query-args")).unwrap();
        assert_eq!(
            args,
            "-target\nx86_64-pc-windows-msvc\n-S\n-emit-llvm\n-x\nc\n-\n-o\n-\n"
        );
        assert!(!temporary.path.join("fake clang.compiled").exists());
    }

    #[cfg(unix)]
    #[test]
    fn foreign_clang_default_prevents_run_before_compilation() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let foreign = if cfg!(target_os = "linux") {
            "x86_64-apple-macosx"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        let toolchain = fake_clang(&temporary.path, foreign);
        let opts = options(&["run", "not-read.bh"]);
        let source = "def main() {}";
        let checked = crate::check(source).unwrap();
        let error = build_with_toolchain(&opts, Some(&checked), source, "not-read.bh", &toolchain)
            .unwrap_err();
        assert!(error.contains("refusing to run target"), "{error}");
        assert!(!temporary.path.join("fake clang.compiled").exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_clang_preserves_previous_output_and_cleans_partial_artifacts() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let source_path = temporary.path.join("source.bh");
        let output = temporary.path.join("output");
        let source = "def main() {}";
        fs::write(&source_path, source).unwrap();
        fs::write(&output, "previous output").unwrap();
        let toolchain = fake_clang(&temporary.path, &Target::Native.triple_for_ir().unwrap());
        let mut opts = options(&["build", "source.bh"]);
        opts.source = source_path.clone();
        opts.output = Some(output.clone());
        let checked = crate::check(source).unwrap();
        let error = build_with_toolchain(&opts, Some(&checked), source, "source.bh", &toolchain)
            .unwrap_err();
        assert!(error.contains("Clang failed"), "{error}");
        assert!(temporary.path.join("fake clang.compiled").exists());
        assert_eq!(fs::read_to_string(&source_path).unwrap(), source);
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous output");
        assert!(fs::read_dir(&temporary.path).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".bhasa-tmp-")));
    }
}
