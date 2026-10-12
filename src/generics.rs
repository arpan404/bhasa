//! Explicit, source-level monomorphization shared by both compiler frontends.
//!
//! Templates disappear before ordinary parsing. Every concrete instantiation is
//! checked by the existing type/ownership checker; no boxed or erased ABI exists.
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{self, Span};
use crate::Diagnostic;

const PREFIX: &str = "__bhg_";
const MAX_TYPE_DEPTH: usize = 64;
const MAX_INSTANTIATIONS: usize = 1024;
const MAX_GENERATED_BYTES: usize = 16 * 1024 * 1024;

type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Debug, Clone)]
struct Mapping {
    generated: Span,
    original: Span,
    linear: bool,
}

/// Expanded source and a mapping back to the caller's original virtual source.
#[derive(Debug, Clone)]
pub struct Expansion {
    pub(crate) channels: Option<crate::channel_registry::ExpansionReceipts>,
    pub source: String,
    /// EOF positions, each followed by a separator byte unless at source EOF.
    pub boundaries: Vec<usize>,
    mappings: Vec<Mapping>,
    names: BTreeMap<String, String>,
    original_len: usize,
    identity: bool,
}

impl Expansion {
    /// True when no templates or applications changed the input.
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    pub fn map_span(&self, span: Span) -> Span {
        fn position(expansion: &Expansion, offset: usize, end: bool) -> usize {
            let offset = offset.min(expansion.source.len());
            let index = expansion.mappings.partition_point(|map| {
                if end {
                    map.generated.end < offset
                } else {
                    map.generated.end <= offset
                }
            });
            let map = expansion
                .mappings
                .get(index)
                .or_else(|| expansion.mappings.last());
            match map {
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
                None => offset.min(expansion.original_len),
            }
        }
        let start = position(self, span.start, false);
        let end = position(self, span.end, true).max(start);
        Span::new(start.min(self.original_len), end.min(self.original_len))
    }

    /// Map a concrete specialization back to its declaration identifier. Method
    /// suffixes remain intact, so `__bhg_0.get` becomes `Box.get`.
    pub fn original_name(&self, name: &str) -> String {
        let (base, suffix) = name.split_once('.').unwrap_or((name, ""));
        let Some(display) = self.names.get(base) else {
            return name.to_owned();
        };
        let original = display
            .split_once('[')
            .map_or(display.as_str(), |(name, _)| name);
        if suffix.is_empty() {
            original.to_owned()
        } else {
            format!("{original}.{suffix}")
        }
    }

    /// Restore concrete source names in diagnostic or tooling text.
    pub fn display_names(&self, text: &str) -> String {
        let mut names: Vec<_> = self.names.iter().collect();
        names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        let mut result = text.to_owned();
        for (name, display) in names {
            result = result.replace(name, display);
        }
        result
    }

    /// Restore readable names without changing already-remapped spans.
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

    /// Remap syntax locations before checking, so later diagnostics and tooling
    /// continue to use the original files' virtual offsets.
    pub fn remap_program(&self, program: &mut ast::Program) {
        fn expression(expansion: &Expansion, value: &mut ast::Expr) {
            value.span = expansion.map_span(value.span);
            match &mut value.kind {
                ast::ExprKind::Move { name_span, .. } => {
                    *name_span = expansion.map_span(*name_span)
                }
                ast::ExprKind::Call { args, .. } => {
                    for arg in args {
                        expression(expansion, arg);
                    }
                }
                ast::ExprKind::MethodCall { receiver, args, .. } => {
                    expression(expansion, receiver);
                    for arg in args {
                        expression(expansion, arg);
                    }
                }
                ast::ExprKind::Field { base, .. } | ast::ExprKind::Unary { operand: base, .. } => {
                    expression(expansion, base)
                }
                ast::ExprKind::Binary { left, right, .. } => {
                    expression(expansion, left);
                    expression(expansion, right);
                }
                ast::ExprKind::Int(_)
                | ast::ExprKind::Float(_)
                | ast::ExprKind::Bool(_)
                | ast::ExprKind::Str(_)
                | ast::ExprKind::Name(_) => {}
            }
        }
        fn block(expansion: &Expansion, body: &mut ast::Block) {
            for statement in body {
                statement.span = expansion.map_span(statement.span);
                match &mut statement.kind {
                    ast::StmtKind::TryVar {
                        name_span,
                        try_span,
                        value,
                        ..
                    } => {
                        *name_span = expansion.map_span(*name_span);
                        *try_span = expansion.map_span(*try_span);
                        expression(expansion, value);
                    }
                    ast::StmtKind::Var { value, .. }
                    | ast::StmtKind::Assign { value, .. }
                    | ast::StmtKind::Expr(value) => expression(expansion, value),
                    ast::StmtKind::FieldAssign { target, value } => {
                        expression(expansion, target);
                        expression(expansion, value);
                    }
                    ast::StmtKind::Return(value) => {
                        if let Some(value) = value {
                            expression(expansion, value);
                        }
                    }
                    ast::StmtKind::If {
                        condition,
                        then_block,
                        else_block,
                    } => {
                        expression(expansion, condition);
                        block(expansion, then_block);
                        if let Some(body) = else_block {
                            block(expansion, body);
                        }
                    }
                    ast::StmtKind::While { condition, body } => {
                        expression(expansion, condition);
                        block(expansion, body);
                    }
                    ast::StmtKind::For {
                        name_span,
                        source,
                        body,
                        ..
                    } => {
                        *name_span = expansion.map_span(*name_span);
                        match source {
                            ast::ForSource::Range { start, end, step } => {
                                expression(expansion, start);
                                expression(expansion, end);
                                if let Some(step) = step {
                                    expression(expansion, step);
                                }
                            }
                            ast::ForSource::Iterable(value) => expression(expansion, value),
                        }
                        block(expansion, body);
                    }
                    ast::StmtKind::Break | ast::StmtKind::Continue => {}
                    ast::StmtKind::Match { value, arms } => {
                        expression(expansion, value);
                        for arm in arms {
                            arm.span = expansion.map_span(arm.span);
                            for binding in &mut arm.bindings {
                                binding.span = expansion.map_span(binding.span);
                            }
                            block(expansion, &mut arm.body);
                        }
                    }
                }
            }
        }
        fn function(expansion: &Expansion, function: &mut ast::Function) {
            function.span = expansion.map_span(function.span);
            for param in &mut function.params {
                param.span = expansion.map_span(param.span);
            }
            if let Some(body) = &mut function.body {
                block(expansion, body);
            }
        }
        for value in &mut program.functions {
            function(self, value);
        }
        for record in &mut program.records {
            record.span = self.map_span(record.span);
            for field in &mut record.fields {
                field.span = self.map_span(field.span);
            }
            for method in &mut record.methods {
                function(self, method);
            }
        }
        for declaration in &mut program.enums {
            declaration.span = self.map_span(declaration.span);
            for variant in &mut declaration.variants {
                variant.span = self.map_span(variant.span);
                for field in &mut variant.fields {
                    field.span = self.map_span(field.span);
                }
            }
        }
    }
}

