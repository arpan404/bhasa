use bhasa::ast::{Span, Type};
use bhasa::project::{ModuleImport, SourceFile};

fn accepts(source: &str) {
    bhasa::check(source).unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
    bhasa::compile_to_llvm(source, "aarch64-apple-darwin").unwrap();
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_with_bhasa(source, "aarch64-apple-darwin").unwrap();
}
fn rejects(source: &str, message: &str) {
    let errors = bhasa::check(source).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
    #[cfg(feature = "bhasa-frontend")]
    assert!(bhasa::compile_with_bhasa(source, "aarch64-apple-darwin").is_err());
}

#[test]
fn aliases_preserve_concrete_types_and_owned_copy_semantics() {
    let source = "type Count = int\ntype Bytes = buffer\ntype Again = Count\nstruct Cell{value:Again}\ntype Item = Cell\ndef read(value:Item)->Count{return value.value}\ndef main(){var a:Item=Item(42);var b:Cell=a;b.value=7;print(read(a));var bytes:Bytes=buffer();push(bytes,42)}";
    accepts(source);
    let checked = bhasa::check(source).unwrap();
    assert_eq!(checked.program.records[0].fields[0].ty, Type::Int);
    assert_eq!(checked.program.functions[0].return_type, Type::Int);
    assert_eq!(checked.program.type_names.len(), 1);
}

#[test]
fn aliases_share_generic_identity_and_preserve_parameter_shadowing() {
    accepts("struct Box[T]{value:T}\ntype Number = int\ntype IntBox = Box[Number]\ntype Nested = Box[IntBox]\ndef identity[T](x:T)->T{return x}\ndef echo[Number](x:Number)->Number{return x}\ndef main(){var a:IntBox=Box[int](3);var b:Box[int]=IntBox(4);var c:Nested=Nested(a);print(identity[IntBox](b).value);print(echo[bool](true))}");
}

#[test]
fn contextual_type_names_and_value_shadows_remain_valid() {
    accepts("def type(value:int)->int{return value}\ndef main(){var type=42;print(type);print(1)}");
    accepts("type Count=int\ndef main(){var Count=42;var value:Count=Count;print(value)}");
    rejects(
        "type Count=int\ndef main(){var Count=42;print(Count())}",
        "not callable",
    );
}

#[test]
fn malformed_unused_aliases_fail_at_the_declaration() {
    for (source, message) in [
        ("type A=B\ntype B=A\ndef main(){}", "cyclic"),
        ("type A=Missing\ndef main(){}", "unknown alias target"),
        ("type A=int + int\ndef main(){}", "unexpected token"),
        ("type A[T]=T\ndef main(){}", "generic type aliases"),
        ("type A=List[]\ndef main(){}", "expected alias target"),
        ("type A=List\ndef main(){}", "requires its concrete"),
        ("type A=Map[int]\ndef main(){}", "requires its concrete"),
        ("type A=int[int]\ndef main(){}", "does not accept"),
        ("type A=int\ntype A=bool\ndef main(){}", "duplicate"),
        ("struct A{}\ntype A=int\ndef main(){}", "duplicate"),
        ("type A=int\nstruct A{}\ndef main(){}", "duplicate"),
        (
            "def value()->int{return 1}\ntype A=value\ndef main(){}",
            "must be a type",
        ),
        (
            "private struct Hidden{value:int}\ntype Visible=Hidden\ndef main(){}",
            "cannot expose a private",
        ),
    ] {
        rejects(source, message);
    }
    let source = "type A=Missing\ndef main(){}";
    let error = bhasa::check(source).unwrap_err().remove(0);
    assert_eq!(&source[error.span.start..error.span.end], "Missing");
}

fn graph(main: &str, library: &str) -> Vec<SourceFile> {
    vec![
        SourceFile {
            path: "main.bh".into(),
            source: main.into(),
            base: 0,
            import_spans: vec![],
            imports: vec![ModuleImport {
                path: "library.bh".into(),
                target: "library.bh".into(),
                alias: Some("lib".into()),
                span: Span::new(0, 0),
            }],
        },
        SourceFile {
            path: "library.bh".into(),
            source: library.into(),
            base: main.len() + 1,
            import_spans: vec![],
            imports: vec![],
        },
    ]
}
fn graph_check(files: &[SourceFile]) -> Result<(), Vec<bhasa::Diagnostic>> {
    let modules = bhasa::modules::expand_files(files)?;
    let generics = bhasa::generics::expand_with_type_roots(
        &modules.source,
        &modules.boundaries,
        &modules.type_roots()?,
    )
    .map_err(|e| modules.map_diagnostics(e))?;
    let mut program =
        bhasa::generics::parse_expansion(&generics).map_err(|e| modules.map_diagnostics(e))?;
    modules.remap_program(&mut program);
    let checked = bhasa::sema::check(program)
        .map_err(|errors| modules.display_diagnostics(generics.display_diagnostics(errors)))?;
    modules.check_privacy(&checked, |name| generics.original_name(name))
}

