//! Shared bootstrap syntax tree. Changes require integration coordination.
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn join(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Buffer,
    Ints,
    TcpListener,
    TcpSocket,
    TlsSocket,
    Task,
    Void,
    /// Index into the containing program's `type_names` table.
    Record(usize),
    /// Index into the same nominal type-name table as records.
    Enum(usize),
}
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Str => "str",
            Self::Buffer => "buffer",
            Self::Ints => "ints",
            Self::TcpListener => "tcp_listener",
            Self::TcpSocket => "tcp_socket",
            Self::TlsSocket => "tls_socket",
            Self::Task => "task",
            Self::Void => "void",
            Self::Record(id) => return write!(f, "record#{id}"),
            Self::Enum(id) => return write!(f, "enum#{id}"),
        })
    }
}
impl Type {
    pub fn is_owned(self) -> bool {
        matches!(
            self,
            Self::Buffer
                | Self::Ints
                | Self::TcpListener
                | Self::TcpSocket
                | Self::TlsSocket
                | Self::Record(_)
                | Self::Enum(_)
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub functions: Vec<Function>,
    pub records: Vec<RecordDecl>,
    pub enums: Vec<EnumDecl>,
    /// Includes unresolved names so checking can report unknown types by name.
    pub type_names: Vec<String>,
}
impl Program {
    pub fn type_name(&self, ty: Type) -> String {
        match ty {
            Type::Record(id) | Type::Enum(id) => self
                .type_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("record#{id}")),
            _ => ty.to_string(),
        }
    }

    pub fn record(&self, id: usize) -> Option<&RecordDecl> {
        let name = self.type_names.get(id)?;
        self.records.iter().find(|record| &record.name == name)
    }

    pub fn enum_decl(&self, id: usize) -> Option<&EnumDecl> {
        let name = self.type_names.get(id)?;
        self.enums
            .iter()
            .find(|declaration| &declaration.name == name)
    }

    pub fn intern_type_name(&mut self, name: &str) -> usize {
        if let Some(id) = self.type_names.iter().position(|existing| existing == name) {
            id
        } else {
            let id = self.type_names.len();
            self.type_names.push(name.to_owned());
            id
        }
    }

    /// Merge a parsed module after converting its local type IDs to this program's
    /// IDs. Duplicate declarations remain intact for semantic diagnostics.
    pub fn merge(&mut self, mut other: Program) {
        let ids: Vec<_> = other
            .type_names
            .iter()
            .map(|name| self.intern_type_name(name))
            .collect();
        for record in &mut other.records {
            for field in &mut record.fields {
                remap_type(&mut field.ty, &ids);
            }
            for method in &mut record.methods {
                remap_function(method, &ids);
            }
        }
        for function in &mut other.functions {
            remap_function(function, &ids);
        }
        for declaration in &mut other.enums {
            for variant in &mut declaration.variants {
                for field in &mut variant.fields {
                    remap_type(&mut field.ty, &ids);
                }
            }
        }
        self.records.extend(other.records);
        self.enums.extend(other.enums);
        self.functions.extend(other.functions);
    }
}

/// Qualified method keys cannot be spelled as source-level global identifiers.
pub fn method_key(record: &str, method: &str) -> String {
    format!("{record}.{method}")
}

fn remap_function(function: &mut Function, ids: &[usize]) {
    for param in &mut function.params {
        remap_type(&mut param.ty, ids);
    }
    remap_type(&mut function.return_type, ids);
    if let Some(body) = &mut function.body {
        remap_block(body, ids);
    }
}

fn remap_type(ty: &mut Type, ids: &[usize]) {
    if let Type::Record(id) | Type::Enum(id) = ty {
        *id = ids[*id];
    }
}

fn remap_expression(expr: &mut Expr, ids: &[usize]) {
    if let Some(ty) = &mut expr.ty {
        remap_type(ty, ids);
    }
    match &mut expr.kind {
        ExprKind::Unary { operand, .. } => remap_expression(operand, ids),
        ExprKind::Binary { left, right, .. } => {
            remap_expression(left, ids);
            remap_expression(right, ids);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                remap_expression(arg, ids);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            remap_expression(receiver, ids);
            for arg in args {
                remap_expression(arg, ids);
            }
        }
        ExprKind::Field { base, .. } => remap_expression(base, ids),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Name(_) => {}
    }
}

fn remap_block(block: &mut Block, ids: &[usize]) {
    for statement in block {
        match &mut statement.kind {
            StmtKind::Var {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    remap_type(ty, ids);
                }
                remap_expression(value, ids);
            }
            StmtKind::TryVar {
                annotation,
                success_type,
                value,
                ..
            } => {
                if let Some(ty) = annotation {
                    remap_type(ty, ids);
                }
                if let Some(ty) = success_type {
                    remap_type(ty, ids);
                }
                remap_expression(value, ids);
            }
            StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
                remap_expression(value, ids);
            }
            StmtKind::FieldAssign { target, value } => {
                remap_expression(target, ids);
                remap_expression(value, ids);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    remap_expression(value, ids);
                }
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                remap_expression(condition, ids);
                remap_block(then_block, ids);
                if let Some(block) = else_block {
                    remap_block(block, ids);
                }
            }
            StmtKind::While { condition, body } => {
                remap_expression(condition, ids);
                remap_block(body, ids);
            }
            StmtKind::For {
                source,
                element_type,
                body,
                ..
            } => {
                match source {
                    ForSource::Range { start, end, step } => {
                        remap_expression(start, ids);
                        remap_expression(end, ids);
                        if let Some(step) = step {
                            remap_expression(step, ids);
                        }
                    }
                    ForSource::Iterable(value) => remap_expression(value, ids),
                }
                if let Some(ty) = element_type {
                    remap_type(ty, ids);
                }
                remap_block(body, ids);
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Match { value, arms } => {
                remap_expression(value, ids);
                for arm in arms {
                    for binding in &mut arm.bindings {
                        if let Some(ty) = &mut binding.ty {
                            remap_type(ty, ids);
                        }
                    }
                    remap_block(&mut arm.body, ids);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordDecl {
    pub name: String,
    pub fields: Vec<RecordField>,
    /// Source method names and declarations remain attached to their record.
    /// The first parameter is the inferred, borrowed `self` receiver.
    pub methods: Vec<Function>,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct RecordField {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}
/// The concrete enum protocol accepted by initializer propagation and bare-discard checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultShape {
    pub ok_index: usize,
    pub error_index: usize,
    pub success: Type,
    pub error: Type,
}
impl EnumDecl {
    pub fn result_shape(&self) -> Option<ResultShape> {
        if self.variants.len() != 2 {
            return None;
        }
        let (ok_index, ok) = self
            .variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant.name == "ok")?;
        let (error_index, error) = self
            .variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant.name == "error")?;
        let [success] = ok.fields.as_slice() else {
            return None;
        };
        let [failure] = error.fields.as_slice() else {
            return None;
        };
        if matches!(success.ty, Type::Void | Type::Task)
            || matches!(failure.ty, Type::Void | Type::Task)
        {
            return None;
        }
        Some(ResultShape {
            ok_index,
            error_index,
            success: success.ty,
            error: failure.ty,
        })
    }
}
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<RecordField>,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub enum_name: String,
    pub variant: String,
    pub bindings: Vec<MatchBinding>,
    pub body: Block,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct MatchBinding {
    pub name: String,
    pub ty: Option<Type>,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type,
    /// None declares a trusted C ABI import; Some defines a Bhasa function.
    pub body: Option<Block>,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}
pub type Block = Vec<Stmt>;
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub enum StmtKind {
    Var {
        name: String,
        annotation: Option<Type>,
        value: Expr,
    },
    /// Only an entire variable initializer may propagate an error.
    TryVar {
        name: String,
        name_span: Span,
        annotation: Option<Type>,
        value: Expr,
        try_span: Span,
        /// Filled by checking; value.ty remains the operand enum type.
        success_type: Option<Type>,
    },
    Assign {
        name: String,
        value: Expr,
    },
    FieldAssign {
        target: Expr,
        value: Expr,
    },
    Expr(Expr),
    Return(Option<Expr>),
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    While {
        condition: Expr,
        body: Block,
    },
    For {
        name: String,
        name_span: Span,
        source: ForSource,
        element_type: Option<Type>,
        body: Block,
    },
    Break,
    Continue,
    Match {
        value: Expr,
        arms: Vec<MatchArm>,
    },
}
#[derive(Debug, Clone)]
pub enum ForSource {
    /// A half-open range. Bounds and optional signed step run once, in order.
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        step: Option<Box<Expr>>,
    },
    /// The loop owns a deep snapshot of this collection and each yielded value.
    /// Buffers and ints yield integers; Maps yield their int/str keys.
    Iterable(Box<Expr>),
}
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub ty: Option<Type>,
}
impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self {
            kind,
            span,
            ty: None,
        }
    }
}
#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Name(String),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        name: String,
        args: Vec<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSig {
    pub params: Vec<Type>,
    pub return_type: Type,
    pub external: bool,
}
#[derive(Debug, Clone)]
pub struct CheckedProgram {
    pub program: Program,
    pub signatures: BTreeMap<String, FunctionSig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression_types(expr: &mut Expr, visit: &mut impl FnMut(&mut Type)) {
        visit(expr.ty.get_or_insert(Type::Record(0)));
        match &mut expr.kind {
            ExprKind::Unary { operand, .. } => expression_types(operand, visit),
            ExprKind::Binary { left, right, .. } => {
                expression_types(left, visit);
                expression_types(right, visit);
            }
            ExprKind::Call { args, .. } => {
                for arg in args {
                    expression_types(arg, visit);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                expression_types(receiver, visit);
                for arg in args {
                    expression_types(arg, visit);
                }
            }
            ExprKind::Field { base, .. } => expression_types(base, visit),
            _ => {}
        }
    }

    fn block_expression_types(block: &mut Block, visit: &mut impl FnMut(&mut Type)) {
        for statement in block {
            match &mut statement.kind {
                StmtKind::TryVar {
                    value,
                    success_type,
                    ..
                } => {
                    if let Some(ty) = success_type {
                        visit(ty);
                    }
                    expression_types(value, visit);
                }
                StmtKind::Var { value, .. }
                | StmtKind::Assign { value, .. }
                | StmtKind::Expr(value) => expression_types(value, visit),
                StmtKind::FieldAssign { target, value } => {
                    expression_types(target, visit);
                    expression_types(value, visit);
                }
                StmtKind::Return(Some(value)) => expression_types(value, visit),
                StmtKind::Return(None) => {}
                StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    expression_types(condition, visit);
                    block_expression_types(then_block, visit);
                    if let Some(block) = else_block {
                        block_expression_types(block, visit);
                    }
                }
                StmtKind::While { condition, body } => {
                    expression_types(condition, visit);
                    block_expression_types(body, visit);
                }
                StmtKind::For {
                    source,
                    element_type,
                    body,
                    ..
                } => {
                    if let Some(ty) = element_type {
                        visit(ty);
                    }
                    match source {
                        ForSource::Range { start, end, step } => {
                            expression_types(start, visit);
                            expression_types(end, visit);
                            if let Some(step) = step {
                                expression_types(step, visit);
                            }
                        }
                        ForSource::Iterable(value) => expression_types(value, visit),
                    }
                    block_expression_types(body, visit);
                }
                StmtKind::Break | StmtKind::Continue => {}
                StmtKind::Match { value, arms } => {
                    expression_types(value, visit);
                    for arm in arms {
                        block_expression_types(&mut arm.body, visit);
                    }
                }
            }
        }
    }