/// Parse each module separately; an incomplete declaration may never consume
/// tokens from the next original module or generated specialization.
pub fn parse_expansion(
    expansion: &Expansion,
) -> std::result::Result<ast::Program, Vec<Diagnostic>> {
    if let Some(receipts) = &expansion.channels {
        receipts
            .validate(&expansion.source, &expansion.boundaries)
            .map_err(|error| vec![error])?;
    }
    let mut program = ast::Program::default();
    let mut start = 0;
    let mut ends = expansion.boundaries.clone();
    if ends.last().copied() != Some(expansion.source.len()) {
        ends.push(expansion.source.len());
    }
    for end in ends {
        if end < start {
            continue;
        }
        let source = &expansion.source[start..end];
        let mut tokens = crate::lexer::lex(source).map_err(|mut errors| {
            for error in &mut errors {
                error.span.start += start;
                error.span.end += start;
            }
            expansion.map_diagnostics(errors)
        })?;
        for token in &mut tokens {
            token.span.start += start;
            token.span.end += start;
        }
        let unit = crate::parser::parse_tokens(tokens)
            .map_err(|errors| expansion.map_diagnostics(errors))?;
        program.merge(unit);
        start = end.saturating_add(1);
    }
    expansion.remap_program(&mut program);
    Ok(program)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Atom,
    Symbol,
    Newline,
    Boundary,
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    span: Span,
}
impl Token {
    fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.start..self.span.end]
    }
}

fn error(code: &str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(code, message, span)
}
fn word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}
fn word_byte(byte: u8) -> bool {
    word_start(byte) || byte.is_ascii_digit()
}
fn primitive(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "float"
            | "bool"
            | "str"
            | "buffer"
            | "ints"
            | "tcp_listener"
            | "tcp_socket"
            | "tls_socket"
            | "file_handle"
    )
}
fn keyword(name: &str) -> bool {
    matches!(
        name,
        "def"
            | "struct"
            | "enum"
            | "match"
            | "case"
            | "extern"
            | "var"
            | "if"
            | "else"
            | "while"
            | "for"
            | "in"
            | "break"
            | "continue"
            | "return"
            | "true"
            | "false"
            | "and"
            | "or"
            | "not"
            | "int"
            | "float"
            | "bool"
            | "str"
            | "void"
    )
}

