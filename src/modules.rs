//! Module aliases and privacy, lowered before generic specialization.
//!
//! The source map composes with the generic map. Member privacy is checked on
//! typed expressions, never guessed from the spelling of a receiver variable.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{self, Span};
use crate::lexer::{Token, TokenKind as K};
use crate::project::SourceFile;
use crate::Diagnostic;

const PREFIX: &str = "__bhm_";
const MAX_LOWERED_BYTES: usize = 128 * 1024 * 1024;
const MAX_ALIAS_TARGET_BYTES: usize = 16 * 1024 * 1024;
type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Debug, Clone)]
struct Mapping {
    generated: Span,
    original: Span,
    linear: bool,
}
#[derive(Debug, Clone)]
struct MemberPrivacy {
    owner: usize,
    fields: BTreeSet<String>,
    methods: BTreeSet<String>,
}
/// Source-only alias identity; canonical targets retain their original nominal IDs.
#[derive(Debug, Clone)]
pub struct TypeAliasInfo {
    pub name: String,
    pub internal_name: String,
    pub canonical_target: String,
    pub owner: usize,
    pub private: bool,
    pub span: Span,
    pub name_span: Span,
    pub target_span: Span,
}
#[derive(Debug, Clone)]
pub struct TypeAliasUse {
    pub internal_name: String,
    pub span: Span,
    pub type_context: bool,
}
/// Resolved source anchors for nominal names erased with an alias RHS.
/// The editor still requires a successful checked graph for these references.
#[derive(Debug, Clone)]
pub struct TypeAliasNominalUse {
    pub declaration_span: Span,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct Expansion {
    pub(crate) channels: Option<crate::channel_registry::ModuleEvidence>,
    /// Original-source metadata for API inspection and editor symbol identity.
    pub type_aliases: Vec<TypeAliasInfo>,
    pub type_alias_uses: Vec<TypeAliasUse>,
    pub type_alias_nominal_uses: Vec<TypeAliasNominalUse>,
    pub source: String,
    pub boundaries: Vec<usize>,
    mappings: Vec<Mapping>,
    names: BTreeMap<String, String>,
    members: BTreeMap<String, MemberPrivacy>,
    private_enums: BTreeMap<String, usize>,
    files: Vec<Span>,
    original_len: usize,
    identity: bool,
}
impl Expansion {
    /// Canonical types requested even when their aliases have no source uses.
    /// Root sites use lowered offsets, so the generic map composes normally.
    pub fn type_roots(
        &self,
    ) -> std::result::Result<Vec<crate::generics::TypeRoot>, Vec<Diagnostic>> {
        self.type_aliases
            .iter()
            .map(|alias| {
                let index = self
                    .mappings
                    .partition_point(|mapping| mapping.original.end <= alias.target_span.start);
                let mapping = self
                    .mappings
                    .get(index)
                    .filter(|mapping| {
                        mapping.linear
                            && mapping.original.start <= alias.target_span.start
                            && mapping.original.end >= alias.target_span.end
                    })
                    .ok_or_else(|| {
                        vec![error(
                            "invalid alias target source mapping",
                            alias.target_span,
                        )]
                    })?;
                Ok(crate::generics::TypeRoot {
                    spelling: alias.canonical_target.clone(),
                    span: Span::new(
                        mapping.generated.start + alias.target_span.start - mapping.original.start,
                        mapping.generated.start + alias.target_span.end - mapping.original.start,
                    ),
                })
            })
            .collect()
    }
    pub fn is_identity(&self) -> bool {
        self.identity
    }
    pub fn has_member_privacy(&self) -> bool {
        !self.members.is_empty() || !self.private_enums.is_empty()
    }
    pub fn map_span(&self, span: Span) -> Span {
        let position = |offset: usize, end: bool| {
            let offset = offset.min(self.source.len());
            let index = self.mappings.partition_point(|map| {
                if end {
                    map.generated.end < offset
                } else {
                    map.generated.end <= offset
                }
            });
            match self.mappings.get(index).or_else(|| self.mappings.last()) {
                Some(map) if map.linear => {
                    map.original.start
                        + offset
                            .saturating_sub(map.generated.start)
                            .min(map.original.end - map.original.start)
                }
                Some(map) => {
                    if end {
                        map.original.end
                    } else {
                        map.original.start
                    }
                }
                None => offset.min(self.original_len),
            }
        };
        let start = position(span.start, false).min(self.original_len);
        Span::new(
            start,
            position(span.end, true).max(start).min(self.original_len),
        )
    }
    pub fn original_name(&self, name: &str) -> String {
        let (base, suffix) = name.split_once('.').unwrap_or((name, ""));
        let name = self.names.get(base).map_or(base, String::as_str);
        if suffix.is_empty() {
            name.to_owned()
        } else {
            format!("{name}.{suffix}")
        }
    }
    pub fn display_names(&self, text: &str) -> String {
        let mut names: Vec<_> = self.names.iter().collect();
        names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        let mut text = text.to_owned();
        for (name, original) in names {
            text = text.replace(name, original);
        }
        text
    }
    pub fn display_diagnostics(&self, mut errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
        for error in &mut errors {
            error.message = self.display_names(&error.message);
        }
        errors
    }
    pub fn map_diagnostics(&self, mut errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
        for error in &mut errors {
            error.span = self.map_span(error.span);
        }
        self.display_diagnostics(errors)
    }
    pub fn remap_program(&self, program: &mut ast::Program) {
        visit_program_spans(program, &|span| self.map_span(span));
    }