#[test]
fn imported_aliases_cannot_bypass_nominal_privacy() {
    let library = "struct Secret{private value:int;def read(self)->int{return self.value}}\ntype Public=Secret\nprivate type Hidden=Secret\ndef create()->Public{return Secret(42)}";
    let files = graph(
        "def main(){var x:lib.Public=lib.create();print(x.read())}",
        library,
    );
    graph_check(&files).unwrap();
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_files_with_bhasa(&files, "aarch64-apple-darwin").unwrap();
    for main in [
        "def main(){var x=lib.Public(1)}",
        "def main(){var x=lib.create();print(x.value)}",
        "def main(){var x:lib.Hidden=lib.create()}",
    ] {
        let files = graph(main, library);
        assert!(graph_check(&files)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("private")));
        #[cfg(feature = "bhasa-frontend")]
        assert!(bhasa::compile_files_with_bhasa(&files, "aarch64-apple-darwin").is_err());
    }
}

#[test]
fn enum_aliases_work_in_variant_construction_and_match_patterns() {
    accepts("enum Choice[T]{none;some(value:T)}\ntype Number=int\ntype Selected=Choice[Number]\ndef main(){var x:Selected=Selected.some(42);match x{case Selected.none{print(0)}case Selected.some(value){print(value)}}}");
}

#[test]
fn defining_module_resolution_survives_import_aliases_and_local_collisions() {
    let library =
        "struct Item{value:int}\ntype Selected=Item\ndef create()->Selected{return Item(42)}";
    let main = "struct Item{value:bool}\ntype Local=lib.Selected\ndef main(){var x:Local=lib.create();print(x.value)}";
    let files = graph(main, library);
    graph_check(&files).unwrap();
    #[cfg(feature = "bhasa-frontend")]
    bhasa::compile_files_with_bhasa(&files, "aarch64-apple-darwin").unwrap();
    let files = graph(
        "struct Item{value:bool}\ntype Local=lib.Selected\ndef main(){var x:Item=lib.create()}",
        library,
    );
    assert!(graph_check(&files).is_err());
}

#[test]
fn alias_resolution_is_bounded_and_keeps_unicode_original_locations() {
    let mut source = String::from("type Alias0=int\n");
    for index in 1..70 {
        source.push_str(&format!("type Alias{index}=Alias{}\n", index - 1));
    }
    source.push_str("def main(){}");
    // Resolved cache entries can shorten a chain. A reverse-order dependency
    // ensures that the entire chain is initially unresolved.
    let mut reverse = String::new();
    for index in 0..70 {
        reverse.push_str(&format!("type Alias{index}=Alias{}\n", index + 1));
    }
    reverse.push_str("type Alias70=int\ndef main(){}");
    rejects(&reverse, "deep");
    accepts(&source);
    let source = "# नेपाली\ntype Value=int\ndef main(){var value:Value=true}";
    let errors = bhasa::check(source).unwrap_err();
    assert!(errors.iter().all(|error| error.span.end <= source.len()));
    assert!(errors
        .iter()
        .any(|error| source[error.span.start..error.span.end].contains("true")));
}

#[test]
fn unused_concrete_alias_targets_receive_full_semantic_validation() {
    for (source, message) in [
        (
            "type Bad=Map[buffer,int]\ndef main(){}",
            "Map keys must be int or str",
        ),
        ("type Bad=List[void]\ndef main(){}", "void"),
        (
            "struct Box[T]{value:T}\ntype Bad=Box[void]\ndef main(){}",
            "void",
        ),
        (
            "enum Choice[T]{some(value:T)}\ntype Bad=Choice[void]\ndef main(){}",
            "void",
        ),
        (
            "struct Box[T]{value:T}\ntype Bad=Box[task]\ndef main(){}",
            "task",
        ),
    ] {
        rejects(source, message);
    }
    rejects(
        "struct Bad[T]{value:void}\ntype Unused=Bad[int]\ndef main(){}",
        "cannot have type void",
    );
    rejects(
        "enum Bad[T]{some(value:void)}\ntype Unused=Bad[int]\ndef main(){}",
        "enum payloads",
    );
    accepts("type Nothing=void\ndef main()->Nothing{}");
    accepts("type Handle=task\ndef main(){}");
    rejects("type Handle=task\ndef worker(x:int)->int{return x}def main(){var t:Handle=spawn(worker(42));var value=join(t)}", "must be inferred");
    accepts("type Unused=List[int]\nstruct Box[T]{value:T}\ntype UnusedBox=Box[int]\ndef main(){}");
    let checked = bhasa::check(
        "struct Box[T]{value:T}\ntype First=Box[int]\ntype Second=First\ndef main(){}",
    )
    .unwrap();
    // One existing concrete target ID, no alias wrapper functions or alias types.
    assert_eq!(checked.program.type_names.len(), 1);
    assert_eq!(checked.program.records.len(), 1);
    assert_eq!(checked.program.functions.len(), 1);
}