fn scan(source: &str, boundaries: &[usize]) -> Result<Vec<Token>> {
    if boundaries.windows(2).any(|pair| pair[0] >= pair[1])
        || boundaries.iter().any(|&index| {
            index > source.len()
                || !source.is_char_boundary(index)
                || (index < source.len() && !matches!(source.as_bytes()[index], b'\n' | b' '))
        })
    {
        return Err(error(
            "G001",
            "invalid generic expansion module boundaries",
            Span::default(),
        ));
    }
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut boundary = 0;
    while index < bytes.len() {
        if boundaries.get(boundary) == Some(&index) {
            tokens.push(Token {
                kind: Kind::Boundary,
                span: Span::new(index, index),
            });
            boundary += 1;
            index += 1;
            continue;
        }
        let start = index;
        let byte = bytes[index];
        if byte == 0 {
            return Err(error(
                "L006",
                "NUL bytes are not allowed in source",
                Span::new(index, index + 1),
            ));
        }
        let kind = if matches!(byte, b' ' | b'\t' | 0x0b | 0x0c) {
            index += 1;
            continue;
        } else if matches!(byte, b'\r' | b'\n') {
            index += 1;
            if byte == b'\r'
                && bytes.get(index) == Some(&b'\n')
                && boundaries.get(boundary) != Some(&index)
            {
                index += 1;
            }
            Kind::Newline
        } else if byte == b'#' || (byte == b'/' && bytes.get(index + 1) == Some(&b'/')) {
            while index < bytes.len()
                && !matches!(bytes[index], b'\r' | b'\n')
                && boundaries.get(boundary) != Some(&index)
            {
                if bytes[index] == 0 {
                    return Err(error(
                        "L006",
                        "NUL bytes are not allowed in source",
                        Span::new(index, index + 1),
                    ));
                }
                index += 1;
            }
            continue;
        } else if byte == b'"' {
            index += 1;
            let mut closed = false;
            while index < bytes.len() && boundaries.get(boundary) != Some(&index) {
                let character_start = index;
                // Walk complete characters even on invalid input: diagnostic
                // spans must remain valid UTF-8 offsets for direct API clients.
                let character = source[index..].chars().next().unwrap();
                index += character.len_utf8();
                if character == '"' {
                    closed = true;
                    break;
                }
                let escaped = character == '\\';
                let character = if escaped {
                    if index == bytes.len() || boundaries.get(boundary) == Some(&index) {
                        break;
                    }
                    let character = source[index..].chars().next().unwrap();
                    index += character.len_utf8();
                    character
                } else {
                    character
                };
                if character == '\0' {
                    return Err(error(
                        "L006",
                        "NUL bytes are not allowed in strings",
                        Span::new(character_start, index),
                    ));
                }
                if matches!(character, '\r' | '\n') {
                    if character == '\r'
                        && bytes.get(index) == Some(&b'\n')
                        && boundaries.get(boundary) != Some(&index)
                    {
                        index += 1;
                    }
                    return Err(error(
                        "L005",
                        "string literals cannot contain raw line breaks; use \\n or \\r",
                        Span::new(character_start, index),
                    ));
                }
                if escaped && !matches!(character, 'n' | 'r' | 't' | '\\' | '"') {
                    return Err(error(
                        "L004",
                        format!("unsupported string escape \\{character}"),
                        Span::new(character_start, index),
                    ));
                }
            }
            if !closed {
                return Err(error(
                    "L003",
                    "unterminated string literal",
                    Span::new(start, index),
                ));
            }
            Kind::Atom
        } else if word_start(byte) {
            index += 1;
            while index < bytes.len() && word_byte(bytes[index]) {
                index += 1;
            }
            if source[start..index].starts_with(PREFIX) {
                return Err(error(
                    "G008",
                    "identifier prefix '__bhg_' is reserved for generic specializations",
                    Span::new(start, index),
                ));
            }
            Kind::Word
        } else if byte.is_ascii_digit() {
            index += 1;
            while index < bytes.len() && word_byte(bytes[index]) {
                index += 1;
            }
            Kind::Atom
        } else {
            index += source[index..].chars().next().unwrap().len_utf8();
            if byte == b'-' && bytes.get(index) == Some(&b'>') {
                index += 1;
            }
            Kind::Symbol
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, index),
        });
    }
    if boundaries.get(boundary) == Some(&index) {
        tokens.push(Token {
            kind: Kind::Boundary,
            span: Span::new(index, index),
        });
    }
    Ok(tokens)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclarationKind {
    Struct,
    Enum,
    Function,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intrinsic {
    List,
    Map,
    Sender,
    Receiver,
    Channel,
}
#[derive(Debug, Clone)]
struct Template {
    intrinsic: Option<Intrinsic>,
    name: String,
    kind: DeclarationKind,
    params: Vec<String>,
    parameter_span: Span,
    name_span: Span,
    span: Span,
    tokens: std::ops::Range<usize>,
}
#[derive(Debug, Clone)]
struct TypeExpr {
    name: String,
    args: Vec<TypeExpr>,
    span: Span,
}
#[derive(Debug, Clone)]
struct Concrete {
    name: String,
    display: String,
}
#[derive(Debug, Clone)]
struct Instance {
    template: usize,
    args: Vec<Concrete>,
    name: String,
    display: String,
    ancestors: Vec<usize>,
    site: Span,
}
#[derive(Debug, Clone)]
struct Edit {
    span: Span,
    text: String,
    linear: bool,
}

struct Engine<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    contextual_markers: BTreeSet<usize>,
    templates: Vec<Template>,
    by_name: BTreeMap<String, usize>,
    types: BTreeSet<String>,
    instances: Vec<Instance>,
    instance_names: BTreeMap<String, usize>,
    channels: Option<crate::channel_registry::ModuleEvidence>,
    families: Vec<crate::channel_registry::FamilyNames>,
    family_keys: BTreeSet<String>,
}

