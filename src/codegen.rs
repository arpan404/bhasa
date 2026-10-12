//! Textual LLVM IR for the checked bootstrap language.
//!
//! Locals use entry-block allocations so loops do not grow the stack. Arithmetic
//! branches to a trap before an overflowing division can produce LLVM poison.
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;

type Result<T> = std::result::Result<T, Diagnostic>;

/// Emit a complete LLVM module without invoking Clang or consulting the host.
pub fn emit_llvm(
    program: &CheckedProgram,
    target_triple: &str,
) -> std::result::Result<String, Vec<Diagnostic>> {
    let errors = crate::ownership::check_checked(program);
    if !errors.is_empty() {
        return Err(errors);
    }

    let failures = crate::failure_use::check(&program.program);
    if !failures.is_empty() {
        return Err(failures);
    }
    emit_module(program, target_triple).map_err(|error| vec![error])
}

fn error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new("C001", message, span)
}

fn llvm_type(ty: Type) -> &'static str {
    match ty {
        Type::Int => "i64",
        Type::Float => "double",
        Type::Bool => "i1",
        Type::Str
        | Type::Buffer
        | Type::Ints
        | Type::Task
        | Type::Record(_)
        | Type::Enum(_)
        | Type::TcpListener
        | Type::TcpSocket
        | Type::TlsSocket
        | Type::FileHandle => "ptr",
        Type::Void => "void",
    }
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn symbol(name: &str, external: bool) -> String {
    // A dot cannot occur in a source identifier, including a trusted import.
    if external {
        format!("@{name}")
    } else {
        format!("@bhasa.fn.{name}")
    }
}

fn validate_declarations(program: &CheckedProgram) -> Result<()> {
    let mut record_names = BTreeSet::new();
    for record in &program.program.records {
        if !identifier(&record.name)
            || crate::builtins::is_builtin(&record.name)
            || record.name.starts_with("bhasa_rt_")
            || !record_names.insert(&record.name)
            || !program.program.type_names.contains(&record.name)
        {
            return Err(error(
                "invalid or duplicate record declaration",
                record.span,
            ));
        }
        let mut fields = BTreeSet::new();
        for field in &record.fields {
            if !identifier(&field.name) || !fields.insert(&field.name) {
                return Err(error("invalid or duplicate record field", field.span));
            }
            if matches!(field.ty, Type::Void | Type::Task) {
                return Err(error("invalid record field type", field.span));
            }
        }
        for method in &record.methods {
            if !identifier(&method.name) || !fields.insert(&method.name) {
                return Err(error("invalid or duplicate record member", method.span));
            }
            let id = program
                .program
                .type_names
                .iter()
                .position(|name| name == &record.name);
            if method.body.is_none()
                || !matches!(method.params.first(), Some(param) if param.name == "self" && Some(param.ty) == id.map(Type::Record))
            {
                return Err(error(
                    "invalid record method receiver or missing body",
                    method.span,
                ));
            }
        }
    }
    for declaration in &program.program.enums {
        if !identifier(&declaration.name)
            || crate::builtins::is_builtin(&declaration.name)
            || declaration.name.starts_with("bhasa_rt_")
            || !record_names.insert(&declaration.name)
            || !program.program.type_names.contains(&declaration.name)
            || declaration.variants.is_empty()
        {
            return Err(error(
                "invalid or duplicate enum declaration",
                declaration.span,
            ));
        }
        let mut variants = BTreeSet::new();
        for variant in &declaration.variants {
            if !identifier(&variant.name) || !variants.insert(&variant.name) {
                return Err(error("invalid or duplicate enum variant", variant.span));
            }
            let mut fields = BTreeSet::new();
            for field in &variant.fields {
                if !identifier(&field.name)
                    || !fields.insert(&field.name)
                    || matches!(field.ty, Type::Void | Type::Task)
                {
                    return Err(error("invalid enum payload field", field.span));
                }
                validate_record_type(&program.program, field.ty, field.span)?;
            }
        }
    }
    let unique_types: BTreeSet<_> = program.program.type_names.iter().collect();
    if unique_types.len() != program.program.type_names.len() {
        return Err(error("duplicate checked record type name", Span::default()));
    }
    for record in &program.program.records {
        crate::collections::schema(record).map_err(|message| error(message, record.span))?;
        for field in &record.fields {
            validate_record_type(&program.program, field.ty, field.span)?;
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeMap::new();
    for record in &program.program.records {
        validate_record_layout(&program.program, record, &mut visiting, &mut visited)?;
    }
    for declaration in &program.program.enums {
        validate_aggregate_layout(
            &program.program,
            &declaration.name,
            declaration.span,
            &mut visiting,
            &mut visited,
        )?;
    }
    let mut names = BTreeSet::new();
    for (key, function, method) in program
        .program
        .functions
        .iter()
        .map(|function| (function.name.clone(), function, false))
        .chain(program.program.records.iter().flat_map(|record| {
            record
                .methods
                .iter()
                .map(move |method| (method_key(&record.name, &method.name), method, true))
        }))
    {
        if !identifier(&function.name)
            || (!method
                && (crate::builtins::is_builtin(&function.name)
                    || function.name.starts_with("bhasa_rt_")))
        {
            return Err(error("invalid or reserved function name", function.span));
        }
        if (!method && record_names.contains(&function.name)) || !names.insert(key.clone()) {
            return Err(error("duplicate function declaration", function.span));
        }
        let external = function.body.is_none();
        validate_record_type(&program.program, function.return_type, function.span)?;
        let expected = FunctionSig {
            modes: function.params.iter().map(|param| param.mode).collect(),
            params: function.params.iter().map(|param| param.ty).collect(),
            return_type: function.return_type,
            external,
        };
        if program.signatures.get(&key) != Some(&expected) {
            return Err(error(
                format!(
                    "missing or inconsistent checked signature for '{}'",
                    function.name
                ),
                function.span,
            ));
        }
        if external && !matches!(function.return_type, Type::Int | Type::Void) {
            return Err(error(
                "C ABI imports must return int or void",
                function.span,
            ));
        }
        let mut params = BTreeSet::new();
        for param in &function.params {
            validate_record_type(&program.program, param.ty, param.span)?;
            if !identifier(&param.name) || !params.insert(&param.name) {
                return Err(error("invalid or duplicate parameter name", param.span));
            }
            if param.ty == Type::Void || (external && param.ty != Type::Int) {
                return Err(error("invalid parameter type", param.span));
            }
        }
        if !method
            && function.name == "main"
            && (external
                || !function.params.is_empty()
                || !matches!(function.return_type, Type::Int | Type::Void))
        {
            return Err(error(
                "main must be a Bhasa definition with no parameters returning int or void",
                function.span,
            ));
        }
    }
    if program.signatures.len() != names.len() {
        return Err(error(
            "checked signatures contain undeclared functions",
            Span::default(),
        ));
    }
    Ok(())
}

fn validate_record_type(program: &Program, ty: Type, span: Span) -> Result<()> {
    let known = match ty {
        Type::Record(id) => program
            .type_names
            .get(id)
            .is_some_and(|name| program.records.iter().any(|record| &record.name == name)),
        Type::Enum(id) => program.enum_decl(id).is_some(),
        _ => true,
    };
    if !known {
        return Err(error("unknown aggregate type in checked AST", span));
    }
    Ok(())
}

fn validate_record_layout<'a>(
    program: &'a Program,
    record: &'a RecordDecl,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeMap<&'a str, usize>,
) -> Result<usize> {
    validate_aggregate_layout(program, &record.name, record.span, visiting, visited)
}

fn validate_aggregate_layout<'a>(
    program: &'a Program,
    name: &'a str,
    span: Span,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeMap<&'a str, usize>,
) -> Result<usize> {
    if let Some(depth) = visited.get(name) {
        if visiting.len() + depth > 128 {
            return Err(error("aggregate nesting exceeds 128 levels", span));
        }
        return Ok(*depth);
    }
    if visiting.len() >= 128 {
        return Err(error("aggregate nesting exceeds 128 levels", span));
    }
    if !visiting.insert(name) {
        return Err(error("recursive aggregate layout in checked AST", span));
    }
    let fields: Vec<_> =
        if let Some(record) = program.records.iter().find(|record| record.name == name) {
            record.fields.iter().collect()
        } else if let Some(declaration) = program
            .enums
            .iter()
            .find(|declaration| declaration.name == name)
        {
            declaration
                .variants
                .iter()
                .flat_map(|variant| &variant.fields)
                .collect()
        } else {
            return Err(error("unknown aggregate in checked AST", span));
        };
    let mut depth = 1;
    for field in fields {
        if let Type::Record(id) | Type::Enum(id) = field.ty {
            let nested = program
                .type_names
                .get(id)
                .ok_or_else(|| error("unknown nested aggregate", field.span))?;
            depth = depth.max(
                1 + validate_aggregate_layout(program, nested, field.span, visiting, visited)?,
            );
        }
    }
    visiting.remove(name);
    visited.insert(name, depth);
    Ok(depth)
}