    #[test]
    fn module_merge_remaps_declarations_annotations_and_checked_expressions() {
        let mut combined = crate::parser::parse(
            "struct Seed {} def first(value: Outer) -> Inner { return value.inner }",
        )
        .unwrap();
        let mut module = crate::parser::parse(
            "struct Inner { value: int } struct Outer { inner: Inner }\n\
             def second(value: Inner) -> Outer {\n\
                 var local: Outer = Outer(value)\n\
                 local.inner = value\n\
                 if true { var nested: Inner = value; local = Outer(nested) }\n\
                 else { while false { var nested: Outer = local; print(-1 + 2) } }\n\
                 return local\n\
             }",
        )
        .unwrap();
        assert_eq!(module.type_names, ["Inner", "Outer"]);
        let mut count = 0;
        block_expression_types(module.functions[0].body.as_mut().unwrap(), &mut |_| {
            count += 1;
        });
        combined.merge(module);
        assert_eq!(combined.type_names, ["Seed", "Outer", "Inner"]);
        assert_eq!(combined.record(1).unwrap().fields[0].ty, Type::Record(2));
        assert_eq!(combined.functions[1].params[0].ty, Type::Record(2));
        assert_eq!(combined.functions[1].return_type, Type::Record(1));
        let body = combined.functions[1].body.as_mut().unwrap();
        assert!(matches!(
            body[0].kind,
            StmtKind::Var {
                annotation: Some(Type::Record(1)),
                ..
            }
        ));
        let StmtKind::If {
            then_block,
            else_block: Some(else_block),
            ..
        } = &body[2].kind
        else {
            panic!("expected if")
        };
        assert!(matches!(
            then_block[0].kind,
            StmtKind::Var {
                annotation: Some(Type::Record(2)),
                ..
            }
        ));
        let StmtKind::While {
            body: loop_body, ..
        } = &else_block[0].kind
        else {
            panic!("expected while")
        };
        assert!(matches!(
            loop_body[0].kind,
            StmtKind::Var {
                annotation: Some(Type::Record(1)),
                ..
            }
        ));
        let mut checked = 0;
        block_expression_types(body, &mut |ty| {
            assert_eq!(*ty, Type::Record(2));
            checked += 1;
        });
        assert_eq!(checked, count);
        assert!(checked > 15);
    }