impl Engine<'_> {
    fn text(&self, index: usize) -> &str {
        self.tokens
            .get(index)
            .map_or("", |token| token.text(self.source))
    }
    fn at(&self, index: usize, spelling: &str) -> bool {
        self.text(index) == spelling
    }
    fn span(&self, index: usize) -> Span {
        self.tokens
            .get(index)
            .map_or(Span::new(self.source.len(), self.source.len()), |token| {
                token.span
            })
    }
    fn newlines(&self, index: &mut usize) {
        while self
            .tokens
            .get(*index)
            .is_some_and(|token| token.kind == Kind::Newline)
        {
            *index += 1;
        }
    }

    fn type_expr(&self, index: &mut usize, depth: usize) -> Result<TypeExpr> {
        if depth >= MAX_TYPE_DEPTH {
            return Err(error(
                "G006",
                "generic type nesting exceeds 64 levels",
                self.span(*index),
            ));
        }
        let start = *index;
        if !self
            .tokens
            .get(*index)
            .is_some_and(|token| token.kind == Kind::Word)
        {
            return Err(error(
                "G001",
                "expected a concrete type argument",
                self.span(*index),
            ));
        }
        let name = self.text(*index).to_owned();
        *index += 1;
        let args = if self.at(*index, "[") {
            self.arguments(index, depth + 1)?
        } else {
            Vec::new()
        };
        let end = self.span(index.saturating_sub(1));
        Ok(TypeExpr {
            name,
            args,
            span: self.span(start).join(end),
        })
    }

    fn arguments(&self, index: &mut usize, depth: usize) -> Result<Vec<TypeExpr>> {
        let opening = self.span(*index);
        *index += 1;
        self.newlines(index);
        let mut args = Vec::new();
        if self.at(*index, "]") {
            return Err(error(
                "G002",
                "generic argument lists cannot be empty",
                opening.join(self.span(*index)),
            ));
        }
        loop {
            args.push(self.type_expr(index, depth)?);
            self.newlines(index);
            if self.at(*index, "]") {
                *index += 1;
                break;
            }
            if !self.at(*index, ",") {
                return Err(error(
                    "G001",
                    "expected ',' or ']' after a type argument",
                    self.span(*index),
                ));
            }
            *index += 1;
            self.newlines(index);
            if self.at(*index, "]") {
                *index += 1;
                break;
            }
        }
        Ok(args)
    }

    fn discover(&mut self) -> Result<()> {
        let mut declarations = BTreeMap::new();
        let mut depth: usize = 0;
        let mut index = 0;
        while index < self.tokens.len() {
            if self.tokens[index].kind == Kind::Boundary {
                depth = 0;
                index += 1;
                continue;
            }
            let kind = match self.text(index) {
                "struct" => Some(DeclarationKind::Struct),
                "enum" => Some(DeclarationKind::Enum),
                "def" => Some(DeclarationKind::Function),
                _ => None,
            };
            if depth == 0 {
                if let Some(kind) = kind {
                    if self
                        .tokens
                        .get(index + 1)
                        .is_some_and(|token| token.kind == Kind::Word)
                    {
                        let name = self.text(index + 1).to_owned();
                        if matches!(name.as_str(), "List" | "Map") {
                            return Err(error(
                                "G002",
                                format!("'{name}' is a reserved collection type"),
                                self.span(index + 1),
                            ));
                        }
                        let generic = self.at(index + 2, "[");
                        if let Some((_, previous_generic)) =
                            declarations.insert(name.clone(), (self.span(index + 1), generic))
                        {
                            if generic || previous_generic {
                                return Err(error("G002", format!("generic declaration '{name}' conflicts with another declaration"), self.span(index + 1)));
                            }
                        }
                        if !generic && kind != DeclarationKind::Function {
                            self.types.insert(name.clone());
                        }
                        if generic {
                            if matches!(name.as_str(), "main" | "range")
                                || primitive(&name)
                                || keyword(&name)
                                || crate::builtins::is_builtin(&name)
                                || name.starts_with("bhasa_rt_")
                            {
                                return Err(error(
                                    "G002",
                                    "generic declaration uses a reserved name",
                                    self.span(index + 1),
                                ));
                            }
                            if index > 0 && self.at(index - 1, "extern") {
                                return Err(error("G007", "generic extern declarations are unsupported; use a concrete C ABI adapter", self.span(index)));
                            }
                            let mut cursor = index + 2;
                            let arguments = self.arguments(&mut cursor, 0)?;
                            if arguments.len() > 32 {
                                return Err(error(
                                    "G006",
                                    "generic declarations support at most 32 type parameters",
                                    self.span(index + 2),
                                ));
                            }
                            let mut params = BTreeSet::new();
                            for argument in &arguments {
                                if !argument.args.is_empty()
                                    || keyword(&argument.name)
                                    || primitive(&argument.name)
                                    || matches!(argument.name.as_str(), "List" | "Map" | "range")
                                    || argument.name == name
                                    || !params.insert(argument.name.clone())
                                {
                                    return Err(error(
                                        "G002",
                                        "type parameters must be distinct, non-reserved names",
                                        argument.span,
                                    ));
                                }
                            }
                            let parameter_span = self.span(index + 2).join(self.span(cursor - 1));
                            let mut end = cursor;
                            while end < self.tokens.len()
                                && !self.at(end, "{")
                                && self.tokens[end].kind != Kind::Boundary
                            {
                                end += 1;
                            }
                            if end == self.tokens.len() || self.tokens[end].kind == Kind::Boundary {
                                return Err(error(
                                    "G001",
                                    "generic declaration requires a complete body in its module",
                                    self.span(index),
                                ));
                            }
                            let mut braces = 1;
                            end += 1;
                            while end < self.tokens.len() && braces != 0 {
                                if self.tokens[end].kind == Kind::Boundary {
                                    return Err(error(
                                        "G001",
                                        "generic declaration cannot cross a module boundary",
                                        self.span(end),
                                    ));
                                }
                                if self.at(end, "{") {
                                    braces += 1;
                                }
                                if self.at(end, "}") {
                                    braces -= 1;
                                }
                                if self.at(end, "def") && self.at(end + 2, "[") {
                                    return Err(error("G007", "method-specific type parameters are unsupported; use the enclosing struct's parameters", self.span(end)));
                                }
                                end += 1;
                            }
                            if braces != 0 {
                                return Err(error(
                                    "G001",
                                    "unterminated generic declaration",
                                    self.span(index),
                                ));
                            }
                            let template = Template {
                                intrinsic: None,
                                name: name.clone(),
                                kind,
                                params: arguments.into_iter().map(|arg| arg.name).collect(),
                                parameter_span,
                                name_span: self.span(index + 1),
                                span: self.span(index).join(self.span(end - 1)),
                                tokens: index..end,
                            };
                            self.by_name.insert(name, self.templates.len());
                            self.templates.push(template);
                            index = end;
                            continue;
                        }
                    }
                }
            } else if kind == Some(DeclarationKind::Function) && self.at(index + 2, "[") {
                return Err(error("G007", "method-specific type parameters are unsupported; use the enclosing struct's parameters", self.span(index)));
            }
            if self.at(index, "{") {
                depth += 1;
            }
            if self.at(index, "}") {
                depth = depth.saturating_sub(1);
            }
            index += 1;
        }
        Ok(())
    }

    fn add_intrinsics(&mut self) {
        for (name, intrinsic, params) in [
            ("List", Intrinsic::List, vec!["T".to_owned()]),
            ("Map", Intrinsic::Map, vec!["K".to_owned(), "V".to_owned()]),
        ] {
            self.by_name.insert(name.into(), self.templates.len());
            self.templates.push(Template {
                intrinsic: Some(intrinsic),
                name: name.into(),
                kind: DeclarationKind::Struct,
                params,
                parameter_span: Span::default(),
                name_span: Span::default(),
                span: Span::default(),
                tokens: 0..0,
            });
        }
    }

    fn register_channels(&mut self) -> Result<()> {
        let Some(evidence) = &self.channels else {
            return Ok(());
        };
        for (role, intrinsic) in [
            ("Sender", Intrinsic::Sender),
            ("Receiver", Intrinsic::Receiver),
            ("Channel", Intrinsic::Channel),
        ] {
            let name = &evidence.names()[role];
            let template = *self.by_name.get(name).ok_or_else(|| {
                error(
                    "G008",
                    "registered channel template missing",
                    Span::default(),
                )
            })?;
            if self.templates[template].kind != DeclarationKind::Struct
                || self.templates[template].params.len() != 1
            {
                return Err(error(
                    "G008",
                    "registered channel template identity changed",
                    self.templates[template].span,
                ));
            }
            self.templates[template].intrinsic = Some(intrinsic);
        }
        Ok(())
    }
    fn instantiate(
        &mut self,
        template: usize,
        args: Vec<Concrete>,
        site: Span,
        ancestors: &[usize],
    ) -> Result<Concrete> {
        let result = self.intern_instance(template, args.clone(), site, ancestors)?;
        let channel_use = matches!(
            self.templates[template].intrinsic,
            Some(Intrinsic::Sender | Intrinsic::Receiver | Intrinsic::Channel)
        ) || self
            .channels
            .as_ref()
            .is_some_and(|e| e.names().get("channel") == Some(&self.templates[template].name));
        if channel_use {
            let [payload] = args.as_slice() else {
                return Err(error("G008", "channel requires one concrete payload", site));
            };
            if self.family_keys.insert(payload.name.clone()) {
                let names = self.channels.as_ref().unwrap().names().clone();
                let mut family = BTreeMap::new();
                for role in [
                    "Sender",
                    "Receiver",
                    "Channel",
                    "SendFailure",
                    "SendResult",
                    "ReceiveResult",
                    "channel",
                ] {
                    let template = *self
                        .by_name
                        .get(&names[role])
                        .ok_or_else(|| error("G008", "channel family template missing", site))?;
                    family.insert(
                        role,
                        self.intern_instance(template, vec![payload.clone()], site, ancestors)?
                            .name,
                    );
                }
                self.families.push(crate::channel_registry::FamilyNames {
                    payload: payload.name.clone(),
                    sender: family["Sender"].clone(),
                    receiver: family["Receiver"].clone(),
                    pair: family["Channel"].clone(),
                    send_failure: family["SendFailure"].clone(),
                    send_result: family["SendResult"].clone(),
                    receive_result: family["ReceiveResult"].clone(),
                    factory: family["channel"].clone(),
                    receipt: names["ChannelSent"].clone(),
                    receive_error: names["ReceiveError"].clone(),
                });
            }
        }
        Ok(result)
    }

    fn intern_instance(
        &mut self,
        template: usize,
        args: Vec<Concrete>,
        site: Span,
        ancestors: &[usize],
    ) -> Result<Concrete> {
        let declaration = &self.templates[template];
        if args.len() != declaration.params.len() {
            return Err(error(
                "G002",
                format!(
                    "generic '{}' expects {} type argument(s), received {}",
                    declaration.name,
                    declaration.params.len(),
                    args.len()
                ),
                site,
            ));
        }
        if declaration.intrinsic == Some(Intrinsic::Map)
            && !matches!(args[0].name.as_str(), "int" | "str")
        {
            return Err(error("G003", "Map keys must be int or str", site));
        }
        if declaration.intrinsic.is_some() && args.iter().any(|arg| arg.name == "void") {
            return Err(error("G003", "collection elements cannot be void", site));
        }
        if args
            .iter()
            .try_fold(declaration.name.len() + 2, |sum, arg| {
                sum.checked_add(arg.display.len() + 2)
            })
            .is_none_or(|size| size > 65536)
        {
            return Err(error(
                "G006",
                "generic type spelling exceeds 64 KiB; expanding instantiations are unsupported",
                site,
            ));
        }
        let display = format!(
            "{}[{}]",
            declaration.name,
            args.iter()
                .map(|arg| arg.display.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some(index) = self.instance_names.get(&display) {
            return Ok(Concrete {
                name: self.instances[*index].name.clone(),
                display,
            });
        }
        if ancestors.len() >= MAX_TYPE_DEPTH || self.instances.len() >= MAX_INSTANTIATIONS {
            return Err(error("G006", "generic instantiation limit exceeded; recursive expanding instantiations are unsupported", site));
        }
        let name = match declaration.intrinsic {
            Some(Intrinsic::List) => format!("{PREFIX}list_{}", self.instances.len()),
            Some(Intrinsic::Map) => format!("{PREFIX}map_{}", self.instances.len()),
            Some(Intrinsic::Sender) => format!("{PREFIX}sender_{}", self.instances.len()),
            Some(Intrinsic::Receiver) => format!("{PREFIX}receiver_{}", self.instances.len()),
            Some(Intrinsic::Channel) => format!("{PREFIX}channel_{}", self.instances.len()),
            None => format!("{PREFIX}{}", self.instances.len()),
        };
        self.instance_names
            .insert(display.clone(), self.instances.len());
        self.instances.push(Instance {
            template,
            args,
            name: name.clone(),
            display: display.clone(),
            ancestors: ancestors.to_vec(),
            site,
        });
        Ok(Concrete { name, display })
    }

    fn concrete(
        &mut self,
        ty: &TypeExpr,
        params: &BTreeMap<String, Concrete>,
        ancestors: &[usize],
    ) -> Result<Concrete> {
        if let Some(value) = params.get(&ty.name) {
            if !ty.args.is_empty() {
                return Err(error(
                    "G003",
                    "type parameters cannot themselves take type arguments",
                    ty.span,
                ));
            }
            return Ok(value.clone());
        }
        if let Some(&template) = self.by_name.get(&ty.name) {
            if self.templates[template].kind == DeclarationKind::Function {
                return Err(error(
                    "G003",
                    format!("generic function '{}' is not a type", ty.name),
                    ty.span,
                ));
            }
            let args = ty
                .args
                .iter()
                .map(|arg| self.concrete(arg, params, ancestors))
                .collect::<Result<Vec<_>>>()?;
            return self.instantiate(template, args, ty.span, ancestors);
        }
        if !ty.args.is_empty() {
            return Err(error(
                "G002",
                format!("'{}' is not a generic type", ty.name),
                ty.span,
            ));
        }
        if primitive(&ty.name) || self.types.contains(&ty.name) {
            return Ok(Concrete {
                name: ty.name.clone(),
                display: ty.name.clone(),
            });
        }
        Err(error(
            "G003",
            format!("unknown or unsupported type argument '{}'", ty.name),
            ty.span,
        ))
    }

    fn rewrite(
        &mut self,
        range: std::ops::Range<usize>,
        params: &BTreeMap<String, Concrete>,
        ancestors: &[usize],
        template: Option<&Template>,
    ) -> Result<Vec<Edit>> {
        let mut edits = Vec::new();
        let mut index = range.start;
        while index < range.end {
            if let Some(template) = template {
                if self.span(index) == template.name_span {
                    index += 1;
                    continue;
                }
                if self.span(index).start >= template.parameter_span.start
                    && self.span(index).end <= template.parameter_span.end
                {
                    index += 1;
                    continue;
                }
            } else if let Some(declaration) = self
                .templates
                .iter()
                .find(|template| !template.tokens.is_empty() && template.tokens.start == index)
            {
                index = declaration.tokens.end;
                continue;
            }
            if self.tokens[index].kind != Kind::Word {
                index += 1;
                continue;
            }
            if self.contextual_markers.contains(&self.span(index).start) {
                index += 1;
                continue;
            }
            let name = self.text(index).to_owned();
            let mut previous = index;
            while previous > range.start && self.tokens[previous - 1].kind == Kind::Newline {
                previous -= 1;
            }
            let type_position =
                previous > range.start && matches!(self.text(previous - 1), ":" | "->");
            if self.at(index + 1, "[") {
                if index > range.start && self.at(index - 1, ".") {
                    return Err(error(
                        "G007",
                        "method-specific type arguments are unsupported",
                        self.span(index),
                    ));
                }
                let mut end = index + 1;
                let args = self.arguments(&mut end, 0)?;
                let span = self.span(index).join(self.span(end - 1));
                let template = self.by_name.get(&name).copied().ok_or_else(|| {
                    error(
                        "G002",
                        format!("'{name}' is not a generic declaration"),
                        span,
                    )
                })?;
                if type_position && self.templates[template].kind == DeclarationKind::Function {
                    return Err(error(
                        "G003",
                        "a generic function cannot be used as a type",
                        span,
                    ));
                }
                let args = args
                    .iter()
                    .map(|arg| self.concrete(arg, params, ancestors))
                    .collect::<Result<Vec<_>>>()?;
                let value = self.instantiate(template, args, span, ancestors)?;
                edits.push(Edit {
                    span,
                    text: value.name,
                    linear: false,
                });
                index = end;
                continue;
            }
            if type_position {
                if let Some(value) = params.get(&name) {
                    edits.push(Edit {
                        span: self.span(index),
                        text: value.name.clone(),
                        linear: false,
                    });
                } else if self.by_name.contains_key(&name) {
                    return Err(error(
                        "G002",
                        format!("generic type '{name}' requires explicit type arguments"),
                        self.span(index),
                    ));
                }
            }
            index += 1;
        }
        Ok(edits)
    }

    fn validate_symbolic_type(&self, ty: &TypeExpr, params: &[String]) -> Result<()> {
        if params.contains(&ty.name) {
            if ty.args.is_empty() {
                return Ok(());
            }
            return Err(error(
                "G003",
                "type parameters cannot themselves take type arguments",
                ty.span,
            ));
        }
        if let Some(&index) = self.by_name.get(&ty.name) {
            let template = &self.templates[index];
            if template.kind == DeclarationKind::Function {
                return Err(error(
                    "G003",
                    "a generic function cannot be used as a type",
                    ty.span,
                ));
            }
            self.validate_symbolic_arguments(template, &ty.args, params, ty.span)?;
        } else if !ty.args.is_empty() || (!primitive(&ty.name) && !self.types.contains(&ty.name)) {
            return Err(error(
                "G003",
                format!("unknown or unsupported type argument '{}'", ty.name),
                ty.span,
            ));
        }
        Ok(())
    }

    fn validate_symbolic_arguments(
        &self,
        declaration: &Template,
        args: &[TypeExpr],
        params: &[String],
        span: Span,
    ) -> Result<()> {
        if args.len() != declaration.params.len() {
            return Err(error(
                "G002",
                format!(
                    "generic '{}' expects {} type argument(s), received {}",
                    declaration.name,
                    declaration.params.len(),
                    args.len()
                ),
                span,
            ));
        }
        for argument in args {
            self.validate_symbolic_type(argument, params)?;
        }
        Ok(())
    }

    fn template_program(&self, template: &Template) -> Result<ast::Program> {
        let mut bytes = self.source.as_bytes()[template.span.start..template.span.end].to_vec();
        let mut ranges = vec![template.parameter_span];
        let mut index = template.tokens.start;
        while index < template.tokens.end {
            if self.span(index).start >= template.parameter_span.start
                && self.span(index).end <= template.parameter_span.end
            {
                index += 1;
                continue;
            }
            if self.tokens[index].kind == Kind::Word && self.at(index + 1, "[") {
                let mut end = index + 1;
                let args = self.arguments(&mut end, 0)?;
                let name = self.text(index);
                let &declaration = self.by_name.get(name).ok_or_else(|| {
                    error(
                        "G002",
                        format!("'{name}' is not a generic declaration"),
                        self.span(index),
                    )
                })?;
                self.validate_symbolic_arguments(
                    &self.templates[declaration],
                    &args,
                    &template.params,
                    self.span(index).join(self.span(end - 1)),
                )?;
                ranges.push(self.span(index + 1).join(self.span(end - 1)));
                index = end;
            } else {
                index += 1;
            }
        }
        for span in ranges {
            for byte in &mut bytes[span.start - template.span.start..span.end - template.span.start]
            {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }
        let source = String::from_utf8(bytes).expect("blanking token spans preserves UTF-8");
        let mut tokens = crate::lexer::lex(&source).map_err(|errors| {
            let mut error = errors.into_iter().next().unwrap();
            error.span.start += template.span.start;
            error.span.end += template.span.start;
            error
        })?;
        for token in &mut tokens {
            token.span.start += template.span.start;
            token.span.end += template.span.start;
        }
        let program = crate::parser::parse_tokens(tokens)
            .map_err(|errors| errors.into_iter().next().unwrap())?;
        for function in program
            .functions
            .iter()
            .chain(program.records.iter().flat_map(|r| r.methods.iter()))
        {
            for param in &function.params {
                if param.mode == ast::ParamMode::Owned {
                    return Err(error(
                        "S038",
                        "generic owned parameter modes are unsupported",
                        param.span,
                    ));
                }
            }
        }
        Ok(program)
    }
}

struct Builder<'a> {
    original: &'a str,
    output: String,
    mappings: Vec<Mapping>,
    boundaries: Vec<usize>,
}
impl Builder<'_> {
    fn append(&mut self, text: &str, original: Span, linear: bool) {
        if text.is_empty() {
            return;
        }
        let start = self.output.len();
        self.output.push_str(text);
        self.mappings.push(Mapping {
            generated: Span::new(start, self.output.len()),
            original,
            linear,
        });
    }
    fn render(&mut self, span: Span, mut edits: Vec<Edit>, boundaries: &[usize]) {
        edits.sort_by_key(|edit| edit.span.start);
        let mut cursor = span.start;
        for edit in edits {
            self.copy(Span::new(cursor, edit.span.start), boundaries);
            self.append(&edit.text, edit.span, edit.linear);
            cursor = edit.span.end;
        }
        self.copy(Span::new(cursor, span.end), boundaries);
    }
    fn copy(&mut self, span: Span, boundaries: &[usize]) {
        for &boundary in boundaries {
            if boundary >= span.start && boundary < span.end {
                self.boundaries
                    .push(self.output.len() + boundary - span.start);
            }
        }
        self.append(&self.original[span.start..span.end], span, true);
    }
}