#[test]
fn unused_alias_intrinsic_error_keeps_its_original_rhs_location() {
    let source = "# नेपाली\nstruct Prefix{value:int}\ntype Bad = Map[buffer,int]\ndef main(){}";
    let error = bhasa::check(source).unwrap_err().remove(0);
    assert_eq!(&source[error.span.start..error.span.end], "Map[buffer,int]");
    let files = graph("def main(){}", "# नेपाली\ntype Bad = Map[buffer,int]");
    let error = graph_check(&files).unwrap_err().remove(0);
    let original = bhasa::project::flattened_source(&files);
    assert_eq!(
        &original[error.span.start..error.span.end],
        "Map[buffer,int]"
    );
    assert!(error.span.start >= files[1].base);
}

#[test]
fn alias_metadata_preserves_source_identity_without_exposing_nominal_wrappers() {
    let source="struct Item{value:int}\nprivate type Selected=Item\ndef main(){var item:Selected=Selected(42)}";
    let files = vec![SourceFile {
        path: "main.bh".into(),
        source: source.into(),
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let expansion = bhasa::modules::expand_files(&files).unwrap();
    let alias = &expansion.type_aliases[0];
    assert_eq!(alias.name, "Selected");
    assert!(alias.private);
    assert_eq!(alias.owner, 0);
    assert_eq!(
        &source[alias.name_span.start..alias.name_span.end],
        "Selected"
    );
    assert_eq!(
        &source[alias.target_span.start..alias.target_span.end],
        "Item"
    );
    assert_eq!(expansion.type_alias_uses.len(), 2);
    assert!(expansion
        .type_alias_uses
        .iter()
        .all(|use_| use_.internal_name == alias.internal_name));
    assert!(expansion
        .type_alias_uses
        .iter()
        .any(|use_| use_.type_context));
    assert!(expansion
        .type_alias_uses
        .iter()
        .any(|use_| !use_.type_context));
    assert!(expansion
        .type_alias_uses
        .iter()
        .all(|use_| &source[use_.span.start..use_.span.end] == "Selected"));
    let root = &expansion.type_roots().unwrap()[0];
    assert_eq!(expansion.map_span(root.span), alias.target_span);
    let mut corrupted = expansion.clone();
    corrupted.type_aliases[0].target_span = Span::new(source.len() + 10, source.len() + 11);
    assert!(corrupted.type_roots().is_err());
}

#[test]
fn alias_rhs_dependencies_are_preserved_for_rename_and_navigation() {
    let main = "type Local=lib.Value\ndef main(){var x:Local=42}";
    let library = "type Number=int\ntype Value=Number";
    let files = graph(main, library);
    let expansion = bhasa::modules::expand_files(&files).unwrap();
    let value = expansion
        .type_aliases
        .iter()
        .find(|alias| alias.name == "Value")
        .unwrap();
    let number = expansion
        .type_aliases
        .iter()
        .find(|alias| alias.name == "Number")
        .unwrap();
    let original = bhasa::project::flattened_source(&files);
    assert!(expansion
        .type_alias_uses
        .iter()
        .any(|use_| use_.internal_name == value.internal_name
            && &original[use_.span.start..use_.span.end] == "lib.Value"));
    assert!(expansion
        .type_alias_uses
        .iter()
        .any(|use_| use_.internal_name == number.internal_name
            && &original[use_.span.start..use_.span.end] == "Number"));
    assert!(expansion
        .type_alias_uses
        .windows(2)
        .all(|pair| pair[0].span.start < pair[1].span.start));
}

#[test]
fn canonical_alias_metadata_has_a_cumulative_memory_bound() {
    let mut source = String::from("struct Pair[A,B]{left:A;right:B}\ntype Level0=int\n");
    for index in 1..12 {
        source.push_str(&format!(
            "type Level{index}=Pair[Level{},Level{}]\n",
            index - 1,
            index - 1
        ));
    }
    for index in 0..500 {
        source.push_str(&format!("type Repeat{index}=Level11\n"));
    }
    source.push_str("def main(){}");
    let files = vec![SourceFile {
        path: "main.bh".into(),
        source,
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let errors = bhasa::modules::expand_files(&files).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("16 MiB budget")));
}

#[test]
fn excessive_generic_arguments_stop_before_resolving_or_appending_the_extra_type() {
    // Resolving Missing would produce an unknown-type error. Arity must win.
    let source = "type Wrong=Map[int,str,Missing]\ndef main(){}";
    let files = vec![SourceFile {
        path: "main.bh".into(),
        source: source.into(),
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let error = bhasa::modules::expand_files(&files).unwrap_err().remove(0);
    assert!(
        error.message.contains("excess generic arguments"),
        "{error:?}"
    );
    assert_eq!(&source[error.span.start..error.span.end], "Missing");

    // Build one cached, near-limit alias from compact recursive spelling, then
    // reference it thousands of times in an invalid third-and-later argument.
    // The invalid count must reject before expanding any of those references.
    let mut source = String::from("struct Pair[A,B]{left:A;right:B}\ntype Level0=int\n");
    for index in 1..12 {
        source.push_str(&format!(
            "type Level{index}=Pair[Level{},Level{}]\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("type Wrong=Map[int,str,");
    let excess = source.len();
    source.push_str(&vec!["Level11"; 4096].join(","));
    source.push_str("]\ndef main(){}");
    let files = vec![SourceFile {
        path: "main.bh".into(),
        source: source.clone(),
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let error = bhasa::modules::expand_files(&files).unwrap_err().remove(0);
    assert!(
        error.message.contains("excess generic arguments"),
        "{error:?}"
    );
    assert_eq!(error.span.start, excess);
    assert_eq!(&source[error.span.start..error.span.end], "Level11");
}

#[test]
fn cumulative_generic_spelling_is_checked_before_each_argument_append() {
    let mut source = String::from("struct Pair[A,B]{left:A;right:B}\ntype Level0=int\n");
    for index in 1..13 {
        source.push_str(&format!(
            "type Level{index}=Pair[Level{},Level{}]\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("def main(){}");
    let files = vec![SourceFile {
        path: "main.bh".into(),
        source: source.clone(),
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let error = bhasa::modules::expand_files(&files).unwrap_err().remove(0);
    assert!(
        error.message.contains("spelling exceeds 64 KiB"),
        "{error:?}"
    );
    assert_eq!(&source[error.span.start..error.span.end], "Pair");
}

#[test]
fn alias_named_main_is_a_type_and_does_not_become_an_entry_function() {
    let source = "type main=int;def identity(x:main)->main{return x}";
    accepts(source);
    let checked = bhasa::check(source).unwrap();
    assert_eq!(checked.program.functions.len(), 1);
    assert_eq!(checked.program.functions[0].return_type, Type::Int);
    assert!(checked.program.type_names.is_empty());
    accepts(
        "type main=int;def identity(x:main)->main{return x};def entry()->main{return identity(42)}",
    );
}

#[test]
fn alias_targets_accept_the_same_trailing_comma_syntax_as_direct_generic_types() {
    accepts("struct Box[T]{value:T}\ntype Selected=Box[int,]\ndef main(){var a:Selected=Box[int,](42);var b:Box[int]=Selected(7);print(a.value);print(b.value)}");
    accepts("type Values=List[int,]\ntype Table=Map[int,str,]\ndef main(){var a:Values=List[int]();a.push(42);var b:Table=Map[int,str,]();b.put(1,\"ok\");print(a.get(0))}");
    rejects("type Bad=Map[int,]\ndef main(){}", "requires its concrete");
    rejects(
        "struct Box[T]{value:T}\ntype Bad=Box[int,str,]\ndef main(){}",
        "excess generic arguments",
    );
}

#[test]
fn alias_nominal_rhs_metadata_records_original_resolved_anchors_at_every_depth() {
    let source="struct Point{value:int}\nstruct Box[T]{value:T}\ntype Node=Point\ntype Item=Box[Box[Point]]\ntype Chain=Box[Node]\ndef main(){}\n";
    let file = bhasa::project::SourceFile {
        path: "main.bh".into(),
        source: source.into(),
        base: 0,
        imports: vec![],
        import_spans: vec![],
    };
    let expanded = bhasa::modules::expand_files(&[file]).unwrap();
    let uses = &expanded.type_alias_nominal_uses;
    assert_eq!(uses.len(), 5);
    let points: Vec<_> = uses
        .iter()
        .filter(|use_| &source[use_.declaration_span.start..use_.declaration_span.end] == "Point")
        .collect();
    assert_eq!(points.len(), 2);
    assert!(points
        .iter()
        .all(|use_| use_.declaration_span.start == source.find("Point{").unwrap()));
    assert!(uses
        .iter()
        .all(|use_| &source[use_.span.start..use_.span.end] != "Node"));
    assert!(uses
        .windows(2)
        .all(|pair| pair[0].span.start < pair[1].span.start));
    assert!(expanded
        .type_alias_uses
        .iter()
        .any(|use_| &source[use_.span.start..use_.span.end] == "Node"));
}
