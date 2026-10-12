//! Read-only project and checked API queries for agents, without native tools.
use crate::ast::{Function, Program, Span, Type};
use crate::json::Value;
use crate::lexer::{Token, TokenKind};
use crate::project::{LocatedDiagnostic, SourceFile};
use std::io::{self, Write};
use std::path::Path;

const MAX_OUTPUT: usize = 16 * 1024 * 1024;

fn diagnostics(errors: &[LocatedDiagnostic]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                crate::json::parse(
                    &error
                        .diagnostic
                        .to_json(&error.source, &error.path.display().to_string()),
                )
                .expect("compiler diagnostics produce valid JSON")
            })
            .collect(),
    )
}

fn failure(errors: &[LocatedDiagnostic]) -> Value {
    Value::object([
        ("schema_version", 1_i64.into()),
        ("ok", false.into()),
        ("diagnostics", diagnostics(errors)),
    ])
}

fn write(value: Value, status: i32) -> i32 {
    let mut text = value.to_string();
    if text.len() > MAX_OUTPUT {
        return usage_error("inspection output exceeds the 16 MiB limit");
    }
    text.push('\n');
    if let Err(error) = io::stdout().lock().write_all(text.as_bytes()) {
        eprintln!("error: cannot write inspection output: {error}");
        return 1;
    }
    status
}

pub(crate) fn usage_error(message: &str) -> i32 {
    let error = LocatedDiagnostic {
        path: "<command line>".into(),
        source: String::new(),
        diagnostic: crate::Diagnostic::new("Q001", message, Span::default()),
    };
    write(failure(&[error]), 2)
}

struct Query<'a> {
    files: &'a [SourceFile],
    tokens: Vec<Vec<Token>>,
    program: &'a Program,
    expansion: &'a crate::ExpandedFiles,
}