/// Instantiate explicit generic applications in a complete, import-resolved
/// source graph. Generic bodies are syntax-checked even when never instantiated.
pub fn expand(
    source: &str,
    boundaries: &[usize],
) -> std::result::Result<Expansion, Vec<Diagnostic>> {
    expand_inner(source, boundaries, &[], None).map_err(|error| vec![error])
}

/// A canonical type requested by source metadata, without a runtime wrapper.
/// The site is in the pre-specialization source and participates in source maps.
#[derive(Debug, Clone)]
pub struct TypeRoot {
    pub spelling: String,
    pub span: Span,
}

/// Specialize source uses and requested alias targets in one identity table.
/// Unused target declarations therefore pass the ordinary semantic checker.
pub fn expand_with_type_roots(
    source: &str,
    boundaries: &[usize],
    roots: &[TypeRoot],
) -> std::result::Result<Expansion, Vec<Diagnostic>> {
    expand_inner(source, boundaries, roots, None).map_err(|error| vec![error])
}

fn parse_type_root(root: &TypeRoot) -> Result<TypeExpr> {
    let parse = || {
        let engine = Engine {
            source: &root.spelling,
            tokens: scan(&root.spelling, &[])?,
            contextual_markers: BTreeSet::new(),
            templates: Vec::new(),
            by_name: BTreeMap::new(),
            types: BTreeSet::new(),
            instances: Vec::new(),
            instance_names: BTreeMap::new(),
            channels: None,
            families: Vec::new(),
            family_keys: BTreeSet::new(),
        };
        let mut index = 0;
        let expression = engine.type_expr(&mut index, 0)?;
        if index != engine.tokens.len() {
            return Err(error(
                "G001",
                "unexpected token after requested alias target",
                engine.span(index),
            ));
        }
        Ok(expression)
    };
    let mut expression = parse().map_err(|mut error: Diagnostic| {
        error.span = root.span;
        error
    })?;
    fn locate(expression: &mut TypeExpr, span: Span) {
        expression.span = span;
        for argument in &mut expression.args {
            locate(argument, span);
        }
    }
    locate(&mut expression, root.span);
    Ok(expression)
}