    #[test]
    fn module_merge_preserves_duplicate_declarations_and_unresolved_names() {
        let mut program = crate::parser::parse("struct Point { value: Missing }").unwrap();
        program.merge(
            crate::parser::parse("struct Other { value: Missing } struct Point {}").unwrap(),
        );
        assert_eq!(program.type_names, ["Point", "Missing", "Other"]);
        assert_eq!(program.records.len(), 3);
        assert_eq!(program.records[1].fields[0].ty, Type::Record(1));
        assert_eq!(program.records[0].name, program.records[2].name);
        assert!(program.record(1).is_none());
        assert_eq!(program.type_name(Type::Record(1)), "Missing");
    }

    #[test]
    fn module_merge_remaps_method_signatures_bodies_and_receivers() {
        let mut combined = crate::parser::parse(
            "struct Seed {} def first(value: Outer) -> Inner { return value.inner }",
        )
        .unwrap();
        let mut module = crate::parser::parse(
            "struct Inner { value: int;\n\
                 def wrap(self, other: Outer) -> Outer {\n\
                     var result: Outer = other.with_inner(self)\n\
                     return result\n\
                 }\n\
             }\n\
             struct Outer { inner: Inner;\n\
                 def with_inner(self, inner: Inner) -> Outer {\n\
                     self.inner = inner\n\
                     return self\n\
                 }\n\
             }\n\
             def second(value: Inner) -> Outer { return value.wrap(Outer(value)) }",
        )
        .unwrap();
        assert_eq!(module.type_names, ["Inner", "Outer"]);
        let mut visited = 0;
        for function in module.functions.iter_mut().chain(
            module
                .records
                .iter_mut()
                .flat_map(|record| record.methods.iter_mut()),
        ) {
            block_expression_types(function.body.as_mut().unwrap(), &mut |_| visited += 1);
        }
        combined.merge(module);
        assert_eq!(combined.type_names, ["Seed", "Outer", "Inner"]);
        let inner = combined.record(2).unwrap();
        let wrap = &inner.methods[0];
        assert_eq!(wrap.name, "wrap");
        assert_eq!(wrap.params[0].ty, Type::Record(2));
        assert_eq!(wrap.params[1].ty, Type::Record(1));
        assert_eq!(wrap.return_type, Type::Record(1));
        assert!(matches!(
            wrap.body.as_ref().unwrap()[0].kind,
            StmtKind::Var {
                annotation: Some(Type::Record(1)),
                ..
            }
        ));
        let outer = combined.record(1).unwrap();
        assert_eq!(outer.methods[0].params[0].ty, Type::Record(1));
        assert_eq!(outer.methods[0].params[1].ty, Type::Record(2));
        assert_eq!(outer.methods[0].return_type, Type::Record(1));
        let mut checked = 0;
        for function in combined.functions.iter_mut().skip(1).chain(
            combined
                .records
                .iter_mut()
                .flat_map(|record| record.methods.iter_mut()),
        ) {
            block_expression_types(function.body.as_mut().unwrap(), &mut |ty| {
                assert_eq!(*ty, Type::Record(2));
                checked += 1;
            });
        }
        assert_eq!(checked, visited);
        assert!(checked > 10);
    }

    #[test]
    fn method_keys_preserve_names_without_ambiguous_concatenation() {
        assert_eq!(method_key("Point", "total"), "Point.total");
        assert_ne!(method_key("A_b", "c"), method_key("A", "b_c"));
        assert_ne!(method_key("Point", "total"), "Point_total");
    }
}
