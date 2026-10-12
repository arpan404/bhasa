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
                                (
                                    "mode",
                                    match param.mode {
                                        crate::ast::ParamMode::Borrowed => "borrowed",
                                        crate::ast::ParamMode::Owned => "owned",
                                    }
                                    .into(),
                                ),
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
    fn channel_methods(&self, record: &crate::ast::RecordDecl) -> Vec<Value> {
        let operations = self
            .program
            .channels
            .as_ref()
            .and_then(|authority| authority.operations(self.program).ok())
            .unwrap_or_default();
        operations
            .into_iter()
            .filter(|operation| {
                operation.owner
                    == self
                        .program
                        .type_names
                        .iter()
                        .position(|name| name == &record.name)
                        .unwrap_or(usize::MAX)
            })
            .map(|operation| {
                Value::object([
                    ("name", operation.operation.name().into()),
                    ("kind", "intrinsic_method".into()),
                    ("owner", self.ty(Type::Record(operation.owner))),
                    ("private", false.into()),
                    ("location", self.location(record.span)),
                    (
                        "parameters",
                        Value::Array(
                            operation
                                .params
                                .iter()
                                .enumerate()
                                .map(|(index, ty)| {
                                    Value::object([
                                        (
                                            "name",
                                            if index == 0 {
                                                "self".to_owned()
                                            } else {
                                                format!("arg{index}")
                                            }
                                            .into(),
                                        ),
                                        ("type", self.ty(*ty)),
                                        ("mode", "borrowed".into()),
                                        ("mode_id", 0_i64.into()),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    ("returns", self.ty(operation.result)),
                ])
            })
            .collect()
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
            // These rows exist only after authority validation and an exact owner ID match.
            let channel_methods = self.channel_methods(record);
            let fields = if channel_methods.is_empty() {
                self.fields(&record.fields)
            } else {
                Value::Array(Vec::new())
            };
            declarations.push(Value::object([
                ("name", self.display(&record.name).into()),
                ("kind", "struct".into()),
                ("private", self.private(record.span).into()),
                ("location", self.location(record.span)),
                ("fields", fields),
                (
                    "methods",
                    Value::Array(
                        record
                            .methods
                            .iter()
                            .map(|method| self.function(method))
                            .chain(channel_methods)
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
        for alias in &self.expansion.modules.type_aliases {
            let file = &self.files[alias.owner];
            let declared = file
                .source
                .get(alias.target_span.start - file.base..alias.target_span.end - file.base)
                .unwrap_or("");
            let nominal_location = self
                .program
                .records
                .iter()
                .filter(|record| !crate::collections::is_collection(&record.name))
                .map(|record| (&record.name, record.span))
                .chain(
                    self.program
                        .enums
                        .iter()
                        .map(|item| (&item.name, item.span)),
                )
                .find(|(name, _)| {
                    self.expansion.generics.display_names(name) == alias.canonical_target
                })
                .map_or(Value::Null, |(_, span)| self.location(span));
            declarations.push(Value::object([
                ("name", alias.name.clone().into()),
                ("kind", "type_alias".into()),
                ("private", alias.private.into()),
                ("location", self.location(alias.span)),
                ("name_location", self.location(alias.name_span)),
                ("target_location", self.location(alias.target_span)),
                ("declared_target", declared.into()),
                (
                    "canonical_target",
                    self.display(&alias.canonical_target).into(),
                ),
                ("canonical_target_location", nominal_location),
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

fn package_metadata(
    loaded: &crate::project::LoadedProgram,
) -> Result<(Value, Value), LocatedDiagnostic> {
    let make_error = |path: &Path, message: String| LocatedDiagnostic {
        path: path.to_owned(),
        source: String::new(),
        diagnostic: crate::Diagnostic::new("Q002", message, Span::default()),
    };
    let lock = if let Some(root) = crate::package_lock::root_manifest(loaded) {
        let path = root.root.join("bhasa.lock");
        let actual = crate::package_lock::Snapshot::capture(loaded, root)
            .map_err(|message| make_error(&path, message))?;
        let existing = crate::package_lock::read_if_present(&root.root)
            .map_err(|message| make_error(&path, message))?;
        if let Some(expected) = &existing {
            expected
                .verify(&actual)
                .map_err(|message| make_error(&path, message))?;
        }
        Value::object([
            ("path", path.display().to_string().into()),
            ("present", existing.is_some().into()),
            ("verified", existing.is_some().into()),
            (
                "snapshot",
                crate::json::parse(&actual.encode()).expect("snapshot is JSON"),
            ),
        ])
    } else {
        Value::Null
    };
    let mut adapters = Vec::new();
    for manifest in &loaded.manifests {
        for (name, dependency) in &manifest.rust_dependencies {
            let cargo_manifest = crate::rust_deps::resolve_manifest(&manifest.root, dependency)
                .map_err(|message| {
                    make_error(
                        &manifest.path,
                        format!("Rust dependency {name:?}: {message}"),
                    )
                })?;
            let root = cargo_manifest
                .parent()
                .expect("canonical Cargo manifest has parent");
            let path = root.join("bhasa-adapter.json");
            let contract = crate::ffi_contract::read_if_present(root)
                .map_err(|message| make_error(&path, message))?;
            adapters.push(Value::object([
                (
                    "package_manifest",
                    manifest.path.display().to_string().into(),
                ),
                ("name", name.clone().into()),
                (
                    "cargo_manifest",
                    cargo_manifest.display().to_string().into(),
                ),
                ("metadata_path", path.display().to_string().into()),
                ("source_review_verified", contract.is_some().into()),
                ("worker_extern_allowed", false.into()),
                ("no_gc_proven", false.into()),
                (
                    "contract",
                    contract.map_or(Value::Null, |adapter| adapter.to_value()),
                ),
            ]));
        }
    }
    Ok((lock, Value::Array(adapters)))
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
    let (lock, adapters) = match package_metadata(&loaded) {
        Ok(values) => values,
        Err(error) => return write(failure(&[error]), 1),
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
                ("checksum", manifest.checksum.clone().into()),
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
            ("lock", lock),
            ("adapters", adapters),
            ("declarations", query.declarations()),
            (
                "declaration_scope",
                "checked concrete declarations and validated type aliases; unused generic templates are omitted".into(),
            ),
            ("diagnostics", Value::Array(Vec::new())),
        ]),
        0,
    )
}

#[cfg(test)]
mod alias_inspect_tests {
    use super::*;
    #[test]
    fn checked_alias_inspection_includes_unused_private_and_declared_targets() {
        let source="# 🌍\ntype Count=int\ntype Again=Count\nprivate type Bytes=buffer\ntype Unused=List[int]\nstruct Point{value:int}\ntype Item=Point\ndef type(value:int)->int{return value}\ndef main(){}\n";
        let files = vec![SourceFile {
            path: "aliases.bh".into(),
            source: source.into(),
            base: 0,
            imports: Vec::new(),
            import_spans: Vec::new(),
        }];
        let expansion = crate::expand_files(&files).unwrap();
        let checked = expansion.check().unwrap();
        let query = Query {
            files: &files,
            tokens: vec![crate::lexer::lex_rust(source).unwrap()],
            program: &checked.program,
            expansion: &expansion,
        };
        let declarations = query.declarations();
        let aliases: Vec<_> = declarations
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item.get("kind").and_then(Value::as_str) == Some("type_alias"))
            .collect();
        assert_eq!(aliases.len(), 5);
        let again = aliases
            .iter()
            .find(|item| item.get("name").and_then(Value::as_str) == Some("Again"))
            .unwrap();
        assert_eq!(
            again.get("declared_target").and_then(Value::as_str),
            Some("Count")
        );
        assert_eq!(
            again.get("canonical_target").and_then(Value::as_str),
            Some("int")
        );
        assert!(!declarations.to_string().contains("__bhm_"));
        assert!(!declarations.to_string().contains("__bhg_"));
        assert!(aliases
            .iter()
            .any(
                |item| item.get("name").and_then(Value::as_str) == Some("Bytes")
                    && item.get("private") == Some(&Value::Bool(true))
            ));
        let item = aliases
            .iter()
            .find(|item| item.get("name").and_then(Value::as_str) == Some("Item"))
            .unwrap();
        assert_eq!(
            item.get("canonical_target_location")
                .unwrap()
                .get("span")
                .unwrap()
                .get("start")
                .and_then(Value::as_i64),
            Some(source.find("struct Point").unwrap() as i64)
        );
        assert_eq!(again.get("canonical_target_location"), Some(&Value::Null));
        assert_eq!(declarations.to_string(), query.declarations().to_string());
    }
}