/// Uninstantiated template declaration shapes for editor symbols. Type parameter
/// lists are omitted from this syntax-only AST; all source spans are original.
/// This is not a checked program and must never be passed directly to codegen.
pub fn template_program(
    source: &str,
    boundaries: &[usize],
) -> std::result::Result<ast::Program, Vec<Diagnostic>> {
    let result = || -> Result<ast::Program> {
        let mut engine = Engine {
            source,
            tokens: scan(source, boundaries)?,
            contextual_markers: contextual_marker_spans(source),
            templates: Vec::new(),
            by_name: BTreeMap::new(),
            types: BTreeSet::new(),
            instances: Vec::new(),
            instance_names: BTreeMap::new(),
            channels: None,
            families: Vec::new(),
            family_keys: BTreeSet::new(),
        };
        engine.discover()?;
        engine.add_intrinsics();
        let mut program = ast::Program::default();
        for template in engine
            .templates
            .iter()
            .filter(|template| !template.tokens.is_empty())
        {
            program.merge(engine.template_program(template)?);
        }
        Ok(program)
    };
    result().map_err(|error| vec![error])
}

pub(crate) fn expand_with_channel_roots(
    source: &str,
    boundaries: &[usize],
    roots: &[TypeRoot],
    evidence: Option<&crate::channel_registry::ModuleEvidence>,
) -> std::result::Result<Expansion, Vec<Diagnostic>> {
    expand_inner(source, boundaries, roots, evidence).map_err(|error| vec![error])
}
fn expand_inner(
    source: &str,
    boundaries: &[usize],
    roots: &[TypeRoot],
    evidence: Option<&crate::channel_registry::ModuleEvidence>,
) -> Result<Expansion> {
    if let Some(evidence) = evidence {
        evidence.validate(source, boundaries)?;
    }
    let tokens = scan(source, boundaries)?;
    let mut engine = Engine {
        source,
        contextual_markers: contextual_marker_spans(source),
        tokens,
        templates: Vec::new(),
        by_name: BTreeMap::new(),
        types: BTreeSet::new(),
        instances: Vec::new(),
        instance_names: BTreeMap::new(),
        channels: None,
        families: Vec::new(),
        family_keys: BTreeSet::new(),
    };
    engine.channels = evidence.cloned();
    engine.discover()?;
    engine.register_channels()?;
    let has_arguments = engine.tokens.iter().any(|token| token.text(source) == "[");
    if engine.templates.is_empty() && !has_arguments && roots.is_empty() {
        return Ok(Expansion {
            channels: None,
            source: source.to_owned(),
            boundaries: boundaries.to_vec(),
            mappings: vec![Mapping {
                generated: Span::new(0, source.len()),
                original: Span::new(0, source.len()),
                linear: true,
            }],
            names: BTreeMap::new(),
            original_len: source.len(),
            identity: true,
        });
    }
    engine.add_intrinsics();
    for template in engine
        .templates
        .iter()
        .filter(|template| !template.tokens.is_empty())
    {
        engine.template_program(template)?;
    }
    for root in roots {
        if root.spelling.len() > 65536 {
            return Err(error(
                "G006",
                "requested alias target exceeds 64 KiB",
                root.span,
            ));
        }
        if root.span.start > root.span.end
            || root.span.end > source.len()
            || !source.is_char_boundary(root.span.start)
            || !source.is_char_boundary(root.span.end)
        {
            return Err(error(
                "G001",
                "invalid requested alias target source span",
                Span::new(0, 0),
            ));
        }
        let target = parse_type_root(root)?;
        // These are legal whole alias targets, but existing generic arguments
        // deliberately exclude void and task. Do not relax that argument rule.
        if !target.args.is_empty() || !matches!(target.name.as_str(), "void" | "task") {
            engine.concrete(&target, &BTreeMap::new(), &[])?;
        }
    }
    let mut edits = engine.rewrite(0..engine.tokens.len(), &BTreeMap::new(), &[], None)?;
    for template in engine
        .templates
        .iter()
        .filter(|template| !template.tokens.is_empty())
    {
        let blank = source[template.span.start..template.span.end]
            .bytes()
            .map(|byte| {
                if matches!(byte, b'\r' | b'\n') {
                    byte
                } else {
                    b' '
                }
            })
            .collect::<Vec<_>>();
        edits.push(Edit {
            span: template.span,
            text: String::from_utf8(blank).unwrap(),
            linear: true,
        });
    }
    let mut builder = Builder {
        original: source,
        output: String::new(),
        mappings: Vec::new(),
        boundaries: Vec::new(),
    };
    builder.render(Span::new(0, source.len()), edits, boundaries);
    if boundaries.last().copied() == Some(source.len()) {
        builder.boundaries.push(builder.output.len());
    }
    let initial_len = builder.output.len();
    let mut index = 0;
    while index < engine.instances.len() {
        let instance = engine.instances[index].clone();
        let template = engine.templates[instance.template].clone();
        if let Some(intrinsic) = template.intrinsic {
            if builder.boundaries.last().copied() != Some(builder.output.len()) {
                builder.boundaries.push(builder.output.len());
            }
            let schema = match intrinsic {
                Intrinsic::List => format!(
                    "\nstruct {} {{ __element: {} }}\n",
                    instance.name, instance.args[0].name
                ),
                Intrinsic::Map => format!(
                    "\nstruct {} {{ __key: {}; __value: {} }}\n",
                    instance.name, instance.args[0].name, instance.args[1].name
                ),
                Intrinsic::Sender | Intrinsic::Receiver => format!(
                    "\nstruct {} {{ __payload: {} }}\n",
                    instance.name, instance.args[0].name
                ),
                Intrinsic::Channel => {
                    let family = engine
                        .families
                        .iter()
                        .find(|f| f.pair == instance.name)
                        .ok_or_else(|| {
                            error(
                                "G008",
                                "channel specialization lost its receipt",
                                instance.site,
                            )
                        })?;
                    format!(
                        "\nstruct {} {{ __sender: {}; __receiver: {} }}\n",
                        instance.name, family.sender, family.receiver
                    )
                }
            };
            builder.append(&schema, instance.site, false);
            if builder.output.len() - initial_len > MAX_GENERATED_BYTES {
                return Err(error(
                    "G006",
                    "generic expansion exceeds the 16 MiB generated-source limit",
                    instance.site,
                ));
            }
            index += 1;
            continue;
        }
        let params = template
            .params
            .iter()
            .cloned()
            .zip(instance.args.iter().cloned())
            .collect();
        let mut ancestors = instance.ancestors.clone();
        ancestors.push(index);
        let mut edits = engine.rewrite(
            template.tokens.clone(),
            &params,
            &ancestors,
            Some(&template),
        )?;
        edits.push(Edit {
            span: template.name_span,
            text: instance.name.clone(),
            linear: false,
        });
        edits.push(Edit {
            span: template.parameter_span,
            text: String::new(),
            linear: false,
        });
        if builder.boundaries.last().copied() != Some(builder.output.len()) {
            builder.boundaries.push(builder.output.len());
        }
        builder.append(
            "\n",
            Span::new(template.span.start, template.span.start),
            false,
        );
        builder.render(template.span, edits, &[]);
        if builder.output.len() - initial_len > MAX_GENERATED_BYTES {
            return Err(error(
                "G006",
                "generic expansion exceeds the 16 MiB generated-source limit",
                template.span,
            ));
        }
        index += 1;
    }
    if !engine.instances.is_empty() {
        builder.boundaries.push(builder.output.len());
    }
    let channel_receipts = engine.channels.clone().map(|module| {
        crate::channel_registry::expanded_receipts(
            module,
            engine.families.clone(),
            &builder.output,
            &builder.boundaries,
        )
    });
    let names = engine
        .instances
        .iter()
        .map(|instance| (instance.name.clone(), instance.display.clone()))
        .collect();
    Ok(Expansion {
        channels: channel_receipts,
        source: builder.output,
        boundaries: builder.boundaries,
        mappings: builder.mappings,
        names,
        original_len: source.len(),
        identity: false,
    })
}

fn contextual_marker_spans(source: &str) -> BTreeSet<usize> {
    let Ok(tokens) = crate::lexer::lex_rust(source) else {
        return BTreeSet::new();
    };
    crate::modules::contextual_binding_markers(&tokens)
        .into_iter()
        .map(|index| tokens[index].span.start)
        .collect()
}
