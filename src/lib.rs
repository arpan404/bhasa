//! Bhasa's Rust bootstrap frontend and LLVM IR generation.
pub mod ast;
#[cfg(feature = "bhasa-lexer")]
mod bhasa_lexer;
pub mod build_cache;
pub mod builtins;
pub mod codegen;
pub mod collections;
pub mod diagnostic;
pub mod driver;
mod failure_use;
pub mod ffi_contract;
pub mod format;
pub mod generics;
mod inspect;
pub mod json;
pub mod lexer;
mod lock_cli;
pub mod lsp;
pub mod modules;
pub mod ownership;
pub mod package_lock;
pub mod parser;
pub mod project;
pub mod rust_deps;
pub mod sema;
pub mod target;
pub mod test_runner;

pub use diagnostic::Diagnostic;

/// The host lowering maps compose in the same order as source transformation.
#[derive(Debug, Clone)]
pub(crate) struct ExpandedFiles {
    pub modules: modules::Expansion,
    pub generics: generics::Expansion,
}
impl std::ops::Deref for ExpandedFiles {
    type Target = generics::Expansion;
    fn deref(&self) -> &Self::Target {
        &self.generics
    }
}
impl ExpandedFiles {
    pub fn original_name(&self, name: &str) -> String {
        self.modules
            .original_name(&self.generics.original_name(name))
    }
    pub fn display_diagnostics(&self, errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
        self.modules
            .display_diagnostics(self.generics.display_diagnostics(errors))
    }
    #[cfg(feature = "bhasa-frontend")]
    pub fn map_diagnostics(&self, errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
        self.modules
            .map_diagnostics(self.generics.map_diagnostics(errors))
    }
    pub fn parse(&self) -> Result<ast::Program, Vec<Diagnostic>> {
        let mut program = generics::parse_expansion(&self.generics)
            .map_err(|errors| self.modules.map_diagnostics(errors))?;
        self.modules.remap_program(&mut program);
        Ok(program)
    }
    pub fn check(&self) -> Result<ast::CheckedProgram, Vec<Diagnostic>> {
        let checked =
            sema::check(self.parse()?).map_err(|errors| self.display_diagnostics(errors))?;
        self.modules
            .check_privacy(&checked, |name| self.generics.original_name(name))?;
        Ok(checked)
    }
}
fn single_source(source: &str) -> Vec<project::SourceFile> {
    vec![project::SourceFile {
        path: std::path::PathBuf::new(),
        source: source.to_owned(),
        base: 0,
        import_spans: Vec::new(),
        imports: Vec::new(),
    }]
}
/// Parse source after deterministic module and generic lowering.
pub fn parse(source: &str) -> Result<ast::Program, Vec<Diagnostic>> {
    parse_files(&single_source(source))
}
/// Parse a whole module graph while preserving per-file EOF boundaries.
pub fn parse_files(files: &[project::SourceFile]) -> Result<ast::Program, Vec<Diagnostic>> {
    expand_files(files)?.parse()
}
pub(crate) fn expand_files(
    files: &[project::SourceFile],
) -> Result<ExpandedFiles, Vec<Diagnostic>> {
    let modules = modules::expand_files(files)?;
    let type_roots = modules.type_roots()?;
    let generics =
        generics::expand_with_type_roots(&modules.source, &modules.boundaries, &type_roots)
            .map_err(|errors| modules.map_diagnostics(errors))?;
    Ok(ExpandedFiles { modules, generics })
}
pub(crate) fn check_files(
    files: &[project::SourceFile],
) -> Result<ast::CheckedProgram, Vec<Diagnostic>> {
    expand_files(files)?.check()
}
/// Parse and type-check source without invoking external build tools.
pub fn check(source: &str) -> Result<ast::CheckedProgram, Vec<Diagnostic>> {
    check_files(&single_source(source))
}
/// Compile source into textual LLVM IR for an explicit target triple.
pub fn compile_to_llvm(source: &str, target_triple: &str) -> Result<String, Vec<Diagnostic>> {
    let expanded = expand_files(&single_source(source))?;
    codegen::emit_llvm(&expanded.check()?, target_triple)
        .map_err(|errors| expanded.display_diagnostics(errors))
}
/// Run the Bhasa-written frontend after shared host lowering and privacy checks.
pub fn compile_with_bhasa(source: &str, target_triple: &str) -> Result<String, Vec<Diagnostic>> {
    compile_files_with_bhasa(&single_source(source), target_triple)
}
/// Preserve module boundaries while the Bhasa frontend parses a loaded graph.
pub fn compile_files_with_bhasa(
    files: &[project::SourceFile],
    target_triple: &str,
) -> Result<String, Vec<Diagnostic>> {
    #[cfg(feature = "bhasa-frontend")]
    {
        let expanded = expand_files(files)?;
        // Member access needs receiver types. This shared host verifier is an
        // additional privacy check; native checking/emission never falls back.
        if expanded.modules.has_member_privacy() {
            expanded.check()?;
        }
        bhasa_lexer::compile(&expanded.source, target_triple, &expanded.boundaries)
            .map_err(|errors| expanded.map_diagnostics(errors))
    }
    #[cfg(not(feature = "bhasa-frontend"))]
    {
        let _ = (files, target_triple);
        Err(vec![Diagnostic::new("H002", "this compiler was built without the Bhasa frontend; run sh bootstrap/build-hybrid.sh --frontend", ast::Span::default())])
    }
}
