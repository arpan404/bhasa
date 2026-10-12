//! Bounded buffer/ints transfer analysis. Pending results and tasks stay separate.
use crate::ast::*;
use crate::Diagnostic;
use std::collections::BTreeMap;

type Key = (usize, String);
#[derive(Clone)]
struct Binding {
    ty: Type,
    owner: bool,
    initialized: bool,
    mutable: bool,
    control: usize,
}
struct Analysis<'a> {
    program: &'a Program,
    modes: BTreeMap<String, Vec<ParamMode>>,
    scopes: Vec<BTreeMap<String, Binding>>,
    control: usize,
    method: bool,
    errors: Vec<Diagnostic>,
}
fn eligible(ty: Type) -> bool {
    matches!(ty, Type::Buffer | Type::Ints)
}
impl Analysis<'_> {
    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.errors.push(Diagnostic::new("S038", message, span));
    }
    fn key(&self, name: &str) -> Option<Key> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, scope)| scope.contains_key(name))
            .map(|(index, _)| (index, name.into()))
    }
    fn binding(&self, key: &Key) -> &Binding {
        &self.scopes[key.0][&key.1]
    }
    fn read(&mut self, name: &str, span: Span) {
        if let Some(key) = self.key(name) {
            if !self.binding(&key).initialized {
                self.error(format!("moved binding '{name}' is uninitialized"), span);
            }
        }
    }
    fn root(&self, value: &Expr) -> Option<Key> {
        match &value.kind {
            ExprKind::Name(name) => self.key(name),
            ExprKind::Field { base, .. } => self.root(base),
            _ => None,
        }
    }
    fn expression(&mut self, value: &Expr, allow_move: bool, loans: &[Key]) {
        match &value.kind {
            ExprKind::Move { name, .. } => {
                let Some(key) = self.key(name) else {
                    self.error("move source is not a local binding", value.span);
                    return;
                };
                let binding = self.binding(&key).clone();
                if !allow_move || self.method {
                    self.error("move is only allowed as a whole local initializer, owned argument or return",value.span);
                    return;
                }
                if !eligible(binding.ty) || !binding.owner {
                    self.error("move requires an owned buffer or ints binding", value.span);
                    return;
                }
                if value.ty != Some(binding.ty) {
                    self.error("move has inconsistent checked type", value.span);
                    return;
                }
                if !binding.initialized {
                    self.error(format!("binding '{name}' was already moved"), value.span);
                    return;
                }
                if binding.control != self.control {
                    self.error(
                        "cannot move an outer binding from a conditional or loop",
                        value.span,
                    );
                    return;
                }
                if loans.contains(&key) {
                    self.error("move overlaps an earlier borrowed argument", value.span);
                    return;
                }
                self.scopes[key.0].get_mut(name).unwrap().initialized = false;
            }
            ExprKind::Name(name) => self.read(name, value.span),
            ExprKind::Call { name, args } => {
                if matches!(name.as_str(), "spawn" | "async_spawn") {
                    for arg in args {
                        if let ExprKind::Call {
                            name: worker,
                            args: inputs,
                        } = &arg.kind
                        {
                            if self
                                .modes
                                .get(worker)
                                .is_some_and(|modes| modes.contains(&ParamMode::Owned))
                            {
                                self.error("task targets cannot have owned parameters", arg.span);
                            }
                            for input in inputs {
                                self.expression(input, false, loans);
                            }
                        } else {
                            self.expression(arg, false, loans);
                        }
                    }
                    return;
                }
                let modes = self.modes.get(name).cloned().unwrap_or_default();
                let mut active = loans.to_vec();
                for (index, arg) in args.iter().enumerate() {
                    let owned = modes.get(index) == Some(&ParamMode::Owned);
                    if owned && !matches!(arg.kind, ExprKind::Move { .. } | ExprKind::Call { .. }) {
                        self.error(
                            "owned arguments require move or a fresh owned result",
                            arg.span,
                        );
                    }
                    self.expression(arg, owned, &active);
                    if !owned && arg.ty.is_some_and(Type::is_owned) {
                        if let Some(root) = self.root(arg) {
                            active.push(root);
                        }
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.expression(operand, false, loans),
            ExprKind::Binary { left, right, op } => {
                self.expression(left, false, loans);
                let conditional = matches!(op, BinaryOp::And | BinaryOp::Or);
                if conditional {
                    self.control += 1;
                }
                self.expression(right, false, loans);
                if conditional {
                    self.control -= 1;
                }
            }
            ExprKind::Field { base, .. } => self.expression(base, false, loans),
            ExprKind::MethodCall { receiver, args, .. } => {
                self.expression(receiver, false, loans);
                let mut active = loans.to_vec();
                if let Some(root) = self.root(receiver) {
                    active.push(root);
                }
                for arg in args {
                    self.expression(arg, false, &active);
                }
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {}
        }
    }
    fn block(&mut self, body: &Block, scoped: bool) {
        if scoped {
            self.scopes.push(BTreeMap::new());
        }
        for statement in body {
            match &statement.kind {
                StmtKind::Var {
                    name,
                    reassignable,
                    value,
                    ..
                } => {
                    if matches!(&value.kind,ExprKind::Move{name:source,..} if source==name) {
                        self.error("self move is unsupported", value.span);
                    }
                    self.expression(value, true, &[]);
                    self.scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Binding {
                            ty: value.ty.unwrap_or(Type::Void),
                            owner: true,
                            initialized: true,
                            mutable: *reassignable,
                            control: self.control,
                        },
                    );
                }
                StmtKind::TryVar {
                    name,
                    reassignable,
                    success_type,
                    value,
                    ..
                } => {
                    self.expression(value, false, &[]);
                    self.scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Binding {
                            ty: success_type.unwrap_or(Type::Void),
                            owner: true,
                            initialized: true,
                            mutable: *reassignable,
                            control: self.control,
                        },
                    );
                }
                StmtKind::Assign { name, value } => {
                    self.expression(value, false, &[]);
                    if let Some(key) = self.key(name) {
                        let binding = self.binding(&key);
                        if !binding.initialized
                            && (!binding.mutable || key.0 + 1 != self.scopes.len())
                        {
                            self.error("moved binding can only reinitialize in its mutable straight-line scope",statement.span);
                        }
                        self.scopes[key.0].get_mut(name).unwrap().initialized = true;
                    }
                }
                StmtKind::FieldAssign { target, value } => {
                    self.expression(target, false, &[]);
                    self.expression(value, false, &[]);
                }
                StmtKind::Expr(value) => self.expression(value, false, &[]),
                StmtKind::Return(Some(value)) => self.expression(value, true, &[]),
                StmtKind::Return(None) => {}
                StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.control += 1;
                    self.expression(condition, false, &[]);
                    self.block(then_block, true);
                    if let Some(body) = else_block {
                        self.block(body, true);
                    }
                    self.control -= 1;
                }
                StmtKind::While { condition, body } => {
                    self.control += 1;
                    self.expression(condition, false, &[]);
                    self.block(body, true);
                    self.control -= 1;
                }
                StmtKind::For {
                    name, source, body, ..
                } => {
                    self.control += 1;
                    let ty = match source {
                        ForSource::Range { start, end, step } => {
                            self.expression(start, false, &[]);
                            self.expression(end, false, &[]);
                            if let Some(step) = step {
                                self.expression(step, false, &[]);
                            }
                            Type::Int
                        }
                        ForSource::Iterable(value) => {
                            self.expression(value, false, &[]);
                            match value.ty {
                                Some(Type::Record(id)) => self
                                    .program
                                    .record(id)
                                    .and_then(|record| {
                                        crate::collections::schema(record).ok().flatten()
                                    })
                                    .map(|schema| match schema {
                                        crate::collections::Schema::List(value) => value,
                                        crate::collections::Schema::Map(key, _) => key,
                                    })
                                    .unwrap_or(Type::Int),
                                _ => Type::Int,
                            }
                        }
                    };
                    self.scopes.push(BTreeMap::new());
                    self.scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Binding {
                            ty,
                            owner: true,
                            initialized: true,
                            mutable: true,
                            control: self.control,
                        },
                    );
                    self.block(body, false);
                    self.scopes.pop();
                    self.control -= 1;
                }
                StmtKind::Match { value, arms } => {
                    self.expression(value, false, &[]);
                    self.control += 1;
                    for arm in arms {
                        self.scopes.push(BTreeMap::new());
                        if let Some(Type::Enum(id)) = value.ty {
                            if let Some(variant) = self
                                .program
                                .enum_decl(id)
                                .and_then(|e| e.variants.iter().find(|v| v.name == arm.variant))
                            {
                                for (name, field) in arm.bindings.iter().zip(&variant.fields) {
                                    self.scopes.last_mut().unwrap().insert(
                                        name.name.clone(),
                                        Binding {
                                            ty: field.ty,
                                            owner: true,
                                            initialized: true,
                                            mutable: true,
                                            control: self.control,
                                        },
                                    );
                                }
                            }
                        }
                        self.block(&arm.body, false);
                        self.scopes.pop();
                    }
                    self.control -= 1;
                }
                _ => {}
            }
        }
        if scoped {
            self.scopes.pop();
        }
    }
}
pub fn check_modes(program: &Program) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for (function, method) in program.functions.iter().map(|f| (f, false)).chain(
        program
            .records
            .iter()
            .flat_map(|r| r.methods.iter().map(|f| (f, true))),
    ) {
        for param in &function.params {
            if param.mode == ParamMode::Owned
                && (!eligible(param.ty)
                    || function.body.is_none()
                    || method
                    || function.name.starts_with("__bhg_"))
            {
                errors.push(Diagnostic::new("S038", "owned parameters require buffer/ints on monomorphic synchronous free functions", param.span));
            }
        }
    }
    errors
}
pub fn check(program: &Program) -> Vec<Diagnostic> {
    let modes = program
        .functions
        .iter()
        .map(|f| (f.name.clone(), f.params.iter().map(|p| p.mode).collect()))
        .collect();
    let mut analysis = Analysis {
        program,
        modes,
        scopes: Vec::new(),
        control: 0,
        method: false,
        errors: check_modes(program),
    };
    let functions = program.functions.iter().map(|f| (f, false)).chain(
        program
            .records
            .iter()
            .flat_map(|r| r.methods.iter().map(|f| (f, true))),
    );
    for (function, method) in functions {
        analysis.method = method;
        analysis.control = 0;
        analysis.scopes = vec![BTreeMap::new()];
        for param in &function.params {
            analysis.scopes[0].insert(
                param.name.clone(),
                Binding {
                    ty: param.ty,
                    owner: param.mode == ParamMode::Owned,
                    initialized: true,
                    mutable: true,
                    control: 0,
                },
            );
        }
        if let Some(body) = &function.body {
            analysis.block(body, false);
        }
    }
    analysis.errors
}
pub fn check_checked(program: &CheckedProgram) -> Vec<Diagnostic> {
    let mut errors = check(&program.program);
    for function in &program.program.functions {
        if program
            .signatures
            .get(&function.name)
            .is_none_or(|signature| {
                signature.modes != function.params.iter().map(|p| p.mode).collect::<Vec<_>>()
            })
        {
            errors.push(Diagnostic::new(
                "S038",
                "inconsistent checked parameter modes",
                function.span,
            ));
        }
    }
    errors
}
