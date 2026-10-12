//! Symbol resolution, type checking, and definite-return analysis.

use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;

/// Check a complete program and annotate every expression on success.
pub fn check(mut program: Program) -> Result<CheckedProgram, Vec<Diagnostic>> {
    normalize_enum_types(&mut program);
    if let Some(authority) = &program.channels {
        authority
            .validate(&program)
            .map_err(|message| vec![Diagnostic::new("C101", message, Span::default())])?;
    }
    let mut checker = Checker {
        signatures: BTreeMap::new(),
        diagnostics: Vec::new(),
        scopes: Vec::new(),
        return_type: Type::Void,
        tasks: Vec::new(),
        loops: Vec::new(),
        borrowed_params: BTreeSet::new(),
        calls: BTreeMap::new(),
        pending_workers: Vec::new(),
        records: BTreeMap::new(),
        record_names: BTreeMap::new(),
        enums: BTreeMap::new(),
        enum_names: BTreeMap::new(),
        type_names: program.type_names.clone(),
        channel_program: program.clone(),
        current_function: String::new(),
    };
    for declaration in &program.enums {
        if let Some(id) = program
            .type_names
            .iter()
            .position(|name| name == &declaration.name)
        {
            if checker
                .enum_names
                .insert(declaration.name.clone(), id)
                .is_some()
            {
                checker.error("S030", "enum is declared more than once", declaration.span);
            }
            checker.enums.insert(id, declaration.clone());
        }
    }
    checker
        .diagnostics
        .extend(crate::ownership::check_modes(&program));
    checker.declare_records(&program.records);
    checker.check_enums(&program);
    // Bodies may refer to any declared function, including themselves.
    for function in &program.functions {
        checker.declare_function(function);
    }
    for record in &program.records {
        for method in &record.methods {
            checker.declare_method(record, method);
        }
    }
    for function in &mut program.functions {
        checker.check_function(function);
    }
    for record in &mut program.records {
        for method in &mut record.methods {
            checker.check_function(method);
        }
    }
    // Method receiver types are unknown before checking. Build this graph only
    // after every method call has a resolved key, including forward/cyclic calls.
    checker.calls = call_graph(&program);
    for (worker, span) in std::mem::take(&mut checker.pending_workers) {
        if let Some(operation) =
            file_worker_violation(&program, &checker.signatures, &checker.calls, &worker)
        {
            checker.error(
                "S020",
                format!("worker '{worker}' reaches single-thread file resource '{operation}'"),
                span,
            );
        }
        if let Some(operation) = checker.reachable_async(&worker) {
            checker.error("S020", format!("worker '{worker}' can call '{operation}'; scheduling or awaiting pooled tasks from workers could deadlock"), span);
        }
        if let Some(external) = checker.reachable_extern(&worker) {
            checker.error("S020", format!("worker '{worker}' can call extern function '{external}', whose thread safety is unknown"), span);
        }
    }
    if checker.diagnostics.is_empty() {
        checker.diagnostics.extend(channel_worker_effects(&program));
    }
    if checker.diagnostics.is_empty() {
        checker
            .diagnostics
            .extend(crate::ownership::check(&program));
        checker
            .diagnostics
            .extend(crate::failure_use::check(&program));
    }
    if checker.diagnostics.is_empty() {
        Ok(CheckedProgram {
            program,
            signatures: checker.signatures,
        })
    } else {
        Err(checker.diagnostics)
    }
}

struct Checker {
    channel_program: Program,
    current_function: String,
    signatures: BTreeMap<String, FunctionSig>,
    diagnostics: Vec<Diagnostic>,
    // None records a binding whose initializer failed, avoiding spurious
    // "unknown variable" diagnostics on subsequent uses of that binding.
    scopes: Vec<BTreeMap<String, LocalBinding>>,
    return_type: Type,
    tasks: Vec<BTreeMap<String, TaskState>>,
    /// First body scope exited by a break/continue, nearest loop last.
    loops: Vec<usize>,
    borrowed_params: BTreeSet<String>,
    calls: BTreeMap<String, BTreeSet<String>>,
    pending_workers: Vec<(String, Span)>,
    records: BTreeMap<usize, RecordDecl>,
    record_names: BTreeMap<String, usize>,
    enums: BTreeMap<usize, EnumDecl>,
    enum_names: BTreeMap<String, usize>,
    type_names: Vec<String>,
}

#[derive(Clone, Copy)]
struct LocalBinding {
    ty: Option<Type>,
    reassignable: bool,
}

#[derive(Clone, Copy)]
struct TaskState {
    live: bool,
    result: Type,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Flow {
    next: bool,
    returns: bool,
    exits: bool,
}
impl Flow {
    const NEXT: Self = Self {
        next: true,
        returns: false,
        exits: false,
    };
    const RETURN: Self = Self {
        next: false,
        returns: true,
        exits: false,
    };
    const EXIT: Self = Self {
        next: false,
        returns: false,
        exits: true,
    };
    const NONE: Self = Self {
        next: false,
        returns: false,
        exits: false,
    };
    fn branch(self, other: Self) -> Self {
        Self {
            next: self.next || other.next,
            returns: self.returns || other.returns,
            exits: self.exits || other.exits,
        }
    }
    fn then(self, other: Self) -> Self {
        if !self.next {
            return self;
        }
        Self {
            next: other.next,
            returns: self.returns || other.returns,
            exits: self.exits || other.exits,
        }
    }
}

impl Checker {
    fn check_enums(&mut self, program: &Program) {
        for declaration in &program.enums {
            self.check_reserved_name(&declaration.name, declaration.span);
            if self.record_names.contains_key(&declaration.name)
                || matches!(
                    declaration.name.as_str(),
                    "int"
                        | "bool"
                        | "str"
                        | "void"
                        | "buffer"
                        | "ints"
                        | "task"
                        | "tcp_socket"
                        | "tls_socket"
                        | "file_handle"
                        | "tcp_listener"
                )
            {
                self.error(
                    "S030",
                    "enum name conflicts with another type",
                    declaration.span,
                );
            }
            if declaration.variants.is_empty() {
                self.error(
                    "S030",
                    "an enum requires at least one variant",
                    declaration.span,
                );
            }
            let mut names = BTreeSet::new();
            for variant in &declaration.variants {
                if !names.insert(&variant.name) {
                    self.error("S030", "duplicate enum variant", variant.span);
                }
                let mut fields = BTreeSet::new();
                for field in &variant.fields {
                    if !fields.insert(&field.name) {
                        self.error("S030", "duplicate enum payload name", field.span);
                    }
                    if matches!(field.ty, Type::Void | Type::Task) {
                        self.error(
                            "S030",
                            "enum payloads cannot have void or task type",
                            field.span,
                        );
                    }
                    self.check_known_type(field.ty, field.span);
                }
            }
        }
        // Aggregate cycles can cross a record/enum boundary. Check the complete
        // graph iteratively, so a malicious schema never recurses on the host stack.
        let mut graph = BTreeMap::new();
        for record in &program.records {
            if let Some(id) = program
                .type_names
                .iter()
                .position(|name| name == &record.name)
            {
                graph.insert(
                    id,
                    (
                        record.span,
                        record
                            .fields
                            .iter()
                            .map(|field| field.ty)
                            .collect::<Vec<_>>(),
                    ),
                );
            }
        }
        for declaration in &program.enums {
            if let Some(id) = program
                .type_names
                .iter()
                .position(|name| name == &declaration.name)
            {
                graph.insert(
                    id,
                    (
                        declaration.span,
                        declaration
                            .variants
                            .iter()
                            .flat_map(|variant| variant.fields.iter().map(|field| field.ty))
                            .collect::<Vec<_>>(),
                    ),
                );
            }
        }
        let mut heights: BTreeMap<usize, usize> = BTreeMap::new();
        for id in graph.keys().copied() {
            let mut active = BTreeSet::new();
            let mut pending = vec![(id, false)];
            while let Some((node, leaving)) = pending.pop() {
                let Some((span, fields)) = graph.get(&node) else {
                    continue;
                };
                if leaving {
                    active.remove(&node);
                    let height = 1 + fields
                        .iter()
                        .filter_map(|ty| match ty {
                            Type::Record(id) | Type::Enum(id) => heights.get(id).copied(),
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0);
                    if height > 128 {
                        self.error("S030", "aggregate schema nesting exceeds 128 levels", *span);
                        break;
                    }
                    heights.insert(node, height);
                } else if !heights.contains_key(&node) {
                    if !active.insert(node) {
                        self.error("S030", "recursive aggregate schema is unsupported", *span);
                        break;
                    }
                    if active.len() > 128 {
                        self.error("S030", "aggregate schema nesting exceeds 128 levels", *span);
                        break;
                    }
                    pending.push((node, true));
                    for ty in fields {
                        if let Type::Record(id) | Type::Enum(id) = ty {
                            pending.push((*id, false));
                        }
                    }
                }
            }
        }
    }
    fn declare_method(&mut self, record: &RecordDecl, method: &Function) {
        let id = self.record_names.get(&record.name).copied();
        if !matches!(method.params.first(), Some(param) if param.name == "self" && Some(param.ty) == id.map(Type::Record))
        {
            self.error(
                "S029",
                format!(
                    "method '{}.{}' requires its enclosing record as the first 'self' parameter",
                    record.name, method.name
                ),
                method.span,
            );
        }
        if method.body.is_none() {
            self.error(
                "S029",
                "record methods require a Bhasa body; extern methods are unsupported",
                method.span,
            );
        }
        if record.fields.iter().any(|field| field.name == method.name) {
            self.error(
                "S029",
                format!(
                    "method '{}' conflicts with a field in record '{}'",
                    method.name, record.name
                ),
                method.span,
            );
        }
        let mut declaration = method.clone();
        declaration.name = method_key(&record.name, &method.name);
        self.declare_function(&declaration);
    }

    fn type_name(&self, ty: Type) -> String {
        match ty {
            Type::Record(id) | Type::Enum(id) => self
                .type_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("unknown record #{id}")),
            _ => ty.to_string(),
        }
    }

    fn check_known_type(&mut self, ty: Type, span: Span) -> bool {
        if let Type::Record(id) = ty {
            if !self.records.contains_key(&id) {
                self.error(
                    "S025",
                    format!("unknown type '{}'", self.type_name(ty)),
                    span,
                );
                return false;
            }
        }
        if let Type::Enum(id) = ty {
            if !self.enums.contains_key(&id) {
                self.error(
                    "S030",
                    format!("unknown enum type '{}'", self.type_name(ty)),
                    span,
                );
                return false;
            }
        }
        true
    }