    /// Requires typed expressions with spans already mapped to the original
    /// files. The callback maps concrete generic type names to template names.
    pub fn check_privacy(
        &self,
        checked: &ast::CheckedProgram,
        generic_original_name: impl Fn(&str) -> String,
    ) -> std::result::Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let program = &checked.program;
        let owner = |span: Span| {
            self.files
                .iter()
                .position(|file| span.start >= file.start && span.start <= file.end)
        };
        let check = |ty: ast::Type,
                     member: Option<(&str, bool)>,
                     span: Span,
                     errors: &mut Vec<Diagnostic>| {
            let ast::Type::Record(_) = ty else {
                return;
            };
            let name = generic_original_name(&program.type_name(ty));
            let Some(privacy) = self.members.get(&name) else {
                return;
            };
            if owner(span) == Some(privacy.owner) {
                return;
            }
            let message = match member {
                Some((member, method))
                    if if method {
                        privacy.methods.contains(member)
                    } else {
                        privacy.fields.contains(member)
                    } =>
                {
                    format!(
                        "{} '{}.{}' is private to its defining module",
                        if method { "method" } else { "field" },
                        self.original_name(&name),
                        member
                    )
                }
                None if !privacy.fields.is_empty() => format!(
                    "record '{}' has private fields; use a public factory from its defining module",
                    self.original_name(&name)
                ),
                _ => return,
            };
            errors.push(Diagnostic::new("M005", message, span));
        };
        visit_program_expressions(program, &mut |expr| match &expr.kind {
            ast::ExprKind::Field { base, name } => {
                if let Some(ty) = base.ty {
                    check(ty, Some((name, false)), expr.span, &mut errors);
                }
            }
            ast::ExprKind::MethodCall { receiver, name, .. } => {
                if let Some(ty) = receiver.ty {
                    check(ty, Some((name, true)), expr.span, &mut errors);
                }
            }
            ast::ExprKind::Call { name, args } => {
                // The checker lowers method calls to a typed call whose first
                // argument is its receiver. Resolve privacy from that type.
                if let Some((record, method)) = name.split_once('.') {
                    if let Some(receiver) = args.first() {
                        if let Some(ty @ ast::Type::Record(_)) = receiver.ty {
                            if program.type_name(ty) == record {
                                check(ty, Some((method, true)), expr.span, &mut errors);
                            }
                        }
                    }
                }
                if let Some(ty @ ast::Type::Record(_)) = expr.ty {
                    // A function returning a record is not a record constructor.
                    if program.type_name(ty) == *name {
                        check(ty, None, expr.span, &mut errors);
                    }
                }
            }
            ast::ExprKind::Int(_)
            | ast::ExprKind::Float(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Str(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::Move { .. }
            | ast::ExprKind::Unary { .. }
            | ast::ExprKind::Binary { .. } => {}
        });
        visit_program_statements(program, &mut |statement| {
            if let ast::StmtKind::TryVar {
                value, try_span, ..
            } = &statement.kind
            {
                if let Some(ty @ ast::Type::Enum(_)) = value.ty {
                    let name = generic_original_name(&program.type_name(ty));
                    if let Some(&defining) = self.private_enums.get(&name) {
                        if owner(*try_span) != Some(defining) {
                            errors.push(Diagnostic::new("M005",format!("enum '{}' is private to its defining module; try cannot inspect its payload",self.original_name(&name)),*try_span));
                        }
                    }
                }
            }
        });
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Clone)]
struct Declaration {
    name: String,
    internal: String,
    private: bool,
    index: usize,
    kind: K,
}
struct Binding {
    name: String,
    start: usize,
    end: usize,
    type_only: bool,
}
struct TypeAlias {
    target: Vec<Token>,
    span: Span,
}
struct Module {
    type_aliases: BTreeMap<String, TypeAlias>,
    tokens: Vec<Token>,
    declarations: BTreeMap<String, Declaration>,
    exports: BTreeMap<String, String>,
    scope: BTreeMap<String, String>,
    aliases: BTreeMap<String, usize>,
    protected: BTreeSet<usize>,
    modifiers: BTreeSet<usize>,
    bindings: Vec<Binding>,
    type_tokens: BTreeSet<usize>,
    pairs: BTreeMap<usize, usize>,
    private_members: BTreeMap<String, MemberPrivacy>,
}
fn error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new("M003", message, span)
}
fn ident(token: &Token) -> Option<&str> {
    if let K::Ident(name) = &token.kind {
        Some(name)
    } else {
        None
    }
}
fn separator(kind: &K) -> bool {
    matches!(kind, K::Newline | K::Semicolon)
}
fn reserved(name: &str) -> bool {
    crate::builtins::is_builtin(name)
        || name.starts_with("bhasa_rt_")
        || matches!(
            name,
            "tcp_socket"
                | "tcp_listener"
                | "tls_socket"
                | "file_handle"
                | "task"
                | "List"
                | "Map"
                | "range"
        )
}
fn next(tokens: &[Token], mut index: usize) -> usize {
    while index < tokens.len() && matches!(tokens[index].kind, K::Newline) {
        index += 1;
    }
    index
}
fn pairs(tokens: &[Token]) -> BTreeMap<usize, usize> {
    let mut stack = Vec::new();
    let mut result = BTreeMap::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            K::LeftBrace | K::LeftParen | K::LeftBracket => stack.push(index),
            K::RightBrace | K::RightParen | K::RightBracket => {
                if let Some(open) = stack.pop() {
                    let valid = matches!(
                        (&tokens[open].kind, &token.kind),
                        (K::LeftBrace, K::RightBrace)
                            | (K::LeftParen, K::RightParen)
                            | (K::LeftBracket, K::RightBracket)
                    );
                    if valid {
                        result.insert(open, index);
                    } else {
                        stack.clear();
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Lower a loaded graph. Unaliased graphs without privacy retain the previous
/// spelling and ABI. Namespace/private graphs isolate every owned declaration.
pub fn expand_files(files: &[SourceFile]) -> std::result::Result<Expansion, Vec<Diagnostic>> {
    expand_inner(files).map_err(|error| vec![error])
}
fn expand_inner(files: &[SourceFile]) -> Result<Expansion> {
    let loaded_channels = crate::channel_registry::registered_module(files)?;
    let original = crate::project::flattened_source(files);
    let mut modules = Vec::new();
    let mut enabled = files
        .iter()
        .any(|file| file.imports.iter().any(|import| import.alias.is_some()));
    enabled |= loaded_channels.is_some();
    for (owner, file) in files.iter().enumerate() {
        let mut tokens = crate::lexer::lex(&file.source).map_err(|mut errors| {
            let mut error = errors.remove(0);
            error.span.start += file.base;
            error.span.end += file.base;
            error
        })?;
        for token in &tokens {
            if ident(token).is_some_and(|name| name.starts_with(PREFIX)) {
                return Err(error(
                    "identifiers beginning with '__bhm_' are reserved for module lowering",
                    Span::new(file.base + token.span.start, file.base + token.span.end),
                ));
            }
            if ident(token).is_some_and(|name| name.starts_with("__bhg_")) {
                return Err(error(
                    "identifiers beginning with '__bhg_' are reserved for generic specialization",
                    Span::new(file.base + token.span.start, file.base + token.span.end),
                ));
            }
        }
        tokens.retain(|token| {
            !file
                .import_spans
                .iter()
                .any(|span| token.span.start >= span.start && token.span.end <= span.end)
        });
        for token in &mut tokens {
            token.span.start += file.base;
            token.span.end += file.base;
        }
        let mut module = Module {
            pairs: pairs(&tokens),
            tokens,
            declarations: BTreeMap::new(),
            exports: BTreeMap::new(),
            scope: BTreeMap::new(),
            aliases: BTreeMap::new(),
            protected: BTreeSet::new(),
            modifiers: BTreeSet::new(),
            bindings: Vec::new(),
            type_tokens: BTreeSet::new(),
            private_members: BTreeMap::new(),
            type_aliases: BTreeMap::new(),
        };
        collect(&mut module, owner)?;
        enabled |= !module.modifiers.is_empty() || !module.type_aliases.is_empty();
        modules.push(module);
    }
    let mut expansion = Expansion {
        channels: None,
        type_aliases: Vec::new(),
        type_alias_uses: Vec::new(),
        type_alias_nominal_uses: Vec::new(),
        source: String::new(),
        boundaries: Vec::new(),
        mappings: Vec::new(),
        names: BTreeMap::new(),
        members: BTreeMap::new(),
        private_enums: BTreeMap::new(),
        files: files
            .iter()
            .map(|file| Span::new(file.base, file.base + file.source.len()))
            .collect(),
        original_len: original.len(),
        identity: !enabled,
    };
    if !enabled {
        expansion.boundaries = files
            .iter()
            .map(|file| file.base + file.source.len())
            .collect();
        expansion.mappings.push(Mapping {
            generated: Span::new(0, original.len()),
            original: Span::new(0, original.len()),
            linear: true,
        });
        expansion.source = original;
        return Ok(expansion);
    }
    for (owner, module) in modules.iter_mut().enumerate() {
        for declaration in module.declarations.values_mut() {
            if reserved(&declaration.name) {
                return Err(error(
                    format!("'{}' is a reserved builtin name", declaration.name),
                    module.tokens[declaration.index].span,
                ));
            }
            declaration.internal = if declaration.kind == K::Extern
                || owner == 0 && declaration.kind == K::Def && declaration.name == "main"
            {
                declaration.name.clone()
            } else {
                format!("{PREFIX}{owner}_{}", declaration.name)
            };
            expansion
                .names
                .insert(declaration.internal.clone(), declaration.name.clone());
            module
                .scope
                .insert(declaration.name.clone(), declaration.internal.clone());
            if declaration.private && declaration.kind == K::Enum {
                expansion
                    .private_enums
                    .insert(declaration.internal.clone(), owner);
            }
            if !declaration.private {
                module
                    .exports
                    .insert(declaration.name.clone(), declaration.internal.clone());
            }
            if let Some(privacy) = module.private_members.remove(&declaration.name) {
                expansion
                    .members
                    .insert(declaration.internal.clone(), privacy);
            }
        }
    }
    let paths: BTreeMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(i, file)| (file.path.clone(), i))
        .collect();
    let mut complete = BTreeSet::new();
    let mut active = BTreeSet::new();
    for owner in 0..files.len() {
        resolve(
            owner,
            files,
            &paths,
            &mut modules,
            &mut complete,
            &mut active,
        )?;
    }
    let canonical_aliases = canonical_aliases(&modules)?;
    let mut erased_alias_spans = BTreeSet::new();
    for (owner, module) in modules.iter().enumerate() {
        for (name, alias) in &module.type_aliases {
            let declaration = &module.declarations[name];
            erased_alias_spans.insert((alias.span.start, alias.span.end));
            expansion.type_aliases.push(TypeAliasInfo {
                name: name.clone(),
                internal_name: declaration.internal.clone(),
                canonical_target: canonical_aliases[&declaration.internal].clone(),
                owner,
                private: declaration.private,
                span: alias.span,
                name_span: module.tokens[declaration.index].span,
                target_span: alias
                    .target
                    .first()
                    .unwrap()
                    .span
                    .join(alias.target.last().unwrap().span),
            });
        }
    }
    let nominal_declarations: BTreeMap<_, _> = modules
        .iter()
        .flat_map(|module| {
            module
                .declarations
                .values()
                .filter(|decl| matches!(decl.kind, K::Struct | K::Enum))
                .map(|decl| (decl.internal.as_str(), module.tokens[decl.index].span))
        })
        .collect();
    // Alias RHS references disappear from the parsed AST, but remain rename/
    // navigation uses of their source-only alias declaration identities.
    for module in &modules {
        for alias in module.type_aliases.values() {
            let mut index = 0;
            while index < alias.target.len() {
                let token = &alias.target[index];
                let Some(name) = ident(token) else {
                    index += 1;
                    continue;
                };
                let (internal, span, consumed) = if alias
                    .target
                    .get(index + 1)
                    .is_some_and(|t| t.kind == K::Dot)
                {
                    let target = module.aliases.get(name).and_then(|owner| {
                        modules[*owner]
                            .exports
                            .get(ident(&alias.target[index + 2]).unwrap())
                    });
                    (target, token.span.join(alias.target[index + 2].span), 3)
                } else {
                    (module.scope.get(name), token.span, 1)
                };
                if let Some(declaration_span) =
                    internal.and_then(|name| nominal_declarations.get(name.as_str()))
                {
                    expansion.type_alias_nominal_uses.push(TypeAliasNominalUse {
                        declaration_span: *declaration_span,
                        span,
                    });
                }
                if let Some(internal) =
                    internal.filter(|name| canonical_aliases.contains_key(*name))
                {
                    expansion.type_alias_uses.push(TypeAliasUse {
                        internal_name: internal.clone(),
                        span,
                        type_context: true,
                    });
                }
                index += consumed;
            }
        }
    }
    expansion.type_aliases.sort_by_key(|alias| alias.span.start);
    expansion
        .type_alias_nominal_uses
        .sort_by_key(|reference| reference.span.start);
    let foreign_names: BTreeSet<_> = modules
        .iter()
        .flat_map(|module| module.declarations.keys().cloned())
        .collect();
    let mut edits: Vec<(Span, Cow<'_, str>)> = Vec::new();
    for module in &modules {
        for alias in module.type_aliases.values() {
            edits.push((
                alias.span,
                Cow::Owned(
                    original[alias.span.start..alias.span.end]
                        .bytes()
                        .map(|byte| {
                            if matches!(byte, b'\n' | b'\r') {
                                byte as char
                            } else {
                                ' '
                            }
                        })
                        .collect(),
                ),
            ));
        }
        for modifier in &module.modifiers {
            edits.push((
                module.tokens[*modifier].span,
                Cow::Owned(" ".repeat(
                    module.tokens[*modifier].span.end - module.tokens[*modifier].span.start,
                )),
            ));
        }
        let mut index = 0;
        while index < module.tokens.len() {
            let token = &module.tokens[index];
            let Some(name) = ident(token) else {
                index += 1;
                continue;
            };
            if module.protected.contains(&index) || module.modifiers.contains(&index) {
                index += 1;
                continue;
            }
            let type_context = module.type_tokens.contains(&index);
            let shadows = |binding: &Binding| {
                binding.name == name
                    && token.span.start >= binding.start
                    && token.span.start < binding.end
                    && (!type_context || binding.type_only)
            };
            let shadowed = module.bindings.iter().any(shadows);
            if shadowed {
                // Function-call syntax does not provide callable local values.
                // Leaving a shadowed C name bare would let the downstream
                // checker resolve a private extern through its global ABI name.
                if !type_context
                    && module
                        .bindings
                        .iter()
                        .any(|binding| !binding.type_only && shadows(binding))
                    && module
                        .tokens
                        .get(index + 1)
                        .is_some_and(|token| matches!(token.kind, K::LeftParen | K::LeftBracket))
                {
                    return Err(error(
                        format!("local binding '{name}' is not callable"),
                        token.span,
                    ));
                }
                index += 1;
                continue;
            }
            if let Some(target) = module.aliases.get(name) {
                if module
                    .tokens
                    .get(index + 1)
                    .is_some_and(|token| token.kind == K::Dot)
                {
                    let Some(member) = module.tokens.get(index + 2).and_then(ident) else {
                        return Err(error(
                            "expected an exported declaration after the module namespace",
                            token.span,
                        ));
                    };
                    let member_token = &module.tokens[index + 2];
                    let Some(internal) = modules[*target].exports.get(member) else {
                        let reason = if modules[*target]
                            .declarations
                            .get(member)
                            .is_some_and(|decl| decl.private)
                        {
                            "is private to its defining module"
                        } else {
                            "is not exported by this module"
                        };
                        return Err(error(
                            format!("'{name}.{member}' {reason}"),
                            token.span.join(member_token.span),
                        ));
                    };
                    if canonical_aliases.contains_key(internal) {
                        expansion.type_alias_uses.push(TypeAliasUse {
                            internal_name: internal.clone(),
                            span: token.span.join(member_token.span),
                            type_context,
                        });
                    }
                    edits.push((
                        token.span.join(member_token.span),
                        Cow::Borrowed(canonical_aliases.get(internal).unwrap_or(internal).as_str()),
                    ));
                    index += 3;
                    continue;
                }
                return Err(error(
                    format!("module namespace '{name}' requires a qualified declaration name"),
                    token.span,
                ));
            }
            if let Some(internal) = module.scope.get(name) {
                if canonical_aliases.contains_key(internal) {
                    expansion.type_alias_uses.push(TypeAliasUse {
                        internal_name: internal.clone(),
                        span: token.span,
                        type_context,
                    });
                }
                if internal != name || canonical_aliases.contains_key(internal) {
                    edits.push((
                        token.span,
                        Cow::Borrowed(canonical_aliases.get(internal).unwrap_or(internal).as_str()),
                    ));
                }
            } else if foreign_names.contains(name) {
                return Err(error(format!("declaration '{name}' is not visible in this module; import its public namespace"),token.span));
            }
            index += 1;
        }
    }
    edits.sort_by_key(|(span, _)| span.start);
    let mut cursor = 0;
    let mut edit_index = 0;
    for file in files {
        let end = file.base + file.source.len();
        while edit_index < edits.len() && edits[edit_index].0.start < end {
            let (span, replacement) = &edits[edit_index];
            if span.start < cursor {
                return Err(error("overlapping module rewrite", *span));
            }
            append(
                &mut expansion,
                &original[cursor..span.start],
                Span::new(cursor, span.start),
                true,
            )?;
            append(
                &mut expansion,
                replacement,
                *span,
                erased_alias_spans.contains(&(span.start, span.end)),
            )?;
            cursor = span.end;
            edit_index += 1;
        }
        append(
            &mut expansion,
            &original[cursor..end],
            Span::new(cursor, end),
            true,
        )?;
        expansion.boundaries.push(expansion.source.len());
        append(
            &mut expansion,
            &original[end..end + 1],
            Span::new(end, end + 1),
            true,
        )?;
        cursor = end + 1;
    }
    expansion
        .type_alias_uses
        .sort_by_key(|use_| use_.span.start);
    if let Some(loaded) = loaded_channels {
        let owner = loaded.owner();
        let bindings = modules[owner]
            .declarations
            .iter()
            .map(|(name, declaration)| (name.clone(), declaration.internal.clone()))
            .collect();
        expansion.channels = Some(crate::channel_registry::lowered_receipt(
            loaded,
            bindings,
            &expansion.source,
            &expansion.boundaries,
        )?);
    }
    Ok(expansion)
}
fn append(expansion: &mut Expansion, text: &str, original: Span, linear: bool) -> Result<()> {
    if expansion.source.len().saturating_add(text.len()) > MAX_LOWERED_BYTES {
        return Err(error(
            "module lowering exceeds its 128 MiB source limit",
            original,
        ));
    }
    if !text.is_empty() {
        let start = expansion.source.len();
        expansion.source.push_str(text);
        expansion.mappings.push(Mapping {
            generated: Span::new(start, expansion.source.len()),
            original,
            linear,
        });
    }
    Ok(())
}
fn resolve(
    owner: usize,
    files: &[SourceFile],
    paths: &BTreeMap<std::path::PathBuf, usize>,
    modules: &mut [Module],
    complete: &mut BTreeSet<usize>,
    active: &mut BTreeSet<usize>,
) -> Result<()> {
    if complete.contains(&owner) {
        return Ok(());
    }
    if !active.insert(owner) {
        return Err(error(
            "import cycle detected",
            Span::new(files[owner].base, files[owner].base),
        ));
    }
    for import in &files[owner].imports {
        let span = Span::new(
            files[owner].base + import.span.start,
            files[owner].base + import.span.end,
        );
        let Some(&target) = paths.get(&import.target) else {
            return Err(error(
                "import target is not loaded; save imports before checking this document",
                span,
            ));
        };
        resolve(target, files, paths, modules, complete, active)?;
        if let Some(alias) = &import.alias {
            if reserved(alias)
                || modules[owner].scope.contains_key(alias)
                || modules[owner]
                    .aliases
                    .insert(alias.clone(), target)
                    .is_some()
            {
                return Err(error(
                    format!("duplicate or reserved module namespace '{alias}'"),
                    span,
                ));
            }
        } else {
            let exports = modules[target].exports.clone();
            for (name, internal) in exports {
                if modules[owner].aliases.contains_key(&name)
                    || modules[owner]
                        .scope
                        .get(&name)
                        .is_some_and(|existing| existing != &internal)
                {
                    return Err(error(
                        format!(
                            "duplicate exported declaration '{name}'; import one module with 'as'"
                        ),
                        span,
                    ));
                }
                modules[owner].scope.insert(name.clone(), internal.clone());
                modules[owner].exports.insert(name, internal);
            }
        }
    }
    active.remove(&owner);
    complete.insert(owner);
    Ok(())
}

fn append_alias_spelling(value: &mut String, text: &str, span: Span) -> Result<()> {
    if value
        .len()
        .checked_add(text.len())
        .is_none_or(|length| length > 65536)
    {
        return Err(error("type alias spelling exceeds 64 KiB", span));
    }
    value.push_str(text);
    Ok(())
}

// Resolve in the defining module, before substituting at any caller site.
// Canonical names retain nominal privacy and feed generic identity directly.
fn canonical_aliases(modules: &[Module]) -> Result<BTreeMap<String, String>> {
    let declarations: BTreeMap<_, _> = modules
        .iter()
        .enumerate()
        .flat_map(|(owner, module)| {
            module
                .declarations
                .values()
                .map(move |decl| (decl.internal.clone(), (owner, decl)))
        })
        .collect();
    fn resolve_alias(
        name: &str,
        modules: &[Module],
        declarations: &BTreeMap<String, (usize, &Declaration)>,
        active: &mut BTreeSet<String>,
        cache: &mut BTreeMap<String, String>,
        depth: usize,
        cache_bytes: &mut usize,
    ) -> Result<String> {
        if let Some(value) = cache.get(name) {
            return Ok(value.clone());
        }
        let (owner, decl) = declarations[name];
        let alias = &modules[owner].type_aliases[&decl.name];
        if depth >= 64 || !active.insert(name.to_owned()) {
            return Err(error("cyclic or excessively deep type alias", alias.span));
        }
        #[allow(clippy::too_many_arguments)] // Explicit bounded resolver context.
        fn ty(
            tokens: &[Token],
            index: &mut usize,
            owner: usize,
            public: bool,
            modules: &[Module],
            declarations: &BTreeMap<String, (usize, &Declaration)>,
            active: &mut BTreeSet<String>,
            cache: &mut BTreeMap<String, String>,
            depth: usize,
            cache_bytes: &mut usize,
        ) -> Result<String> {
            let token = tokens
                .get(*index)
                .ok_or_else(|| error("expected alias target type", tokens.last().unwrap().span))?;
            if depth >= 64 {
                return Err(error("type alias nesting exceeds 64", token.span));
            }
            *index += 1;
            let mut arity = 0;
            let mut value = match &token.kind {
                K::IntType => "int".to_owned(),
                K::FloatType => "float".to_owned(),
                K::BoolType => "bool".to_owned(),
                K::StrType => "str".to_owned(),
                K::VoidType => "void".to_owned(),
                K::Ident(name) => {
                    let module = &modules[owner];
                    let internal = if tokens.get(*index).is_some_and(|t| t.kind == K::Dot) {
                        *index += 1;
                        let member = tokens
                            .get(*index)
                            .and_then(ident)
                            .ok_or_else(|| error("expected qualified target type", token.span))?;
                        *index += 1;
                        let target = module
                            .aliases
                            .get(name)
                            .ok_or_else(|| error("unknown target namespace", token.span))?;
                        modules[*target]
                            .exports
                            .get(member)
                            .cloned()
                            .ok_or_else(|| {
                                error("target type is private or not exported", token.span)
                            })?
                    } else if let Some(internal) = module.scope.get(name) {
                        internal.clone()
                    } else if matches!(
                        name.as_str(),
                        "buffer"
                            | "ints"
                            | "tcp_listener"
                            | "tcp_socket"
                            | "tls_socket"
                            | "file_handle"
                            | "task"
                            | "List"
                            | "Map"
                    ) {
                        name.clone()
                    } else {
                        return Err(error(
                            format!("unknown alias target type '{name}'"),
                            token.span,
                        ));
                    };
                    if let Some(&(target_owner, target_decl)) = declarations.get(&internal) {
                        if public && target_decl.private {
                            return Err(error(
                                "public type alias cannot expose a private declaration",
                                token.span,
                            ));
                        }
                        if modules[target_owner]
                            .type_aliases
                            .contains_key(&target_decl.name)
                        {
                            resolve_alias(
                                &internal,
                                modules,
                                declarations,
                                active,
                                cache,
                                depth + 1,
                                cache_bytes,
                            )?
                        } else if matches!(target_decl.kind, K::Struct | K::Enum) {
                            let target_module = &modules[target_owner];
                            let open = next(&target_module.tokens, target_decl.index + 1);
                            if target_module
                                .tokens
                                .get(open)
                                .is_some_and(|t| t.kind == K::LeftBracket)
                            {
                                let close =
                                    target_module.pairs.get(&open).copied().ok_or_else(|| {
                                        error("unclosed generic target declaration", token.span)
                                    })?;
                                arity = target_module.tokens[open + 1..close]
                                    .iter()
                                    .filter(|t| ident(t).is_some())
                                    .count();
                            }
                            internal
                        } else {
                            return Err(error("type alias target must be a type", token.span));
                        }
                    } else {
                        arity = match internal.as_str() {
                            "List" => 1,
                            "Map" => 2,
                            _ => 0,
                        };
                        internal
                    }
                }
                _ => return Err(error("expected alias target type", token.span)),
            };
            let mut arguments = 0;
            if tokens.get(*index).is_some_and(|t| t.kind == K::LeftBracket) {
                if arity == 0 {
                    return Err(error(
                        "alias target does not accept generic arguments",
                        token.span,
                    ));
                }
                *index += 1;
                append_alias_spelling(&mut value, "[", token.span)?;
                loop {
                    if arguments >= arity {
                        return Err(error(
                            "alias target has excess generic arguments",
                            tokens.get(*index).map_or(token.span, |token| token.span),
                        ));
                    }
                    arguments += 1;
                    let argument = ty(
                        tokens,
                        index,
                        owner,
                        public,
                        modules,
                        declarations,
                        active,
                        cache,
                        depth + 1,
                        cache_bytes,
                    )?;
                    append_alias_spelling(&mut value, &argument, token.span)?;
                    match tokens.get(*index).map(|t| &t.kind) {
                        Some(K::Comma) => {
                            // Match ordinary generic syntax: one trailing comma
                            // before the closing bracket adds no argument.
                            if tokens
                                .get(*index + 1)
                                .is_some_and(|token| token.kind == K::RightBracket)
                            {
                                *index += 2;
                                append_alias_spelling(&mut value, "]", token.span)?;
                                break;
                            }
                            if arguments >= arity {
                                return Err(error(
                                    "alias target has excess generic arguments",
                                    tokens
                                        .get(*index + 1)
                                        .map_or(token.span, |token| token.span),
                                ));
                            }
                            *index += 1;
                            append_alias_spelling(&mut value, ",", token.span)?;
                        }
                        Some(K::RightBracket) => {
                            *index += 1;
                            append_alias_spelling(&mut value, "]", token.span)?;
                            break;
                        }
                        _ => return Err(error("expected ',' or ']' in alias target", token.span)),
                    }
                }
            }
            if arguments != arity {
                return Err(error(
                    "alias target requires its concrete generic arguments",
                    token.span,
                ));
            }
            if value.len() > 65536 {
                return Err(error("type alias spelling exceeds 64 KiB", token.span));
            }
            Ok(value)
        }
        let mut index = 0;
        let value = ty(
            &alias.target,
            &mut index,
            owner,
            !decl.private,
            modules,
            declarations,
            active,
            cache,
            depth + 1,
            cache_bytes,
        )?;
        if index != alias.target.len() {
            return Err(error(
                "unexpected token after alias target type",
                alias.target[index].span,
            ));
        }
        active.remove(name);
        *cache_bytes = cache_bytes
            .checked_add(value.len())
            .filter(|bytes| *bytes <= MAX_ALIAS_TARGET_BYTES)
            .ok_or_else(|| {
                error(
                    "canonical alias targets exceed the 16 MiB budget",
                    alias.span,
                )
            })?;
        cache.insert(name.to_owned(), value.clone());
        Ok(value)
    }
    let mut cache = BTreeMap::new();
    let mut cache_bytes = 0;
    for (name, &(owner, decl)) in &declarations {
        if modules[owner].type_aliases.contains_key(&decl.name) {
            resolve_alias(
                name,
                modules,
                &declarations,
                &mut BTreeSet::new(),
                &mut cache,
                0,
                &mut cache_bytes,
            )?;
        }
    }
    Ok(cache)
}

fn collect(module: &mut Module, owner: usize) -> Result<()> {
    let mut index = 0;
    while index < module.tokens.len() {
        if separator(&module.tokens[index].kind) || module.tokens[index].kind == K::Eof {
            index += 1;
            continue;
        }
        let mut private = false;
        if ident(&module.tokens[index]) == Some("private") {
            private = true;
            module.modifiers.insert(index);
            index = next(&module.tokens, index + 1);
        }
        // Contextual top-level syntax leaves ordinary functions named type intact.
        if ident(&module.tokens[index]) == Some("type")
            && module.tokens.get(index + 1).and_then(ident).is_some()
        {
            let keyword = index;
            let name_index = index + 1;
            let name = ident(&module.tokens[name_index]).unwrap().to_owned();
            if module
                .tokens
                .get(index + 2)
                .is_some_and(|t| t.kind == K::LeftBracket)
            {
                return Err(error(
                    "generic type aliases are not supported",
                    module.tokens[name_index].span,
                ));
            }
            if !module
                .tokens
                .get(index + 2)
                .is_some_and(|t| t.kind == K::Equal)
            {
                return Err(error(
                    "expected '=' after type alias name",
                    module.tokens[name_index].span,
                ));
            }
            let start = index + 3;
            let mut end = start;
            while end < module.tokens.len()
                && !separator(&module.tokens[end].kind)
                && module.tokens[end].kind != K::Eof
            {
                end += 1;
            }
            if end == start {
                return Err(error(
                    "expected a type alias target",
                    module.tokens[name_index].span,
                ));
            }
            if module.declarations.contains_key(&name) {
                return Err(error(
                    format!("duplicate declaration '{name}'"),
                    module.tokens[name_index].span,
                ));
            }
            let span = module.tokens[keyword]
                .span
                .join(module.tokens[end - 1].span);
            module.type_aliases.insert(
                name.clone(),
                TypeAlias {
                    target: module.tokens[start..end].to_vec(),
                    span,
                },
            );
            module.declarations.insert(
                name.clone(),
                Declaration {
                    name,
                    internal: String::new(),
                    private,
                    index: name_index,
                    kind: K::Ident("type".to_owned()),
                },
            );
            module.protected.extend(keyword..end);
            index = end;
            continue;
        }
        let kind = module
            .tokens
            .get(index)
            .map(|token| token.kind.clone())
            .unwrap_or(K::Eof);
        let external = kind == K::Extern;
        let keyword = if external {
            next(&module.tokens, index + 1)
        } else {
            index
        };
        if !matches!(
            module.tokens.get(keyword).map(|token| &token.kind),
            Some(K::Def | K::Struct | K::Enum)
        ) {
            if private {
                return Err(error(
                    "'private' must precede a function, struct, or enum declaration",
                    module.tokens[index.saturating_sub(1)].span,
                ));
            }
            // Invalid source is left to the parser; skip balanced blocks so its
            // contents cannot accidentally become global declarations.
            index = module
                .pairs
                .get(&index)
                .map_or(index + 1, |close| close + 1);
            continue;
        }
        let declaration_kind = module.tokens[keyword].kind.clone();
        let name_index = next(&module.tokens, keyword + 1);
        let Some(name) = module
            .tokens
            .get(name_index)
            .and_then(ident)
            .map(str::to_owned)
        else {
            index = keyword + 1;
            continue;
        };
        let mut header = next(&module.tokens, name_index + 1);
        let generic_open = if module
            .tokens
            .get(header)
            .is_some_and(|token| token.kind == K::LeftBracket)
        {
            Some(header)
        } else {
            None
        };
        if let Some(open) = generic_open {
            header = module
                .pairs
                .get(&open)
                .map_or(header, |close| next(&module.tokens, close + 1));
        }
        let body = if declaration_kind == K::Def {
            if module
                .tokens
                .get(header)
                .is_some_and(|token| token.kind == K::LeftParen)
            {
                let close = module.pairs.get(&header).copied().unwrap_or(header);
                find_body(&module.tokens, close + 1, external)
            } else {
                None
            }
        } else {
            module
                .tokens
                .get(header)
                .filter(|token| token.kind == K::LeftBrace)
                .map(|_| header)
        };
        let end = body
            .and_then(|open| module.pairs.get(&open).copied())
            .unwrap_or_else(|| {
                let mut end = header;
                while end + 1 < module.tokens.len()
                    && !separator(&module.tokens[end].kind)
                    && module.tokens[end].kind != K::Eof
                {
                    end += 1;
                }
                end
            });
        let declaration = Declaration {
            name: name.clone(),
            internal: String::new(),
            private,
            index: name_index,
            kind: if external {
                K::Extern
            } else {
                declaration_kind.clone()
            },
        };
        if module.type_aliases.contains_key(&name) {
            return Err(error(
                format!("duplicate declaration '{name}'"),
                module.tokens[name_index].span,
            ));
        }
        // Keep duplicate declarations in the source for the ordinary checker.
        module.declarations.insert(name.clone(), declaration);
        if let Some(open) = generic_open {
            if let Some(&close) = module.pairs.get(&open) {
                for token_index in open + 1..close {
                    if let Some(parameter) = ident(&module.tokens[token_index]) {
                        module.protected.insert(token_index);
                        module.bindings.push(Binding {
                            name: parameter.to_owned(),
                            start: module.tokens[name_index].span.end,
                            end: module.tokens[end].span.end,
                            type_only: true,
                        });
                    }
                }
            }
        }
        if declaration_kind == K::Struct {
            if let Some(open) = body {
                collect_members(module, owner, &name, open, end)?;
            }
        } else if declaration_kind == K::Enum {
            if let Some(open) = body {
                let mut token_index = open + 1;
                while token_index < end {
                    if ident(&module.tokens[token_index]) == Some("private") {
                        return Err(error(
                            "enum variants are public; put privacy on the enum declaration",
                            module.tokens[token_index].span,
                        ));
                    }
                    if ident(&module.tokens[token_index]).is_some()
                        && (token_index == open + 1
                            || separator(&module.tokens[token_index - 1].kind)
                            || module.tokens[token_index - 1].kind == K::Comma)
                    {
                        module.protected.insert(token_index);
                    }
                    if let Some(close) = module.pairs.get(&token_index) {
                        token_index = *close + 1;
                    } else {
                        token_index += 1;
                    }
                }
            }
        }
        if declaration_kind == K::Def {
            collect_parameters(module, header, body, end);
        }
        index = end + 1;
    }
    // Field/parameter labels, receiver member names and local binding names are
    // not module declarations. Mark type syntax separately from value scopes.
    for index in 0..module.tokens.len() {
        if ident(&module.tokens[index]) == Some("move")
            && module.tokens.get(index + 1).and_then(ident).is_some()
        {
            module.protected.insert(index);
        }
        if module.tokens[index].kind == K::Colon || module.tokens[index].kind == K::Arrow {
            if module.tokens[index].kind == K::Colon
                && index > 0
                && ident(&module.tokens[index - 1]).is_some()
            {
                module.protected.insert(index - 1);
            }
            mark_type(module, index + 1);
        }
        if module.tokens[index].kind == K::Dot && index + 1 < module.tokens.len() {
            module.protected.insert(index + 1);
        }
        // Brackets only spell generic parameter/type arguments in this grammar.
        // Value locals must not shadow type names in `identity[Point](value)`.
        if module.tokens[index].kind == K::LeftBracket {
            let mut argument = index + 1;
            loop {
                let end = mark_type(module, argument);
                if end <= argument {
                    break;
                }
                argument = end;
                if module
                    .tokens
                    .get(argument)
                    .is_some_and(|token| token.kind == K::Comma)
                {
                    argument += 1;
                } else {
                    break;
                }
            }
        }
    }
    collect_bindings(module);
    Ok(())
}
fn find_body(tokens: &[Token], mut index: usize, external: bool) -> Option<usize> {
    if external {
        return None;
    }
    while index < tokens.len() {
        match tokens[index].kind {
            K::LeftBrace => return Some(index),
            K::Semicolon | K::Eof => return None,
            K::Def | K::Struct | K::Enum => return None,
            _ => index += 1,
        }
    }
    None
}
fn collect_parameters(module: &mut Module, open: usize, body: Option<usize>, end: usize) {
    let Some(&close) = module.pairs.get(&open) else {
        return;
    };
    if module.tokens[open].kind != K::LeftParen {
        return;
    }
    let start = body.map_or(module.tokens[end].span.end, |body| {
        module.tokens[body].span.end
    });
    let mut index = open + 1;
    while index < close {
        if let Some(name) = ident(&module.tokens[index]) {
            if index == open + 1 || module.tokens[index - 1].kind == K::Comma {
                let mut parameter_index = index;
                if name == "owned" && module.tokens.get(index + 1).and_then(ident).is_some() {
                    module.protected.insert(index);
                    parameter_index += 1;
                }
                let name = ident(&module.tokens[parameter_index]).unwrap();
                module.protected.insert(parameter_index);
                module.bindings.push(Binding {
                    name: name.to_owned(),
                    start,
                    end: module.tokens[end].span.end,
                    type_only: false,
                });
            }
        }
        if let Some(close) = module.pairs.get(&index) {
            index = *close + 1;
        } else {
            index += 1;
        }
    }
}
fn collect_members(
    module: &mut Module,
    owner: usize,
    record: &str,
    open: usize,
    end: usize,
) -> Result<()> {
    let mut privacy = MemberPrivacy {
        owner,
        fields: BTreeSet::new(),
        methods: BTreeSet::new(),
    };
    let mut index = open + 1;
    while index < end {
        if separator(&module.tokens[index].kind) || module.tokens[index].kind == K::Comma {
            index += 1;
            continue;
        }
        let mut private = false;
        if ident(&module.tokens[index]) == Some("private") {
            private = true;
            module.modifiers.insert(index);
            index = next(&module.tokens, index + 1);
        }
        if module
            .tokens
            .get(index)
            .is_some_and(|token| token.kind == K::Def)
        {
            let name_index = next(&module.tokens, index + 1);
            let Some(name) = module
                .tokens
                .get(name_index)
                .and_then(ident)
                .map(str::to_owned)
            else {
                return Ok(());
            };
            module.protected.insert(name_index);
            if private {
                privacy.methods.insert(name);
            }
            let params = next(&module.tokens, name_index + 1);
            let close = module.pairs.get(&params).copied().unwrap_or(params);
            let body = find_body(&module.tokens, close + 1, false);
            let method_end = body
                .and_then(|body| module.pairs.get(&body).copied())
                .unwrap_or(close);
            collect_parameters(module, params, body, method_end);
            index = method_end + 1;
        } else if let Some(name) = module.tokens.get(index).and_then(ident).map(str::to_owned) {
            if module
                .tokens
                .get(index + 1)
                .is_some_and(|token| token.kind == K::Colon)
            {
                module.protected.insert(index);
                if private {
                    privacy.fields.insert(name);
                }
                index += 2;
                while index < end
                    && !separator(&module.tokens[index].kind)
                    && module.tokens[index].kind != K::Comma
                {
                    if let Some(close) = module.pairs.get(&index) {
                        index = *close + 1;
                    } else {
                        index += 1;
                    }
                }
            } else if private {
                return Err(error(
                    "'private' must precede a record field or method",
                    module.tokens[index].span,
                ));
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    if !privacy.fields.is_empty() || !privacy.methods.is_empty() {
        module.private_members.insert(record.to_owned(), privacy);
    }
    Ok(())
}
fn mark_type(module: &mut Module, index: usize) -> usize {
    mark_type_inner(module, index, 0)
}
fn mark_type_inner(module: &mut Module, index: usize, depth: usize) -> usize {
    if depth > 128 {
        return index;
    }
    let mut index = next(&module.tokens, index);
    if index >= module.tokens.len() {
        return index;
    }
    if matches!(
        module.tokens[index].kind,
        K::Ident(_) | K::IntType | K::FloatType | K::BoolType | K::StrType | K::VoidType
    ) {
        module.type_tokens.insert(index);
        index += 1;
        if module
            .tokens
            .get(index)
            .is_some_and(|token| token.kind == K::Dot)
            && module.tokens.get(index + 1).and_then(ident).is_some()
        {
            module.type_tokens.insert(index + 1);
            index += 2;
        }
        if module
            .tokens
            .get(index)
            .is_some_and(|token| token.kind == K::LeftBracket)
        {
            index += 1;
            loop {
                let next_index = mark_type_inner(module, index, depth + 1);
                if next_index <= index {
                    break;
                }
                index = next_index;
                if module
                    .tokens
                    .get(index)
                    .is_some_and(|token| token.kind == K::Comma)
                {
                    index += 1;
                } else {
                    break;
                }
            }
            if module
                .tokens
                .get(index)
                .is_some_and(|token| token.kind == K::RightBracket)
            {
                index += 1;
            }
        }
    }
    index
}
/// Contextual let/try markers in local declaration headers.
/// The parser owns the immediate marker/operand predicate; lowering protects
/// these identifiers while still rewriting every name in the operand.
pub(crate) fn contextual_binding_markers(tokens: &[Token]) -> BTreeSet<usize> {
    let mut markers = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        let immutable = crate::parser::starts_let_declaration(tokens, index);
        if token.kind != K::Var && !immutable {
            continue;
        }
        if immutable {
            markers.insert(index);
        }
        let name = next(tokens, index + 1);
        if tokens.get(name).and_then(ident).is_none() {
            continue;
        }
        let mut cursor = name + 1;
        if tokens
            .get(cursor)
            .is_some_and(|token| token.kind == K::Colon)
        {
            cursor = next(tokens, cursor + 1);
            if !tokens.get(cursor).is_some_and(|token| {
                matches!(
                    token.kind,
                    K::Ident(_)
                        | K::IntType
                        | K::FloatType
                        | K::BoolType
                        | K::StrType
                        | K::VoidType
                )
            }) {
                continue;
            }
            cursor += 1;
            while tokens.get(cursor).is_some_and(|token| token.kind == K::Dot) {
                if tokens.get(cursor + 1).and_then(ident).is_none() {
                    break;
                }
                cursor += 2;
            }
            if tokens
                .get(cursor)
                .is_some_and(|token| token.kind == K::LeftBracket)
            {
                let mut depth = 0usize;
                while cursor < tokens.len() {
                    match tokens[cursor].kind {
                        K::LeftBracket => {
                            depth += 1;
                            if depth > 64 {
                                break;
                            }
                        }
                        K::RightBracket => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                cursor += 1;
                                break;
                            }
                        }
                        K::Ident(_)
                        | K::IntType
                        | K::FloatType
                        | K::BoolType
                        | K::StrType
                        | K::VoidType
                        | K::Dot
                        | K::Comma
                        | K::Newline => {}
                        _ => break,
                    }
                    cursor += 1;
                }
            }
        }
        if tokens
            .get(cursor)
            .is_some_and(|token| token.kind == K::Equal)
            && crate::parser::starts_try_initializer(tokens, cursor + 1)
        {
            markers.insert(cursor + 1);
        }
    }
    markers
}

fn collect_bindings(module: &mut Module) {
    module
        .protected
        .extend(contextual_binding_markers(&module.tokens));
    let mut braces = Vec::new();
    let mut expression_depth = 0usize;
    for index in 0..module.tokens.len() {
        match module.tokens[index].kind {
            K::LeftParen | K::LeftBracket => expression_depth += 1,
            K::RightParen | K::RightBracket => {
                expression_depth = expression_depth.saturating_sub(1)
            }
            K::LeftBrace => braces.push(index),
            K::RightBrace => {
                braces.pop();
            }
            K::For if expression_depth == 0 => {
                let name_index = next(&module.tokens, index + 1);
                let Some(name) = module.tokens.get(name_index).and_then(ident) else {
                    continue;
                };
                let name = name.to_owned();
                module.protected.insert(name_index);
                // The binder becomes visible only inside its body, never in the
                // source expression. Skip bracket/paren pairs in that expression.
                let mut open = name_index + 1;
                while open < module.tokens.len() && module.tokens[open].kind != K::LeftBrace {
                    if let Some(close) = module.pairs.get(&open) {
                        open = *close + 1;
                    } else {
                        open += 1;
                    }
                }
                if let Some(close) = module.pairs.get(&open) {
                    module.bindings.push(Binding {
                        name,
                        start: module.tokens[open].span.end,
                        end: module.tokens[*close].span.start,
                        type_only: false,
                    });
                }
            }
            // Declarations are statements, never expressions. Malformed nested
            // annotations must not create thousands of synthetic local scopes.
            _ if expression_depth == 0
                && (module.tokens[index].kind == K::Var
                    || crate::parser::starts_let_declaration(&module.tokens, index)) =>
            {
                let name_index = next(&module.tokens, index + 1);
                let Some(name) = module.tokens.get(name_index).and_then(ident) else {
                    continue;
                };
                let name = name.to_owned();
                module.protected.insert(name_index);
                let end = braces
                    .last()
                    .and_then(|open| module.pairs.get(open))
                    .map_or(module.tokens.last().unwrap().span.end, |close| {
                        module.tokens[*close].span.start
                    });
                let mut finish = name_index + 1;
                while finish < module.tokens.len()
                    && !separator(&module.tokens[finish].kind)
                    && module.tokens[finish].kind != K::RightBrace
                    && module.tokens[finish].kind != K::Eof
                {
                    if let Some(close) = module.pairs.get(&finish) {
                        finish = *close + 1;
                    } else {
                        finish += 1;
                    }
                }
                let start = module
                    .tokens
                    .get(finish)
                    .map_or(end, |token| token.span.start);
                module.bindings.push(Binding {
                    name,
                    start,
                    end,
                    type_only: false,
                });
            }
            K::Case if expression_depth == 0 => {
                let mut cursor = index + 1;
                while cursor < module.tokens.len()
                    && !matches!(
                        module.tokens[cursor].kind,
                        K::LeftParen | K::LeftBrace | K::Eof
                    )
                {
                    cursor += 1;
                }
                if module
                    .tokens
                    .get(cursor)
                    .is_some_and(|token| token.kind == K::LeftParen)
                {
                    let Some(&close) = module.pairs.get(&cursor) else {
                        continue;
                    };
                    let body = next(&module.tokens, close + 1);
                    let Some(&end) = module.pairs.get(&body) else {
                        continue;
                    };
                    for binding in cursor + 1..close {
                        if let Some(name) = ident(&module.tokens[binding]) {
                            module.protected.insert(binding);
                            module.bindings.push(Binding {
                                name: name.to_owned(),
                                start: module.tokens[body].span.end,
                                end: module.tokens[end].span.start,
                                type_only: false,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn visit_program_spans(program: &mut ast::Program, map: &impl Fn(Span) -> Span) {
    fn expr(value: &mut ast::Expr, map: &impl Fn(Span) -> Span) {
        value.span = map(value.span);
        match &mut value.kind {
            ast::ExprKind::Move { name_span, .. } => *name_span = map(*name_span),
            ast::ExprKind::Call { args, .. } => {
                for arg in args {
                    expr(arg, map);
                }
            }
            ast::ExprKind::MethodCall { receiver, args, .. } => {
                expr(receiver, map);
                for arg in args {
                    expr(arg, map);
                }
            }
            ast::ExprKind::Field { base, .. } | ast::ExprKind::Unary { operand: base, .. } => {
                expr(base, map)
            }
            ast::ExprKind::Binary { left, right, .. } => {
                expr(left, map);
                expr(right, map);
            }
            ast::ExprKind::Int(_)
            | ast::ExprKind::Float(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Str(_)
            | ast::ExprKind::Name(_) => {}
        }
    }
    fn block(body: &mut ast::Block, map: &impl Fn(Span) -> Span) {
        for statement in body {
            statement.span = map(statement.span);
            match &mut statement.kind {
                ast::StmtKind::TryVar {
                    name_span,
                    try_span,
                    value,
                    ..
                } => {
                    *name_span = map(*name_span);
                    *try_span = map(*try_span);
                    expr(value, map);
                }
                ast::StmtKind::Var { value, .. }
                | ast::StmtKind::Assign { value, .. }
                | ast::StmtKind::Expr(value) => expr(value, map),
                ast::StmtKind::FieldAssign { target, value } => {
                    expr(target, map);
                    expr(value, map);
                }
                ast::StmtKind::Return(value) => {
                    if let Some(value) = value {
                        expr(value, map);
                    }
                }
                ast::StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    expr(condition, map);
                    block(then_block, map);
                    if let Some(body) = else_block {
                        block(body, map);
                    }
                }
                ast::StmtKind::While { condition, body } => {
                    expr(condition, map);
                    block(body, map);
                }
                ast::StmtKind::For {
                    name_span,
                    source,
                    body,
                    ..
                } => {
                    *name_span = map(*name_span);
                    match source {
                        ast::ForSource::Range { start, end, step } => {
                            expr(start, map);
                            expr(end, map);
                            if let Some(step) = step {
                                expr(step, map);
                            }
                        }
                        ast::ForSource::Iterable(value) => expr(value, map),
                    }
                    block(body, map);
                }
                ast::StmtKind::Break | ast::StmtKind::Continue => {}
                ast::StmtKind::Match { value, arms } => {
                    expr(value, map);
                    for arm in arms {
                        arm.span = map(arm.span);
                        for binding in &mut arm.bindings {
                            binding.span = map(binding.span);
                        }
                        block(&mut arm.body, map);
                    }
                }
            }
        }
    }
    fn function(function: &mut ast::Function, map: &impl Fn(Span) -> Span) {
        function.span = map(function.span);
        for param in &mut function.params {
            param.span = map(param.span);
        }
        if let Some(body) = &mut function.body {
            block(body, map);
        }
    }
    for item in &mut program.functions {
        function(item, map);
    }
    for item in &mut program.records {
        item.span = map(item.span);
        for field in &mut item.fields {
            field.span = map(field.span);
        }
        for method in &mut item.methods {
            function(method, map);
        }
    }
    for item in &mut program.enums {
        item.span = map(item.span);
        for variant in &mut item.variants {
            variant.span = map(variant.span);
            for field in &mut variant.fields {
                field.span = map(field.span);
            }
        }
    }
}
pub(crate) fn visit_program_expressions(
    program: &ast::Program,
    visit: &mut impl FnMut(&ast::Expr),
) {
    fn expr(value: &ast::Expr, visit: &mut impl FnMut(&ast::Expr)) {
        visit(value);
        match &value.kind {
            ast::ExprKind::Call { args, .. } => {
                for arg in args {
                    expr(arg, visit);
                }
            }
            ast::ExprKind::MethodCall { receiver, args, .. } => {
                expr(receiver, visit);
                for arg in args {
                    expr(arg, visit);
                }
            }
            ast::ExprKind::Field { base, .. } | ast::ExprKind::Unary { operand: base, .. } => {
                expr(base, visit)
            }
            ast::ExprKind::Binary { left, right, .. } => {
                expr(left, visit);
                expr(right, visit);
            }
            ast::ExprKind::Int(_)
            | ast::ExprKind::Float(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Str(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::Move { .. } => {}
        }
    }
    fn block(body: &ast::Block, visit: &mut impl FnMut(&ast::Expr)) {
        for statement in body {
            match &statement.kind {
                ast::StmtKind::Var { value, .. }
                | ast::StmtKind::TryVar { value, .. }
                | ast::StmtKind::Assign { value, .. }
                | ast::StmtKind::Expr(value) => expr(value, visit),
                ast::StmtKind::FieldAssign { target, value } => {
                    expr(target, visit);
                    expr(value, visit);
                }
                ast::StmtKind::Return(value) => {
                    if let Some(value) = value {
                        expr(value, visit);
                    }
                }
                ast::StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    expr(condition, visit);
                    block(then_block, visit);
                    if let Some(body) = else_block {
                        block(body, visit);
                    }
                }
                ast::StmtKind::While { condition, body } => {
                    expr(condition, visit);
                    block(body, visit);
                }
                ast::StmtKind::For { source, body, .. } => {
                    match source {
                        ast::ForSource::Range { start, end, step } => {
                            expr(start, visit);
                            expr(end, visit);
                            if let Some(step) = step {
                                expr(step, visit);
                            }
                        }
                        ast::ForSource::Iterable(value) => expr(value, visit),
                    }
                    block(body, visit);
                }
                ast::StmtKind::Break | ast::StmtKind::Continue => {}
                ast::StmtKind::Match { value, arms } => {
                    expr(value, visit);
                    for arm in arms {
                        block(&arm.body, visit);
                    }
                }
            }
        }
    }
    for function in &program.functions {
        if let Some(body) = &function.body {
            block(body, visit);
        }
    }
    for record in &program.records {
        for function in &record.methods {
            if let Some(body) = &function.body {
                block(body, visit);
            }
        }
    }
}

fn visit_program_statements(program: &ast::Program, visit: &mut impl FnMut(&ast::Stmt)) {
    fn block(body: &ast::Block, visit: &mut impl FnMut(&ast::Stmt)) {
        for statement in body {
            visit(statement);
            match &statement.kind {
                ast::StmtKind::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    block(then_block, visit);
                    if let Some(body) = else_block {
                        block(body, visit)
                    }
                }
                ast::StmtKind::While { body, .. } | ast::StmtKind::For { body, .. } => {
                    block(body, visit)
                }
                ast::StmtKind::Match { arms, .. } => {
                    for arm in arms {
                        block(&arm.body, visit)
                    }
                }
                _ => {}
            }
        }
    }
    for function in &program.functions {
        if let Some(body) = &function.body {
            block(body, visit)
        }
    }
    for record in &program.records {
        for method in &record.methods {
            if let Some(body) = &method.body {
                block(body, visit)
            }
        }
    }
}

#[cfg(test)]
mod try_marker_tests {
    fn bindings(text: &str) -> Vec<super::Binding> {
        let tokens = crate::lexer::lex_rust(text).unwrap();
        let mut module = super::Module {
            pairs: super::pairs(&tokens),
            tokens,
            declarations: Default::default(),
            exports: Default::default(),
            scope: Default::default(),
            aliases: Default::default(),
            protected: Default::default(),
            modifiers: Default::default(),
            bindings: Default::default(),
            type_tokens: Default::default(),
            private_members: Default::default(),
            type_aliases: Default::default(),
        };
        super::collect_bindings(&mut module);
        module.bindings
    }

    #[test]
    fn malformed_nested_var_headers_do_not_protect_a_propagation_marker() {
        let text = format!(
            "def main(){{{}{}=try source}}",
            "var x:T[".repeat(4000),
            "]".repeat(4000)
        );
        let tokens = crate::lexer::lex_rust(&text).unwrap();
        assert!(super::contextual_binding_markers(&tokens).is_empty());
        assert_eq!(bindings(&text).len(), 1);
    }

    #[test]
    fn expression_delimiters_do_not_hide_subsequent_statement_bindings() {
        let text = "def main(){var initial=make[List[int]](nested(1));for item in range(3){var loop=1}match Some.ok(1){case Some.ok(value){var branch=1}}}";
        let names: Vec<_> = bindings(text)
            .into_iter()
            .map(|binding| binding.name)
            .collect();
        assert_eq!(names, ["initial", "item", "loop", "value", "branch"]);
    }
}