impl Query<'_> {
    fn display(&self, name: &str) -> String {
        self.expansion
            .modules
            .display_names(&self.expansion.generics.display_names(name))
    }
    fn ty(&self, ty: Type) -> Value {
        self.display(&self.program.type_name(ty)).into()
    }
    fn location(&self, span: Span) -> Value {
        let diagnostic = crate::Diagnostic::new("", "", span);
        let located = crate::project::locate(self.files, &diagnostic);
        let position =
            crate::json::parse(&located.diagnostic.to_json(&located.source, "")).unwrap();
        Value::object([
            ("file", located.path.display().to_string().into()),
            (
                "span",
                Value::object([
                    ("start", located.diagnostic.span.start.into()),
                    ("end", located.diagnostic.span.end.into()),
                ]),
            ),
            ("line", position.get("line").unwrap().clone()),
            ("column", position.get("column").unwrap().clone()),
        ])
    }
    fn private(&self, span: Span) -> bool {
        let index = self
            .files
            .partition_point(|file| file.base <= span.start)
            .saturating_sub(1);
        let local = span.start.saturating_sub(self.files[index].base);
        let tokens = &self.tokens[index];
        let end = tokens.partition_point(|token| token.span.end <= local);
        for token in tokens[..end]
            .iter()
            .rev()
            .filter(|token| token.kind != TokenKind::Newline)
        {
            match &token.kind {
                TokenKind::Ident(name) => return name == "private",
                TokenKind::Extern => continue,
                _ => return false,
            }
        }
        false
    }
    fn function(&self, function: &Function) -> Value {
        Value::object([
            ("name", self.display(&function.name).into()),
            (
                "kind",
                if function.body.is_none() {
                    "extern"
                } else {
                    "function"
                }
                .into(),
            ),
            ("private", self.private(function.span).into()),
            ("location", self.location(function.span)),
            (
                "parameters",
                Value::Array(
                    function
                        .params
                        .iter()
                        .map(|param| {
                            Value::object([
                                ("name", param.name.clone().into()),
                                ("type", self.ty(param.ty)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("returns", self.ty(function.return_type)),
        ])
    }
    fn fields(&self, fields: &[crate::ast::RecordField]) -> Value {
        Value::Array(
            fields
                .iter()
                .map(|field| {
                    Value::object([
                        ("name", field.name.clone().into()),
                        ("type", self.ty(field.ty)),
                        ("private", self.private(field.span).into()),
                        ("location", self.location(field.span)),
                    ])
                })
                .collect(),
        )
    }
    fn declarations(&self) -> Value {
        let mut declarations = Vec::new();
        for function in &self.program.functions {
            declarations.push(self.function(function));
        }
        for record in &self.program.records {
            if crate::collections::is_collection(&record.name) {
                continue;
            }
            declarations.push(Value::object([
                ("name", self.display(&record.name).into()),
                ("kind", "struct".into()),
                ("private", self.private(record.span).into()),
                ("location", self.location(record.span)),
                ("fields", self.fields(&record.fields)),
                (
                    "methods",
                    Value::Array(
                        record
                            .methods
                            .iter()
                            .map(|method| self.function(method))
                            .collect(),
                    ),
                ),
            ]));
        }
        for declaration in &self.program.enums {
            declarations.push(Value::object([
                ("name", self.display(&declaration.name).into()),
                ("kind", "enum".into()),
                ("private", self.private(declaration.span).into()),
                ("location", self.location(declaration.span)),
                (
                    "variants",
                    Value::Array(
                        declaration
                            .variants
                            .iter()
                            .map(|variant| {
                                Value::object([
                                    ("name", variant.name.clone().into()),
                                    ("fields", self.fields(&variant.fields)),
                                    ("location", self.location(variant.span)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]));
        }
        declarations.sort_by_key(|value| {
            (
                value
                    .get("location")
                    .and_then(|value| value.get("file"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                value.to_string(),
            )
        });
        Value::Array(declarations)
    }
}

pub(crate) fn run(source: Option<&Path>) -> i32 {
    let loaded = match crate::project::load_unparsed(source) {
        Ok(loaded) => loaded,
        Err(errors) => return write(failure(&errors), 1),
    };
    let checked = crate::expand_files(&loaded.files).and_then(|expansion| {
        let checked = expansion.check()?;
        Ok((expansion, checked))
    });
    let (expansion, checked) = match checked {
        Ok(checked) => checked,
        Err(errors) => {
            let located = errors
                .iter()
                .map(|error| crate::project::locate(&loaded.files, error))
                .collect::<Vec<_>>();
            return write(failure(&located), 1);
        }
    };
    let query = Query {
        tokens: loaded
            .files
            .iter()
            .map(|file| {
                crate::lexer::lex_rust(&file.source).expect("checked source is lexically valid")
            })
            .collect(),
        files: &loaded.files,
        program: &checked.program,
        expansion: &expansion,
    };
    let modules = loaded
        .files
        .iter()
        .map(|file| {
            Value::object([
                ("file", file.path.display().to_string().into()),
                (
                    "imports",
                    Value::Array(
                        file.imports
                            .iter()
                            .map(|import| {
                                Value::object([
                                    ("path", import.path.clone().into()),
                                    ("file", import.target.display().to_string().into()),
                                    (
                                        "alias",
                                        import.alias.clone().map_or(Value::Null, Value::from),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    let packages = loaded
        .manifests
        .iter()
        .map(|manifest| {
            Value::object([
                ("name", manifest.name.clone().into()),
                ("version", manifest.version.clone().into()),
                ("manifest", manifest.path.display().to_string().into()),
                ("entry", manifest.entry.display().to_string().into()),
                (
                    "dependencies",
                    Value::Object(
                        manifest
                            .dependencies
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone().into()))
                            .collect(),
                    ),
                ),
                (
                    "rust_dependencies",
                    Value::Object(
                        manifest
                            .rust_dependencies
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone().into()))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    write(
        Value::object([
            ("schema_version", 1_i64.into()),
            ("ok", true.into()),
            ("entry", loaded.entry.display().to_string().into()),
            ("modules", Value::Array(modules)),
            ("packages", Value::Array(packages)),
            ("declarations", query.declarations()),
            (
                "declaration_scope",
                "checked concrete declarations; unused generic templates are omitted".into(),
            ),
            ("diagnostics", Value::Array(Vec::new())),
        ]),
        0,
    )
}