    fn declare_records(&mut self, records: &[RecordDecl]) {
        for record in records {
            self.check_reserved_name(&record.name, record.span);
            if matches!(
                record.name.as_str(),
                "int"
                    | "bool"
                    | "str"
                    | "void"
                    | "buffer"
                    | "ints"
                    | "task"
                    | "tcp_listener"
                    | "tcp_socket"
                    | "tls_socket"
                    | "file_handle"
            ) {
                self.error(
                    "S024",
                    format!("record '{}' conflicts with a builtin type", record.name),
                    record.span,
                );
            }
            let Some(id) = self.type_names.iter().position(|name| name == &record.name) else {
                self.error(
                    "S025",
                    format!("record '{}' is missing its type name", record.name),
                    record.span,
                );
                continue;
            };
            if self.record_names.insert(record.name.clone(), id).is_some() {
                self.error(
                    "S024",
                    format!("record '{}' is declared more than once", record.name),
                    record.span,
                );
            } else {
                self.records.insert(id, record.clone());
            }
        }
        for record in records {
            if let Err(message) = crate::collections::schema(record) {
                self.error("S025", message, record.span);
            }
            let mut names = BTreeSet::new();
            for field in &record.fields {
                if !names.insert(&field.name) {
                    self.error(
                        "S024",
                        format!(
                            "field '{}' is declared more than once in record '{}'",
                            field.name, record.name
                        ),
                        field.span,
                    );
                }
                if matches!(field.ty, Type::Void | Type::Task) {
                    self.error(
                        "S025",
                        format!(
                            "field '{}' of record '{}' cannot have type {}",
                            field.name,
                            record.name,
                            self.type_name(field.ty)
                        ),
                        field.span,
                    );
                }
                self.check_known_type(field.ty, field.span);
            }
        }
        // Keep this traversal iterative: malicious declaration chains must not
        // overflow the compiler's own call stack before they can be rejected.
        let mut finished = BTreeSet::new();
        let mut heights = BTreeMap::new();
        for id in self.records.keys().copied().collect::<Vec<_>>() {
            let mut active = BTreeSet::new();
            let mut pending = vec![(id, false)];
            while let Some((current, leaving)) = pending.pop() {
                if leaving {
                    active.remove(&current);
                    let record = &self.records[&current];
                    let height = 1 + record
                        .fields
                        .iter()
                        .filter_map(|field| match field.ty {
                            Type::Record(child) => heights.get(&child).copied(),
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0);
                    if height > 128 {
                        self.error(
                            "S025",
                            "record nesting exceeds the limit of 128 levels",
                            record.span,
                        );
                        break;
                    }
                    heights.insert(current, height);
                    finished.insert(current);
                    continue;
                }
                if active.contains(&current) {
                    let record = &self.records[&current];
                    self.error(
                        "S025",
                        format!("record '{}' has an infinite recursive layout", record.name),
                        record.span,
                    );
                    break;
                }
                if finished.contains(&current) {
                    continue;
                }
                let Some(record) = self.records.get(&current) else {
                    continue;
                };
                if active.len() >= 128 {
                    self.error(
                        "S025",
                        "record nesting exceeds the limit of 128 levels",
                        record.span,
                    );
                    break;
                }
                active.insert(current);
                pending.push((current, true));
                for field in &record.fields {
                    if let Type::Record(child) = field.ty {
                        pending.push((child, false));
                    }
                }
            }
        }
    }

    fn check_argument_aliases(&mut self, args: &[Expr]) {
        for (index, earlier) in args.iter().enumerate() {
            let Some(earlier_ty) = earlier.ty.filter(|ty| ty.is_owned()) else {
                continue;
            };
            let Some(earlier_path) = access_path(earlier) else {
                continue;
            };
            for later in &args[index + 1..] {
                // These two borrows survive until the outer call completes.
                // A record and its descendant must not enter the callee under
                // unrelated parameter names that could hide their aliasing.
                if let Some((later_path, later_ty)) = later
                    .ty
                    .filter(|ty| ty.is_owned())
                    .and_then(|ty| access_path(later).map(|path| (path, ty)))
                {
                    if (matches!(earlier_ty, Type::Record(_))
                        || matches!(later_ty, Type::Record(_)))
                        && (earlier_path.starts_with(&later_path)
                            || later_path.starts_with(&earlier_path))
                    {
                        self.error("S028", format!("overlapping record arguments '{}' and '{}' may invalidate a borrowed field; make an independent copy first", earlier_path.join("."), later_path.join(".")), later.span);
                        continue;
                    }
                }

                // A borrow used while evaluating the later argument ends
                // before the outer call. Only replacing an ancestor's fields
                // can invalidate the earlier captured handle. Borrowed record
                // parameters cannot rebind their own handle.
                let mut mutations = Vec::new();
                argument_mutations(later, &self.record_names, &self.enum_names, &mut mutations);
                for (record_path, span) in mutations {
                    if earlier_path.len() > record_path.len()
                        && earlier_path.starts_with(&record_path)
                    {
                        self.error("S028", format!("overlapping record arguments '{}' and '{}' may invalidate a borrowed field; make an independent copy first", earlier_path.join("."), record_path.join(".")), span);
                        break;
                    }
                }
            }
        }
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::new(code, message, span));
    }

    fn declare_function(&mut self, function: &Function) {
        self.check_reserved_name(&function.name, function.span);
        if self.record_names.contains_key(&function.name)
            || self.enum_names.contains_key(&function.name)
        {
            self.error(
                "S024",
                format!("function '{}' conflicts with a record type", function.name),
                function.span,
            );
        }
        self.check_known_type(function.return_type, function.span);
        if function.return_type == Type::Task {
            self.error(
                "S019",
                "task handles cannot be returned from a function",
                function.span,
            );
        }
        let signature = FunctionSig {
            modes: function.params.iter().map(|param| param.mode).collect(),
            params: function.params.iter().map(|param| param.ty).collect(),
            return_type: function.return_type,
            external: function.body.is_none(),
        };
        match self.signatures.entry(function.name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(signature);
            }
            Entry::Occupied(_) => self.error(
                "S001",
                format!("function '{}' is declared more than once", function.name),
                function.span,
            ),
        }

        let mut names = BTreeMap::new();
        for param in &function.params {
            if names.insert(&param.name, ()).is_some() {
                self.error(
                    "S007",
                    format!("parameter '{}' is declared more than once", param.name),
                    param.span,
                );
            }
            self.check_reserved_name(&param.name, param.span);
            self.check_known_type(param.ty, param.span);
            if param.ty == Type::Task {
                self.error(
                    "S019",
                    "task handles cannot be function parameters",
                    param.span,
                );
            }
            if param.ty == Type::Void {
                self.error(
                    "S003",
                    format!("parameter '{}' cannot have type void", param.name),
                    param.span,
                );
            } else if function.body.is_none() && param.ty != Type::Int {
                self.error(
                    "S004",
                    format!(
                        "extern parameter '{}' must have type int, found {}",
                        param.name,
                        self.type_name(param.ty)
                    ),
                    param.span,
                );
            }
        }
        if function.body.is_none() && !matches!(function.return_type, Type::Int | Type::Void) {
            self.error(
                "S005",
                format!(
                    "extern function '{}' must return int or void, found {}",
                    function.name,
                    self.type_name(function.return_type)
                ),
                function.span,
            );
        }
        if function.name == "main"
            && (function.body.is_none()
                || !function.params.is_empty()
                || !matches!(function.return_type, Type::Int | Type::Void))
        {
            self.error(
                "S006",
                "'main' must be a Bhasa definition with no parameters and return int or void",
                function.span,
            );
        }
    }

    fn check_function(&mut self, function: &mut Function) {
        self.current_function = function.name.clone();
        let Some(body) = &mut function.body else {
            return;
        };
        self.return_type = function.return_type;
        self.borrowed_params = function
            .params
            .iter()
            .filter(|param| param.ty.is_owned() && param.mode == ParamMode::Borrowed)
            .map(|param| param.name.clone())
            .collect();
        let mut scope = BTreeMap::new();
        for param in &function.params {
            scope.entry(param.name.clone()).or_insert(LocalBinding {
                ty: (param.ty != Type::Void).then_some(param.ty),
                reassignable: true,
            });
        }
        // Parameters and declarations in the outer body share one scope.
        self.scopes.push(scope);
        self.tasks.push(BTreeMap::new());
        let returns = self.check_block(body);
        self.tasks.pop();
        self.scopes.pop();
        if function.return_type != Type::Void && returns != Flow::RETURN {
            self.error(
                "S014",
                format!(
                    "function '{}' must return {} on every path",
                    function.name,
                    self.type_name(function.return_type)
                ),
                function.span,
            );
        }
    }

    fn check_block(&mut self, block: &mut Block) -> Flow {
        let mut flow = Flow::NEXT;
        for statement in block.iter_mut() {
            if !flow.next {
                self.error(
                    "S015",
                    "unreachable statement after a guaranteed return or loop exit",
                    statement.span,
                );
            }
            // Diagnose independent errors even in unreachable source, without
            // letting its outcomes change the reachable control flow.
            let next = self.check_statement(statement);
            flow = flow.then(next);
        }
        if flow.next {
            let span = block
                .last()
                .map_or(Span::default(), |statement| statement.span);
            self.check_outstanding_tasks(false, span);
        }
        flow
    }

    fn check_scoped_block(&mut self, block: &mut Block) -> Flow {
        self.scopes.push(BTreeMap::new());
        self.tasks.push(BTreeMap::new());
        let flow = self.check_block(block);
        self.tasks.pop();
        self.scopes.pop();
        flow
    }

    fn lookup_binding(&self, name: &str) -> Option<LocalBinding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn lookup_local(&self, name: &str) -> Option<Option<Type>> {
        self.lookup_binding(name).map(|binding| binding.ty)
    }

    fn declare_local(&mut self, name: &str, ty: Option<Type>, span: Span, reassignable: bool) {
        self.check_reserved_name(name, span);
        if self.lookup_local(name) == Some(Some(Type::Task)) {
            self.error(
                "S019",
                format!("task handle '{name}' cannot be shadowed"),
                span,
            );
        }
        if let Some(scope) = self.scopes.last_mut() {
            match scope.entry(name.to_owned()) {
                Entry::Vacant(entry) => {
                    entry.insert(LocalBinding { ty, reassignable });
                }
                Entry::Occupied(_) => self.error(
                    "S007",
                    format!("variable '{name}' is already declared in this scope"),
                    span,
                ),
            }
        }
    }

    fn check_reserved_name(&mut self, name: &str, span: Span) {
        if is_reserved(name) {
            self.error("S002", format!("'{name}' is a reserved builtin name"), span);
        }
    }