fn emit_module(program: &CheckedProgram, target: &str) -> Result<String> {
    if target.is_empty()
        || !target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(error("invalid LLVM target triple", Span::default()));
    }
    validate_declarations(program)?;
    let mut module = Module {
        program: &program.program,
        signatures: &program.signatures,
        task_wrappers: BTreeSet::new(),
        record_helpers: BTreeSet::new(),
        enum_helpers: BTreeSet::new(),
        strings: Vec::new(),
        string_ids: BTreeMap::new(),
        intrinsics: BTreeSet::new(),
        needs_trap: false,
        needs_printf: false,
        runtime: BTreeSet::new(),
        threads: false,
        metal: false,
        net: false,
        file: false,
        text: false,
        tls: false,
        collections: false,
    };
    let mut definitions = String::new();
    for function in &program.program.functions {
        if function.body.is_some() {
            definitions.push_str(&FunctionEmitter::emit(
                &mut module,
                function,
                &function.name,
            )?);
        }
    }
    for record in &program.program.records {
        for method in &record.methods {
            definitions.push_str(&FunctionEmitter::emit(
                &mut module,
                method,
                &method_key(&record.name, &method.name),
            )?);
        }
    }
    definitions.push_str(&module.emit_task_wrappers()?);
    definitions.push_str(&module.emit_record_helpers(target)?);
    if let Some(main) = program.signatures.get("main") {
        if module.runtime.is_empty() {
            definitions.push_str("define i32 @main() {\nentry:\n");
        } else {
            definitions.push_str("define i32 @main(i32 %argc, ptr %argv) {\nentry:\n  call void @bhasa_rt_init(i32 %argc, ptr %argv)\n");
            module
                .runtime
                .insert("declare void @bhasa_rt_init(i32, ptr)".into());
        }
        if main.return_type == Type::Void {
            definitions.push_str("  call void @bhasa.fn.main()\n  ret i32 0\n}\n");
        } else {
            definitions.push_str(
                "  %result = call i64 @bhasa.fn.main()\n  %exit = trunc i64 %result to i32\n  ret i32 %exit\n}\n",
            );
        }
    }
    let mut output = format!("; Bhasa bootstrap LLVM module\ntarget triple = \"{target}\"\n\n");
    for record in &program.program.records {
        let id = program
            .program
            .type_names
            .iter()
            .position(|name| name == &record.name)
            .ok_or_else(|| error("missing record type ID", record.span))?;
        output.push_str(&format!(
            "%bhasa.record.{id} = type {{ {} }}\n",
            record
                .fields
                .iter()
                .map(|field| llvm_type(field.ty))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for declaration in &program.program.enums {
        let id = program
            .program
            .type_names
            .iter()
            .position(|name| name == &declaration.name)
            .ok_or_else(|| error("missing enum type ID", declaration.span))?;
        output.push_str(&format!("%bhasa.enum.{id} = type {{ i64, ptr }}\n"));
        for (index, variant) in declaration.variants.iter().enumerate() {
            output.push_str(&format!(
                "%bhasa.enum.{id}.variant.{index} = type {{ {} }}\n",
                variant
                    .fields
                    .iter()
                    .map(|field| llvm_type(field.ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if !module.record_helpers.is_empty() || !module.enum_helpers.is_empty() {
        output.push_str(&format!(
            "declare ptr @malloc({})\ndeclare void @free(ptr)\n",
            allocation_size_type(target)
        ));
    }
    if !module.runtime.is_empty() {
        output.push_str("; bhasa runtime: core\n");
    }
    if module.threads {
        output.push_str("; bhasa runtime: threads\n");
    }
    if module.metal {
        output.push_str("; bhasa runtime: gpu\n");
    }
    if module.net {
        output.push_str("; bhasa runtime: net\n");
    }
    if module.file {
        output.push_str("; bhasa runtime: file\n");
    }
    if module.tls {
        output.push_str("; bhasa runtime: tls\n");
    }
    if module.collections {
        output.push_str("; bhasa runtime: collections\n");
    }
    if module.text {
        output.push_str("; bhasa runtime: text\n");
    }
    for declaration in &module.runtime {
        output.push_str(declaration);
        output.push('\n');
    }
    for (index, value) in module.strings.iter().enumerate() {
        let mut bytes = String::new();
        for byte in value.bytes().chain(std::iter::once(0)) {
            use std::fmt::Write;
            let _ = write!(bytes, "\\{byte:02X}");
        }
        output.push_str(&format!(
            "@.bhasa.str.{index} = private unnamed_addr constant [{} x i8] c\"{bytes}\", align 1\n",
            value.len() + 1
        ));
    }
    output.push('\n');
    if module.needs_printf {
        output.push_str("declare i32 @printf(ptr, ...)\n");
    }
    for function in &program.program.functions {
        if function.body.is_none() {
            if (!module.record_helpers.is_empty() || !module.enum_helpers.is_empty())
                && matches!(function.name.as_str(), "malloc" | "free")
            {
                return Err(error(
                    "external allocator symbol conflicts with record storage; rename the foreign adapter",
                    function.span,
                ));
            }
            // A scalar source import cannot describe printf's variadic ABI.
            // Reject the collision instead of silently calling one symbol
            // through incompatible prototypes.
            if module.needs_printf && function.name == "printf" {
                return Err(error(
                    "external 'printf' conflicts with the runtime required by print; rename the foreign adapter",
                    function.span,
                ));
            }
            output.push_str(&format!(
                "declare {} {}({})\n",
                llvm_type(function.return_type),
                symbol(&function.name, true),
                function
                    .params
                    .iter()
                    .map(|param| llvm_type(param.ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    for intrinsic in &module.intrinsics {
        output.push_str(&format!(
            "declare {{ i64, i1 }} @llvm.{intrinsic}.with.overflow.i64(i64, i64)\n"
        ));
    }
    if module.needs_trap {
        output.push_str("declare void @llvm.trap() cold noreturn nounwind\n");
    }
    output.push('\n');
    output.push_str(&definitions);
    Ok(output)
}

struct Module<'a> {
    program: &'a Program,
    signatures: &'a BTreeMap<String, FunctionSig>,
    task_wrappers: BTreeSet<String>,
    record_helpers: BTreeSet<usize>,
    enum_helpers: BTreeSet<usize>,
    strings: Vec<String>,
    string_ids: BTreeMap<String, usize>,
    intrinsics: BTreeSet<&'static str>,
    needs_trap: bool,
    needs_printf: bool,
    runtime: BTreeSet<String>,
    threads: bool,
    metal: bool,
    net: bool,
    file: bool,
    text: bool,
    tls: bool,
    collections: bool,
}

fn allocation_size_type(target: &str) -> &'static str {
    // All supported native targets use a 64-bit size_t. Wasm32 object
    // emission uses its 32-bit allocator ABI instead.
    if target.starts_with("wasm32-") {
        "i32"
    } else {
        "i64"
    }
}

fn task_frame_size(ty: Type) -> String {
    format!(
        "ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64)",
        llvm_type(ty)
    )
}

impl Module<'_> {
    fn emit_task_wrappers(&mut self) -> Result<String> {
        let mut output = String::new();
        for worker in self.task_wrappers.clone() {
            let signature = self
                .signatures
                .get(&worker)
                .ok_or_else(|| error("missing task worker signature", Span::default()))?
                .clone();
            let input = signature.params[0];
            let result = signature.return_type;
            self.runtime
                .insert("declare ptr @bhasa_rt_task_alloc(i64)".into());
            self.runtime
                .insert("declare void @bhasa_rt_task_free(ptr)".into());
            self.threads = true;
            output.push_str(&format!("define internal ptr @bhasa.task.{worker}(ptr %frame) {{\nentry:\n  %input = load {}, ptr %frame\n", llvm_type(input)));
            if result == Type::Void {
                output.push_str(&format!(
                    "  call void {}({} %input)\n",
                    symbol(&worker, false),
                    llvm_type(input)
                ));
            } else {
                output.push_str(&format!(
                    "  %value = call {} {}({} %input)\n",
                    llvm_type(result),
                    symbol(&worker, false),
                    llvm_type(input)
                ));
            }
            if let Some(drop) = self.record_field_helper(input, false) {
                output.push_str(&format!("  call void {drop}(ptr %input)\n"));
            }
            output.push_str("  call void @bhasa_rt_task_free(ptr %frame)\n");
            if result == Type::Void {
                output.push_str("  ret ptr null\n}\n\n");
            } else {
                output.push_str(&format!("  %result = call ptr @bhasa_rt_task_alloc(i64 {})\n  store {} %value, ptr %result\n  ret ptr %result\n}}\n\n", task_frame_size(result), llvm_type(result)));
            }
        }
        Ok(output)
    }

    fn record(&self, id: usize, span: Span) -> Result<&RecordDecl> {
        let name = self
            .program
            .type_names
            .get(id)
            .ok_or_else(|| error("unknown record type ID", span))?;
        self.program
            .records
            .iter()
            .find(|record| &record.name == name)
            .ok_or_else(|| error(format!("unknown record '{name}'"), span))
    }

    fn field(&self, ty: Type, name: &str, span: Span) -> Result<(usize, usize, Type)> {
        let Type::Record(id) = ty else {
            return Err(error("field access requires a record", span));
        };
        let record = self.record(id, span)?;
        if crate::collections::is_collection(&record.name) {
            return Err(error("collection storage fields are inaccessible", span));
        }
        let (index, field) = record
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
            .ok_or_else(|| {
                error(
                    format!("record '{}' has no field '{name}'", record.name),
                    span,
                )
            })?;
        Ok((id, index, field.ty))
    }

    fn emit_record_helpers(&mut self, target: &str) -> Result<String> {
        let mut output = String::new();
        let mut emitted = BTreeSet::new();
        let mut emitted_enums = BTreeSet::new();
        loop {
            let Some(id) = self.record_helpers.difference(&emitted).next().copied() else {
                if let Some(id) = self.enum_helpers.difference(&emitted_enums).next().copied() {
                    emitted_enums.insert(id);
                    output.push_str(&self.emit_enum_helpers(id, target)?);
                    continue;
                }
                break;
            };
            emitted.insert(id);
            let record = self.record(id, Span::default())?.clone();
            if let Some(schema) = crate::collections::schema(&record)
                .map_err(|message| error(message, record.span))?
            {
                output.push_str(&self.emit_collection_helpers(id, schema));
                continue;
            }
            self.needs_trap = true;
            let size_ty = allocation_size_type(target);
            // Empty records still have a distinct valid allocation.
            let size = if record.fields.is_empty() {
                "1".to_owned()
            } else {
                format!(
                    "ptrtoint (ptr getelementptr (%bhasa.record.{id}, ptr null, i32 1) to {size_ty})"
                )
            };
            output.push_str(&format!(
                "define internal ptr @bhasa.record.{id}.new() {{\nentry:\n  %record = call ptr @malloc({size_ty} {size})\n  %failed = icmp eq ptr %record, null\n  br i1 %failed, label %failure, label %success\nfailure:\n  call void @llvm.trap()\n  unreachable\nsuccess:\n  ret ptr %record\n}}\n\n"
            ));
            output.push_str(&format!("define internal ptr @bhasa.record.{id}.clone(ptr %source) {{\nentry:\n  %record = call ptr @bhasa.record.{id}.new()\n"));
            for (index, field) in record.fields.iter().enumerate() {
                let ty = llvm_type(field.ty);
                output.push_str(&format!(
                    "  %source.{index} = getelementptr %bhasa.record.{id}, ptr %source, i32 0, i32 {index}\n  %value.{index} = load {ty}, ptr %source.{index}\n"
                ));
                let value = if let Some(clone) = self.record_field_helper(field.ty, true) {
                    output.push_str(&format!(
                        "  %copy.{index} = call ptr {clone}(ptr %value.{index})\n"
                    ));
                    format!("%copy.{index}")
                } else {
                    format!("%value.{index}")
                };
                output.push_str(&format!(
                    "  %target.{index} = getelementptr %bhasa.record.{id}, ptr %record, i32 0, i32 {index}\n  store {ty} {value}, ptr %target.{index}\n"
                ));
            }
            output.push_str("  ret ptr %record\n}\n\n");
            output.push_str(&format!(
                "define internal void @bhasa.record.{id}.drop(ptr %record) {{\nentry:\n"
            ));
            for (index, field) in record.fields.iter().enumerate().rev() {
                if let Some(drop) = self.record_field_helper(field.ty, false) {
                    output.push_str(&format!(
                        "  %field.{index} = getelementptr %bhasa.record.{id}, ptr %record, i32 0, i32 {index}\n  %value.{index} = load ptr, ptr %field.{index}\n  call void {drop}(ptr %value.{index})\n"
                    ));
                }
            }
            output.push_str("  call void @free(ptr %record)\n  ret void\n}\n\n");
        }
        Ok(output)
    }

    fn emit_collection_helpers(&mut self, id: usize, schema: crate::collections::Schema) -> String {
        let kind = schema.kind();
        self.collections = true;
        let mut output = String::new();
        for declaration in [
            "declare ptr @bhasa_rt_list_new(i64, ptr, ptr)",
            "declare ptr @bhasa_rt_map_new(i64, i64, ptr, ptr, ptr, ptr, ptr, ptr)",
            "declare void @bhasa_rt_list_get(ptr, i64, ptr)",
            "declare void @bhasa_rt_list_push(ptr, ptr)",
            "declare void @bhasa_rt_list_set(ptr, i64, ptr)",
            "declare void @bhasa_rt_map_get(ptr, ptr, ptr)",
            "declare void @bhasa_rt_map_put(ptr, ptr, ptr)",
            "declare i64 @bhasa_rt_map_contains(ptr, ptr)",
            "declare i64 @bhasa_rt_map_remove(ptr, ptr)",
            "declare i64 @bhasa_rt_map_hash_i64(ptr)",
            "declare i64 @bhasa_rt_map_hash_str(ptr)",
            "declare i64 @bhasa_rt_map_equal_i64(ptr, ptr)",
            "declare i64 @bhasa_rt_map_equal_str(ptr, ptr)",
        ] {
            self.runtime.insert(declaration.into());
        }
        for (op, result) in [
            ("clone", "ptr"),
            ("free", "void"),
            ("count", "i64"),
            ("clear", "void"),
        ] {
            self.runtime
                .insert(format!("declare {result} @bhasa_rt_{kind}_{op}(ptr)"));
        }
        let elements = schema.elements();
        for (index, ty) in elements.iter().copied().enumerate() {
            let llvm = llvm_type(ty);
            output.push_str(&format!("define internal void @bhasa.collection.{id}.{index}.clone(ptr %dst, ptr %src) {{\nentry:\n  %value = load {llvm}, ptr %src\n"));
            let value = if let Some(helper) = self.record_field_helper(ty, true) {
                output.push_str(&format!("  %copy = call ptr {helper}(ptr %value)\n"));
                "%copy"
            } else {
                "%value"
            };
            output.push_str(&format!(
                "  store {llvm} {value}, ptr %dst\n  ret void\n}}\n\n"
            ));
            output.push_str(&format!(
                "define internal void @bhasa.collection.{id}.{index}.drop(ptr %slot) {{\nentry:\n"
            ));
            if let Some(helper) = self.record_field_helper(ty, false) {
                output.push_str(&format!(
                    "  %value = load ptr, ptr %slot\n  call void {helper}(ptr %value)\n"
                ));
            }
            output.push_str("  ret void\n}\n\n");
        }
        let mut constructor = elements
            .iter()
            .map(|ty| {
                format!(
                    "i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64)",
                    llvm_type(*ty)
                )
            })
            .collect::<Vec<_>>();
        for index in 0..elements.len() {
            constructor.push(format!("ptr @bhasa.collection.{id}.{index}.clone"));
            constructor.push(format!("ptr @bhasa.collection.{id}.{index}.drop"));
        }
        if let crate::collections::Schema::Map(key, _) = schema {
            let suffix = if key == Type::Int { "i64" } else { "str" };
            constructor.push(format!("ptr @bhasa_rt_map_hash_{suffix}"));
            constructor.push(format!("ptr @bhasa_rt_map_equal_{suffix}"));
        }
        output.push_str(&format!("define internal ptr @bhasa.record.{id}.new() {{\nentry:\n  %result = call ptr @bhasa_rt_{kind}_new({})\n  ret ptr %result\n}}\n\n", constructor.join(", ")));
        output.push_str(&format!("define internal ptr @bhasa.record.{id}.clone(ptr %source) {{\nentry:\n  %result = call ptr @bhasa_rt_{kind}_clone(ptr %source)\n  ret ptr %result\n}}\n\n"));
        output.push_str(&format!("define internal void @bhasa.record.{id}.drop(ptr %source) {{\nentry:\n  call void @bhasa_rt_{kind}_free(ptr %source)\n  ret void\n}}\n\n"));
        for method in schema.methods() {
            let (params, result) = schema.method(method).unwrap();
            let return_ty = llvm_type(result);
            let mut signature = vec!["ptr %self".to_owned()];
            signature.extend(
                params
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| format!("{} %arg.{index}", llvm_type(*ty))),
            );
            output.push_str(&format!(
                "define internal {return_ty} @bhasa.method.{id}.{method}({}) {{\nentry:\n",
                signature.join(", ")
            ));
            let mut arguments = vec!["ptr %self".to_owned()];
            for (index, ty) in params.iter().enumerate() {
                if kind == "list" && index == 0 && matches!(*method, "get" | "set") {
                    arguments.push(format!("i64 %arg.{index}"));
                } else {
                    let llvm = llvm_type(*ty);
                    output.push_str(&format!("  %slot.{index} = alloca {llvm}\n  store {llvm} %arg.{index}, ptr %slot.{index}\n"));
                    arguments.push(format!("ptr %slot.{index}"));
                }
            }
            if *method == "get" {
                output.push_str(&format!("  %out = alloca {return_ty}\n"));
                arguments.push("ptr %out".into());
                output.push_str(&format!("  call void @bhasa_rt_{kind}_{method}({})\n  %result = load {return_ty}, ptr %out\n  ret {return_ty} %result\n", arguments.join(", ")));
            } else if result == Type::Void {
                output.push_str(&format!(
                    "  call void @bhasa_rt_{kind}_{method}({})\n  ret void\n",
                    arguments.join(", ")
                ));
            } else {
                output.push_str(&format!(
                    "  %result = call i64 @bhasa_rt_{kind}_{method}({})\n",
                    arguments.join(", ")
                ));
                if result == Type::Bool {
                    output.push_str("  %boolean = icmp ne i64 %result, 0\n  ret i1 %boolean\n");
                } else {
                    output.push_str("  ret i64 %result\n");
                }
            }
            output.push_str("}\n\n");
        }
        output
    }

    fn enumeration(&self, id: usize, span: Span) -> Result<&EnumDecl> {
        self.program
            .enum_decl(id)
            .ok_or_else(|| error("unknown enum type ID", span))
    }

    fn emit_enum_helpers(&mut self, id: usize, target: &str) -> Result<String> {
        let declaration = self.enumeration(id, Span::default())?.clone();
        self.needs_trap = true;
        let size_ty = allocation_size_type(target);
        let mut output = format!(
            "define internal ptr @bhasa.enum.{id}.new(i64 %tag, ptr %payload) {{\nentry:\n  %record = call ptr @malloc({size_ty} ptrtoint (ptr getelementptr (%bhasa.enum.{id}, ptr null, i32 1) to {size_ty}))\n  %failed = icmp eq ptr %record, null\n  br i1 %failed, label %failure, label %success\nfailure:\n  call void @llvm.trap()\n  unreachable\nsuccess:\n  %tag.addr = getelementptr %bhasa.enum.{id}, ptr %record, i32 0, i32 0\n  store i64 %tag, ptr %tag.addr\n  %payload.addr = getelementptr %bhasa.enum.{id}, ptr %record, i32 0, i32 1\n  store ptr %payload, ptr %payload.addr\n  ret ptr %record\n}}\n\n"
        );
        for (index, variant) in declaration.variants.iter().enumerate() {
            if !variant.fields.is_empty() {
                output.push_str(&format!("define internal ptr @bhasa.enum.{id}.variant.{index}.new() {{\nentry:\n  %payload = call ptr @malloc({size_ty} ptrtoint (ptr getelementptr (%bhasa.enum.{id}.variant.{index}, ptr null, i32 1) to {size_ty}))\n  %failed = icmp eq ptr %payload, null\n  br i1 %failed, label %failure, label %success\nfailure:\n  call void @llvm.trap()\n  unreachable\nsuccess:\n  ret ptr %payload\n}}\n\n"));
            }
        }
        for cloning in [true, false] {
            let (operation, result) = if cloning {
                ("clone", "ptr")
            } else {
                ("drop", "void")
            };
            output.push_str(&format!("define internal {result} @bhasa.enum.{id}.{operation}(ptr %source) {{\nentry:\n  %tag.addr = getelementptr %bhasa.enum.{id}, ptr %source, i32 0, i32 0\n  %tag = load i64, ptr %tag.addr\n  %payload.addr = getelementptr %bhasa.enum.{id}, ptr %source, i32 0, i32 1\n  %payload = load ptr, ptr %payload.addr\n  switch i64 %tag, label %invalid [\n"));
            for index in 0..declaration.variants.len() {
                output.push_str(&format!("    i64 {index}, label %variant.{index}\n"));
            }
            output.push_str("  ]\ninvalid:\n  call void @llvm.trap()\n  unreachable\n");
            for (index, variant) in declaration.variants.iter().enumerate() {
                output.push_str(&format!("variant.{index}:\n"));
                if cloning && !variant.fields.is_empty() {
                    output.push_str(&format!(
                        "  %copy.{index} = call ptr @bhasa.enum.{id}.variant.{index}.new()\n"
                    ));
                }
                let fields: Vec<_> = if cloning {
                    variant.fields.iter().enumerate().collect()
                } else {
                    variant.fields.iter().enumerate().rev().collect()
                };
                for (field_index, field) in fields {
                    let helper = self.record_field_helper(field.ty, cloning);
                    if cloning || helper.is_some() {
                        let ty = llvm_type(field.ty);
                        output.push_str(&format!("  %field.{index}.{field_index} = getelementptr %bhasa.enum.{id}.variant.{index}, ptr %payload, i32 0, i32 {field_index}\n  %value.{index}.{field_index} = load {ty}, ptr %field.{index}.{field_index}\n"));
                        let value = if let Some(helper) = helper {
                            if cloning {
                                output.push_str(&format!("  %owned.{index}.{field_index} = call ptr {helper}(ptr %value.{index}.{field_index})\n"));
                                format!("%owned.{index}.{field_index}")
                            } else {
                                output.push_str(&format!(
                                    "  call void {helper}(ptr %value.{index}.{field_index})\n"
                                ));
                                String::new()
                            }
                        } else {
                            format!("%value.{index}.{field_index}")
                        };
                        if cloning {
                            output.push_str(&format!("  %target.{index}.{field_index} = getelementptr %bhasa.enum.{id}.variant.{index}, ptr %copy.{index}, i32 0, i32 {field_index}\n  store {ty} {value}, ptr %target.{index}.{field_index}\n"));
                        }
                    }
                }
                if cloning {
                    let payload = if variant.fields.is_empty() {
                        "null".to_owned()
                    } else {
                        format!("%copy.{index}")
                    };
                    output.push_str(&format!("  %result.{index} = call ptr @bhasa.enum.{id}.new(i64 {index}, ptr {payload})\n  ret ptr %result.{index}\n"));
                } else {
                    output.push_str("  call void @free(ptr %payload)\n  call void @free(ptr %source)\n  ret void\n");
                }
            }
            output.push_str("}\n\n");
        }
        Ok(output)
    }

    fn record_field_helper(&mut self, ty: Type, cloning: bool) -> Option<String> {
        if let Type::Record(id) = ty {
            self.record_helpers.insert(id);
            return Some(format!(
                "@bhasa.record.{id}.{}",
                if cloning { "clone" } else { "drop" }
            ));
        }
        if let Type::Enum(id) = ty {
            self.enum_helpers.insert(id);
            return Some(format!(
                "@bhasa.enum.{id}.{}",
                if cloning { "clone" } else { "drop" }
            ));
        }
        let kind = match ty {
            Type::Buffer => "buffer",
            Type::Ints => "ints",
            Type::TcpListener => "tcp_listener",
            Type::TcpSocket => "tcp_socket",
            Type::TlsSocket => "tls_socket",
            Type::FileHandle => "file",
            _ => return None,
        };
        self.net |= matches!(ty, Type::TcpListener | Type::TcpSocket);
        self.tls |= ty == Type::TlsSocket;
        self.file |= ty == Type::FileHandle;
        let name = format!(
            "@bhasa_rt_{kind}_{}",
            if cloning { "clone" } else { "free" }
        );
        self.runtime.insert(format!(
            "declare {} {name}(ptr)",
            if cloning { "ptr" } else { "void" }
        ));
        Some(name)
    }

    fn string(&mut self, text: &str) -> String {
        let index = if let Some(index) = self.string_ids.get(text) {
            *index
        } else {
            let index = self.strings.len();
            self.strings.push(text.to_owned());
            self.string_ids.insert(text.to_owned(), index);
            index
        };
        format!("@.bhasa.str.{index}")
    }
}

#[derive(Clone, Copy)]
struct TaskInfo {
    result: Type,
    owned_abi: bool,
}

#[derive(Clone)]
struct Local {
    initialized: bool,
    ty: Type,
    reassignable: bool,
    pointer: String,
    owned: bool,
    task: Option<TaskInfo>,
}

struct Value {
    ty: Type,
    operand: String,
    owned: bool,
    task: Option<TaskInfo>,
}

impl Value {
    fn new(ty: Type, operand: impl Into<String>) -> Self {
        Self {
            ty,
            operand: operand.into(),
            owned: false,
            task: None,
        }
    }

    fn with_task(mut self, task: TaskInfo) -> Self {
        self.task = Some(task);
        self
    }

    fn owned(mut self) -> Self {
        self.owned = self.ty.is_owned();
        self
    }

    fn require(&self, ty: Type, span: Span) -> Result<()> {
        if self.ty == ty {
            Ok(())
        } else {
            Err(error(
                format!("expected {ty}, found {} in checked AST", self.ty),
                span,
            ))
        }
    }
}

struct FunctionEmitter<'a, 'b> {
    module: &'a mut Module<'b>,
    return_type: Type,
    scopes: Vec<BTreeMap<String, Local>>,
    allocations: String,
    body: String,
    next_value: usize,
    next_block: usize,
    current_block: String,
    terminated: bool,
    loops: Vec<LoopTarget>,
}

struct LoopTarget {
    scope: usize,
    next: String,
    exit: String,
}

impl<'a, 'b> FunctionEmitter<'a, 'b> {
    fn emit(module: &'a mut Module<'b>, function: &Function, name: &str) -> Result<String> {
        let mut emitter = Self {
            module,
            return_type: function.return_type,
            scopes: vec![BTreeMap::new()],
            allocations: String::new(),
            body: String::new(),
            next_value: 0,
            next_block: 0,
            current_block: "entry".into(),
            terminated: false,
            loops: Vec::new(),
        };
        let mut params = Vec::new();
        for (index, param) in function.params.iter().enumerate() {
            params.push(format!("{} %arg{index}", llvm_type(param.ty)));
            emitter.bind(
                &param.name,
                &Value::new(param.ty, format!("%arg{index}")),
                param.span,
                param.mode == ParamMode::Owned,
                true,
            )?;
        }
        let body = function
            .body
            .as_ref()
            .ok_or_else(|| error("missing function body", function.span))?;
        emitter.block(body, false)?;
        if !emitter.terminated {
            if function.return_type == Type::Void {
                emitter.drop_locals(true);
                emitter.terminate("ret void");
            } else {
                return Err(error(
                    "non-void function can reach its end without returning",
                    function.span,
                ));
            }
        }
        Ok(format!(
            "define {} {}({}) {{\nentry:\n{}{}}}\n\n",
            llvm_type(function.return_type),
            symbol(name, false),
            params.join(", "),
            emitter.allocations,
            emitter.body
        ))
    }

    fn fresh(&mut self) -> String {
        let result = format!("%v{}", self.next_value);
        self.next_value += 1;
        result
    }

    fn label(&mut self) -> String {
        let result = format!("bb{}", self.next_block);
        self.next_block += 1;
        result
    }

    fn instruction(&mut self, instruction: impl AsRef<str>) {
        self.body.push_str("  ");
        self.body.push_str(instruction.as_ref());
        self.body.push('\n');
    }

    fn terminate(&mut self, instruction: impl AsRef<str>) {
        self.instruction(instruction);
        self.terminated = true;
    }

    fn start_block(&mut self, label: &str) {
        self.body.push_str(&format!("{label}:\n"));
        self.current_block = label.to_owned();
        self.terminated = false;
    }

    fn bind(
        &mut self,
        name: &str,
        value: &Value,
        span: Span,
        owned: bool,
        reassignable: bool,
    ) -> Result<()> {
        if !identifier(name) || value.ty == Type::Void {
            return Err(error("invalid local name or void local value", span));
        }
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            return Err(error(
                format!("duplicate local '{name}' in checked AST"),
                span,
            ));
        }
        let pointer = self.fresh();
        self.allocations
            .push_str(&format!("  {pointer} = alloca {}\n", llvm_type(value.ty)));
        self.instruction(format!(
            "store {} {}, ptr {pointer}",
            llvm_type(value.ty),
            value.operand
        ));
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| error("missing local scope", span))?;
        scope.insert(
            name.to_owned(),
            Local {
                initialized: true,
                ty: value.ty,
                reassignable,
                pointer,
                owned: owned && value.ty.is_owned(),
                task: value.task,
            },
        );
        Ok(())
    }

    fn local(&self, name: &str, span: Span) -> Result<Local> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .ok_or_else(|| error(format!("unknown local '{name}' in checked AST"), span))
    }

    fn runtime_call(&mut self, name: &str, args: &[Value], result: Type) -> Value {
        let types = args
            .iter()
            .map(|value| llvm_type(value.ty))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = args
            .iter()
            .map(|value| format!("{} {}", llvm_type(value.ty), value.operand))
            .collect::<Vec<_>>()
            .join(", ");
        // C runtime booleans deliberately use the portable int64_t ABI.
        let actual_result = if result == Type::Bool {
            Type::Int
        } else {
            result
        };
        self.module.runtime.insert(format!(
            "declare {} @{name}({types})",
            llvm_type(actual_result)
        ));
        self.module.threads |= name.contains("spawn_i64")
            || name.contains("join_i64")
            || name.contains("await_i64")
            || name.contains("spawn_owned")
            || name.contains("join_owned")
            || name.contains("await_owned")
            || name.starts_with("bhasa_rt_task_");
        self.module.metal |= name.contains("gpu_");
        self.module.net |= name.starts_with("bhasa_rt_tcp_") || name == "bhasa_rt_monotonic_millis";
        self.module.file |= name.starts_with("bhasa_rt_file_");
        self.module.text |= name.starts_with("bhasa_rt_utf8_");
        self.module.tls |= name.starts_with("bhasa_rt_tls_");
        if result == Type::Void {
            self.instruction(format!("call void @{name}({arguments})"));
            Value::new(Type::Void, "")
        } else {
            let value = self.fresh();
            self.instruction(format!(
                "{value} = call {} @{name}({arguments})",
                llvm_type(actual_result)
            ));
            if result == Type::Bool {
                let boolean = self.fresh();
                self.instruction(format!("{boolean} = icmp ne i64 {value}, 0"));
                Value::new(Type::Bool, boolean)
            } else {
                Value::new(result, value).owned()
            }
        }
    }

    fn acquire(&mut self, value: Value) -> Value {
        if value.ty.is_owned() && !value.owned {
            let helper = self
                .module
                .record_field_helper(value.ty, true)
                .expect("owned values have clone support");
            let result = self.fresh();
            self.instruction(format!(
                "{result} = call ptr {helper}(ptr {})",
                value.operand
            ));
            Value::new(value.ty, result).owned()
        } else {
            value
        }
    }

    fn release(&mut self, value: &Value) {
        if value.owned {
            let helper = self
                .module
                .record_field_helper(value.ty, false)
                .expect("owned values have cleanup support");
            self.instruction(format!("call void {helper}(ptr {})", value.operand));
        }
    }

    fn drop_locals(&mut self, all: bool) {
        let first = if all {
            0
        } else {
            self.scopes.len().saturating_sub(1)
        };
        self.drop_scopes(first);
    }

    fn drop_scopes(&mut self, first: usize) {
        let locals: Vec<_> = self
            .scopes
            .iter()
            .skip(first)
            .rev()
            .flat_map(|scope| {
                scope
                    .values()
                    .filter(|local| local.owned && local.initialized)
                    .cloned()
            })
            .collect();
        for local in locals {
            let pointer = self.fresh();
            self.instruction(format!("{pointer} = load ptr, ptr {}", local.pointer));
            self.release(&Value::new(local.ty, pointer).owned());
        }
    }

    fn block(&mut self, statements: &Block, scoped: bool) -> Result<()> {
        if scoped {
            self.scopes.push(BTreeMap::new());
        }
        for statement in statements {
            if self.terminated {
                return Err(error(
                    "unreachable statement in checked AST",
                    statement.span,
                ));
            }
            self.statement(statement)?;
        }
        if scoped {
            if !self.terminated {
                self.drop_locals(false);
            }
            self.scopes.pop();
        }
        Ok(())
    }

    fn statement(&mut self, statement: &Stmt) -> Result<()> {
        match &statement.kind {
            StmtKind::Var {
                name,
                reassignable,
                annotation,
                value,
            } => {
                let value = self.expression(value)?;
                if let Some(ty) = annotation {
                    value.require(*ty, statement.span)?;
                }
                let value = self.acquire(value);
                self.bind(name, &value, statement.span, true, *reassignable)?;
            }
            StmtKind::TryVar {
                name,
                reassignable,
                annotation,
                value,
                try_span,
                success_type,
                ..
            } => {
                self.try_initializer(
                    name,
                    value,
                    *annotation,
                    *success_type,
                    *try_span,
                    *reassignable,
                )?;
            }
            StmtKind::Assign { name, value } => {
                let local = self.local(name, statement.span)?;
                if !local.reassignable {
                    return Err(error(
                        format!("cannot reassign immutable binding '{name}' in checked AST"),
                        statement.span,
                    ));
                }
                if local.ty.is_owned() && !local.owned {
                    return Err(error("cannot rebind a borrowed parameter", statement.span));
                }
                let value = self.expression(value)?;
                value.require(local.ty, statement.span)?;
                let value = self.acquire(value);
                // Evaluating an owned call on the RHS may have moved this target.
                let local = self.local(name, statement.span)?;
                if local.owned && local.initialized {
                    let old = self.fresh();
                    self.instruction(format!("{old} = load ptr, ptr {}", local.pointer));
                    self.release(&Value::new(local.ty, old).owned());
                }
                self.instruction(format!(
                    "store {} {}, ptr {}",
                    llvm_type(local.ty),
                    value.operand,
                    local.pointer
                ));
                for scope in self.scopes.iter_mut().rev() {
                    if let Some(binding) = scope.get_mut(name) {
                        binding.initialized = true;
                        break;
                    }
                }
            }
            StmtKind::FieldAssign { target, value } => {
                // A call on the RHS can replace any enclosing field. Resolve
                // the destination only after acquiring the RHS so neither a
                // stale destination nor a borrowed RHS survives destruction.
                let value = self.expression(value)?;
                let value = self.acquire(value);
                let (pointer, ty) = self.field_destination(target)?;
                value.require(ty, statement.span)?;
                if ty.is_owned() {
                    let previous = self.fresh();
                    self.instruction(format!("{previous} = load ptr, ptr {pointer}"));
                    self.release(&Value::new(ty, previous).owned());
                }
                self.instruction(format!(
                    "store {} {}, ptr {pointer}",
                    llvm_type(ty),
                    value.operand
                ));
            }
            StmtKind::Expr(expression) => {
                let value = self.expression(expression)?;
                if let Type::Enum(id) = value.ty {
                    if self
                        .module
                        .enumeration(id, expression.span)?
                        .result_shape()
                        .is_some()
                    {
                        return Err(error(
                            "discarded result-shaped enum in checked AST",
                            expression.span,
                        ));
                    }
                }
                self.release(&value);
            }
            StmtKind::Return(expression) => {
                if let Some(expression) = expression {
                    let value = self.expression(expression)?;
                    if value.ty == Type::Void {
                        return Err(error("cannot return a void expression", statement.span));
                    }
                    value.require(self.return_type, statement.span)?;
                    let value = self.acquire(value);
                    self.drop_locals(true);
                    self.terminate(format!("ret {} {}", llvm_type(value.ty), value.operand));
                } else if self.return_type == Type::Void {
                    self.drop_locals(true);
                    self.terminate("ret void");
                } else {
                    return Err(error("non-void return needs a value", statement.span));
                }
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let value = self.expression(condition)?;
                value.require(Type::Bool, condition.span)?;
                let then_label = self.label();
                let else_label = self.label();
                let merge_label = self.label();
                let false_label = if else_block.is_some() {
                    &else_label
                } else {
                    &merge_label
                };
                self.terminate(format!(
                    "br i1 {}, label %{then_label}, label %{false_label}",
                    value.operand
                ));
                self.start_block(&then_label);
                self.block(then_block, true)?;
                let then_returns = self.terminated;
                if !then_returns {
                    self.terminate(format!("br label %{merge_label}"));
                }
                let mut else_returns = false;
                if let Some(else_block) = else_block {
                    self.start_block(&else_label);
                    self.block(else_block, true)?;
                    else_returns = self.terminated;
                    if !else_returns {
                        self.terminate(format!("br label %{merge_label}"));
                    }
                }
                if !then_returns || !else_returns {
                    self.start_block(&merge_label);
                }
            }
            StmtKind::Match { value, arms } => {
                let value = self.expression(value)?;
                let Type::Enum(id) = value.ty else {
                    return Err(error("match requires an enum", statement.span));
                };
                let declaration = self.module.enumeration(id, statement.span)?.clone();
                let mut seen = BTreeSet::new();
                let mut cases = Vec::new();
                for arm in arms {
                    let (index, variant) = declaration
                        .variants
                        .iter()
                        .enumerate()
                        .find(|(_, variant)| variant.name == arm.variant)
                        .ok_or_else(|| error("unknown match variant in checked AST", arm.span))?;
                    if arm.enum_name != declaration.name
                        || !seen.insert(index)
                        || arm.bindings.len() != variant.fields.len()
                        || arm
                            .bindings
                            .iter()
                            .zip(&variant.fields)
                            .any(|(binding, field)| binding.ty != Some(field.ty))
                    {
                        return Err(error("inconsistent match pattern in checked AST", arm.span));
                    }
                    cases.push((index, variant.clone(), arm));
                }
                if seen.len() != declaration.variants.len() {
                    return Err(error("non-exhaustive match in checked AST", statement.span));
                }
                // Own the discriminated value before executing any arm. Each arm
                // copies its payload bindings before disposing of this snapshot.
                let value = self.acquire(value);
                let tag_pointer = self.fresh();
                self.instruction(format!(
                    "{tag_pointer} = getelementptr %bhasa.enum.{id}, ptr {}, i32 0, i32 0",
                    value.operand
                ));
                let tag = self.fresh();
                self.instruction(format!("{tag} = load i64, ptr {tag_pointer}"));
                let payload_pointer = self.fresh();
                self.instruction(format!(
                    "{payload_pointer} = getelementptr %bhasa.enum.{id}, ptr {}, i32 0, i32 1",
                    value.operand
                ));
                let payload = self.fresh();
                self.instruction(format!("{payload} = load ptr, ptr {payload_pointer}"));
                let invalid = self.label();
                let merge = self.label();
                let labels: Vec<_> = cases.iter().map(|_| self.label()).collect();
                let mut switch = format!("switch i64 {tag}, label %{invalid} [");
                for ((index, _, _), label) in cases.iter().zip(&labels) {
                    switch.push_str(&format!(" i64 {index}, label %{label}"));
                }
                switch.push_str(" ]");
                self.terminate(switch);
                self.start_block(&invalid);
                self.module.needs_trap = true;
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");
                let mut reaches_merge = false;
                for ((index, variant, arm), label) in cases.into_iter().zip(labels) {
                    self.start_block(&label);
                    self.scopes.push(BTreeMap::new());
                    for (field_index, (binding, field)) in
                        arm.bindings.iter().zip(&variant.fields).enumerate()
                    {
                        let pointer = self.fresh();
                        self.instruction(format!("{pointer} = getelementptr %bhasa.enum.{id}.variant.{index}, ptr {payload}, i32 0, i32 {field_index}"));
                        let loaded = self.fresh();
                        self.instruction(format!(
                            "{loaded} = load {}, ptr {pointer}",
                            llvm_type(field.ty)
                        ));
                        let binding_value = self.acquire(Value::new(field.ty, loaded));
                        self.bind(&binding.name, &binding_value, binding.span, true, true)?;
                    }
                    self.release(&value);
                    self.block(&arm.body, false)?;
                    if !self.terminated {
                        self.drop_locals(false);
                        self.terminate(format!("br label %{merge}"));
                        reaches_merge = true;
                    }
                    self.scopes.pop();
                }
                if reaches_merge {
                    self.start_block(&merge);
                }
            }
            StmtKind::While { condition, body } => {
                let condition_label = self.label();
                let body_label = self.label();
                let exit_label = self.label();
                self.terminate(format!("br label %{condition_label}"));
                self.start_block(&condition_label);
                let value = self.expression(condition)?;
                value.require(Type::Bool, condition.span)?;
                self.terminate(format!(
                    "br i1 {}, label %{body_label}, label %{exit_label}",
                    value.operand
                ));
                self.start_block(&body_label);
                self.loops.push(LoopTarget {
                    scope: self.scopes.len(),
                    next: condition_label.clone(),
                    exit: exit_label.clone(),
                });
                self.block(body, true)?;
                self.loops.pop();
                if !self.terminated {
                    self.terminate(format!("br label %{condition_label}"));
                }
                self.start_block(&exit_label);
            }
            StmtKind::For {
                name,
                name_span,
                source,
                element_type,
                body,
            } => {
                self.for_loop(name, *name_span, source, *element_type, body)?;
            }
            StmtKind::Break | StmtKind::Continue => {
                let target = self.loops.last().ok_or_else(|| {
                    error(
                        "loop exit without enclosing loop in checked AST",
                        statement.span,
                    )
                })?;
                let first = target.scope;
                let label = if matches!(statement.kind, StmtKind::Break) {
                    target.exit.clone()
                } else {
                    target.next.clone()
                };
                self.drop_scopes(first);
                self.terminate(format!("br label %{label}"));
            }
        }
        Ok(())
    }

    fn try_initializer(
        &mut self,
        name: &str,
        expression: &Expr,
        annotation: Option<Type>,
        success_type: Option<Type>,
        span: Span,
        reassignable: bool,
    ) -> Result<()> {
        let value = self.expression(expression)?;
        let Type::Enum(source_id) = value.ty else {
            return Err(error(
                "try operand must be a result-shaped enum in checked AST",
                span,
            ));
        };
        let Type::Enum(target_id) = self.return_type else {
            return Err(error(
                "try caller must return a result-shaped enum in checked AST",
                span,
            ));
        };
        let source = self
            .module
            .enumeration(source_id, span)?
            .result_shape()
            .ok_or_else(|| error("invalid try operand enum shape in checked AST", span))?;
        let target = self
            .module
            .enumeration(target_id, span)?
            .result_shape()
            .ok_or_else(|| error("invalid try return enum shape in checked AST", span))?;
        if source.error != target.error
            || success_type != Some(source.success)
            || annotation.is_some_and(|ty| ty != source.success)
        {
            return Err(error("inconsistent try payload types in checked AST", span));
        }
        self.module.enum_helpers.insert(source_id);
        self.module.enum_helpers.insert(target_id);
        // This statement owns one SSA snapshot. No hidden source identifier is
        // introduced, since `try` remains a legal ordinary identifier.
        let value = self.acquire(value);
        let tag_pointer = self.fresh();
        self.instruction(format!(
            "{tag_pointer} = getelementptr %bhasa.enum.{source_id}, ptr {}, i32 0, i32 0",
            value.operand
        ));
        let tag = self.fresh();
        self.instruction(format!("{tag} = load i64, ptr {tag_pointer}"));
        let payload_pointer = self.fresh();
        self.instruction(format!(
            "{payload_pointer} = getelementptr %bhasa.enum.{source_id}, ptr {}, i32 0, i32 1",
            value.operand
        ));
        let payload = self.fresh();
        self.instruction(format!("{payload} = load ptr, ptr {payload_pointer}"));
        let success = self.label();
        let failure = self.label();
        let invalid = self.label();
        self.terminate(format!("switch i64 {tag}, label %{invalid} [ i64 {}, label %{success} i64 {}, label %{failure} ]", source.ok_index, source.error_index));
        self.start_block(&invalid);
        self.module.needs_trap = true;
        self.instruction("call void @llvm.trap()");
        self.terminate("unreachable");

        self.start_block(&failure);
        let field = self.fresh();
        self.instruction(format!("{field} = getelementptr %bhasa.enum.{source_id}.variant.{}, ptr {payload}, i32 0, i32 0", source.error_index));
        let loaded = self.fresh();
        self.instruction(format!(
            "{loaded} = load {}, ptr {field}",
            llvm_type(source.error)
        ));
        // Acquire before destroying either the source enum or caller locals.
        let failure_value = self.acquire(Value::new(source.error, loaded));
        let target_payload = self.fresh();
        self.instruction(format!(
            "{target_payload} = call ptr @bhasa.enum.{target_id}.variant.{}.new()",
            target.error_index
        ));
        let field = self.fresh();
        self.instruction(format!("{field} = getelementptr %bhasa.enum.{target_id}.variant.{}, ptr {target_payload}, i32 0, i32 0", target.error_index));
        self.instruction(format!(
            "store {} {}, ptr {field}",
            llvm_type(target.error),
            failure_value.operand
        ));
        let result = self.fresh();
        self.instruction(format!(
            "{result} = call ptr @bhasa.enum.{target_id}.new(i64 {}, ptr {target_payload})",
            target.error_index
        ));
        self.release(&value);
        self.drop_locals(true);
        self.terminate(format!("ret ptr {result}"));

        self.start_block(&success);
        let field = self.fresh();
        self.instruction(format!("{field} = getelementptr %bhasa.enum.{source_id}.variant.{}, ptr {payload}, i32 0, i32 0", source.ok_index));
        let loaded = self.fresh();
        self.instruction(format!(
            "{loaded} = load {}, ptr {field}",
            llvm_type(source.success)
        ));
        let success_value = self.acquire(Value::new(source.success, loaded));
        self.release(&value);
        self.bind(name, &success_value, span, true, reassignable)?;
        Ok(())
    }

    fn for_loop(
        &mut self,
        name: &str,
        span: Span,
        source: &ForSource,
        element_type: Option<Type>,
        body: &Block,
    ) -> Result<()> {
        // The private scope holds the iteration snapshot through every continue.
        // Return drops it with all locals; break drops it at the common exit.
        self.scopes.push(BTreeMap::new());
        enum Iteration {
            Indexed {
                snapshot: String,
                get: String,
                element: Type,
            },
            Map {
                snapshot: String,
                output: String,
                element: Type,
            },
        }
        let (start, end, iterable, step) = match source {
            ForSource::Range { start, end, step } => {
                let start = self.expression(start)?;
                start.require(Type::Int, span)?;
                let end = self.expression(end)?;
                end.require(Type::Int, span)?;
                if element_type != Some(Type::Int) {
                    return Err(error("invalid range binding type in checked AST", span));
                }
                let step = if let Some(step) = step {
                    let step = self.expression(step)?;
                    step.require(Type::Int, span)?;
                    // Validate before checking either direction, even for an
                    // empty range. Operand evaluation follows start/end/step.
                    let zero = self.fresh();
                    self.instruction(format!("{zero} = icmp eq i64 {}, 0", step.operand));
                    self.trap_if(&zero);
                    let ascending = self.fresh();
                    self.instruction(format!("{ascending} = icmp sgt i64 {}, 0", step.operand));
                    Some((step.operand, ascending))
                } else {
                    None
                };
                (start.operand, end.operand, None, step)
            }
            ForSource::Iterable(expression) => {
                let value = self.expression(expression)?;
                let (element, count, get) = match value.ty {
                    Type::Buffer | Type::Ints => {
                        let kind = if value.ty == Type::Buffer {
                            "buffer"
                        } else {
                            "ints"
                        };
                        let count = format!("@bhasa_rt_{kind}_len");
                        let get = format!("@bhasa_rt_{kind}_get");
                        self.module
                            .runtime
                            .insert(format!("declare i64 {count}(ptr)"));
                        self.module
                            .runtime
                            .insert(format!("declare i64 {get}(ptr, i64)"));
                        (Type::Int, Some(count), Some(get))
                    }
                    Type::Record(id) => {
                        let schema = crate::collections::schema(self.module.record(id, span)?)
                            .map_err(|message| error(message, span))?;
                        let result = match schema {
                            Some(crate::collections::Schema::List(element)) => (
                                element,
                                Some(format!("@bhasa.method.{id}.count")),
                                Some(format!("@bhasa.method.{id}.get")),
                            ),
                            Some(crate::collections::Schema::Map(key, _)) => (key, None, None),
                            None => {
                                return Err(error("invalid iterable type in checked AST", span));
                            }
                        };
                        self.module.record_helpers.insert(id);
                        result
                    }
                    _ => return Err(error("invalid iterable type in checked AST", span)),
                };
                if element_type != Some(element) {
                    return Err(error("invalid iterable binding type in checked AST", span));
                }
                let value = self.acquire(value);
                self.bind("bhasa_rt_iteration_snapshot", &value, span, true, true)?;
                if let (Some(count), Some(get)) = (count, get) {
                    let end = self.fresh();
                    self.instruction(format!("{end} = call i64 {count}(ptr {})", value.operand));
                    (
                        "0".into(),
                        end,
                        Some(Iteration::Indexed {
                            snapshot: value.operand,
                            get,
                            element,
                        }),
                        None,
                    )
                } else {
                    self.module
                        .runtime
                        .insert("declare i64 @bhasa_rt_map_next_key(ptr, ptr, ptr)".into());
                    let output = self.fresh();
                    // The opaque runtime ABI requires an eight-byte key slot,
                    // including when a str pointer is narrower than i64.
                    self.allocations
                        .push_str(&format!("  {output} = alloca i64, align 8\n"));
                    (
                        "0".into(),
                        "0".into(),
                        Some(Iteration::Map {
                            snapshot: value.operand,
                            output,
                            element,
                        }),
                        None,
                    )
                }
            }
        };
        let cursor = self.fresh();
        self.allocations
            .push_str(&format!("  {cursor} = alloca i64, align 8\n"));
        self.instruction(format!("store i64 {start}, ptr {cursor}"));
        let condition = self.label();
        let iteration = self.label();
        let advance = self.label();
        let exit = self.label();
        self.terminate(format!("br label %{condition}"));
        self.start_block(&condition);
        let index = self.fresh();
        self.instruction(format!("{index} = load i64, ptr {cursor}"));
        let valid = self.fresh();
        if let Some(Iteration::Map {
            snapshot, output, ..
        }) = &iterable
        {
            let found = self.fresh();
            self.instruction(format!("{found} = call i64 @bhasa_rt_map_next_key(ptr {snapshot}, ptr {cursor}, ptr {output})"));
            self.instruction(format!("{valid} = icmp ne i64 {found}, 0"));
        } else if let Some((_, ascending)) = &step {
            let below = self.fresh();
            let above = self.fresh();
            self.instruction(format!("{below} = icmp slt i64 {index}, {end}"));
            self.instruction(format!("{above} = icmp sgt i64 {index}, {end}"));
            self.instruction(format!(
                "{valid} = select i1 {ascending}, i1 {below}, i1 {above}"
            ));
        } else {
            self.instruction(format!("{valid} = icmp slt i64 {index}, {end}"));
        }
        self.terminate(format!("br i1 {valid}, label %{iteration}, label %{exit}"));
        self.start_block(&iteration);
        self.loops.push(LoopTarget {
            scope: self.scopes.len(),
            next: advance.clone(),
            exit: exit.clone(),
        });
        self.scopes.push(BTreeMap::new());
        let map_iteration = matches!(iterable, Some(Iteration::Map { .. }));
        let value = match iterable {
            Some(Iteration::Indexed {
                snapshot,
                get,
                element,
            }) => {
                let value = self.fresh();
                self.instruction(format!(
                    "{value} = call {} {get}(ptr {snapshot}, i64 {index})",
                    llvm_type(element)
                ));
                Value::new(element, value).owned()
            }
            Some(Iteration::Map {
                output, element, ..
            }) => {
                // Exhaustion leaves output uninitialized. This block is only
                // reachable after next_key succeeds; never load it beforehand.
                let value = self.fresh();
                self.instruction(format!(
                    "{value} = load {}, ptr {output}",
                    llvm_type(element)
                ));
                Value::new(element, value).owned()
            }
            None => Value::new(Type::Int, index),
        };
        self.bind(name, &value, span, true, true)?;
        self.block(body, false)?;
        if !self.terminated {
            self.drop_locals(false);
            self.terminate(format!("br label %{advance}"));
        }
        self.scopes.pop();
        self.loops.pop();
        self.start_block(&advance);
        if map_iteration {
            // The runtime scan advances the cursor and may skip empty buckets.
            self.terminate(format!("br label %{condition}"));
        } else {
            let previous = self.fresh();
            let next = self.fresh();
            self.instruction(format!("{previous} = load i64, ptr {cursor}"));
            if let Some((step, _)) = step {
                self.module.intrinsics.insert("sadd");
                let pair = self.fresh();
                let overflow = self.fresh();
                let store = self.label();
                self.instruction(format!("{pair} = call {{ i64, i1 }} @llvm.sadd.with.overflow.i64(i64 {previous}, i64 {step})"));
                self.instruction(format!("{next} = extractvalue {{ i64, i1 }} {pair}, 0"));
                self.instruction(format!("{overflow} = extractvalue {{ i64, i1 }} {pair}, 1"));
                // An out-of-range cursor ends iteration; user arithmetic still traps.
                self.terminate(format!("br i1 {overflow}, label %{exit}, label %{store}"));
                self.start_block(&store);
            } else {
                // Unit ranges and sequence indices satisfy index < end <= INT64_MAX.
                self.instruction(format!("{next} = add nsw i64 {previous}, 1"));
            }
            self.instruction(format!("store i64 {next}, ptr {cursor}"));
            self.terminate(format!("br label %{condition}"));
        }
        self.start_block(&exit);
        self.drop_locals(false);
        self.scopes.pop();
        Ok(())
    }

    fn expression(&mut self, expression: &Expr) -> Result<Value> {
        let expected = expression
            .ty
            .ok_or_else(|| error("expression has no checked type", expression.span))?;
        let value = match &expression.kind {
            ExprKind::Int(value) => Value::new(Type::Int, value.to_string()),
            ExprKind::Float(value) => {
                if !value.is_finite() {
                    return Err(error("float literals must be finite", expression.span));
                }
                Value::new(Type::Float, format!("0x{:016X}", value.to_bits()))
            }
            ExprKind::Bool(value) => Value::new(Type::Bool, if *value { "true" } else { "false" }),
            ExprKind::Str(value) => {
                if value.contains('\0') {
                    return Err(error("string contains an embedded NUL", expression.span));
                }
                Value::new(Type::Str, self.module.string(value))
            }
            ExprKind::Move { name, .. } => {
                let local = self.local(name, expression.span)?;
                if !local.owned
                    || !local.initialized
                    || !matches!(local.ty, Type::Buffer | Type::Ints)
                {
                    return Err(Diagnostic::new(
                        "S038",
                        "invalid owned move in checked AST",
                        expression.span,
                    ));
                }
                let result = self.fresh();
                self.instruction(format!("{result} = load ptr, ptr {}", local.pointer));
                self.scopes
                    .iter_mut()
                    .rev()
                    .find_map(|scope| scope.get_mut(name))
                    .unwrap()
                    .initialized = false;
                Value::new(local.ty, result).owned()
            }
            ExprKind::Name(name) => {
                let local = self.local(name, expression.span)?;
                if !local.initialized {
                    return Err(Diagnostic::new(
                        "S038",
                        "read of moved binding in checked AST",
                        expression.span,
                    ));
                }
                let result = self.fresh();
                self.instruction(format!(
                    "{result} = load {}, ptr {}",
                    llvm_type(local.ty),
                    local.pointer
                ));
                let mut value = Value::new(local.ty, result);
                value.task = local.task;
                value
            }
            ExprKind::Field { base, name } => {
                let base = self.expression(base)?;
                let (id, index, ty) = self.module.field(base.ty, name, expression.span)?;
                let pointer = self.fresh();
                self.instruction(format!(
                    "{pointer} = getelementptr %bhasa.record.{id}, ptr {}, i32 0, i32 {index}",
                    base.operand
                ));
                let result = self.fresh();
                self.instruction(format!("{result} = load {}, ptr {pointer}", llvm_type(ty)));
                let mut value = Value::new(ty, result);
                if base.owned {
                    // A field of a temporary must outlive that temporary.
                    value = self.acquire(value);
                    self.release(&base);
                }
                value
            }
            ExprKind::Unary { op, operand } => {
                let operand = self.expression(operand)?;
                match op {
                    UnaryOp::Neg if operand.ty == Type::Float => {
                        let result = self.fresh();
                        self.instruction(format!("{result} = fneg double {}", operand.operand));
                        Value::new(Type::Float, result)
                    }
                    UnaryOp::Neg => {
                        operand.require(Type::Int, expression.span)?;
                        self.overflow("ssub", "0", &operand.operand)
                    }
                    UnaryOp::Not => {
                        operand.require(Type::Bool, expression.span)?;
                        let result = self.fresh();
                        self.instruction(format!("{result} = xor i1 {}, true", operand.operand));
                        Value::new(Type::Bool, result)
                    }
                    UnaryOp::BitNot => {
                        operand.require(Type::Int, expression.span)?;
                        let result = self.fresh();
                        self.instruction(format!("{result} = xor i64 {}, -1", operand.operand));
                        Value::new(Type::Int, result)
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.short_circuit(*op, left, right)?
                } else {
                    let left = self.expression(left)?;
                    let right = self.expression(right)?;
                    self.binary(*op, left, right, expression.span)?
                }
            }
            ExprKind::Call { name, args } => self.call(name, args, expression.span)?,
            ExprKind::MethodCall { .. } => {
                return Err(error(
                    "unresolved method call in checked AST",
                    expression.span,
                ));
            }
        };
        value.require(expected, expression.span)?;
        Ok(value)
    }

    fn field_destination(&mut self, target: &Expr) -> Result<(String, Type)> {
        let ExprKind::Field { base, name } = &target.kind else {
            return Err(error(
                "field assignment requires a field target",
                target.span,
            ));
        };
        fn has_local_root(expression: &Expr) -> bool {
            match &expression.kind {
                ExprKind::Name(_) => true,
                ExprKind::Field { base, .. } => has_local_root(base),
                _ => false,
            }
        }
        if !has_local_root(base) {
            return Err(error(
                "field assignment requires a local or borrowed record",
                target.span,
            ));
        }
        let base = self.expression(base)?;
        let (id, index, ty) = self.module.field(base.ty, name, target.span)?;
        let pointer = self.fresh();
        self.instruction(format!(
            "{pointer} = getelementptr %bhasa.record.{id}, ptr {}, i32 0, i32 {index}",
            base.operand
        ));
        Ok((pointer, ty))
    }

    fn overflow(&mut self, intrinsic: &'static str, left: &str, right: &str) -> Value {
        self.module.intrinsics.insert(intrinsic);
        let pair = self.fresh();
        let result = self.fresh();
        let overflow = self.fresh();
        self.instruction(format!("{pair} = call {{ i64, i1 }} @llvm.{intrinsic}.with.overflow.i64(i64 {left}, i64 {right})"));
        self.instruction(format!("{result} = extractvalue {{ i64, i1 }} {pair}, 0"));
        self.instruction(format!("{overflow} = extractvalue {{ i64, i1 }} {pair}, 1"));
        self.trap_if(&overflow);
        Value::new(Type::Int, result)
    }

    fn trap_if(&mut self, condition: &str) {
        self.module.needs_trap = true;
        let trap = self.label();
        let continuation = self.label();
        self.terminate(format!(
            "br i1 {condition}, label %{trap}, label %{continuation}"
        ));
        self.start_block(&trap);
        self.instruction("call void @llvm.trap()");
        self.terminate("unreachable");
        self.start_block(&continuation);
    }

    fn binary(&mut self, op: BinaryOp, left: Value, right: Value, span: Span) -> Result<Value> {
        if left.ty == Type::Float {
            right.require(Type::Float, span)?;
            let (instruction, result_type) = match op {
                BinaryOp::Add => ("fadd", Type::Float),
                BinaryOp::Sub => ("fsub", Type::Float),
                BinaryOp::Mul => ("fmul", Type::Float),
                BinaryOp::Div => ("fdiv", Type::Float),
                BinaryOp::Eq => ("fcmp oeq", Type::Bool),
                BinaryOp::Ne => ("fcmp une", Type::Bool),
                BinaryOp::Lt => ("fcmp olt", Type::Bool),
                BinaryOp::Le => ("fcmp ole", Type::Bool),
                BinaryOp::Gt => ("fcmp ogt", Type::Bool),
                BinaryOp::Ge => ("fcmp oge", Type::Bool),
                _ => return Err(error("operator is not supported for float", span)),
            };
            let result = self.fresh();
            self.instruction(format!(
                "{result} = {instruction} double {}, {}",
                left.operand, right.operand
            ));
            return Ok(Value::new(result_type, result));
        }
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            if !matches!(left.ty, Type::Int | Type::Bool) {
                return Err(error("equality requires int or bool operands", span));
            }
            right.require(left.ty, span)?;
        } else {
            left.require(Type::Int, span)?;
            right.require(Type::Int, span)?;
        }
        match op {
            BinaryOp::Add => Ok(self.overflow("sadd", &left.operand, &right.operand)),
            BinaryOp::Sub => Ok(self.overflow("ssub", &left.operand, &right.operand)),
            BinaryOp::Mul => Ok(self.overflow("smul", &left.operand, &right.operand)),
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                if matches!(op, BinaryOp::Shl | BinaryOp::Shr) {
                    let invalid = self.fresh();
                    // The unsigned comparison also catches negative counts.
                    self.instruction(format!("{invalid} = icmp ugt i64 {}, 63", right.operand));
                    self.trap_if(&invalid);
                }
                let instruction = match op {
                    BinaryOp::BitAnd => "and",
                    BinaryOp::BitOr => "or",
                    BinaryOp::BitXor => "xor",
                    BinaryOp::Shl => "shl",
                    BinaryOp::Shr => "ashr",
                    _ => unreachable!(),
                };
                let result = self.fresh();
                self.instruction(format!(
                    "{result} = {instruction} i64 {}, {}",
                    left.operand, right.operand
                ));
                Ok(Value::new(Type::Int, result))
            }
            BinaryOp::Div | BinaryOp::Rem => {
                let zero = self.fresh();
                let minimum = self.fresh();
                let negative_one = self.fresh();
                let overflow = self.fresh();
                let invalid = self.fresh();
                self.instruction(format!("{zero} = icmp eq i64 {}, 0", right.operand));
                self.instruction(format!(
                    "{minimum} = icmp eq i64 {}, -9223372036854775808",
                    left.operand
                ));
                self.instruction(format!(
                    "{negative_one} = icmp eq i64 {}, -1",
                    right.operand
                ));
                self.instruction(format!("{overflow} = and i1 {minimum}, {negative_one}"));
                self.instruction(format!("{invalid} = or i1 {zero}, {overflow}"));
                self.trap_if(&invalid);
                let result = self.fresh();
                let instruction = if op == BinaryOp::Div { "sdiv" } else { "srem" };
                self.instruction(format!(
                    "{result} = {instruction} i64 {}, {}",
                    left.operand, right.operand
                ));
                Ok(Value::new(Type::Int, result))
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let predicate = match op {
                    BinaryOp::Eq => "eq",
                    BinaryOp::Ne => "ne",
                    BinaryOp::Lt => "slt",
                    BinaryOp::Le => "sle",
                    BinaryOp::Gt => "sgt",
                    BinaryOp::Ge => "sge",
                    _ => return Err(error("invalid comparison", span)),
                };
                let result = self.fresh();
                self.instruction(format!(
                    "{result} = icmp {predicate} {} {}, {}",
                    llvm_type(left.ty),
                    left.operand,
                    right.operand
                ));
                Ok(Value::new(Type::Bool, result))
            }
            BinaryOp::And | BinaryOp::Or => Err(error("invalid eager logical operation", span)),
        }
    }

    fn short_circuit(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Result<Value> {
        let left_value = self.expression(left)?;
        left_value.require(Type::Bool, left.span)?;
        let left_block = self.current_block.clone();
        let right_label = self.label();
        let merge_label = self.label();
        let (when_true, when_false, skipped) = if op == BinaryOp::And {
            (&right_label, &merge_label, "false")
        } else {
            (&merge_label, &right_label, "true")
        };
        self.terminate(format!(
            "br i1 {}, label %{when_true}, label %{when_false}",
            left_value.operand
        ));
        self.start_block(&right_label);
        let right_value = self.expression(right)?;
        right_value.require(Type::Bool, right.span)?;
        // RHS evaluation can introduce nested short-circuit or overflow blocks.
        let right_block = self.current_block.clone();
        self.terminate(format!("br label %{merge_label}"));
        self.start_block(&merge_label);
        let result = self.fresh();
        self.instruction(format!(
            "{result} = phi i1 [ {skipped}, %{left_block} ], [ {}, %{right_block} ]",
            right_value.operand
        ));
        Ok(Value::new(Type::Bool, result))
    }

    fn call(&mut self, name: &str, args: &[Expr], span: Span) -> Result<Value> {
        if let Some((owner, variant_name)) = name.split_once('.') {
            if let Some(id) = self
                .module
                .program
                .type_names
                .iter()
                .position(|candidate| candidate == owner)
            {
                if let Ok(record) = self.module.record(id, span) {
                    if let Some(schema) = crate::collections::schema(record)
                        .map_err(|message| error(message, span))?
                    {
                        let (mut params, result_type) = schema
                            .method(variant_name)
                            .ok_or_else(|| error("unknown collection method", span))?;
                        params.insert(0, Type::Record(id));
                        if args.len() != params.len() {
                            return Err(error("wrong collection method argument count", span));
                        }
                        let mut values = Vec::new();
                        for (argument, ty) in args.iter().zip(params) {
                            let value = self.expression(argument)?;
                            value.require(ty, argument.span)?;
                            values.push(value);
                        }
                        self.module.record_helpers.insert(id);
                        let arguments = values
                            .iter()
                            .map(|value| format!("{} {}", llvm_type(value.ty), value.operand))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let call = format!(
                            "call {} @bhasa.method.{id}.{variant_name}({arguments})",
                            llvm_type(result_type)
                        );
                        let result = if result_type == Type::Void {
                            self.instruction(call);
                            Value::new(Type::Void, "")
                        } else {
                            let result = self.fresh();
                            self.instruction(format!("{result} = {call}"));
                            Value::new(result_type, result).owned()
                        };
                        for value in &values {
                            self.release(value);
                        }
                        return Ok(result);
                    }
                }
                if let Some(declaration) = self.module.program.enum_decl(id) {
                    let (index, variant) = declaration
                        .variants
                        .iter()
                        .enumerate()
                        .find(|(_, variant)| variant.name == variant_name)
                        .ok_or_else(|| error("unknown enum constructor in checked AST", span))?;
                    let variant = variant.clone();
                    if args.len() != variant.fields.len() {
                        return Err(error(
                            "wrong enum constructor argument count in checked AST",
                            span,
                        ));
                    }
                    let mut fields = Vec::new();
                    for (argument, field) in args.iter().zip(&variant.fields) {
                        let value = self.expression(argument)?;
                        value.require(field.ty, argument.span)?;
                        fields.push(self.acquire(value));
                    }
                    self.module.enum_helpers.insert(id);
                    let payload = if fields.is_empty() {
                        "null".to_owned()
                    } else {
                        let payload = self.fresh();
                        self.instruction(format!(
                            "{payload} = call ptr @bhasa.enum.{id}.variant.{index}.new()"
                        ));
                        payload
                    };
                    for (field_index, field) in fields.iter().enumerate() {
                        let pointer = self.fresh();
                        self.instruction(format!("{pointer} = getelementptr %bhasa.enum.{id}.variant.{index}, ptr {payload}, i32 0, i32 {field_index}"));
                        self.instruction(format!(
                            "store {} {}, ptr {pointer}",
                            llvm_type(field.ty),
                            field.operand
                        ));
                    }
                    let result = self.fresh();
                    self.instruction(format!(
                        "{result} = call ptr @bhasa.enum.{id}.new(i64 {index}, ptr {payload})"
                    ));
                    return Ok(Value::new(Type::Enum(id), result).owned());
                }
            }
        }
        if let Some(id) = self
            .module
            .program
            .type_names
            .iter()
            .position(|candidate| candidate == name)
        {
            let record = self.module.record(id, span)?.clone();
            if crate::collections::schema(&record)
                .map_err(|message| error(message, span))?
                .is_some()
            {
                if !args.is_empty() {
                    return Err(error("collection constructor expects no arguments", span));
                }
                self.module.record_helpers.insert(id);
                let result = self.fresh();
                self.instruction(format!("{result} = call ptr @bhasa.record.{id}.new()"));
                return Ok(Value::new(Type::Record(id), result).owned());
            }
            if args.len() != record.fields.len() {
                return Err(error(
                    format!("wrong constructor argument count for '{name}' in checked AST"),
                    span,
                ));
            }
            let mut fields = Vec::new();
            for (argument, field) in args.iter().zip(&record.fields) {
                let value = self.expression(argument)?;
                value.require(field.ty, argument.span)?;
                // Acquire each field before a later argument can mutate the
                // borrowed record from which this argument was read.
                fields.push(self.acquire(value));
            }
            self.module.record_helpers.insert(id);
            let result = self.fresh();
            self.instruction(format!("{result} = call ptr @bhasa.record.{id}.new()"));
            for (index, field) in fields.iter().enumerate() {
                let pointer = self.fresh();
                self.instruction(format!(
                    "{pointer} = getelementptr %bhasa.record.{id}, ptr {result}, i32 0, i32 {index}"
                ));
                self.instruction(format!(
                    "store {} {}, ptr {pointer}",
                    llvm_type(field.ty),
                    field.operand
                ));
            }
            return Ok(Value::new(Type::Record(id), result).owned());
        }
        if matches!(name, "spawn" | "async_spawn") {
            let [Expr {
                kind:
                    ExprKind::Call {
                        name: worker,
                        args: worker_args,
                    },
                ..
            }] = args
            else {
                return Err(error("spawn expects a checked worker call", span));
            };
            let signature = self
                .module
                .signatures
                .get(worker)
                .cloned()
                .ok_or_else(|| error("spawn worker has no checked signature", span))?;
            if !self
                .module
                .program
                .functions
                .iter()
                .any(|function| &function.name == worker && function.body.is_some())
                || signature.external
                || signature.params.len() != 1
                || matches!(signature.params[0], Type::Void | Type::Task)
                || signature.return_type == Type::Task
            {
                return Err(error(
                    "spawn requires a defined global one-argument Bhasa worker; method receivers are unsupported",
                    span,
                ));
            }
            let graph = crate::sema::call_graph(self.module.program);
            if let Some(operation) = crate::sema::file_worker_violation(
                self.module.program,
                self.module.signatures,
                &graph,
                worker,
            ) {
                return Err(error(
                    format!("worker reaches single-thread file resource '{operation}'"),
                    span,
                ));
            }
            // Repeat source-worker call-graph restrictions at the public
            // checked-AST boundary. Argument evaluation remains in the caller.
            let mut pending = vec![worker.as_str()];
            let mut visited = BTreeSet::new();
            while let Some(name) = pending.pop() {
                if !visited.insert(name) {
                    continue;
                }
                if self
                    .module
                    .signatures
                    .get(name)
                    .is_some_and(|sig| sig.external)
                {
                    return Err(error(
                        format!("worker reaches extern function '{name}', whose thread safety is unknown"),
                        span,
                    ));
                }
                if matches!(name, "async_spawn" | "await") {
                    return Err(error(
                        format!("worker reaches '{name}'; pooled scheduling or awaiting from workers could deadlock"),
                        span,
                    ));
                }
                if let Some(calls) = graph.get(name) {
                    pending.extend(calls.iter().map(String::as_str));
                }
            }
            let [argument] = worker_args.as_slice() else {
                return Err(error("worker requires one argument", span));
            };
            let argument = self.expression(argument)?;
            argument.require(signature.params[0], span)?;
            let info = TaskInfo {
                result: signature.return_type,
                owned_abi: signature.params != [Type::Int] || signature.return_type != Type::Int,
            };
            if !info.owned_abi {
                return Ok(self
                    .runtime_call(
                        if name == "async_spawn" {
                            "bhasa_rt_async_spawn_i64"
                        } else {
                            "bhasa_rt_spawn_i64"
                        },
                        &[Value::new(Type::Task, symbol(worker, false)), argument],
                        Type::Task,
                    )
                    .with_task(info));
            }
            // Acquire before submission. Borrowed aggregates clone; exclusive
            // temporaries transfer into the frame without retaining caller storage.
            let argument = self.acquire(argument);
            let frame = self.runtime_call(
                "bhasa_rt_task_alloc",
                &[Value::new(Type::Int, task_frame_size(argument.ty))],
                Type::Task,
            );
            self.instruction(format!(
                "store {} {}, ptr {}",
                llvm_type(argument.ty),
                argument.operand,
                frame.operand
            ));
            self.module.task_wrappers.insert(worker.clone());
            return Ok(self
                .runtime_call(
                    if name == "async_spawn" {
                        "bhasa_rt_async_spawn_owned"
                    } else {
                        "bhasa_rt_spawn_owned"
                    },
                    &[
                        Value::new(Type::Task, format!("@bhasa.task.{worker}")),
                        frame,
                    ],
                    Type::Task,
                )
                .with_task(info));
        } else if matches!(name, "join" | "await") {
            let [argument] = args else {
                return Err(error("join requires one task", span));
            };
            let argument = self.expression(argument)?;
            argument.require(Type::Task, span)?;
            let info = argument
                .task
                .ok_or_else(|| error("task binding has no worker result metadata", span))?;
            if !info.owned_abi {
                return Ok(self.runtime_call(
                    if name == "await" {
                        "bhasa_rt_await_i64"
                    } else {
                        "bhasa_rt_join_i64"
                    },
                    &[argument],
                    Type::Int,
                ));
            }
            let frame = self.runtime_call(
                if name == "await" {
                    "bhasa_rt_await_owned"
                } else {
                    "bhasa_rt_join_owned"
                },
                &[argument],
                Type::Task,
            );
            let result = if info.result == Type::Void {
                Value::new(Type::Void, "")
            } else {
                let value = self.fresh();
                self.instruction(format!(
                    "{value} = load {}, ptr {}",
                    llvm_type(info.result),
                    frame.operand
                ));
                Value::new(info.result, value).owned()
            };
            self.runtime_call("bhasa_rt_task_free", &[frame], Type::Void);
            return Ok(result);
        }
        if matches!(name, "int" | "float") {
            let [argument] = args else {
                return Err(error("numeric conversion expects one argument", span));
            };
            let value = self.expression(argument)?;
            let result = self.fresh();
            if name == "float" {
                value.require(Type::Int, span)?;
                self.instruction(format!("{result} = sitofp i64 {} to double", value.operand));
                return Ok(Value::new(Type::Float, result));
            }
            value.require(Type::Float, span)?;
            let lower = self.fresh();
            let upper = self.fresh();
            let valid = self.fresh();
            let invalid = self.fresh();
            self.instruction(format!(
                "{lower} = fcmp oge double {}, 0xC3E0000000000000",
                value.operand
            ));
            self.instruction(format!(
                "{upper} = fcmp olt double {}, 0x43E0000000000000",
                value.operand
            ));
            self.instruction(format!("{valid} = and i1 {lower}, {upper}"));
            self.instruction(format!("{invalid} = xor i1 {valid}, true"));
            self.trap_if(&invalid);
            self.instruction(format!("{result} = fptosi double {} to i64", value.operand));
            return Ok(Value::new(Type::Int, result));
        }
        if name == "print" {
            if args.len() != 1 {
                return Err(error("print expects exactly one argument", span));
            }
            let value = self.expression(&args[0])?;
            if value.ty == Type::Buffer {
                self.runtime_call(
                    "bhasa_rt_print_buffer",
                    &[Value::new(Type::Buffer, &value.operand)],
                    Type::Void,
                );
                self.release(&value);
                return Ok(Value::new(Type::Void, ""));
            }
            let (format, argument) = match value.ty {
                Type::Int => (
                    self.module.string("%lld\n"),
                    format!("i64 {}", value.operand),
                ),
                Type::Float => (
                    self.module.string("%.17g\n"),
                    format!("double {}", value.operand),
                ),
                Type::Str => (self.module.string("%s\n"), format!("ptr {}", value.operand)),
                Type::Bool => {
                    let yes = self.module.string("true");
                    let no = self.module.string("false");
                    let result = self.fresh();
                    self.instruction(format!(
                        "{result} = select i1 {}, ptr {yes}, ptr {no}",
                        value.operand
                    ));
                    (self.module.string("%s\n"), format!("ptr {result}"))
                }
                _ => return Err(error("print cannot accept this value", span)),
            };
            self.module.needs_printf = true;
            self.instruction(format!(
                "call i32 (ptr, ...) @printf(ptr {format}, {argument})"
            ));
            return Ok(Value::new(Type::Void, ""));
        }
        let types: Vec<_> = args
            .iter()
            .map(|arg| arg.ty.unwrap_or(Type::Void))
            .collect();
        if let Some(runtime_name) = crate::builtins::runtime_name(name, &types) {
            let result_type = crate::builtins::signature(name, &types)
                .ok_or_else(|| error("unknown builtin", span))?
                .map_err(|message| error(message, span))?;
            let mut values = args
                .iter()
                .map(|arg| self.expression(arg))
                .collect::<Result<Vec<_>>>()?;
            if name == "gpu_dispatch" {
                for value in values.iter_mut().take(2) {
                    if value.ty == Type::Str {
                        *value = self.runtime_call(
                            "bhasa_rt_buffer_from_str",
                            &[Value::new(Type::Str, &value.operand)],
                            Type::Buffer,
                        );
                    }
                }
            }
            // Native TLS accepts length-bearing buffers for addresses, names and paths.
            for (index, value) in values.iter_mut().enumerate() {
                let tls_text = (name == "tls_connect" && matches!(index, 0 | 2 | 3))
                    || (name == "tls_accept" && matches!(index, 1 | 2));
                if tls_text && value.ty == Type::Str {
                    *value = self.runtime_call(
                        "bhasa_rt_buffer_from_str",
                        &[Value::new(Type::Str, &value.operand)],
                        Type::Buffer,
                    );
                }
            }
            let result = self.runtime_call(&runtime_name, &values, result_type);
            for value in &values {
                self.release(value);
            }
            return Ok(result);
        }
        let signature = self
            .module
            .signatures
            .get(name)
            .cloned()
            .ok_or_else(|| error(format!("unknown function '{name}' in checked AST"), span))?;
        if args.len() != signature.params.len() {
            return Err(error(
                format!("wrong argument count for '{name}' in checked AST"),
                span,
            ));
        }
        let mut arguments = Vec::new();
        let mut values = Vec::new();
        for (argument, ty) in args.iter().zip(&signature.params) {
            let value = self.expression(argument)?;
            value.require(*ty, argument.span)?;
            arguments.push(format!("{} {}", llvm_type(*ty), value.operand));
            values.push(value);
        }
        let parameter_types = signature
            .params
            .iter()
            .map(|ty| llvm_type(*ty))
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!(
            "call {} ({parameter_types}) {}({})",
            llvm_type(signature.return_type),
            symbol(name, signature.external),
            arguments.join(", ")
        );
        let result = if signature.return_type == Type::Void {
            self.instruction(call);
            Value::new(Type::Void, "")
        } else {
            let result = self.fresh();
            self.instruction(format!("{result} = {call}"));
            Value::new(signature.return_type, result).owned()
        };
        for (value, mode) in values.iter().zip(&signature.modes) {
            if *mode != ParamMode::Owned {
                self.release(value);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::OnceLock;

    fn expr(kind: ExprKind, ty: Type) -> Expr {
        Expr {
            kind,
            span: Span::default(),
            ty: Some(ty),
        }
    }

    fn int(value: i64) -> Expr {
        expr(ExprKind::Int(value), Type::Int)
    }
    fn boolean(value: bool) -> Expr {
        expr(ExprKind::Bool(value), Type::Bool)
    }
    fn string(value: &str) -> Expr {
        expr(ExprKind::Str(value.into()), Type::Str)
    }
    fn name(value: &str, ty: Type) -> Expr {
        expr(ExprKind::Name(value.into()), ty)
    }
    fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        let ty = if matches!(
            op,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
        ) {
            Type::Int
        } else {
            Type::Bool
        };
        expr(
            ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        )
    }
    fn call(name: &str, args: Vec<Expr>, ty: Type) -> Expr {
        expr(
            ExprKind::Call {
                name: name.into(),
                args,
            },
            ty,
        )
    }
    fn stmt(kind: StmtKind) -> Stmt {
        Stmt {
            kind,
            span: Span::default(),
        }
    }
    fn print(value: Expr) -> Stmt {
        stmt(StmtKind::Expr(call("print", vec![value], Type::Void)))
    }
    fn ret(value: Expr) -> Stmt {
        stmt(StmtKind::Return(Some(value)))
    }
    fn var(name: &str, value: Expr) -> Stmt {
        stmt(StmtKind::Var {
            name: name.into(),
            reassignable: true,
            annotation: None,
            value,
        })
    }
    fn function(name: &str, params: &[(&str, Type)], result: Type, body: Block) -> Function {
        Function {
            name: name.into(),
            params: params
                .iter()
                .map(|(name, ty)| Param {
                    mode: crate::ast::ParamMode::Borrowed,
                    name: (*name).into(),
                    ty: *ty,
                    span: Span::default(),
                })
                .collect(),
            return_type: result,
            body: Some(body),
            span: Span::default(),
        }
    }
    fn checked(functions: Vec<Function>) -> CheckedProgram {
        let signatures = functions
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    FunctionSig {
                        modes: function.params.iter().map(|param| param.mode).collect(),
                        params: function.params.iter().map(|param| param.ty).collect(),
                        return_type: function.return_type,
                        external: function.body.is_none(),
                    },
                )
            })
            .collect();
        CheckedProgram {
            program: Program {
                functions,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
            },
            signatures,
        }
    }
    fn main_program(body: Block) -> CheckedProgram {
        checked(vec![function("main", &[], Type::Void, body)])
    }

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "bhasa-codegen-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot create test directory: {error}"),
                }
            }
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn clang() -> Option<&'static OsString> {
        static CLANG: OnceLock<Option<OsString>> = OnceLock::new();
        CLANG
            .get_or_init(|| {
                let clang =
                    std::env::var_os("BHASA_CLANG").unwrap_or_else(|| OsString::from("clang"));
                match Command::new(&clang).arg("--version").output() {
                    Ok(output) if output.status.success() => Some(clang),
                    _ => {
                        eprintln!("skipping native codegen test: Clang is unavailable");
                        None
                    }
                }
            })
            .as_ref()
    }

    fn native_target(clang: &OsString) -> String {
        let output = Command::new(clang)
            .arg("-dumpmachine")
            .output()
            .expect("query Clang target");
        assert!(
            output.status.success(),
            "Clang cannot report its native target"
        );
        String::from_utf8(output.stdout)
            .expect("UTF-8 target")
            .trim()
            .into()
    }

    fn run_native(
        clang: &OsString,
        program: &CheckedProgram,
        optimization: &str,
        c_source: Option<&str>,
    ) -> Output {
        run_native_with_sanitizers(clang, program, optimization, c_source, false)
    }

    fn run_native_with_sanitizers(
        clang: &OsString,
        program: &CheckedProgram,
        optimization: &str,
        c_source: Option<&str>,
        sanitizers: bool,
    ) -> Output {
        let temp = TestDir::new();
        let ir_path = temp.0.join("program.ll");
        let executable = temp.0.join(if cfg!(windows) {
            "program.exe"
        } else {
            "program"
        });
        let target = native_target(clang);
        let mut ir = emit_llvm(program, &target).expect("emit checked AST");
        if sanitizers {
            // Clang receives LLVM rather than C here, so generated functions
            // need this attribute before its ASan pass will instrument loads
            // and stores. The runtime C is instrumented by -fsanitize below.
            ir = ir
                .lines()
                .map(|line| {
                    if line.starts_with("define ") {
                        format!(
                            "{} sanitize_address {{\n",
                            line.strip_suffix(" {").expect("function definition")
                        )
                    } else {
                        format!("{line}\n")
                    }
                })
                .collect();
        }
        std::fs::write(&ir_path, &ir).expect("write generated IR");
        let mut command = Command::new(clang);
        command
            .args(["-target", &target, optimization, "-x", "ir"])
            .arg(&ir_path)
            .args(["-x", "none"]);
        if sanitizers {
            command.args([
                "-fsanitize=address,undefined",
                "-fno-omit-frame-pointer",
                "-g",
            ]);
        }
        if ir.contains("; bhasa runtime: core") {
            command.arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/core.c"));
        }
        if let Some(c_source) = c_source {
            let c_path = temp.0.join("adapter.c");
            std::fs::write(&c_path, c_source).expect("write C adapter");
            command.arg(c_path);
        }
        let output = command
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("invoke Clang");
        assert!(
            output.status.success(),
            "Clang rejected IR:\n{}\n{ir}",
            String::from_utf8_lossy(&output.stderr)
        );
        Command::new(executable)
            .env("ASAN_OPTIONS", "halt_on_error=1")
            .env("UBSAN_OPTIONS", "halt_on_error=1")
            .output()
            .expect("run native generated executable")
    }

    #[test]
    fn records_copy_borrow_return_and_replace_nested_fields_under_sanitizers() {
        let Some(clang) = clang() else {
            return;
        };
        let program = crate::check(
            r#"
            struct Leaf { text: buffer; numbers: ints }
            struct Wrap { leaf: Leaf; number: int; enabled: bool; label: str }
            def make() -> Wrap {
                var numbers = ints()
                push(numbers, 7)
                return Wrap(Leaf(buffer("original"), numbers), 12, true, "static")
            }
            def identity(value: Wrap) -> Wrap { return value }
            def mutate(value: Wrap) {
                value.leaf.text = buffer("changed")
                set(value.leaf.numbers, 0, 99)
            }
            def replace_parent(value: Wrap) -> buffer {
                value.leaf = Leaf(buffer("replaced"), ints())
                return buffer("rhs")
            }
            def early(value: Wrap) -> Leaf {
                var count = 0
                while count < 3 {
                    var local = value
                    if count == 2 { return local.leaf }
                    count = count + 1
                }
                return value.leaf
            }
            def main() {
                var original = make()
                var copy = original
                mutate(copy)
                print(original.leaf.text)
                print(get(original.leaf.numbers, 0))
                print(copy.leaf.text)
                print(get(copy.leaf.numbers, 0))
                var returned = identity(original)
                original.leaf.text = buffer("after")
                print(returned.leaf.text)
                print(make().leaf.text)
                print(get(make().leaf.numbers, 0))
                print(make().number)
                print(make().enabled)
                print(make().label)
                copy.leaf.text = replace_parent(copy)
                print(copy.leaf.text)
                print(len(copy.leaf.numbers))
                copy.leaf = early(returned)
                print(copy.leaf.text)
                copy.leaf = copy.leaf
                copy = copy
                print(copy.leaf.text)
                make()
            }
        "#,
        )
        .expect("check record ownership program");
        for optimization in ["-O0", "-O2"] {
            let output = run_native_with_sanitizers(clang, &program, optimization, None, true);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"original\n7\nchanged\n99\noriginal\noriginal\n7\n12\ntrue\nstatic\nrhs\n0\noriginal\noriginal\n");
        }
    }

    #[test]
    fn constructor_copies_arguments_before_later_mutations() {
        let Some(clang) = clang() else {
            return;
        };
        let program = crate::check(
            r#"
            struct Boxed { text: buffer }
            struct Pair { first: buffer; second: buffer }
            def replace(value: Boxed) -> buffer {
                value.text = buffer("changed")
                return buffer("second")
            }
            def main() {
                var box = Boxed(buffer("first"))
                var pair = Pair(box.text, replace(box))
                print(pair.first)
                print(pair.second)
                print(box.text)
            }
        "#,
        )
        .expect("constructor arguments are copied in evaluation order");
        let output = run_native_with_sanitizers(clang, &program, "-O2", None, true);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"first\nsecond\nchanged\n");
    }

    #[test]
    fn record_allocators_handle_empty_layouts_and_reject_foreign_collisions() {
        let Some(clang) = clang() else {
            return;
        };
        let program =
            crate::check("struct Empty {}\ndef main() { var value = Empty(); var copy = value }")
                .unwrap();
        let output = run_native_with_sanitizers(clang, &program, "-O0", None, true);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = emit_llvm(&program, "wasm32-unknown-unknown").unwrap();
        assert!(ir.contains("declare ptr @malloc(i32)"));
        assert!(ir.contains("call ptr @malloc(i32 1)"));
        assert!(!ir.contains("; bhasa runtime: core"));
        let program = crate::check(
            "struct Empty {}\nextern def malloc(size: int) -> int\ndef main() { Empty() }",
        )
        .unwrap();
        assert!(
            emit_llvm(&program, "x86_64-unknown-linux-gnu").unwrap_err()[0]
                .message
                .contains("allocator symbol conflicts")
        );
    }

    #[test]
    fn malformed_checked_record_layouts_are_rejected_without_recursing_unboundedly() {
        let mut program = main_program(vec![]);
        for id in 0..130 {
            let name = format!("Record{id}");
            program.program.type_names.push(name.clone());
            program.program.records.push(RecordDecl {
                methods: Vec::new(),
                name,
                fields: if id == 0 {
                    vec![]
                } else {
                    vec![RecordField {
                        name: "nested".into(),
                        ty: Type::Record(id - 1),
                        span: Span::default(),
                    }]
                },
                span: Span::default(),
            });
        }
        let errors = emit_llvm(&program, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(errors[0].message.contains("128 levels"));
    }

    #[test]
    fn records_with_network_fields_request_the_correct_runtime_and_helpers() {
        let program = crate::check("struct Connections { listener: tcp_listener; socket: tcp_socket }\ndef retain(value: Connections) -> Connections { return value }").unwrap();
        let ir = emit_llvm(&program, "x86_64-unknown-linux-gnu").unwrap();
        assert!(ir.contains("; bhasa runtime: core"));
        assert!(ir.contains("; bhasa runtime: net"));
        for resource in ["listener", "socket"] {
            assert!(ir.contains(&format!("declare ptr @bhasa_rt_tcp_{resource}_clone(ptr)")));
            assert!(ir.contains(&format!("declare void @bhasa_rt_tcp_{resource}_free(ptr)")));
        }
        assert!(!ir.contains("bhasa_rt_ints_clone"));
        assert!(!ir.contains("bhasa_rt_ints_free"));
    }

    #[test]
    fn recursion_scopes_loops_strings_and_main_compile_and_run() {
        let Some(clang) = clang() else {
            return;
        };
        let factorial = function(
            "factorial",
            &[("n", Type::Int)],
            Type::Int,
            vec![stmt(StmtKind::If {
                condition: binary(BinaryOp::Le, name("n", Type::Int), int(1)),
                then_block: vec![ret(int(1))],
                else_block: Some(vec![ret(binary(
                    BinaryOp::Mul,
                    name("n", Type::Int),
                    call(
                        "factorial",
                        vec![binary(BinaryOp::Sub, name("n", Type::Int), int(1))],
                        Type::Int,
                    ),
                ))]),
            })],
        );
        let main = function(
            "main",
            &[],
            Type::Int,
            vec![
                print(call("factorial", vec![int(5)], Type::Int)),
                var("value", int(3)),
                stmt(StmtKind::While {
                    condition: binary(BinaryOp::Gt, name("value", Type::Int), int(0)),
                    body: vec![
                        var("shadow", name("value", Type::Int)),
                        stmt(StmtKind::If {
                            condition: boolean(true),
                            then_block: vec![
                                var("value", int(77)),
                                print(name("value", Type::Int)),
                            ],
                            else_block: None,
                        }),
                        print(name("shadow", Type::Int)),
                        stmt(StmtKind::Assign {
                            name: "value".into(),
                            value: binary(BinaryOp::Sub, name("value", Type::Int), int(1)),
                        }),
                    ],
                }),
                var("text", string("भाषा %lld \"\\\n")),
                print(call("identity", vec![name("text", Type::Str)], Type::Str)),
                stmt(StmtKind::Assign {
                    name: "text".into(),
                    value: string(""),
                }),
                print(name("text", Type::Str)),
                print(boolean(true)),
                print(boolean(false)),
                ret(int(0)),
            ],
        );
        let identity = function(
            "identity",
            &[("text", Type::Str)],
            Type::Str,
            vec![ret(name("text", Type::Str))],
        );
        let program = checked(vec![main, factorial, identity]);
        for optimization in ["-O0", "-O2"] {
            let output = run_native(clang, &program, optimization, None);
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "120\n77\n3\n77\n2\n77\n1\nभाषा %lld \"\\\n\n\ntrue\nfalse\n"
            );
        }
    }

    #[test]
    fn short_circuit_preserves_effects_and_nested_phi_predecessors() {
        let Some(clang) = clang() else {
            return;
        };
        let noisy = function(
            "noisy",
            &[],
            Type::Bool,
            vec![print(string("called")), ret(boolean(true))],
        );
        let bomb = function(
            "bomb",
            &[],
            Type::Bool,
            vec![
                print(binary(BinaryOp::Div, int(1), int(0))),
                ret(boolean(true)),
            ],
        );
        let program = checked(vec![
            function(
                "main",
                &[],
                Type::Void,
                vec![
                    print(binary(
                        BinaryOp::And,
                        boolean(false),
                        call("bomb", vec![], Type::Bool),
                    )),
                    print(binary(
                        BinaryOp::Or,
                        boolean(true),
                        call("bomb", vec![], Type::Bool),
                    )),
                    print(binary(
                        BinaryOp::And,
                        boolean(true),
                        call("noisy", vec![], Type::Bool),
                    )),
                    print(binary(
                        BinaryOp::Or,
                        boolean(false),
                        call("noisy", vec![], Type::Bool),
                    )),
                    print(binary(
                        BinaryOp::And,
                        binary(BinaryOp::Eq, binary(BinaryOp::Add, int(1), int(1)), int(2)),
                        binary(
                            BinaryOp::Or,
                            boolean(false),
                            binary(BinaryOp::Eq, binary(BinaryOp::Mul, int(3), int(4)), int(12)),
                        ),
                    )),
                ],
            ),
            noisy,
            bomb,
        ]);
        for optimization in ["-O0", "-O2"] {
            let output = run_native(clang, &program, optimization, None);
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "false\ntrue\ncalled\ntrue\ncalled\ntrue\ntrue\n"
            );
        }
    }

    #[test]
    fn signed_arithmetic_traps_at_both_optimization_levels() {
        let Some(clang) = clang() else {
            return;
        };
        let cases = [
            binary(BinaryOp::Add, int(i64::MAX), int(1)),
            binary(BinaryOp::Sub, int(i64::MIN), int(1)),
            binary(BinaryOp::Mul, int(i64::MAX), int(2)),
            binary(BinaryOp::Div, int(3), int(0)),
            binary(BinaryOp::Rem, int(3), int(0)),
            binary(BinaryOp::Div, int(i64::MIN), int(-1)),
            binary(BinaryOp::Rem, int(i64::MIN), int(-1)),
            expr(
                ExprKind::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(int(i64::MIN)),
                },
                Type::Int,
            ),
        ];
        for (index, value) in cases.iter().enumerate() {
            for optimization in ["-O0", "-O2"] {
                let output = run_native(
                    clang,
                    &main_program(vec![stmt(StmtKind::Expr(value.clone()))]),
                    optimization,
                    None,
                );
                assert!(
                    !output.status.success(),
                    "case {index} did not trap at {optimization}"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    assert!(
                        output.status.signal().is_some(),
                        "trap must terminate by signal"
                    );
                }
            }
        }
    }

    #[test]
    fn arithmetic_boundaries_boolean_parameters_and_early_returns() {
        let Some(clang) = clang() else {
            return;
        };
        let choose = function(
            "choose",
            &[("condition", Type::Bool)],
            Type::Int,
            vec![
                stmt(StmtKind::While {
                    condition: name("condition", Type::Bool),
                    body: vec![ret(int(7))],
                }),
                stmt(StmtKind::If {
                    condition: name("condition", Type::Bool),
                    then_block: vec![ret(int(8))],
                    else_block: None,
                }),
                ret(int(9)),
            ],
        );
        let program = checked(vec![
            function(
                "main",
                &[],
                Type::Int,
                vec![
                    print(int(i64::MIN)),
                    print(int(i64::MAX)),
                    print(binary(BinaryOp::Div, int(-7), int(3))),
                    print(binary(BinaryOp::Rem, int(-7), int(3))),
                    print(binary(BinaryOp::Add, int(i64::MIN), int(1))),
                    print(binary(BinaryOp::Sub, int(i64::MAX), int(1))),
                    print(binary(BinaryOp::Mul, int(i64::MIN), int(1))),
                    print(expr(
                        ExprKind::Unary {
                            op: UnaryOp::Neg,
                            operand: Box::new(int(i64::MAX)),
                        },
                        Type::Int,
                    )),
                    print(expr(
                        ExprKind::Unary {
                            op: UnaryOp::Not,
                            operand: Box::new(boolean(false)),
                        },
                        Type::Bool,
                    )),
                    print(binary(BinaryOp::Ne, boolean(true), boolean(false))),
                    print(binary(BinaryOp::Lt, int(-1), int(0))),
                    print(binary(BinaryOp::Ge, int(0), int(0))),
                    print(call("choose", vec![boolean(true)], Type::Int)),
                    print(call("choose", vec![boolean(false)], Type::Int)),
                    ret(int(4_294_967_296)),
                ],
            ),
            choose,
        ]);
        for optimization in ["-O0", "-O2"] {
            let output = run_native(clang, &program, optimization, None);
            assert!(
                output.status.success(),
                "native main must truncate its result to i32"
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "-9223372036854775808\n9223372036854775807\n-2\n-1\n-9223372036854775807\n9223372036854775806\n-9223372036854775808\n-9223372036854775807\ntrue\ntrue\ntrue\ntrue\n7\n9\n"
            );
        }
    }

    #[test]
    fn scalar_c_abi_and_internal_libc_names_link() {
        let Some(clang) = clang() else {
            return;
        };
        let mut store = function("native_store", &[("value", Type::Int)], Type::Void, vec![]);
        store.body = None;
        let mut load = function("native_load", &[], Type::Int, vec![]);
        load.body = None;
        let printf = function("printf", &[], Type::Int, vec![ret(int(42))]);
        let main = function(
            "main",
            &[],
            Type::Void,
            vec![
                stmt(StmtKind::Expr(call(
                    "native_store",
                    vec![call("printf", vec![], Type::Int)],
                    Type::Void,
                ))),
                print(call("native_load", vec![], Type::Int)),
                stmt(StmtKind::Return(None)),
            ],
        );
        let program = checked(vec![main, store, load, printf]);
        let adapter = "static long long saved; void native_store(long long value) { saved = value; } long long native_load(void) { return saved; }";
        for optimization in ["-O0", "-O2"] {
            let output = run_native(clang, &program, optimization, Some(adapter));
            assert!(output.status.success());
            assert_eq!(output.stdout, b"42\n");
        }
    }

    #[test]
    fn pure_library_has_no_runtime_dependency_and_emits_foreign_objects() {
        let program = checked(vec![function(
            "twice",
            &[("value", Type::Int)],
            Type::Int,
            vec![ret(binary(
                BinaryOp::Add,
                name("value", Type::Int),
                name("value", Type::Int),
            ))],
        )]);
        let Some(clang) = clang() else {
            return;
        };
        let temp = TestDir::new();
        let ir_path = temp.0.join("library.ll");
        let object_path = temp.0.join("library.o");
        for (target, magic) in [
            ("x86_64-unknown-linux-gnu", b"\x7fELF"),
            ("wasm32-unknown-unknown", b"\0asm"),
        ] {
            let ir = emit_llvm(&program, target).unwrap();
            assert!(!ir.contains("@printf"));
            assert!(!ir.contains("@main"));
            std::fs::write(&ir_path, ir).unwrap();
            let output = Command::new(clang)
                .args(["-target", target, "-x", "ir", "-c"])
                .arg(&ir_path)
                .arg("-o")
                .arg(&object_path)
                .output()
                .unwrap();
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("No available targets")
                    || stderr.contains("unable to create target")
                    || stderr.contains("unknown target triple")
                {
                    eprintln!("skipping {target} object verification: {stderr}");
                    continue;
                }
                panic!("Clang rejected pure {target} IR: {stderr}");
            }
            assert!(std::fs::read(&object_path).unwrap().starts_with(magic));
        }
    }

    #[test]
    fn external_printf_collision_reports_a_diagnostic() {
        let mut imported = function("printf", &[("value", Type::Int)], Type::Int, vec![]);
        imported.body = None;
        let program = checked(vec![
            imported.clone(),
            function("main", &[], Type::Void, vec![print(int(1))]),
        ]);
        let errors = emit_llvm(&program, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(errors[0].message.contains("external 'printf' conflicts"));
        // An import is allowed when no builtin printing needs that symbol.
        assert!(emit_llvm(&checked(vec![imported]), "x86_64-unknown-linux-gnu").is_ok());
    }

    #[test]
    fn malformed_checked_programs_return_diagnostics() {
        let mut no_type = int(1);
        no_type.ty = None;
        let wrong_tag = expr(ExprKind::Bool(true), Type::Int);
        let cases = vec![
            main_program(vec![print(no_type)]),
            main_program(vec![print(wrong_tag)]),
            main_program(vec![print(string("nul\0byte"))]),
            main_program(vec![print(name("missing", Type::Int))]),
            main_program(vec![var("x", int(1)), var("x", int(2))]),
            main_program(vec![var("x", call("print", vec![int(1)], Type::Void))]),
            main_program(vec![stmt(StmtKind::Assign {
                name: "missing".into(),
                value: int(1),
            })]),
            main_program(vec![
                var("x", int(1)),
                stmt(StmtKind::Assign {
                    name: "x".into(),
                    value: boolean(true),
                }),
            ]),
            main_program(vec![ret(int(1))]),
            main_program(vec![stmt(StmtKind::Return(None)), print(int(1))]),
            main_program(vec![stmt(StmtKind::If {
                condition: int(1),
                then_block: vec![],
                else_block: None,
            })]),
            main_program(vec![print(binary(BinaryOp::Eq, string("a"), string("a")))]),
            main_program(vec![print(binary(BinaryOp::Add, boolean(true), int(1)))]),
            main_program(vec![stmt(StmtKind::Expr(call(
                "unknown",
                vec![],
                Type::Int,
            )))]),
            main_program(vec![stmt(StmtKind::Expr(call(
                "print",
                vec![],
                Type::Void,
            )))]),
            main_program(vec![stmt(StmtKind::Expr(call(
                "main",
                vec![int(1)],
                Type::Void,
            )))]),
            checked(vec![function("missing_return", &[], Type::Int, vec![])]),
            checked(vec![function(
                "main",
                &[("x", Type::Int)],
                Type::Void,
                vec![],
            )]),
            checked(vec![function("bad.name", &[], Type::Void, vec![])]),
        ];
        for (index, program) in cases.iter().enumerate() {
            let errors = emit_llvm(program, "x86_64-unknown-linux-gnu")
                .expect_err(&format!("malformed case {index} accepted"));
            assert!(errors[0].code.starts_with('C'));
        }
        let mut inconsistent = main_program(vec![]);
        inconsistent.signatures.clear();
        assert!(emit_llvm(&inconsistent, "x86_64-unknown-linux-gnu").is_err());
        assert!(emit_llvm(&main_program(vec![]), "bad\"\ninjection").is_err());
    }
}