    fn is_borrowed_param(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, scope)| scope.contains_key(name))
            .is_some_and(|(index, _)| index == 0 && self.borrowed_params.contains(name))
    }

    fn check_outstanding_tasks(&mut self, all_scopes: bool, span: Span) {
        let first = if all_scopes {
            0
        } else {
            self.tasks.len().saturating_sub(1)
        };
        self.check_tasks_from(first, span);
    }

    fn check_tasks_from(&mut self, first: usize, span: Span) {
        let live: Vec<_> = self
            .tasks
            .iter()
            .skip(first)
            .flat_map(|scope| scope.iter())
            .filter(|(_, state)| state.live)
            .map(|(name, _)| name.clone())
            .collect();
        for name in live {
            self.error(
                "S019",
                format!("task handle '{name}' must be joined before leaving its block"),
                span,
            );
        }
    }

    fn reachable_extern(&self, worker: &str) -> Option<String> {
        let mut pending = vec![worker];
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name) {
                continue;
            }
            if self
                .signatures
                .get(name)
                .is_some_and(|signature| signature.external)
            {
                return Some(name.to_owned());
            }
            if let Some(calls) = self.calls.get(name) {
                pending.extend(calls.iter().map(String::as_str));
            }
        }
        None
    }

    fn reachable_async(&self, worker: &str) -> Option<String> {
        let mut pending = vec![worker];
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name) {
                continue;
            }
            if matches!(name, "async_spawn" | "await") {
                return Some(name.to_owned());
            }
            if let Some(calls) = self.calls.get(name) {
                pending.extend(calls.iter().map(String::as_str));
            }
        }
        None
    }

    fn check_spawn(&mut self, expression: &mut Expr) -> Option<Type> {
        let ExprKind::Call { args, .. } = &mut expression.kind else {
            unreachable!()
        };
        self.check_arity("spawn/async_spawn", 1, args.len(), expression.span);
        if args.len() != 1 {
            return None;
        }
        if matches!(args[0].kind, ExprKind::MethodCall { .. }) {
            self.error(
                "S020",
                "spawn does not accept method calls; use a global one-argument Bhasa worker",
                args[0].span,
            );
            return None;
        }
        let ExprKind::Call { name, .. } = &args[0].kind else {
            self.error(
                "S020",
                "spawn requires a call to a defined one-argument Bhasa worker",
                args[0].span,
            );
            return None;
        };
        let worker = name.clone();
        let actual = self.check_expression(&mut args[0]);
        let valid = self.signatures.get(&worker).is_some_and(|signature| {
            !signature.external
                && !worker.contains('.')
                && signature.params.len() == 1
                && !matches!(signature.params[0], Type::Void | Type::Task)
                && signature.return_type != Type::Task
        });
        if !valid {
            self.error(
                "S020",
                "spawn requires a defined global Bhasa worker with one non-task value parameter and a non-task result",
                args[0].span,
            );
            return None;
        }
        if let (Some(authority), Some(signature)) =
            (&self.channel_program.channels, self.signatures.get(&worker))
        {
            for ty in [signature.params[0], signature.return_type] {
                if ty != Type::Void
                    && crate::channel_transfer::check_worker_transfer(
                        &self.channel_program,
                        authority.registry(),
                        authority.context(),
                        ty,
                        &[],
                    )
                    .is_err()
                {
                    self.error(
                        "S020",
                        "worker value cannot transfer across threads",
                        args[0].span,
                    );
                    return None;
                }
            }
        }
        self.pending_workers.push((worker, args[0].span));
        let ty = actual.map(|_| Type::Task);
        expression.ty = ty;
        ty
    }

    fn check_join(&mut self, expression: &mut Expr) -> Option<Type> {
        let ExprKind::Call { args, .. } = &mut expression.kind else {
            unreachable!()
        };
        self.check_arity("join/await", 1, args.len(), expression.span);
        if args.len() != 1 {
            return None;
        }
        let ExprKind::Name(name) = &args[0].kind else {
            self.error("S019", "join requires a task handle name", args[0].span);
            return None;
        };
        if self.lookup_local(name) != Some(Some(Type::Task)) {
            self.error(
                "S019",
                format!("'{name}' is not a task handle"),
                args[0].span,
            );
            return None;
        }
        let state = self.tasks.last().and_then(|scope| scope.get(name)).copied();
        match state {
            Some(TaskState { live: true, result }) => {
                self.tasks.last_mut().unwrap().insert(
                    name.clone(),
                    TaskState {
                        live: false,
                        result,
                    },
                );
                args[0].ty = Some(Type::Task);
                expression.ty = Some(result);
                Some(result)
            }
            Some(TaskState { live: false, .. }) => {
                self.error(
                    "S019",
                    format!("task handle '{name}' has already been joined"),
                    args[0].span,
                );
                None
            }
            None => {
                self.error("S019", format!("task handle '{name}' must be joined in the same lexical block where it was created"), args[0].span);
                None
            }
        }
    }

    fn require_type(
        &mut self,
        actual: Option<Type>,
        expected: Type,
        span: Span,
        context: &str,
    ) -> bool {
        match actual {
            Some(actual) if actual == expected => true,
            Some(actual) => {
                self.error(
                    "S010",
                    format!(
                        "{context} requires {}, found {}",
                        self.type_name(expected),
                        self.type_name(actual)
                    ),
                    span,
                );
                false
            }
            None => false,
        }
    }

    fn require_value(&mut self, ty: Option<Type>, span: Span, context: &str) -> Option<Type> {
        if ty == Some(Type::Void) {
            self.error(
                "S011",
                format!("{context} cannot use a void expression as a value"),
                span,
            );
            None
        } else {
            ty
        }
    }

    fn result_shape(&self, ty: Type) -> Option<ResultShape> {
        let Type::Enum(id) = ty else { return None };
        self.enums.get(&id)?.result_shape()
    }

    fn check_statement(&mut self, statement: &mut Stmt) -> Flow {
        match &mut statement.kind {
            StmtKind::Var {
                name,
                reassignable,
                annotation,
                value,
            } => {
                // Resolve the initializer before introducing the new binding.
                let actual = match &value.kind {
                    ExprKind::Call { name, .. }
                        if matches!(name.as_str(), "spawn" | "async_spawn") =>
                    {
                        self.check_spawn(value)
                    }
                    ExprKind::Call { name, .. } if matches!(name.as_str(), "join" | "await") => {
                        self.check_join(value)
                    }
                    _ => self.check_expression(value),
                };
                let actual = self.require_value(actual, value.span, "variable initializer");
                if *annotation == Some(Type::Task) {
                    self.error(
                        "S019",
                        "task handle types must be inferred from spawn",
                        statement.span,
                    );
                }
                let ty = match *annotation {
                    Some(Type::Void) => {
                        self.error("S011", "a variable cannot have type void", statement.span);
                        None
                    }
                    Some(expected) => {
                        self.check_known_type(expected, statement.span);
                        self.require_type(actual, expected, value.span, "variable initializer");
                        Some(expected)
                    }
                    None => actual,
                };
                self.declare_local(name, ty, statement.span, *reassignable);
                if actual == Some(Type::Task) {
                    let result = match &value.kind {
                        ExprKind::Call { name, args }
                            if matches!(name.as_str(), "spawn" | "async_spawn") =>
                        {
                            args.first().and_then(|worker| worker.ty)
                        }
                        _ => None,
                    };
                    if let Some(result) = result {
                        if let Some(tasks) = self.tasks.last_mut() {
                            tasks.insert(name.clone(), TaskState { live: true, result });
                        }
                    } else {
                        self.error(
                            "S019",
                            "task handle bindings must be initialized by spawn or async_spawn",
                            value.span,
                        );
                    }
                }
            }
            StmtKind::TryVar {
                name,
                reassignable,
                name_span,
                annotation,
                value,
                try_span,
                success_type,
            } => {
                // Joining may consume the one task supplying this result. Any
                // other task would escape its lexical block on the error edge.
                let actual = if matches!(&value.kind, ExprKind::Call { name, .. } if matches!(name.as_str(), "join" | "await"))
                {
                    self.check_join(value)
                } else {
                    self.check_expression(value)
                };
                self.check_outstanding_tasks(true, *try_span);
                let source = actual.and_then(|ty| self.result_shape(ty));
                let target = self.result_shape(self.return_type);
                if actual.is_some() && source.is_none() {
                    self.error(
                        "S035",
                        "try requires an enum with exactly ok(value) and error(value) variants",
                        value.span,
                    );
                }
                if target.is_none() {
                    self.error("S035", "try requires the enclosing function to return an enum with exactly ok(value) and error(value) variants", *try_span);
                }
                if let (Some(source), Some(target)) = (source, target) {
                    if source.error != target.error {
                        self.error(
                            "S035",
                            format!(
                                "try error payload requires {}, found {}",
                                self.type_name(target.error),
                                self.type_name(source.error)
                            ),
                            *try_span,
                        );
                    }
                }
                *success_type = source.map(|shape| shape.success);
                let ty = match *annotation {
                    Some(Type::Void) => {
                        self.error("S011", "a variable cannot have type void", *name_span);
                        None
                    }
                    Some(Type::Task) => {
                        self.error(
                            "S019",
                            "task handle types must be inferred from spawn",
                            *name_span,
                        );
                        None
                    }
                    Some(expected) => {
                        self.check_known_type(expected, *name_span);
                        self.require_type(
                            *success_type,
                            expected,
                            value.span,
                            "try success initializer",
                        );
                        Some(expected)
                    }
                    None => *success_type,
                };
                self.declare_local(name, ty, *name_span, *reassignable);
                return Flow::NEXT.branch(Flow::RETURN);
            }
            StmtKind::Assign { name, value } => {
                if self.lookup_local(name) == Some(Some(Type::Task)) {
                    self.error(
                        "S019",
                        format!("task handle '{name}' cannot be reassigned"),
                        statement.span,
                    );
                }
                if self
                    .lookup_binding(name)
                    .is_some_and(|binding| !binding.reassignable)
                {
                    self.error(
                        "S036",
                        format!("binding '{name}' declared with let cannot be reassigned"),
                        statement.span,
                    );
                }
                if self.is_borrowed_param(name) {
                    self.error("S021", format!("borrowed parameter '{name}' cannot be rebound; mutate its contents instead"), statement.span);
                }
                let actual = self.check_expression(value);
                let actual = self.require_value(actual, value.span, "assignment");
                match self.lookup_local(name) {
                    Some(Some(expected)) => {
                        self.require_type(actual, expected, value.span, "assignment");
                    }
                    Some(None) => {}
                    None => self.error(
                        "S008",
                        format!("cannot assign to unknown variable '{name}'"),
                        statement.span,
                    ),
                }
            }
            StmtKind::FieldAssign { target, value } => {
                let expected = self.check_expression(target);
                if !matches!(target.kind, ExprKind::Field { .. }) || access_path(target).is_none() {
                    self.error(
                        "S027",
                        "field assignment requires a field rooted in a variable",
                        target.span,
                    );
                }
                let actual = self.check_expression(value);
                let actual = self.require_value(actual, value.span, "field assignment");
                if let Some(expected) = expected {
                    self.require_type(actual, expected, value.span, "field assignment");
                }
            }
            StmtKind::Expr(expression) => {
                if matches!(&expression.kind, ExprKind::Call { name, .. } if matches!(name.as_str(), "join" | "await"))
                {
                    self.check_join(expression);
                } else {
                    self.check_expression(expression);
                }
                if expression
                    .ty
                    .is_some_and(|ty| self.result_shape(ty).is_some())
                {
                    self.error("S022", "a result-shaped enum value must be used; bind, match, return or propagate it", expression.span);
                }
                if let ExprKind::Call { name, .. } = &expression.kind {
                    if crate::builtins::must_use(name) {
                        self.error(
                            "S022",
                            format!("the status returned by '{name}' must be used"),
                            expression.span,
                        );
                    }
                }
            }
            StmtKind::Return(value) => {
                self.check_outstanding_tasks(true, statement.span);
                match value {
                    None if self.return_type != Type::Void => self.error(
                        "S013",
                        format!(
                            "return requires a value of type {}",
                            self.type_name(self.return_type)
                        ),
                        statement.span,
                    ),
                    Some(value) => {
                        let actual = self.check_expression(value);
                        let actual = self.require_value(actual, value.span, "return");
                        if self.return_type == Type::Void {
                            if let Some(actual) = actual {
                                self.error(
                                    "S013",
                                    format!(
                                        "a void function cannot return a value of type {}",
                                        self.type_name(actual)
                                    ),
                                    value.span,
                                );
                            }
                        } else {
                            self.require_type(actual, self.return_type, value.span, "return");
                        }
                    }
                    None => {}
                }
                return Flow::RETURN;
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let actual = self.check_expression(condition);
                self.require_type(actual, Type::Bool, condition.span, "if condition");
                let then_returns = self.check_scoped_block(then_block);
                let else_returns = else_block
                    .as_mut()
                    .map_or(Flow::NEXT, |block| self.check_scoped_block(block));
                return then_returns.branch(else_returns);
            }
            StmtKind::While { condition, body } => {
                let actual = self.check_expression(condition);
                self.require_type(actual, Type::Bool, condition.span, "while condition");
                self.loops.push(self.scopes.len());
                self.check_scoped_block(body);
                self.loops.pop();
                // A loop may execute zero times, even if its body always returns.
            }
            StmtKind::For {
                name,
                name_span,
                source,
                element_type,
                body,
            } => {
                *element_type = match source {
                    ForSource::Range { start, end, step } => {
                        let left = self.check_expression(start);
                        self.require_type(left, Type::Int, start.span, "range bound");
                        let right = self.check_expression(end);
                        self.require_type(right, Type::Int, end.span, "range bound");
                        if let Some(step) = step {
                            let actual = self.check_expression(step);
                            self.require_type(actual, Type::Int, step.span, "range step");
                        }
                        Some(Type::Int)
                    }
                    ForSource::Iterable(value) => {
                        let ty = self.check_expression(value);
                        let schema = match ty {
                            Some(Type::Record(id)) => self.records.get(&id).and_then(|record| {
                                crate::collections::schema(record).ok().flatten()
                            }),
                            _ => None,
                        };
                        match (ty, schema) {
                            (Some(Type::Buffer | Type::Ints), _) => Some(Type::Int),
                            (_, Some(crate::collections::Schema::List(element))) => Some(element),
                            (_, Some(crate::collections::Schema::Map(key, _))) => Some(key),
                            _ => {
                                if ty.is_some() {
                                    self.error(
                                        "S034",
                                        "for requires a buffer, ints, List, Map or range",
                                        value.span,
                                    );
                                }
                                None
                            }
                        }
                    }
                };
                self.loops.push(self.scopes.len());
                self.scopes.push(BTreeMap::new());
                self.tasks.push(BTreeMap::new());
                self.declare_local(name, *element_type, *name_span, true);
                self.check_block(body);
                self.tasks.pop();
                self.scopes.pop();
                self.loops.pop();
            }
            StmtKind::Break | StmtKind::Continue => {
                if let Some(&first) = self.loops.last() {
                    self.check_tasks_from(first, statement.span);
                } else {
                    self.error(
                        "S034",
                        "break/continue requires an enclosing loop",
                        statement.span,
                    );
                }
                return Flow::EXIT;
            }
            StmtKind::Match { value, arms } => {
                let ty = self.check_expression(value);
                let declaration = match ty {
                    Some(Type::Enum(id)) => self.enums.get(&id).cloned(),
                    Some(_) => {
                        self.error("S031", "match requires an enum value", value.span);
                        None
                    }
                    None => None,
                };
                let mut seen = BTreeSet::new();
                let mut returns = Flow::NONE;
                for arm in arms {
                    let variant = declaration.as_ref().and_then(|declaration| {
                        (declaration.name == arm.enum_name)
                            .then(|| {
                                declaration
                                    .variants
                                    .iter()
                                    .find(|variant| variant.name == arm.variant)
                                    .cloned()
                            })
                            .flatten()
                    });
                    if variant.is_none() {
                        self.error(
                            "S031",
                            "case must name a variant of the matched enum",
                            arm.span,
                        );
                    }
                    if !seen.insert(arm.variant.clone()) {
                        self.error("S032", "duplicate match case", arm.span);
                    }
                    self.scopes.push(BTreeMap::new());
                    self.tasks.push(BTreeMap::new());
                    if let Some(variant) = &variant {
                        if arm.bindings.len() != variant.fields.len() {
                            self.error(
                                "S031",
                                "case binding count must match the variant payload",
                                arm.span,
                            );
                        }
                    }
                    for (index, binding) in arm.bindings.iter_mut().enumerate() {
                        binding.ty = variant
                            .as_ref()
                            .and_then(|variant| variant.fields.get(index))
                            .map(|field| field.ty);
                        self.declare_local(&binding.name, binding.ty, binding.span, true);
                    }
                    returns = returns.branch(self.check_block(&mut arm.body));
                    self.tasks.pop();
                    self.scopes.pop();
                }
                if let Some(declaration) = declaration {
                    let missing = declaration
                        .variants
                        .iter()
                        .filter(|variant| !seen.contains(&variant.name))
                        .map(|variant| variant.name.as_str())
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        self.error(
                            "S032",
                            format!(
                                "non-exhaustive match; missing variants: {}",
                                missing.join(", ")
                            ),
                            statement.span,
                        );
                    }
                }
                return returns;
            }
        }
        Flow::NEXT
    }

    fn check_expression(&mut self, expression: &mut Expr) -> Option<Type> {
        if matches!(expression.kind, ExprKind::MethodCall { .. }) {
            return self.check_method_call(expression);
        }
        let ty = match &mut expression.kind {
            ExprKind::Int(_) => Some(Type::Int),
            ExprKind::Float(value) => {
                if !value.is_finite() {
                    self.error("S010", "float literals must be finite", expression.span);
                }
                Some(Type::Float)
            }
            ExprKind::Bool(_) => Some(Type::Bool),
            ExprKind::Str(value) => {
                if value.contains('\0') {
                    self.error("S018", "strings cannot contain NUL bytes", expression.span);
                }
                Some(Type::Str)
            }
            ExprKind::Move { name, .. } => match self.lookup_local(name) {
                Some(Some(Type::Task)) => {
                    self.error("S038", "task handles cannot be moved", expression.span);
                    None
                }
                Some(ty) => ty,
                None => {
                    self.error(
                        "S038",
                        "move source is not a local binding",
                        expression.span,
                    );
                    None
                }
            },
            ExprKind::Name(name) => {
                match self.lookup_local(name) {
                    Some(Some(Type::Task)) => {
                        self.error(
                            "S019",
                            format!("task handle '{name}' can only be consumed by join"),
                            expression.span,
                        );
                        None
                    }
                    Some(ty) => ty,
                    None if is_reserved(name) || self.signatures.contains_key(name) => {
                        self.error(
                        "S017",
                        format!("function '{name}' cannot be used as a value; call it with parentheses"),
                        expression.span,
                    );
                        None
                    }
                    None => {
                        self.error(
                            "S008",
                            format!("unknown variable '{name}'"),
                            expression.span,
                        );
                        None
                    }
                }
            }
            ExprKind::Field { base, name } => match self.check_expression(base) {
                Some(Type::Record(id)) => {
                    let field = self
                        .records
                        .get(&id)
                        .filter(|record| {
                            !crate::collections::is_collection(&record.name)
                                && self
                                    .channel_program
                                    .channels
                                    .as_ref()
                                    .and_then(|a| a.schema(Type::Record(id)))
                                    .is_none()
                        })
                        .and_then(|record| record.fields.iter().find(|field| field.name == *name))
                        .map(|field| field.ty);
                    if field.is_none() {
                        self.error(
                            "S026",
                            format!(
                                "record '{}' has no field '{name}'",
                                self.type_name(Type::Record(id))
                            ),
                            expression.span,
                        );
                    }
                    field
                }
                Some(ty) => {
                    self.error(
                        "S026",
                        format!(
                            "field access requires a record, found {}",
                            self.type_name(ty)
                        ),
                        base.span,
                    );
                    None
                }
                None => None,
            },
            ExprKind::Unary { op, operand } => {
                let actual = self.check_expression(operand);
                let (expected, context) = match op {
                    UnaryOp::Neg => (
                        if actual == Some(Type::Float) {
                            Type::Float
                        } else {
                            Type::Int
                        },
                        "unary '-'",
                    ),
                    UnaryOp::Not => (Type::Bool, "unary 'not'"),
                    UnaryOp::BitNot => (Type::Int, "unary '~'"),
                };
                self.require_type(actual, expected, operand.span, context)
                    .then_some(expected)
            }
            ExprKind::Binary { op, left, right } => {
                let left_ty = self.check_expression(left);
                let right_ty = self.check_expression(right);
                let numeric = if left_ty == Some(Type::Float) {
                    Type::Float
                } else {
                    Type::Int
                };
                let (operand_type, result_type, name) = match op {
                    BinaryOp::Add => (numeric, numeric, "+"),
                    BinaryOp::Sub => (numeric, numeric, "-"),
                    BinaryOp::Mul => (numeric, numeric, "*"),
                    BinaryOp::Div => (numeric, numeric, "/"),
                    BinaryOp::Rem => (Type::Int, Type::Int, "%"),
                    BinaryOp::BitAnd => (Type::Int, Type::Int, "&"),
                    BinaryOp::BitOr => (Type::Int, Type::Int, "|"),
                    BinaryOp::BitXor => (Type::Int, Type::Int, "^"),
                    BinaryOp::Shl => (Type::Int, Type::Int, "<<"),
                    BinaryOp::Shr => (Type::Int, Type::Int, ">>"),
                    BinaryOp::Lt => (numeric, Type::Bool, "<"),
                    BinaryOp::Le => (numeric, Type::Bool, "<="),
                    BinaryOp::Gt => (numeric, Type::Bool, ">"),
                    BinaryOp::Ge => (numeric, Type::Bool, ">="),
                    BinaryOp::And => (Type::Bool, Type::Bool, "and"),
                    BinaryOp::Or => (Type::Bool, Type::Bool, "or"),
                    BinaryOp::Eq | BinaryOp::Ne => {
                        let name = if *op == BinaryOp::Eq { "==" } else { "!=" };
                        let ty = match (left_ty, right_ty) {
                            (Some(Type::Int), Some(Type::Int))
                            | (Some(Type::Float), Some(Type::Float))
                            | (Some(Type::Bool), Some(Type::Bool)) => Some(Type::Bool),
                            (Some(left), Some(right)) => {
                                self.error(
                                    "S010",
                                    format!(
                                        "operator '{name}' requires two int, two float, or two bool operands; found {} and {}", self.type_name(left), self.type_name(right)
                                    ),
                                    expression.span,
                                );
                                None
                            }
                            _ => None,
                        };
                        expression.ty = ty;
                        return ty;
                    }
                };
                let context = format!("operator '{name}'");
                let left_valid = self.require_type(left_ty, operand_type, left.span, &context);
                let right_valid = self.require_type(right_ty, operand_type, right.span, &context);
                (left_valid && right_valid).then_some(result_type)
            }
            ExprKind::Call { name, args } => self.check_call(name, args, expression.span),
            ExprKind::MethodCall { .. } => unreachable!("method calls are lowered above"),
        };
        expression.ty = ty;
        ty
    }

    fn check_method_call(&mut self, expression: &mut Expr) -> Option<Type> {
        let ExprKind::MethodCall {
            mut receiver,
            name,
            mut args,
        } = std::mem::replace(&mut expression.kind, ExprKind::Int(0))
        else {
            unreachable!()
        };
        if let ExprKind::Name(enum_name) = &receiver.kind {
            if self.enum_names.contains_key(enum_name) && self.lookup_local(enum_name).is_none() {
                let key = method_key(enum_name, &name);
                let types = args
                    .iter_mut()
                    .map(|arg| self.check_expression(arg))
                    .collect::<Vec<_>>();
                let ty = self.check_typed_call(&key, &mut args, &types, expression.span);
                expression.kind = ExprKind::Call { name: key, args };
                expression.ty = ty;
                return ty;
            }
        }
        let receiver_ty = self.check_expression(&mut receiver);
        let mut arg_types = vec![receiver_ty];
        arg_types.extend(args.iter_mut().map(|arg| self.check_expression(arg)));
        let resolved = if let Some(Type::Record(id)) = receiver_ty {
            let record = self.records.get(&id);
            if record.is_some_and(|record| {
                self.channel_program
                    .channels
                    .as_ref()
                    .and_then(|a| {
                        a.operation(
                            &self.channel_program,
                            &method_key(&self.type_name(Type::Record(id)), &name),
                        )
                    })
                    .is_some()
                    || record.methods.iter().any(|method| method.name == name)
                    || crate::collections::schema(record)
                        .ok()
                        .flatten()
                        .is_some_and(|schema| schema.method(&name).is_some())
            }) {
                Some(method_key(&self.type_name(Type::Record(id)), &name))
            } else {
                self.error(
                    "S029",
                    format!(
                        "record '{}' has no method '{name}'",
                        self.type_name(Type::Record(id))
                    ),
                    expression.span,
                );
                None
            }
        } else {
            if let Some(ty) = receiver_ty {
                self.error(
                    "S029",
                    format!(
                        "method receiver must be a record, found {}",
                        self.type_name(ty)
                    ),
                    receiver.span,
                );
            }
            None
        };
        let Some(key) = resolved else {
            expression.kind = ExprKind::MethodCall {
                receiver,
                name,
                args,
            };
            expression.ty = None;
            return None;
        };
        args.insert(0, *receiver);
        let ty = self.check_typed_call(&key, &mut args, &arg_types, expression.span);
        expression.kind = ExprKind::Call { name: key, args };
        expression.ty = ty;
        ty
    }

    fn check_call(&mut self, name: &str, args: &mut [Expr], span: Span) -> Option<Type> {
        if matches!(name, "spawn" | "join" | "async_spawn" | "await") {
            self.error(
                "S019",
                format!(
                    "'{name}' must be a complete variable initializer{}",
                    if matches!(name, "join" | "await") {
                        " or expression statement"
                    } else {
                        ""
                    }
                ),
                span,
            );
            return None;
        }
        let arg_types: Vec<_> = args
            .iter_mut()
            .map(|arg| self.check_expression(arg))
            .collect();
        self.check_typed_call(name, args, &arg_types, span)
    }

    fn check_typed_call(
        &mut self,
        name: &str,
        args: &mut [Expr],
        arg_types: &[Option<Type>],
        span: Span,
    ) -> Option<Type> {
        if let Some(operation) = self
            .channel_program
            .channels
            .as_ref()
            .and_then(|a| a.operation(&self.channel_program, name))
        {
            self.check_arity(name, operation.params.len(), args.len(), span);
            self.check_argument_aliases(args);
            for ((argument, actual), expected) in args.iter().zip(arg_types).zip(operation.params) {
                self.require_type(*actual, expected, argument.span, "channel argument");
            }
            return Some(operation.result);
        }
        if let Some((owner, method)) = name.split_once('.') {
            if let Some(id) = self.record_names.get(owner).copied() {
                if let Ok(Some(schema)) = crate::collections::schema(&self.records[&id]) {
                    let Some((mut params, result)) = schema.method(method) else {
                        self.error(
                            "S029",
                            format!("unknown collection method '{method}'"),
                            span,
                        );
                        return None;
                    };
                    params.insert(0, Type::Record(id));
                    self.check_arity(name, params.len(), args.len(), span);
                    self.check_argument_aliases(args);
                    for ((arg, actual), expected) in args.iter().zip(arg_types).zip(params) {
                        self.require_type(*actual, expected, arg.span, "collection argument");
                    }
                    return Some(result);
                }
            }
        }
        if let Some((enum_name, variant_name)) = name.split_once('.') {
            if let Some(id) = self.enum_names.get(enum_name).copied() {
                let variant = self.enums[&id]
                    .variants
                    .iter()
                    .find(|variant| variant.name == variant_name)
                    .cloned();
                let Some(variant) = variant else {
                    self.error(
                        "S031",
                        format!("enum '{enum_name}' has no variant '{variant_name}'"),
                        span,
                    );
                    return None;
                };
                if variant.fields.len() != args.len() {
                    self.error(
                        "S012",
                        format!(
                            "enum constructor '{name}' expects {} argument(s), received {}",
                            variant.fields.len(),
                            args.len()
                        ),
                        span,
                    );
                }
                for ((arg, actual), field) in args.iter().zip(arg_types).zip(&variant.fields) {
                    self.require_type(*actual, field.ty, arg.span, "enum payload");
                }
                return Some(Type::Enum(id));
            }
        }
        if self.lookup_local(name).is_some() {
            self.error(
                "S016",
                format!("'{name}' resolves to a local variable and cannot be called"),
                span,
            );
            return None;
        }
        if let Some(id) = self.record_names.get(name).copied() {
            if let Some(authority) = &self.channel_program.channels {
                if let Some(schema) = authority.schema(Type::Record(id)) {
                    match schema {
                        crate::channels::Schema::Endpoint { .. } => {
                            self.error("S029", "endpoint constructors are compiler-only", span);
                            return None;
                        }
                        crate::channels::Schema::Channel { .. } => {
                            if !authority.factory_allowed(id, &self.current_function, span) {
                                self.error(
                                    "S029",
                                    "Channel construction requires its registered factory",
                                    span,
                                );
                                return None;
                            }
                            self.check_arity(name, 1, args.len(), span);
                            if let Some(actual) = arg_types.first() {
                                self.require_type(*actual, Type::Int, span, "channel capacity");
                            }
                            return Some(Type::Record(id));
                        }
                    }
                }
            }
            let record = self.records[&id].clone();
            if crate::collections::is_collection(&record.name) {
                self.check_arity(name, 0, args.len(), span);
                return Some(Type::Record(id));
            }
            self.check_arity(name, record.fields.len(), args.len(), span);
            for (index, ((arg, actual), field)) in args
                .iter()
                .zip(arg_types.iter().copied())
                .zip(&record.fields)
                .enumerate()
            {
                self.require_type(
                    actual,
                    field.ty,
                    arg.span,
                    &format!(
                        "argument {} for field '{}' of '{name}'",
                        index + 1,
                        field.name
                    ),
                );
            }
            return Some(Type::Record(id));
        }
        self.check_argument_aliases(args);
        if name == "print" {
            self.check_arity(name, 1, args.len(), span);
            for (arg, ty) in args.iter().zip(arg_types.iter().copied()) {
                let ty = self.require_value(ty, arg.span, "print argument");
                if ty.is_some_and(|ty| {
                    !matches!(
                        ty,
                        Type::Int | Type::Float | Type::Bool | Type::Str | Type::Buffer
                    )
                }) {
                    self.error(
                        "S010",
                        "print requires int, float, bool, str or buffer",
                        arg.span,
                    );
                }
            }
            return Some(Type::Void);
        }
        if crate::builtins::is_builtin(name) {
            let types: Option<Vec<_>> = arg_types.iter().copied().collect();
            let types = types?;
            return match crate::builtins::signature(name, &types) {
                Some(Ok(ty)) => Some(ty),
                Some(Err(message)) => {
                    let message = if types.iter().any(|ty| matches!(ty, Type::Record(_))) {
                        format!(
                            "invalid arguments to builtin '{name}': ({})",
                            types
                                .iter()
                                .map(|ty| self.type_name(*ty))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        message
                    };
                    self.error("S023", message, span);
                    None
                }
                None => {
                    self.error("S009", format!("unknown builtin '{name}'"), span);
                    None
                }
            };
        }
        let Some(signature) = self.signatures.get(name).cloned() else {
            self.error("S009", format!("unknown function '{name}'"), span);
            return None;
        };
        self.check_arity(name, signature.params.len(), args.len(), span);
        for (index, (arg, actual)) in args.iter().zip(arg_types.iter().copied()).enumerate() {
            let context = if name.contains('.') {
                if index == 0 {
                    format!("receiver of method '{name}'")
                } else {
                    format!("argument {index} to method '{name}'")
                }
            } else {
                format!("argument {} to '{name}'", index + 1)
            };
            let actual = self.require_value(actual, arg.span, &context);
            if let Some(expected) = signature.params.get(index) {
                self.require_type(actual, *expected, arg.span, &context);
            }
        }
        Some(signature.return_type)
    }

    fn check_arity(&mut self, name: &str, expected: usize, actual: usize, span: Span) {
        if expected != actual {
            let message = if name.contains('.') {
                format!(
                    "method '{name}' expects {} argument(s), received {}",
                    expected.saturating_sub(1),
                    actual.saturating_sub(1)
                )
            } else {
                format!("function '{name}' expects {expected} argument(s), received {actual}")
            };
            self.error("S012", message, span);
        }
    }
}

pub(crate) fn normalize_enum_types(program: &mut Program) {
    let ids = program
        .enums
        .iter()
        .filter_map(|declaration| {
            program
                .type_names
                .iter()
                .position(|name| name == &declaration.name)
        })
        .collect::<BTreeSet<_>>();
    fn ty(value: &mut Type, ids: &BTreeSet<usize>) {
        if let Type::Record(id) = *value {
            if ids.contains(&id) {
                *value = Type::Enum(id);
            }
        }
    }
    fn expression(value: &mut Expr, ids: &BTreeSet<usize>) {
        if let Some(value) = &mut value.ty {
            ty(value, ids);
        }
        match &mut value.kind {
            ExprKind::Call { args, .. } => {
                for arg in args {
                    expression(arg, ids);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                expression(receiver, ids);
                for arg in args {
                    expression(arg, ids);
                }
            }
            ExprKind::Field { base, .. } | ExprKind::Unary { operand: base, .. } => {
                expression(base, ids)
            }
            ExprKind::Binary { left, right, .. } => {
                expression(left, ids);
                expression(right, ids);
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Name(_)
            | ExprKind::Move { .. } => {}
        }
    }
    fn block(body: &mut Block, ids: &BTreeSet<usize>) {
        for statement in body {
            match &mut statement.kind {
                StmtKind::Var {
                    annotation, value, ..
                } => {
                    if let Some(value) = annotation {
                        ty(value, ids);
                    }
                    expression(value, ids);
                }
                StmtKind::TryVar {
                    annotation,
                    success_type,
                    value,
                    ..
                } => {
                    if let Some(value) = annotation {
                        ty(value, ids);
                    }
                    if let Some(value) = success_type {
                        ty(value, ids);
                    }
                    expression(value, ids);
                }
                StmtKind::Assign { value, .. } | StmtKind::Expr(value) => expression(value, ids),
                StmtKind::FieldAssign { target, value } => {
                    expression(target, ids);
                    expression(value, ids);
                }
                StmtKind::Return(value) => {
                    if let Some(value) = value {
                        expression(value, ids);
                    }
                }
                StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    expression(condition, ids);
                    block(then_block, ids);
                    if let Some(body) = else_block {
                        block(body, ids);
                    }
                }
                StmtKind::While { condition, body } => {
                    expression(condition, ids);
                    block(body, ids);
                }
                StmtKind::For {
                    source,
                    element_type,
                    body,
                    ..
                } => {
                    if let Some(value) = element_type {
                        ty(value, ids);
                    }
                    match source {
                        ForSource::Range { start, end, step } => {
                            expression(start, ids);
                            expression(end, ids);
                            if let Some(step) = step {
                                expression(step, ids);
                            }
                        }
                        ForSource::Iterable(value) => expression(value, ids),
                    }
                    block(body, ids);
                }
                StmtKind::Break | StmtKind::Continue => {}
                StmtKind::Match { value, arms } => {
                    expression(value, ids);
                    for arm in arms {
                        for binding in &mut arm.bindings {
                            if let Some(value) = &mut binding.ty {
                                ty(value, ids);
                            }
                        }
                        block(&mut arm.body, ids);
                    }
                }
            }
        }
    }
    for record in &mut program.records {
        for field in &mut record.fields {
            ty(&mut field.ty, &ids);
        }
    }
    for declaration in &mut program.enums {
        for variant in &mut declaration.variants {
            for field in &mut variant.fields {
                ty(&mut field.ty, &ids);
            }
        }
    }
    for function in program.functions.iter_mut().chain(
        program
            .records
            .iter_mut()
            .flat_map(|record| &mut record.methods),
    ) {
        for param in &mut function.params {
            ty(&mut param.ty, &ids);
        }
        ty(&mut function.return_type, &ids);
        if let Some(body) = &mut function.body {
            block(body, &ids);
        }
    }
}

fn access_path(expression: &Expr) -> Option<Vec<String>> {
    match &expression.kind {
        ExprKind::Name(name) => Some(vec![name.clone()]),
        ExprKind::Field { base, name } => {
            let mut path = access_path(base)?;
            path.push(name.clone());
            Some(path)
        }
        _ => None,
    }
}

// Record-taking calls can replace descendants of their borrowed arguments.
// Constructors copy their inputs, and container operations cannot replace the
// owning record's field. Still visit their arguments for nested mutating calls.
fn argument_mutations(
    expression: &Expr,
    record_names: &BTreeMap<String, usize>,
    enum_names: &BTreeMap<String, usize>,
    output: &mut Vec<(Vec<String>, Span)>,
) {
    match &expression.kind {
        ExprKind::Call { name, args } => {
            for arg in args {
                if !record_names.contains_key(name)
                    && !name
                        .split_once('.')
                        .is_some_and(|(owner, _)| enum_names.contains_key(owner))
                    && matches!(arg.ty, Some(Type::Record(_)))
                {
                    if let Some(path) = access_path(arg) {
                        output.push((path, arg.span));
                    }
                }
                argument_mutations(arg, record_names, enum_names, output);
            }
        }
        ExprKind::Field { base, .. } => argument_mutations(base, record_names, enum_names, output),
        ExprKind::Unary { operand, .. } => {
            argument_mutations(operand, record_names, enum_names, output);
        }
        ExprKind::Binary { left, right, .. } => {
            argument_mutations(left, record_names, enum_names, output);
            argument_mutations(right, record_names, enum_names, output);
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            argument_mutations(receiver, record_names, enum_names, output);
            for arg in args {
                argument_mutations(arg, record_names, enum_names, output);
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Name(_)
        | ExprKind::Move { .. } => {}
    }
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "print" | "spawn" | "join" | "range")
        || crate::builtins::is_builtin(name)
        || name.starts_with("bhasa_rt_")
}

pub(crate) fn call_graph(program: &Program) -> BTreeMap<String, BTreeSet<String>> {
    fn expression(expr: &Expr, calls: &mut BTreeSet<String>) {
        match &expr.kind {
            ExprKind::Call { name, args } => {
                calls.insert(name.clone());
                for arg in args {
                    expression(arg, calls);
                }
            }
            ExprKind::Unary { operand, .. } => expression(operand, calls),
            ExprKind::Field { base, .. } => expression(base, calls),
            ExprKind::Binary { left, right, .. } => {
                expression(left, calls);
                expression(right, calls);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                expression(receiver, calls);
                for arg in args {
                    expression(arg, calls);
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Name(_)
            | ExprKind::Move { .. } => {}
        }
    }
    fn block(statements: &Block, calls: &mut BTreeSet<String>) {
        for statement in statements {
            match &statement.kind {
                StmtKind::Var { value, .. }
                | StmtKind::TryVar { value, .. }
                | StmtKind::Assign { value, .. }
                | StmtKind::Expr(value) => expression(value, calls),
                StmtKind::FieldAssign { target, value } => {
                    expression(target, calls);
                    expression(value, calls);
                }
                StmtKind::Return(Some(value)) => expression(value, calls),
                StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    expression(condition, calls);
                    block(then_block, calls);
                    if let Some(else_block) = else_block {
                        block(else_block, calls);
                    }
                }
                StmtKind::While { condition, body } => {
                    expression(condition, calls);
                    block(body, calls);
                }
                StmtKind::For {
                    source,
                    element_type: _,
                    body,
                    ..
                } => {
                    match source {
                        ForSource::Range { start, end, step } => {
                            expression(start, calls);
                            expression(end, calls);
                            if let Some(step) = step {
                                expression(step, calls);
                            }
                        }
                        ForSource::Iterable(value) => expression(value, calls),
                    }
                    block(body, calls);
                }
                StmtKind::Break | StmtKind::Continue => {}
                StmtKind::Match { value, arms } => {
                    expression(value, calls);
                    for arm in arms {
                        block(&arm.body, calls);
                    }
                }
                StmtKind::Return(None) => {}
            }
        }
    }
    program
        .functions
        .iter()
        .map(|function| (function.name.clone(), function))
        .chain(program.records.iter().flat_map(|record| {
            record
                .methods
                .iter()
                .map(move |method| (method_key(&record.name, &method.name), method))
        }))
        .map(|(key, function)| {
            let mut calls = BTreeSet::new();
            if let Some(body) = &function.body {
                block(body, &mut calls);
            }
            (key, calls)
        })
        .collect()
}

/// Also used by emission so a corrupted checked AST cannot transfer file state.
pub(crate) fn file_worker_violation(
    program: &Program,
    signatures: &BTreeMap<String, FunctionSig>,
    calls: &BTreeMap<String, BTreeSet<String>>,
    worker: &str,
) -> Option<String> {
    let mut pending = vec![worker.to_owned()];
    let mut visited = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if crate::builtins::is_file_operation(&name)
            || signatures.get(&name).is_some_and(|signature| {
                program.contains_file_handle(signature.return_type)
                    || signature
                        .params
                        .iter()
                        .any(|ty| program.contains_file_handle(*ty))
            })
        {
            return Some(name);
        }
        if let Some(edges) = calls.get(&name) {
            pending.extend(edges.iter().cloned());
        }
    }
    None
}

/// Repeated by codegen to reject a modified checked worker graph.
pub(crate) fn channel_worker_effects(program: &Program) -> Vec<Diagnostic> {
    let Some(authority) = &program.channels else {
        return Vec::new();
    };
    let graph = call_graph(program);
    let mut workers = Vec::new();
    crate::modules::visit_program_expressions(program, &mut |expression| {
        if let ExprKind::Call { name, args } = &expression.kind {
            if matches!(name.as_str(), "spawn" | "async_spawn") {
                if let Some(Expr {
                    kind: ExprKind::Call { name: worker, .. },
                    ..
                }) = args.first()
                {
                    workers.push((worker.clone(), expression.span));
                }
            }
        }
    });
    let mut errors = Vec::new();
    let externs: BTreeSet<_> = program
        .functions
        .iter()
        .filter(|function| function.body.is_none())
        .map(|function| function.name.clone())
        .chain(program.records.iter().flat_map(|record| {
            record
                .methods
                .iter()
                .filter(|method| method.body.is_none())
                .map(|method| method_key(&record.name, &method.name))
        }))
        .collect();
    for (worker, span) in workers {
        let Some(function) = program
            .functions
            .iter()
            .find(|function| function.name == worker)
        else {
            errors.push(Diagnostic::new(
                "S020",
                "channel worker must be a global Bhasa function",
                span,
            ));
            continue;
        };
        if function.body.is_none()
            || function.params.len() != 1
            || crate::channel_transfer::check_worker_transfer(
                program,
                authority.registry(),
                authority.context(),
                function.params[0].ty,
                &[],
            )
            .is_err()
            || (function.return_type != Type::Void
                && crate::channel_transfer::check_worker_transfer(
                    program,
                    authority.registry(),
                    authority.context(),
                    function.return_type,
                    &[],
                )
                .is_err())
        {
            errors.push(Diagnostic::new(
                "S020",
                "channel worker has an unsupported transfer signature",
                span,
            ));
            continue;
        }
        let mut pending = vec![worker.clone()];
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if externs.contains(&current) || matches!(current.as_str(), "async_spawn" | "await") {
                errors.push(Diagnostic::new("S020", format!("worker '{worker}' reaches forbidden extern or pooled operation '{current}'"), span));
                break;
            }
            if authority
                .operation(program, &current)
                .is_some_and(|operation| operation.blocking())
            {
                errors.push(Diagnostic::new(
                    "S020",
                    format!("worker '{worker}' reaches blocking channel operation '{current}'"),
                    span,
                ));
                break;
            }
            if let Some(calls) = graph.get(&current) {
                pending.extend(calls.iter().cloned());
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(kind: ExprKind) -> Expr {
        Expr::new(kind, Span::new(1, 2))
    }

    fn int(value: i64) -> Expr {
        expr(ExprKind::Int(value))
    }

    fn boolean(value: bool) -> Expr {
        expr(ExprKind::Bool(value))
    }

    fn string(value: &str) -> Expr {
        expr(ExprKind::Str(value.into()))
    }

    fn name(value: &str) -> Expr {
        expr(ExprKind::Name(value.into()))
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        expr(ExprKind::Call {
            name: name.into(),
            args,
        })
    }

    fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        expr(ExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn stmt(kind: StmtKind) -> Stmt {
        Stmt {
            kind,
            span: Span::new(0, 3),
        }
    }

    fn evaluate(value: Expr) -> Stmt {
        stmt(StmtKind::Expr(value))
    }

    fn var(name: &str, annotation: Option<Type>, value: Expr) -> Stmt {
        stmt(StmtKind::Var {
            name: name.into(),
            reassignable: true,
            annotation,
            value,
        })
    }

    fn assign(name: &str, value: Expr) -> Stmt {
        stmt(StmtKind::Assign {
            name: name.into(),
            value,
        })
    }

    fn ret(value: Expr) -> Stmt {
        stmt(StmtKind::Return(Some(value)))
    }

    fn branch(then_block: Block, else_block: Option<Block>) -> Stmt {
        stmt(StmtKind::If {
            condition: boolean(true),
            then_block,
            else_block,
        })
    }

    fn function(
        name: &str,
        params: &[(&str, Type)],
        result: Type,
        body: Option<Block>,
    ) -> Function {
        Function {
            name: name.into(),
            params: params
                .iter()
                .map(|(name, ty)| Param {
                    mode: crate::ast::ParamMode::Borrowed,
                    name: (*name).into(),
                    ty: *ty,
                    span: Span::new(4, 5),
                })
                .collect(),
            return_type: result,
            body,
            span: Span::new(0, 10),
        }
    }

    fn program(body: Block) -> Program {
        Program {
            channels: None,
            records: Vec::new(),
            enums: Vec::new(),
            type_names: Vec::new(),
            functions: vec![function("main", &[], Type::Void, Some(body))],
        }
    }

    fn errors(program: Program) -> Vec<Diagnostic> {
        check(program).expect_err("program should be rejected")
    }

    fn rejects(program: Program, message: &str) {
        let diagnostics = errors(program);
        assert!(
            diagnostics
                .iter()
                .any(|error| error.message.contains(message)),
            "expected {message:?}, received {diagnostics:?}"
        );
        assert!(diagnostics.iter().all(|error| error.code.starts_with('S')));
    }

    #[test]
    fn forward_calls_recursion_and_external_signatures() {
        let checked = check(Program {
            channels: None,
            records: Vec::new(),
            enums: Vec::new(),
            type_names: Vec::new(),
            functions: vec![
                function(
                    "caller",
                    &[],
                    Type::Int,
                    Some(vec![ret(call("later", vec![int(1)]))]),
                ),
                function(
                    "later",
                    &[("n", Type::Int)],
                    Type::Int,
                    Some(vec![branch(
                        vec![ret(call("later", vec![name("n")]))],
                        Some(vec![ret(call("native", vec![name("n")]))]),
                    )]),
                ),
                function("native", &[("n", Type::Int)], Type::Int, None),
            ],
        })
        .unwrap();
        assert_eq!(checked.signatures.len(), 3);
        assert_eq!(
            checked.signatures["native"],
            FunctionSig {
                modes: vec![ParamMode::Borrowed],
                params: vec![Type::Int],
                return_type: Type::Int,
                external: true,
            }
        );
        assert!(!checked.signatures["later"].external);
        assert!(!checked.signatures.contains_key("print"));
    }

    #[test]
    fn optional_main_and_legal_entry_points() {
        check(Program {
            channels: None,
            records: Vec::new(),
            enums: Vec::new(),
            type_names: Vec::new(),
            functions: vec![],
        })
        .unwrap();
        check(program(vec![])).unwrap();
        check(Program {
            channels: None,
            records: Vec::new(),
            enums: Vec::new(),
            type_names: Vec::new(),
            functions: vec![function("main", &[], Type::Int, Some(vec![ret(int(0))]))],
        })
        .unwrap();
    }

    #[test]
    fn invalid_entry_points_are_rejected() {
        for function in [
            function("main", &[], Type::Void, None),
            function("main", &[("x", Type::Int)], Type::Void, Some(vec![])),
            function("main", &[], Type::Bool, Some(vec![ret(boolean(true))])),
            function("main", &[], Type::Str, Some(vec![ret(string("bad"))])),
        ] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function],
                },
                "'main' must be a Bhasa definition",
            );
        }
    }

    #[test]
    fn duplicate_functions_and_extern_definition_collisions_are_rejected() {
        for body in [None, Some(vec![])] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![
                        function("f", &[], Type::Void, body),
                        function("f", &[], Type::Void, Some(vec![])),
                    ],
                },
                "function 'f' is declared more than once",
            );
        }
    }

    #[test]
    fn print_is_reserved_for_all_declarations() {
        for body in [None, Some(vec![])] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function("print", &[], Type::Void, body)],
                },
                "reserved builtin",
            );
        }
        rejects(
            program(vec![var("print", None, int(1))]),
            "reserved builtin",
        );
        rejects(
            Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function(
                    "f",
                    &[("print", Type::Int)],
                    Type::Void,
                    Some(vec![]),
                )],
            },
            "reserved builtin",
        );
    }

    #[test]
    fn externs_accept_only_int_parameters_and_int_or_void_results() {
        check(Program {
            channels: None,
            records: Vec::new(),
            enums: Vec::new(),
            type_names: Vec::new(),
            functions: vec![function("f", &[("x", Type::Int)], Type::Void, None)],
        })
        .unwrap();
        for ty in [Type::Bool, Type::Str, Type::Void] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function("f", &[("x", ty)], Type::Void, None)],
                },
                if ty == Type::Void {
                    "cannot have type void"
                } else {
                    "must have type int"
                },
            );
        }
        for ty in [Type::Bool, Type::Str] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function("f", &[], ty, None)],
                },
                "must return int or void",
            );
        }
    }

    #[test]
    fn parameters_cannot_be_void_or_duplicate_including_externs() {
        rejects(
            Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function(
                    "f",
                    &[("x", Type::Void)],
                    Type::Void,
                    Some(vec![]),
                )],
            },
            "cannot have type void",
        );
        for body in [None, Some(vec![])] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function(
                        "f",
                        &[("x", Type::Int), ("x", Type::Int)],
                        Type::Void,
                        body,
                    )],
                },
                "parameter 'x' is declared more than once",
            );
        }
    }

    #[test]
    fn parameters_share_the_outer_body_scope() {
        rejects(
            Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function(
                    "f",
                    &[("x", Type::Int)],
                    Type::Void,
                    Some(vec![var("x", None, int(2))]),
                )],
            },
            "already declared in this scope",
        );
        rejects(
            program(vec![var("x", None, int(1)), var("x", None, int(2))]),
            "already declared in this scope",
        );
    }

    #[test]
    fn nested_shadowing_and_assignment_restore_outer_types() {
        check(program(vec![
            var("x", None, int(1)),
            branch(
                vec![
                    var("x", None, binary(BinaryOp::Eq, name("x"), int(1))),
                    assign("x", boolean(false)),
                ],
                Some(vec![assign("x", int(2))]),
            ),
            assign("x", int(3)),
            stmt(StmtKind::While {
                condition: boolean(false),
                body: vec![assign("x", int(4)), var("x", None, string("nested"))],
            }),
            assign("x", int(5)),
        ]))
        .unwrap();
    }

    #[test]
    fn branch_and_loop_locals_do_not_escape() {
        rejects(
            program(vec![branch(
                vec![var("x", None, int(1))],
                Some(vec![evaluate(name("x"))]),
            )]),
            "unknown variable 'x'",
        );
        rejects(
            program(vec![
                branch(vec![var("x", None, int(1))], None),
                evaluate(name("x")),
            ]),
            "unknown variable 'x'",
        );
        rejects(
            program(vec![
                stmt(StmtKind::While {
                    condition: boolean(false),
                    body: vec![var("x", None, int(1))],
                }),
                evaluate(name("x")),
            ]),
            "unknown variable 'x'",
        );
    }

    #[test]
    fn initializers_cannot_see_their_own_new_binding() {
        rejects(
            program(vec![var("x", None, name("x"))]),
            "unknown variable 'x'",
        );
        let diagnostics = errors(program(vec![
            var("x", None, name("missing")),
            evaluate(name("x")),
        ]));
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn annotations_and_assignments_require_matching_types() {
        rejects(
            program(vec![var("x", Some(Type::Int), boolean(true))]),
            "variable initializer requires int, found bool",
        );
        rejects(
            program(vec![var("x", None, int(1)), assign("x", string("bad"))]),
            "assignment requires int, found str",
        );
        rejects(
            program(vec![assign("missing", int(1))]),
            "cannot assign to unknown variable 'missing'",
        );
    }

    #[test]
    fn locals_cannot_have_void_type_or_initializers() {
        rejects(
            program(vec![var("x", Some(Type::Void), int(1))]),
            "variable cannot have type void",
        );
        rejects(
            program(vec![var("x", None, call("print", vec![int(1)]))]),
            "initializer cannot use a void expression",
        );
        rejects(
            program(vec![
                var("x", None, int(1)),
                assign("x", call("print", vec![int(1)])),
            ]),
            "assignment cannot use a void expression",
        );
    }

    #[test]
    fn calls_check_arity_argument_types_and_existence() {
        let mut input = program(vec![
            evaluate(call("f", vec![])),
            evaluate(call("f", vec![boolean(true)])),
            evaluate(call("f", vec![int(1), name("missing")])),
            evaluate(call("unknown", vec![])),
        ]);
        input
            .functions
            .push(function("f", &[("x", Type::Int)], Type::Void, Some(vec![])));
        let diagnostics = errors(input);
        for message in [
            "expects 1 argument(s), received 0",
            "argument 1 to 'f' requires int, found bool",
            "expects 1 argument(s), received 2",
            "unknown variable 'missing'",
            "unknown function 'unknown'",
        ] {
            assert!(
                diagnostics
                    .iter()
                    .any(|error| error.message.contains(message)),
                "missing {message:?}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn local_function_shadowing_and_bare_function_values_are_rejected() {
        let mut input = program(vec![var("f", None, int(1)), evaluate(call("f", vec![]))]);
        input
            .functions
            .push(function("f", &[], Type::Void, Some(vec![])));
        rejects(input, "local variable and cannot be called");
        rejects(
            program(vec![evaluate(name("main"))]),
            "function 'main' cannot be used as a value",
        );
        rejects(
            program(vec![evaluate(name("print"))]),
            "function 'print' cannot be used as a value",
        );
    }

    #[test]
    fn print_accepts_each_value_type_and_checks_arity() {
        check(program(vec![
            evaluate(call("print", vec![int(1)])),
            evaluate(call("print", vec![boolean(true)])),
            evaluate(call("print", vec![string("नमस्ते")])),
        ]))
        .unwrap();
        rejects(
            program(vec![evaluate(call("print", vec![]))]),
            "expects 1 argument(s), received 0",
        );
        rejects(
            program(vec![evaluate(call("print", vec![int(1), int(2)]))]),
            "expects 1 argument(s), received 2",
        );
        rejects(
            program(vec![evaluate(call(
                "print",
                vec![call("print", vec![int(1)])],
            ))]),
            "print argument cannot use a void expression",
        );
    }

    #[test]
    fn void_calls_are_rejected_in_arguments_conditions_and_returns() {
        let void = || call("print", vec![int(1)]);
        let mut input = program(vec![evaluate(call("f", vec![void()]))]);
        input
            .functions
            .push(function("f", &[("x", Type::Int)], Type::Void, Some(vec![])));
        rejects(input, "argument 1 to 'f' cannot use a void expression");
        rejects(
            program(vec![stmt(StmtKind::If {
                condition: void(),
                then_block: vec![],
                else_block: None,
            })]),
            "if condition requires bool, found void",
        );
        rejects(
            program(vec![ret(void())]),
            "return cannot use a void expression",
        );
        rejects(
            program(vec![evaluate(binary(BinaryOp::Add, int(1), void()))]),
            "operator '+' requires int, found void",
        );
    }

    #[test]
    fn every_binary_operator_accepts_only_its_supported_types() {
        for op in [
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Rem,
            BinaryOp::Lt,
            BinaryOp::Le,
            BinaryOp::Gt,
            BinaryOp::Ge,
            BinaryOp::Eq,
            BinaryOp::Ne,
            BinaryOp::And,
            BinaryOp::Or,
        ] {
            for left in [int(1), boolean(true), string("a")] {
                for right in [int(2), boolean(false), string("b")] {
                    let same_int = matches!(
                        (&left.kind, &right.kind),
                        (ExprKind::Int(_), ExprKind::Int(_))
                    );
                    let same_bool = matches!(
                        (&left.kind, &right.kind),
                        (ExprKind::Bool(_), ExprKind::Bool(_))
                    );
                    let valid = match op {
                        BinaryOp::Eq | BinaryOp::Ne => same_int || same_bool,
                        BinaryOp::And | BinaryOp::Or => same_bool,
                        _ => same_int,
                    };
                    let result = check(program(vec![evaluate(binary(
                        op,
                        left.clone(),
                        right.clone(),
                    ))]));
                    assert_eq!(
                        result.is_ok(),
                        valid,
                        "{op:?}, {:?}, {:?}: {result:?}",
                        left.kind,
                        right.kind
                    );
                }
            }
        }
    }

    #[test]
    fn unary_operators_check_operand_types() {
        for (op, operand, valid) in [
            (UnaryOp::Neg, int(1), true),
            (UnaryOp::Neg, boolean(true), false),
            (UnaryOp::Not, boolean(false), true),
            (UnaryOp::Not, int(1), false),
            (UnaryOp::Neg, call("print", vec![int(1)]), false),
        ] {
            assert_eq!(
                check(program(vec![evaluate(expr(ExprKind::Unary {
                    op,
                    operand: Box::new(operand)
                }))]))
                .is_ok(),
                valid
            );
        }
    }

    #[test]
    fn conditions_require_bool_without_implicit_conversion() {
        rejects(
            program(vec![stmt(StmtKind::If {
                condition: int(1),
                then_block: vec![],
                else_block: None,
            })]),
            "if condition requires bool, found int",
        );
        rejects(
            program(vec![stmt(StmtKind::While {
                condition: string("true"),
                body: vec![],
            })]),
            "while condition requires bool, found str",
        );
    }

    #[test]
    fn returns_match_the_function_result() {
        rejects(
            program(vec![ret(int(1))]),
            "void function cannot return a value",
        );
        rejects(
            Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function(
                    "f",
                    &[],
                    Type::Int,
                    Some(vec![stmt(StmtKind::Return(None))]),
                )],
            },
            "return requires a value of type int",
        );
        rejects(
            Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function("f", &[], Type::Bool, Some(vec![ret(int(1))]))],
            },
            "return requires bool, found int",
        );
        check(program(vec![stmt(StmtKind::Return(None))])).unwrap();
    }

    #[test]
    fn every_path_must_return_in_nonvoid_functions() {
        for body in [
            vec![],
            vec![branch(vec![ret(int(1))], None)],
            vec![branch(vec![ret(int(1))], Some(vec![]))],
            vec![stmt(StmtKind::While {
                condition: boolean(true),
                body: vec![ret(int(1))],
            })],
        ] {
            rejects(
                Program {
                    channels: None,
                    records: Vec::new(),
                    enums: Vec::new(),
                    type_names: Vec::new(),
                    functions: vec![function("f", &[], Type::Int, Some(body))],
                },
                "must return int on every path",
            );
        }
        for body in [
            vec![branch(vec![ret(int(1))], Some(vec![ret(int(2))]))],
            vec![branch(vec![ret(int(1))], None), ret(int(2))],
            vec![branch(
                vec![branch(vec![ret(int(1))], Some(vec![ret(int(2))]))],
                Some(vec![ret(int(3))]),
            )],
        ] {
            check(Program {
                channels: None,
                records: Vec::new(),
                enums: Vec::new(),
                type_names: Vec::new(),
                functions: vec![function("f", &[], Type::Int, Some(body))],
            })
            .unwrap();
        }
    }

    #[test]
    fn guaranteed_returns_make_following_statements_unreachable() {
        for body in [
            vec![stmt(StmtKind::Return(None)), evaluate(int(1))],
            vec![
                branch(
                    vec![stmt(StmtKind::Return(None))],
                    Some(vec![stmt(StmtKind::Return(None))]),
                ),
                evaluate(int(1)),
            ],
            vec![branch(
                vec![stmt(StmtKind::Return(None)), evaluate(int(1))],
                None,
            )],
        ] {
            rejects(
                program(body),
                "unreachable statement after a guaranteed return",
            );
        }
        let diagnostics = errors(program(vec![
            stmt(StmtKind::Return(None)),
            evaluate(name("missing")),
        ]));
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn internal_functions_support_bool_and_static_strings() {
        let mut input = program(vec![
            var(
                "text",
                Some(Type::Str),
                call("identity", vec![string("hello")]),
            ),
            assign("text", string("world")),
            evaluate(call("print", vec![name("text")])),
            evaluate(call("truth", vec![boolean(true)])),
        ]);
        input.functions.push(function(
            "identity",
            &[("value", Type::Str)],
            Type::Str,
            Some(vec![ret(name("value"))]),
        ));
        input.functions.push(function(
            "truth",
            &[("value", Type::Bool)],
            Type::Bool,
            Some(vec![ret(name("value"))]),
        ));
        check(input).unwrap();
    }

    #[test]
    fn diagnostics_preserve_relevant_source_spans() {
        let mut missing = name("missing");
        missing.span = Span::new(42, 49);
        let diagnostics = errors(program(vec![evaluate(missing)]));
        assert_eq!(diagnostics[0].span, Span::new(42, 49));
        rejects(
            program(vec![evaluate(string("embedded\0nul"))]),
            "cannot contain NUL",
        );
    }

    #[test]
    fn successful_check_recomputes_and_populates_every_expression_type() {
        fn visit_expr(expression: &Expr) {
            assert!(
                expression.ty.is_some(),
                "missing annotation: {expression:?}"
            );
            match &expression.kind {
                ExprKind::Unary { operand, .. } => visit_expr(operand),
                ExprKind::Field { base, .. } => visit_expr(base),
                ExprKind::Binary { left, right, .. } => {
                    visit_expr(left);
                    visit_expr(right);
                }
                ExprKind::Call { name, args } => {
                    if name == "print" {
                        assert_eq!(expression.ty, Some(Type::Void));
                    }
                    for arg in args {
                        visit_expr(arg);
                    }
                }
                ExprKind::MethodCall { receiver, args, .. } => {
                    visit_expr(receiver);
                    for arg in args {
                        visit_expr(arg);
                    }
                }
                ExprKind::Int(_)
                | ExprKind::Float(_)
                | ExprKind::Bool(_)
                | ExprKind::Str(_)
                | ExprKind::Name(_)
                | ExprKind::Move { .. } => {}
            }
        }
        fn visit_block(block: &Block) {
            for statement in block {
                match &statement.kind {
                    StmtKind::Var { value, .. }
                    | StmtKind::TryVar { value, .. }
                    | StmtKind::Assign { value, .. }
                    | StmtKind::Expr(value)
                    | StmtKind::Return(Some(value)) => visit_expr(value),
                    StmtKind::FieldAssign { target, value } => {
                        visit_expr(target);
                        visit_expr(value);
                    }
                    StmtKind::If {
                        condition,
                        then_block,
                        else_block,
                    } => {
                        visit_expr(condition);
                        visit_block(then_block);
                        if let Some(block) = else_block {
                            visit_block(block);
                        }
                    }
                    StmtKind::While { condition, body } => {
                        visit_expr(condition);
                        visit_block(body);
                    }
                    StmtKind::For {
                        source,
                        element_type: _,
                        body,
                        ..
                    } => {
                        match source {
                            ForSource::Range { start, end, step } => {
                                visit_expr(start);
                                visit_expr(end);
                                if let Some(step) = step {
                                    visit_expr(step);
                                }
                            }
                            ForSource::Iterable(value) => visit_expr(value),
                        }
                        visit_block(body);
                    }
                    StmtKind::Break | StmtKind::Continue => {}
                    StmtKind::Match { value, arms } => {
                        visit_expr(value);
                        for arm in arms {
                            visit_block(&arm.body);
                        }
                    }
                    StmtKind::Return(None) => {}
                }
            }
        }
        let mut wrong_annotation = int(3);
        wrong_annotation.ty = Some(Type::Str);
        let checked = check(program(vec![
            var("n", Some(Type::Int), wrong_annotation),
            branch(
                vec![assign("n", binary(BinaryOp::Add, name("n"), int(1)))],
                Some(vec![evaluate(call("print", vec![string("else")]))]),
            ),
            stmt(StmtKind::While {
                condition: binary(BinaryOp::Lt, name("n"), int(10)),
                body: vec![
                    assign(
                        "n",
                        expr(ExprKind::Unary {
                            op: UnaryOp::Neg,
                            operand: Box::new(name("n")),
                        }),
                    ),
                    evaluate(call("print", vec![binary(BinaryOp::Eq, name("n"), int(0))])),
                ],
            }),
            evaluate(call("print", vec![name("n")])),
            stmt(StmtKind::Return(None)),
        ]))
        .unwrap();
        let body = checked.program.functions[0].body.as_ref().unwrap();
        visit_block(body);
        let StmtKind::Var { value, .. } = &body[0].kind else {
            panic!("expected variable")
        };
        assert_eq!(value.ty, Some(Type::Int));
    }
}
