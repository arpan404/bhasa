//! Parser for the scalar bootstrap language.

use crate::ast::{
    BinaryOp, Block, EnumDecl, EnumVariant, Expr, ExprKind, ForSource, Function, MatchArm,
    MatchBinding, Param, Program, RecordDecl, RecordField, Span, Stmt, StmtKind, Type, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{lex, Token, TokenKind};

/// Parse a complete source file. Expression types are filled in by semantic checking.
pub fn parse(source: &str) -> Result<Program, Vec<Diagnostic>> {
    parse_tokens(lex(source)?)
}

/// Parse a lexed file, including tokens whose spans were rebased by the module loader.
/// Reject malformed public input before the parser relies on its final EOF token.
pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, Vec<Diagnostic>> {
    if tokens.is_empty()
        || !matches!(tokens.last().map(|token| &token.kind), Some(TokenKind::Eof))
        || tokens[..tokens.len().saturating_sub(1)]
            .iter()
            .any(|token| token.kind == TokenKind::Eof)
    {
        return Err(vec![Diagnostic::new(
            "P001",
            "token input must end with exactly one EOF token",
            tokens.last().map_or(Span::default(), |token| token.span),
        )]);
    }
    Parser {
        tokens,
        cursor: 0,
        expression_block_depth: 0,
        type_names: Vec::new(),
    }
    .program()
    .map_err(|error| vec![error])
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    expression_block_depth: usize,
    type_names: Vec<String>,
}

type ParseResult<T> = Result<T, Diagnostic>;

impl Parser {
    fn program(&mut self) -> ParseResult<Program> {
        let mut functions = Vec::new();
        let mut records = Vec::new();
        let mut enums = Vec::new();
        self.skip_separators();
        while !self.at(&TokenKind::Eof) {
            if self.at(&TokenKind::Struct) {
                records.push(self.record()?);
            } else if self.at(&TokenKind::Enum) {
                enums.push(self.enumeration()?);
            } else {
                functions.push(self.function()?);
            }
            self.skip_separators();
        }
        Ok(Program {
            functions,
            records,
            enums,
            type_names: std::mem::take(&mut self.type_names),
        })
    }

    fn enumeration(&mut self) -> ParseResult<EnumDecl> {
        let start = self.advance().span;
        let (name, _) = self.identifier("an enum name")?;
        self.intern_type_name(&name);
        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace, "'{' after the enum name")?;
        let mut variants = Vec::new();
        self.skip_field_separators();
        while !self.at(&TokenKind::RightBrace) {
            let (name, start) = self.identifier("an enum variant name")?;
            let mut fields = Vec::new();
            let mut end = start;
            if self.take(&TokenKind::LeftParen).is_some() {
                while !self.at(&TokenKind::RightParen) {
                    let (name, start) = self.identifier("a payload name")?;
                    self.expect(&TokenKind::Colon, "':' and a payload type")?;
                    let (ty, end) = self.type_name()?;
                    fields.push(RecordField {
                        name,
                        ty,
                        span: start.join(end),
                    });
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                end = self
                    .expect(&TokenKind::RightParen, "')' after variant payloads")?
                    .span;
            }
            variants.push(EnumVariant {
                name,
                fields,
                span: start.join(end),
            });
            if !self.at(&TokenKind::RightBrace)
                && !matches!(
                    self.current().kind,
                    TokenKind::Newline | TokenKind::Semicolon | TokenKind::Comma
                )
            {
                return Err(self.error("expected a newline, ';', or ',' after the variant"));
            }
            self.skip_field_separators();
        }
        let end = self.advance().span;
        Ok(EnumDecl {
            name,
            variants,
            span: start.join(end),
        })
    }

    fn record(&mut self) -> ParseResult<RecordDecl> {
        let start = self.advance().span;
        let (name, _) = self.identifier("a struct name")?;
        let record_id = self.intern_type_name(&name);
        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace, "'{' after the struct name")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.skip_field_separators();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error("expected '}' to close the struct before end of file"));
            }
            if self.at(&TokenKind::Extern) {
                return Err(self.error("methods cannot be extern; declare a method body"));
            }
            if self.at(&TokenKind::Def) {
                methods.push(self.function_with_receiver(Some(record_id))?);
                self.skip_field_separators();
                continue;
            }
            let (name, start) = self.identifier("a field name")?;
            self.expect(&TokenKind::Colon, "':' and a type after the field name")?;
            let (ty, end) = self.type_name()?;
            fields.push(RecordField {
                name,
                ty,
                span: start.join(end),
            });
            if !self.at(&TokenKind::RightBrace)
                && !matches!(
                    self.current().kind,
                    TokenKind::Newline | TokenKind::Semicolon | TokenKind::Comma
                )
            {
                return Err(self.error("expected a newline, ';', or ',' after the field"));
            }
            self.skip_field_separators();
        }
        let end = self.advance().span;
        Ok(RecordDecl {
            name,
            fields,
            methods,
            span: start.join(end),
        })
    }

    fn function(&mut self) -> ParseResult<Function> {
        self.function_with_receiver(None)
    }

    fn function_with_receiver(&mut self, receiver: Option<usize>) -> ParseResult<Function> {
        let start = self.current().span;
        let external = self.take(&TokenKind::Extern).is_some();
        self.expect(&TokenKind::Def, "'def' to start a function declaration")?;
        let (name, _) = self.identifier("a function name")?;
        self.expect(&TokenKind::LeftParen, "'(' after the function name")?;
        let mut params = Vec::new();
        if let Some(record_id) = receiver {
            if !matches!(&self.current().kind, TokenKind::Ident(name) if name == "self") {
                return Err(self.error("methods require an untyped first 'self' parameter"));
            }
            let span = self.advance().span;
            if self.at(&TokenKind::Colon) {
                return Err(self.error("method receiver 'self' has its record type inferred; remove the type annotation"));
            }
            params.push(Param {
                name: "self".to_owned(),
                ty: Type::Record(record_id),
                span,
            });
            if !self.at(&TokenKind::RightParen) {
                self.expect(&TokenKind::Comma, "',' or ')' after the method receiver")?;
            }
        }
        if !self.at(&TokenKind::RightParen) {
            loop {
                let (name, param_start) = self.identifier("a parameter name")?;
                if receiver.is_some() && name == "self" {
                    return Err(Diagnostic::new(
                        "P001",
                        "method receiver 'self' can only appear as the first parameter",
                        param_start,
                    ));
                }
                self.expect(&TokenKind::Colon, "':' and a type after the parameter name")?;
                let (ty, type_span) = self.type_name()?;
                params.push(Param {
                    name,
                    ty,
                    span: param_start.join(type_span),
                });
                if self.take(&TokenKind::Comma).is_none() || self.at(&TokenKind::RightParen) {
                    break;
                }
            }
        }
        let close = self.expect(&TokenKind::RightParen, "')' after the parameters")?;
        let (return_type, signature_end) = if self.take(&TokenKind::Arrow).is_some() {
            self.type_name()?
        } else {
            (Type::Void, close.span)
        };

        let (body, end) = if external {
            if self.at(&TokenKind::LeftBrace) {
                return Err(self.error("extern functions cannot have a body"));
            }
            self.terminator()?;
            (None, signature_end)
        } else {
            if receiver.is_some() {
                self.skip_newlines();
                if !self.at(&TokenKind::LeftBrace) {
                    return Err(self.error("methods require a body beginning with '{'"));
                }
            }
            let (body, end) = self.block(0)?;
            (Some(body), end)
        };
        Ok(Function {
            name,
            params,
            return_type,
            body,
            span: start.join(end),
        })
    }

    fn block(&mut self, depth: usize) -> ParseResult<(Block, Span)> {
        self.check_depth(depth)?;
        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace, "'{' to start a block")?;
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error("expected '}' to close the block before end of file"));
            }
            statements.push(self.statement(depth)?);
            self.skip_separators();
        }
        let end = self.advance().span;
        Ok((statements, end))
    }

    // Keep pattern parsing in a separate frame: its many temporary tokens and
    // payload bindings otherwise enlarge every recursively parsed if/while body
    // in debug builds, exhausting the default thread stack before depth 128.
    fn statement(&mut self, depth: usize) -> ParseResult<Stmt> {
        if self.at(&TokenKind::Match) {
            self.match_statement(depth)
        } else {
            self.statement_without_match(depth)
        }
    }

    fn match_statement(&mut self, depth: usize) -> ParseResult<Stmt> {
        let start = self.current().span;
        self.advance();
        let value = self.expression(depth)?;
        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace, "'{' after the match value")?;
        let mut arms = Vec::new();
        self.skip_separators();
        while !self.at(&TokenKind::RightBrace) {
            let start = self.expect(&TokenKind::Case, "'case' in match")?.span;
            let (enum_name, _) = self.identifier("an enum name in case")?;
            self.expect(&TokenKind::Dot, "'.' after the enum name")?;
            let (variant, _) = self.identifier("a variant name")?;
            let mut bindings = Vec::new();
            if self.take(&TokenKind::LeftParen).is_some() {
                while !self.at(&TokenKind::RightParen) {
                    let (name, span) = self.identifier("a payload binding")?;
                    bindings.push(MatchBinding {
                        name,
                        span,
                        ty: None,
                    });
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RightParen, "')' after case bindings")?;
            }
            let (body, end) = self.block(depth + 1)?;
            arms.push(MatchArm {
                enum_name,
                variant,
                bindings,
                body,
                span: start.join(end),
            });
            self.skip_separators();
        }
        let end = self.advance().span;
        Ok(Stmt {
            kind: StmtKind::Match { value, arms },
            span: start.join(end),
        })
    }

    fn statement_without_match(&mut self, depth: usize) -> ParseResult<Stmt> {
        let start = self.current().span;
        let (kind, end, simple) = match self.current().kind {
            TokenKind::Var => {
                self.advance();
                let (name, _) = self.identifier("a variable name after 'var'")?;
                let annotation = if self.take(&TokenKind::Colon).is_some() {
                    Some(self.type_name()?.0)
                } else {
                    None
                };
                self.expect(
                    &TokenKind::Equal,
                    "'=' and an initializer in a variable declaration",
                )?;
                let value = self.expression(depth)?;
                let end = value.span;
                (
                    StmtKind::Var {
                        name,
                        annotation,
                        value,
                    },
                    end,
                    true,
                )
            }
            TokenKind::Return => {
                let keyword = self.advance();
                let value = if self.is_terminator() {
                    None
                } else {
                    Some(self.expression(depth)?)
                };
                let end = value.as_ref().map_or(keyword.span, |expr| expr.span);
                (StmtKind::Return(value), end, true)
            }
            TokenKind::If => {
                self.advance();
                let condition = self.expression(depth)?;
                let (then_block, mut end) = self.block(depth + 1)?;
                self.skip_newlines();
                let else_block = if self.take(&TokenKind::Else).is_some() {
                    let (block, else_end) = self.block(depth + 1)?;
                    end = else_end;
                    Some(block)
                } else {
                    None
                };
                (
                    StmtKind::If {
                        condition,
                        then_block,
                        else_block,
                    },
                    end,
                    false,
                )
            }
            TokenKind::While => {
                self.advance();
                let condition = self.expression(depth)?;
                let (body, end) = self.block(depth + 1)?;
                (StmtKind::While { condition, body }, end, false)
            }
            TokenKind::For => {
                self.advance();
                let (name, name_span) = self.identifier("a loop binding after 'for'")?;
                self.expect(&TokenKind::In, "'in' after a loop binding")?;
                let value = self.expression(depth)?;
                let source = match value.kind {
                    ExprKind::Call { name, mut args } if name == "range" => {
                        if args.is_empty() || args.len() > 3 {
                            return Err(Diagnostic::new(
                                "P006",
                                "range requires one, two, or three int arguments",
                                value.span,
                            ));
                        }
                        let step = (args.len() == 3).then(|| Box::new(args.pop().unwrap()));
                        let end = Box::new(args.pop().unwrap());
                        let start = Box::new(
                            args.pop()
                                .unwrap_or_else(|| Expr::new(ExprKind::Int(0), value.span)),
                        );
                        ForSource::Range { start, end, step }
                    }
                    kind => ForSource::List(Box::new(Expr { kind, ..value })),
                };
                let (body, end) = self.block(depth + 1)?;
                (
                    StmtKind::For {
                        name,
                        name_span,
                        source,
                        element_type: None,
                        body,
                    },
                    end,
                    false,
                )
            }
            TokenKind::Break | TokenKind::Continue => {
                let token = self.advance();
                let kind = if token.kind == TokenKind::Break {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                };
                (kind, token.span, true)
            }
            TokenKind::Def | TokenKind::Extern => {
                return Err(self.error("function declarations are only allowed at top level"));
            }
            TokenKind::Struct | TokenKind::Enum => {
                return Err(self.error("struct declarations are only allowed at top level"));
            }
            TokenKind::Ident(_) if self.next_at(&TokenKind::Equal) => {
                let (name, _) = self.identifier("an assignment target")?;
                self.advance();
                let value = self.expression(depth)?;
                let end = value.span;
                (StmtKind::Assign { name, value }, end, true)
            }
            _ => {
                let target_start = self.cursor;
                let target = self.expression(depth)?;
                let target_end = self.cursor;
                if self.take(&TokenKind::Equal).is_some() {
                    if !self.is_field_assignment_target(target_start, target_end) {
                        return Err(Diagnostic::new(
                            "P001",
                            "assignment target must be a variable or a field chain rooted in a variable",
                            target.span,
                        ));
                    }
                    let value = self.expression(depth)?;
                    let end = value.span;
                    (StmtKind::FieldAssign { target, value }, end, true)
                } else {
                    let end = target.span;
                    (StmtKind::Expr(target), end, true)
                }
            }
        };
        if simple {
            self.terminator()?;
        }
        Ok(Stmt {
            kind,
            span: start.join(end),
        })
    }

    fn expression(&mut self, block_depth: usize) -> ParseResult<Expr> {
        self.expression_block_depth = block_depth;
        let (expr, tree_depth) = self.binary_expression(0, block_depth)?;
        self.check_expression_depth(tree_depth, expr.span)?;
        Ok(expr)
    }

    // Carry AST depth separately from parser recursion: a flat left-associative
    // chain builds a deep tree without adding recursive parser calls.
    fn binary_expression(
        &mut self,
        minimum_precedence: u8,
        depth: usize,
    ) -> ParseResult<(Expr, usize)> {
        self.check_depth(depth)?;
        let (mut left, mut tree_depth) = self.unary(depth)?;
        while let Some((op, precedence)) = self.binary_operator() {
            if precedence < minimum_precedence {
                break;
            }
            self.advance();
            let (right, right_depth) = self.binary_expression(precedence + 1, depth + 1)?;
            let span = left.span.join(right.span);
            tree_depth = tree_depth.max(right_depth) + 1;
            self.check_expression_depth(tree_depth, span)?;
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok((left, tree_depth))
    }

    fn unary(&mut self, depth: usize) -> ParseResult<(Expr, usize)> {
        self.check_depth(depth)?;
        let op = match self.current().kind {
            TokenKind::Minus => UnaryOp::Neg,
            TokenKind::Not => UnaryOp::Not,
            TokenKind::BitNot => UnaryOp::BitNot,
            _ => return self.postfix(depth),
        };
        let start = self.advance().span;
        // This magnitude cannot be represented as a positive AST integer. Keep
        // the complete signed literal, while ordinary negation remains an op.
        if op == UnaryOp::Neg {
            if let TokenKind::Int(digits) = &self.current().kind {
                if integer_magnitude(digits) == Ok(1u64 << 63) {
                    let end = self.advance().span;
                    return Ok((Expr::new(ExprKind::Int(i64::MIN), start.join(end)), 1));
                }
            }
        }
        let (operand, operand_depth) = self.unary(depth + 1)?;
        let span = start.join(operand.span);
        let tree_depth = operand_depth + 1;
        self.check_expression_depth(tree_depth, span)?;
        Ok((
            Expr::new(
                ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
                span,
            ),
            tree_depth,
        ))
    }

    fn postfix(&mut self, depth: usize) -> ParseResult<(Expr, usize)> {
        let (mut base, mut tree_depth) = self.primary(depth)?;
        while self.take(&TokenKind::Dot).is_some() {
            let start = base.span;
            let (name, mut end) = self.identifier("a field or method name after '.'")?;
            tree_depth += 1;
            self.check_expression_depth(tree_depth, base.span.join(end))?;
            let kind = if self.take(&TokenKind::LeftParen).is_some() {
                let (args, argument_depth, close) = self.call_arguments(depth, base.span)?;
                tree_depth = tree_depth.max(argument_depth);
                end = close;
                ExprKind::MethodCall {
                    receiver: Box::new(base),
                    name,
                    args,
                }
            } else {
                ExprKind::Field {
                    base: Box::new(base),
                    name,
                }
            };
            let span = start.join(end);
            self.check_expression_depth(tree_depth, span)?;
            base = Expr::new(kind, span);
        }
        Ok((base, tree_depth))
    }

    /// The opening parenthesis has already been consumed. The returned depth
    /// includes the call node, with its receiver counted separately by postfix.
    fn call_arguments(
        &mut self,
        depth: usize,
        start: Span,
    ) -> ParseResult<(Vec<Expr>, usize, Span)> {
        let mut args = Vec::new();
        let mut tree_depth = 1;
        if !self.at(&TokenKind::RightParen) {
            loop {
                let (argument, argument_depth) = self.binary_expression(0, depth + 1)?;
                tree_depth = tree_depth.max(argument_depth + 1);
                self.check_expression_depth(tree_depth, start.join(argument.span))?;
                args.push(argument);
                if self.take(&TokenKind::Comma).is_none() || self.at(&TokenKind::RightParen) {
                    break;
                }
            }
        }
        let end = self
            .expect(&TokenKind::RightParen, "')' after the call arguments")?
            .span;
        Ok((args, tree_depth, end))
    }

    fn primary(&mut self, depth: usize) -> ParseResult<(Expr, usize)> {
        let token = self.advance();
        let kind = match token.kind {
            TokenKind::Int(digits) => ExprKind::Int(integer_magnitude(&digits).ok().and_then(|value| i64::try_from(value).ok()).ok_or_else(|| {
                Diagnostic::new(
                    "P002",
                    "integer literal is outside the signed 64-bit range (-9223372036854775808 through 9223372036854775807)",
                    token.span,
                )
            })?),
            TokenKind::Float(digits) => {
                let value = digits.parse::<f64>().map_err(|_| Diagnostic::new("P005", "float literal cannot be represented as finite binary64", token.span))?;
                if !value.is_finite() { return Err(Diagnostic::new("P005", "float literal cannot be represented as finite binary64", token.span)); }
                ExprKind::Float(value)
            }
            TokenKind::Str(value) => ExprKind::Str(value),
            TokenKind::True => ExprKind::Bool(true),
            TokenKind::False => ExprKind::Bool(false),
            TokenKind::IntType | TokenKind::FloatType => {
                let name = if matches!(token.kind, TokenKind::IntType) { "int" } else { "float" }.to_owned();
                self.expect(&TokenKind::LeftParen, "'(' after a numeric conversion")?;
                let (args, tree_depth, end) = self.call_arguments(depth, token.span)?;
                return Ok((Expr::new(ExprKind::Call { name, args }, token.span.join(end)), tree_depth));
            }
            TokenKind::Ident(name) => {
                if self.take(&TokenKind::LeftParen).is_none() {
                    return Ok((Expr::new(ExprKind::Name(name), token.span), 1));
                }
                let (args, tree_depth, end) = self.call_arguments(depth, token.span)?;
                return Ok((Expr::new(ExprKind::Call { name, args }, token.span.join(end)), tree_depth));
            }
            TokenKind::LeftParen => {
                let (mut value, tree_depth) = self.binary_expression(0, depth + 1)?;
                let end = self.expect(&TokenKind::RightParen, "')' after the expression")?.span;
                value.span = token.span.join(end);
                return Ok((value, tree_depth));
            }
            _ => {
                return Err(Diagnostic::new(
                    "P001",
                    format!("expected an expression, found {}", describe(&token.kind)),
                    token.span,
                ));
            }
        };
        Ok((Expr::new(kind, token.span), 1))
    }

    fn binary_operator(&self) -> Option<(BinaryOp, u8)> {
        Some(match self.current().kind {
            TokenKind::Or => (BinaryOp::Or, 1),
            TokenKind::And => (BinaryOp::And, 2),
            TokenKind::EqualEqual => (BinaryOp::Eq, 3),
            TokenKind::NotEqual => (BinaryOp::Ne, 3),
            TokenKind::Less => (BinaryOp::Lt, 4),
            TokenKind::LessEqual => (BinaryOp::Le, 4),
            TokenKind::Greater => (BinaryOp::Gt, 4),
            TokenKind::GreaterEqual => (BinaryOp::Ge, 4),
            TokenKind::BitOr => (BinaryOp::BitOr, 5),
            TokenKind::BitXor => (BinaryOp::BitXor, 6),
            TokenKind::BitAnd => (BinaryOp::BitAnd, 7),
            TokenKind::ShiftLeft => (BinaryOp::Shl, 8),
            TokenKind::ShiftRight => (BinaryOp::Shr, 8),
            TokenKind::Plus => (BinaryOp::Add, 9),
            TokenKind::Minus => (BinaryOp::Sub, 9),
            TokenKind::Star => (BinaryOp::Mul, 10),
            TokenKind::Slash => (BinaryOp::Div, 10),
            TokenKind::Percent => (BinaryOp::Rem, 10),
            _ => return None,
        })
    }

    fn type_name(&mut self) -> ParseResult<(Type, Span)> {
        let ty = match self.current().kind.clone() {
            TokenKind::IntType => Type::Int,
            TokenKind::FloatType => Type::Float,
            TokenKind::BoolType => Type::Bool,
            TokenKind::StrType => Type::Str,
            TokenKind::VoidType => Type::Void,
            TokenKind::Ident(name) if name == "buffer" => Type::Buffer,
            TokenKind::Ident(name) if name == "ints" => Type::Ints,
            TokenKind::Ident(name) if name == "task" => Type::Task,
            TokenKind::Ident(name) if name == "tcp_listener" => Type::TcpListener,
            TokenKind::Ident(name) if name == "tcp_socket" => Type::TcpSocket,
            TokenKind::Ident(name) if name == "tls_socket" => Type::TlsSocket,
            TokenKind::Ident(name) => Type::Record(self.intern_type_name(&name)),
            _ => return Err(self.error("expected a builtin type or a struct name")),
        };
        Ok((ty, self.advance().span))
    }

    fn intern_type_name(&mut self, name: &str) -> usize {
        if let Some(id) = self.type_names.iter().position(|existing| existing == name) {
            id
        } else {
            let id = self.type_names.len();
            self.type_names.push(name.to_owned());
            id
        }
    }

    fn is_field_assignment_target(&self, start: usize, end: usize) -> bool {
        let tokens = &self.tokens[start..end];
        tokens.len() >= 3
            && tokens.len() % 2 == 1
            && matches!(tokens[0].kind, TokenKind::Ident(_))
            && tokens[1..].as_chunks::<2>().0.iter().all(|pair| {
                pair[0].kind == TokenKind::Dot && matches!(pair[1].kind, TokenKind::Ident(_))
            })
    }

    fn identifier(&mut self, expected: &str) -> ParseResult<(String, Span)> {
        if let TokenKind::Ident(name) = &self.current().kind {
            let name = name.clone();
            return Ok((name, self.advance().span));
        }
        Err(self.error(&format!(
            "expected {expected}, found {}",
            describe(&self.current().kind)
        )))
    }

    fn terminator(&mut self) -> ParseResult<()> {
        if self.take(&TokenKind::Semicolon).is_some() || self.take(&TokenKind::Newline).is_some() {
            return Ok(());
        }
        if self.at(&TokenKind::RightBrace) || self.at(&TokenKind::Eof) {
            return Ok(());
        }
        Err(self.error(&format!(
            "expected a newline or ';' after the statement, found {}",
            describe(&self.current().kind)
        )))
    }

    fn is_terminator(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::Newline | TokenKind::RightBrace | TokenKind::Eof
        )
    }

    fn check_depth(&self, depth: usize) -> ParseResult<()> {
        if depth > 128 {
            Err(Diagnostic::new(
                "P003",
                "syntax nesting exceeds the bootstrap limit of 128",
                self.current().span,
            ))
        } else {
            Ok(())
        }
    }

    fn check_expression_depth(&self, depth: usize, span: Span) -> ParseResult<()> {
        if self.expression_block_depth + depth > 128 {
            Err(Diagnostic::new(
                "P004",
                "combined block and expression tree depth exceeds the bootstrap limit of 128; split the expression into variables",
                span,
            ))
        } else {
            Ok(())
        }
    }

    fn skip_newlines(&mut self) {
        while self.take(&TokenKind::Newline).is_some() {}
    }

    fn skip_separators(&mut self) {
        while self.at(&TokenKind::Newline) || self.at(&TokenKind::Semicolon) {
            self.advance();
        }
    }

    fn skip_field_separators(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::Comma
        ) {
            self.advance();
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if token.kind != TokenKind::Eof {
            self.cursor += 1;
        }
        token
    }

    fn at(&self, kind: &TokenKind) -> bool {
        &self.current().kind == kind
    }

    fn next_at(&self, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.cursor + 1)
            .is_some_and(|token| &token.kind == kind)
    }

    fn take(&mut self, kind: &TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.advance())
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> ParseResult<Token> {
        self.take(kind).ok_or_else(|| {
            self.error(&format!(
                "expected {expected}, found {}",
                describe(&self.current().kind)
            ))
        })
    }

    fn error(&self, message: &str) -> Diagnostic {
        Diagnostic::new("P001", message, self.current().span)
    }
}

fn integer_magnitude(spelling: &str) -> Result<u64, std::num::ParseIntError> {
    if let Some(digits) = spelling
        .strip_prefix("0x")
        .or_else(|| spelling.strip_prefix("0X"))
    {
        u64::from_str_radix(digits, 16)
    } else if let Some(digits) = spelling
        .strip_prefix("0b")
        .or_else(|| spelling.strip_prefix("0B"))
    {
        u64::from_str_radix(digits, 2)
    } else {
        spelling.parse()
    }
}

fn describe(kind: &TokenKind) -> String {
    let spelling = match kind {
        TokenKind::Ident(name) => return format!("identifier '{name}'"),
        TokenKind::Int(_) => return "an integer literal".into(),
        TokenKind::Float(_) => return "a float literal".into(),
        TokenKind::Str(_) => return "a string literal".into(),
        TokenKind::Eof => return "end of file".into(),
        TokenKind::Newline => return "a newline".into(),
        TokenKind::Def => "def",
        TokenKind::Struct => "struct",
        TokenKind::Enum => "enum",
        TokenKind::Match => "match",
        TokenKind::Case => "case",
        TokenKind::Extern => "extern",
        TokenKind::Var => "var",
        TokenKind::If => "if",
        TokenKind::Else => "else",
        TokenKind::While => "while",
        TokenKind::For => "for",
        TokenKind::In => "in",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::Return => "return",
        TokenKind::True => "true",
        TokenKind::False => "false",
        TokenKind::IntType => "int",
        TokenKind::FloatType => "float",
        TokenKind::BoolType => "bool",
        TokenKind::StrType => "str",
        TokenKind::VoidType => "void",
        TokenKind::And => "and",
        TokenKind::Or => "or",
        TokenKind::Not => "not",
        TokenKind::BitAnd => "&",
        TokenKind::BitOr => "|",
        TokenKind::BitXor => "^",
        TokenKind::BitNot => "~",
        TokenKind::ShiftLeft => "<<",
        TokenKind::ShiftRight => ">>",
        TokenKind::LeftParen => "(",
        TokenKind::RightParen => ")",
        TokenKind::LeftBracket => "[",
        TokenKind::RightBracket => "]",
        TokenKind::LeftBrace => "{",
        TokenKind::RightBrace => "}",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::Colon => ":",
        TokenKind::Semicolon => ";",
        TokenKind::Arrow => "->",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::Percent => "%",
        TokenKind::Equal => "=",
        TokenKind::EqualEqual => "==",
        TokenKind::NotEqual => "!=",
        TokenKind::Less => "<",
        TokenKind::LessEqual => "<=",
        TokenKind::Greater => ">",
        TokenKind::GreaterEqual => ">=",
    };
    format!("'{spelling}'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(source: &str) -> Block {
        parse(&format!("def test() {{ {source} }}"))
            .unwrap()
            .functions
            .remove(0)
            .body
            .unwrap()
    }

    fn expression(source: &str) -> Expr {
        let statement = body(&format!("return {source}")).remove(0);
        match statement.kind {
            StmtKind::Return(Some(value)) => value,
            other => panic!("expected a return expression, got {other:?}"),
        }
    }

    fn shape(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Int(value) => value.to_string(),
            ExprKind::Float(value) => value.to_string(),
            ExprKind::Bool(value) => value.to_string(),
            ExprKind::Name(name) => name.clone(),
            ExprKind::Field { base, name } => format!("({}.{})", shape(base), name),
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } => format!(
                "({}.{}({}))",
                shape(receiver),
                name,
                args.iter().map(shape).collect::<Vec<_>>().join(", ")
            ),
            ExprKind::Call { name, args } => format!(
                "{name}({})",
                args.iter().map(shape).collect::<Vec<_>>().join(", ")
            ),
            ExprKind::Unary { op, operand } => format!("({op:?} {})", shape(operand)),
            ExprKind::Binary { op, left, right } => {
                format!("({op:?} {} {})", shape(left), shape(right))
            }
            other => panic!("unexpected expression in precedence test: {other:?}"),
        }
    }

    #[test]
    fn parses_the_bootstrap_example() {
        let program = parse(
            r#"extern def native_double(value: int) -> int

def add(a: int, b: int) -> int {
    return a + b
}

def main() {
    var value = add(20, 22)
    print("Bhasa")
    print(value)
    while value > 0 {
        value = value - 1
    }
    if value == 0 {
        print(true)
    } else {
        print(false)
    }
}"#,
        )
        .unwrap();
        assert_eq!(program.functions.len(), 3);
        let native = &program.functions[0];
        assert_eq!(native.name, "native_double");
        assert!(native.body.is_none());
        assert_eq!(native.params[0].ty, Type::Int);
        assert_eq!(native.return_type, Type::Int);
        let main = &program.functions[2];
        assert_eq!(main.return_type, Type::Void);
        assert_eq!(main.body.as_ref().unwrap().len(), 5);
        assert!(matches!(
            main.body.as_ref().unwrap()[3].kind,
            StmtKind::While { .. }
        ));
        assert!(matches!(
            main.body.as_ref().unwrap()[4].kind,
            StmtKind::If {
                else_block: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn an_empty_or_comment_only_file_is_a_library() {
        for source in ["", "\n;;\n", "# nothing\n// still nothing"] {
            assert!(parse(source).unwrap().functions.is_empty());
        }
    }

    #[test]
    fn parses_all_annotations_without_performing_semantic_checks() {
        let program = parse("def values(i: int, b: bool, s: str) -> void { var a: int = i; var b: bool = true; var s: str = \"text\" }").unwrap();
        assert_eq!(
            program.functions[0]
                .params
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            [Type::Int, Type::Bool, Type::Str]
        );
        let statements = program.functions[0].body.as_ref().unwrap();
        for (statement, expected) in statements.iter().zip([Type::Int, Type::Bool, Type::Str]) {
            match &statement.kind {
                StmtKind::Var {
                    annotation, value, ..
                } => {
                    assert_eq!(*annotation, Some(expected));
                    assert!(value.ty.is_none());
                }
                other => panic!("expected a declaration, got {other:?}"),
            }
        }
    }

    #[test]
    fn parses_records_and_interns_forward_type_references() {
        let source = "def make(value: Outer) -> Inner { var copy: Outer = value; return copy.inner }\nstruct Inner { x: int; y: int }\nstruct Outer\n{ inner: Inner, text: buffer\n values: ints; marker: bool; label: str }\nstruct Empty {}";
        let program = parse(source).unwrap();
        assert_eq!(program.type_names, ["Outer", "Inner", "Empty"]);
        assert_eq!(program.records.len(), 3);
        let function = &program.functions[0];
        assert_eq!(function.params[0].ty, Type::Record(0));
        assert_eq!(function.return_type, Type::Record(1));
        assert_eq!(program.record(0).unwrap().name, "Outer");
        assert_eq!(program.type_name(Type::Record(1)), "Inner");
        assert!(Type::Record(0).is_owned());
        let outer = &program.records[1];
        assert_eq!(
            outer
                .fields
                .iter()
                .map(|field| field.ty)
                .collect::<Vec<_>>(),
            [
                Type::Record(1),
                Type::Buffer,
                Type::Ints,
                Type::Bool,
                Type::Str
            ]
        );
        assert_eq!(
            &source[outer.fields[0].span.start..outer.fields[0].span.end],
            "inner: Inner"
        );
        assert_eq!(
            &source[outer.span.start..outer.span.end],
            "struct Outer\n{ inner: Inner, text: buffer\n values: ints; marker: bool; label: str }"
        );
        assert!(program.records[2].fields.is_empty());
    }

    #[test]
    fn parses_methods_with_inferred_receivers_and_typed_remaining_parameters() {
        let source = "struct Point {\n x: int\n def total(self) -> int { return self.x }\n y: int\n def shift(self, amount: int, other: Point,) { self.x = self.x + amount }\n def copy(self,) -> Point\n { return self }\n}\ndef main() {}";
        let program = parse(source).unwrap();
        assert_eq!(program.type_names, ["Point"]);
        assert_eq!(program.functions.len(), 1);
        let point = &program.records[0];
        assert_eq!(point.fields.len(), 2);
        assert_eq!(point.methods.len(), 3);
        let total = &point.methods[0];
        assert_eq!(total.name, "total");
        assert_eq!(total.params.len(), 1);
        assert_eq!(total.params[0].name, "self");
        assert_eq!(total.params[0].ty, Type::Record(0));
        assert_eq!(
            &source[total.params[0].span.start..total.params[0].span.end],
            "self"
        );
        assert_eq!(total.return_type, Type::Int);
        assert!(total.body.is_some());
        assert_eq!(
            &source[total.span.start..total.span.end],
            "def total(self) -> int { return self.x }"
        );
        let shift = &point.methods[1];
        assert_eq!(
            shift
                .params
                .iter()
                .map(|param| param.ty)
                .collect::<Vec<_>>(),
            [Type::Record(0), Type::Int, Type::Record(0)]
        );
        assert_eq!(shift.return_type, Type::Void);
        assert_eq!(point.methods[2].return_type, Type::Record(0));
    }

    #[test]
    fn methods_keep_source_names_in_each_record() {
        let program = parse("struct First { def copy(self) -> First { return self } } struct Second { def copy(self) -> Second { return self } }").unwrap();
        assert_eq!(program.records[0].methods[0].name, "copy");
        assert_eq!(program.records[1].methods[0].name, "copy");
        assert_eq!(program.records[0].methods[0].params[0].ty, Type::Record(0));
        assert_eq!(program.records[1].methods[0].params[0].ty, Type::Record(1));
    }

    #[test]
    fn rejects_missing_typed_repeated_and_wrong_method_receivers() {
        for (source, message) in [
            ("struct P { def f() {} }", "untyped first 'self'"),
            ("struct P { def f(other: P) {} }", "untyped first 'self'"),
            ("struct P { def f(other) {} }", "untyped first 'self'"),
            (
                "struct P { def f(self: P) {} }",
                "remove the type annotation",
            ),
            (
                "struct P { def f(self: int) {} }",
                "remove the type annotation",
            ),
            (
                "struct P { def f(self, self: P) {} }",
                "only appear as the first",
            ),
            (
                "struct P { def f(self other: int) {} }",
                "after the method receiver",
            ),
            ("struct P { extern def f(self) }", "cannot be extern"),
            ("struct P { def f(self); }", "methods require a body"),
            ("struct P { def f(self) -> int }", "methods require a body"),
        ] {
            let error = parse(source).unwrap_err().remove(0);
            assert_eq!(error.code, "P001", "{source}");
            assert!(error.message.contains(message), "{source}: {error}");
        }
        for source in [
            "struct P { static def f() {} }",
            "struct P { def P.f(self) {} }",
            "def P.f(self) {}",
            "struct P { def f(self) { def nested(self) {} } }",
        ] {
            assert_eq!(parse(source).unwrap_err()[0].code, "P001", "{source}");
        }
    }

    #[test]
    fn method_calls_share_postfix_precedence_with_fields_and_allow_temporary_receivers() {
        assert_eq!(
            shape(&expression("-point.copy().x + point.total(2) * 3")),
            "(Add (Neg ((point.copy()).x)) (Mul (point.total(2)) 3))"
        );
        assert_eq!(
            shape(&expression("make().child.copy(other.child).text")),
            "(((make().child).copy((other.child))).text)"
        );
        assert_eq!(
            shape(&expression("(point).copy().copy().x")),
            "(((point.copy()).copy()).x)"
        );
        assert_eq!(
            shape(&expression("point.shift(\n1,\nother.copy(),\n)")),
            "(point.shift(1, (other.copy())))"
        );
        let source = "def f() { return point.child.copy(1).text }";
        let expr = parse(source)
            .unwrap()
            .functions
            .remove(0)
            .body
            .unwrap()
            .remove(0);
        let StmtKind::Return(Some(Expr {
            kind: ExprKind::Field { base, .. },
            ..
        })) = expr.kind
        else {
            panic!("expected field projection")
        };
        assert_eq!(
            &source[base.span.start..base.span.end],
            "point.child.copy(1)"
        );
        assert!(matches!(base.kind, ExprKind::MethodCall { .. }));
    }

    #[test]
    fn method_results_cannot_be_field_assignment_targets() {
        for target in ["point.copy()", "point.copy().x", "point.child.copy().x"] {
            let error = parse(&format!("def f() {{ {target} = 1 }}"))
                .unwrap_err()
                .remove(0);
            assert_eq!(error.code, "P001");
            assert!(error.message.contains("assignment target"));
        }
    }

    #[test]
    fn method_calls_obey_receiver_argument_and_combined_depth_limits() {
        let chain = format!("point{}", ".copy()".repeat(127));
        assert!(parse(&format!("def f() {{ return {chain} }}")).is_ok());
        assert_eq!(
            parse(&format!("def f() {{ return {chain}.copy() }}")).unwrap_err()[0].code,
            "P004"
        );
        assert_eq!(
            parse(&format!("def f() {{ if true {{ return {chain} }} }}")).unwrap_err()[0].code,
            "P004"
        );
        let argument = format!("1{}", " + 1".repeat(127));
        assert_eq!(
            parse(&format!("def f() {{ return point.take({argument}) }}")).unwrap_err()[0].code,
            "P004"
        );
        let nested = format!("{}point{}", "point.take(".repeat(256), ")".repeat(256));
        assert_eq!(
            parse(&format!("def f() {{ return {nested} }}")).unwrap_err()[0].code,
            "P003"
        );
        let arguments = std::iter::repeat_n("1", 512).collect::<Vec<_>>().join(",");
        assert!(parse(&format!("def f() {{ point.take({arguments}) }}")).is_ok());
    }

    #[test]
    fn named_unknown_types_and_invalid_field_types_reach_semantic_checking() {
        let program = parse(
            "struct Holder { unknown: Missing; bad: void; job: task } def use(value: Other) {}",
        )
        .unwrap();
        assert_eq!(program.type_names, ["Holder", "Missing", "Other"]);
        assert_eq!(program.records[0].fields[0].ty, Type::Record(1));
        assert_eq!(program.records[0].fields[1].ty, Type::Void);
        assert_eq!(program.records[0].fields[2].ty, Type::Task);
        assert_eq!(program.functions[0].params[0].ty, Type::Record(2));
    }

    #[test]
    fn parses_network_resource_annotations_without_interning_record_names() {
        let program = parse("struct Connection { listener: tcp_listener; socket: tcp_socket } def use(socket: tcp_socket) -> tcp_listener { var listener: tcp_listener = tcp_listener(); return listener }").unwrap();
        assert_eq!(program.type_names, ["Connection"]);
        assert_eq!(program.records[0].fields[0].ty, Type::TcpListener);
        assert_eq!(program.records[0].fields[1].ty, Type::TcpSocket);
        assert_eq!(program.functions[0].params[0].ty, Type::TcpSocket);
        assert_eq!(program.functions[0].return_type, Type::TcpListener);
        assert!(Type::TcpListener.is_owned());
        assert!(Type::TcpSocket.is_owned());
        assert_eq!(program.type_name(Type::TcpListener), "tcp_listener");
        assert_eq!(program.type_name(Type::TcpSocket), "tcp_socket");
    }

    #[test]
    fn field_access_binds_before_unary_and_binary_operators() {
        assert_eq!(
            shape(&expression("-p.inner.x + q.y * p.x")),
            "(Add (Neg ((p.inner).x)) (Mul (q.y) (p.x)))"
        );
        assert_eq!(shape(&expression("(p).inner.x")), "((p.inner).x)");
        for source in ["Point(1, 2).x", "make().inner.x"] {
            assert!(matches!(expression(source).kind, ExprKind::Field { .. }));
        }
    }

    #[test]
    fn parses_field_assignments_and_preserves_constructor_calls() {
        let statements = body("var p: Point = Point(1, 2); p.inner.x = p.x + 1; p = Point(3, 4)");
        let StmtKind::Var { value, .. } = &statements[0].kind else {
            panic!("expected variable")
        };
        assert!(
            matches!(&value.kind, ExprKind::Call { name, args } if name == "Point" && args.len() == 2)
        );
        let StmtKind::FieldAssign { target, value } = &statements[1].kind else {
            panic!("expected field assignment")
        };
        assert_eq!(shape(target), "((p.inner).x)");
        assert_eq!(shape(value), "(Add (p.x) 1)");
        assert!(matches!(statements[2].kind, StmtKind::Assign { .. }));
    }

    #[test]
    fn rejects_malformed_records_and_nonvariable_field_writes() {
        for source in [
            "struct {}",
            "struct Point { x int }",
            "struct Point { x: }",
            "struct Point { x: int y: int }",
            "struct Point { x: int",
            "def f() { struct Point {} }",
            "def f() { p. }",
            "def f() { p.1 }",
            "def f() { Point(1).x = 2 }",
            "def f() { (p).x = 2 }",
            "def f() { (p.x) = 2 }",
            "def f() { p.x + p.y = 2 }",
            "def f() { p.x = }",
        ] {
            let errors = parse(source).unwrap_err();
            assert_eq!(errors[0].code, "P001", "{source}");
        }
    }

    #[test]
    fn field_chains_respect_expression_depth_limits() {
        assert!(parse(&format!("def f() {{ return p{} }}", ".x".repeat(127))).is_ok());
        assert_eq!(
            parse(&format!("def f() {{ return p{} }}", ".x".repeat(128))).unwrap_err()[0].code,
            "P004"
        );
        assert_eq!(
            parse(&format!(
                "def f() {{ if true {{ return p{} }} }}",
                ".x".repeat(127)
            ))
            .unwrap_err()[0]
                .code,
            "P004"
        );
    }

    #[test]
    fn respects_all_binary_precedence_levels() {
        assert_eq!(
            shape(&expression("a or b and c == d < e + f * g")),
            "(Or a (And b (Eq c (Lt d (Add e (Mul f g))))))"
        );
        assert_eq!(
            shape(&expression("a || b && c != d >= e - f % g")),
            "(Or a (And b (Ne c (Ge d (Sub e (Rem f g))))))"
        );
        assert_eq!(
            shape(&expression("a <= b or c > d")),
            "(Or (Le a b) (Gt c d))"
        );
    }

    #[test]
    fn operators_at_the_same_precedence_associate_left() {
        assert_eq!(shape(&expression("20 - 5 - 2")), "(Sub (Sub 20 5) 2)");
        assert_eq!(
            shape(&expression("24 / 3 * 2 % 5")),
            "(Rem (Mul (Div 24 3) 2) 5)"
        );
        assert_eq!(shape(&expression("a == b != c")), "(Ne (Eq a b) c)");
    }

    #[test]
    fn unary_operators_bind_before_binary_operators() {
        assert_eq!(shape(&expression("-a * b")), "(Mul (Neg a) b)");
        assert_eq!(shape(&expression("not !a and b")), "(And (Not (Not a)) b)");
        assert_eq!(shape(&expression("-(a + b)")), "(Neg (Add a b))");
        assert_eq!(shape(&expression("(a + b) * c")), "(Mul (Add a b) c)");
    }

    #[test]
    fn accepts_the_full_signed_literal_range() {
        assert!(matches!(
            expression("9223372036854775807").kind,
            ExprKind::Int(i64::MAX)
        ));
        assert!(matches!(
            expression("-9223372036854775808").kind,
            ExprKind::Int(i64::MIN)
        ));
        assert!(matches!(
            expression("-0009223372036854775808").kind,
            ExprKind::Int(i64::MIN)
        ));
        assert_eq!(shape(&expression("-42")), "(Neg 42)");
        assert_eq!(
            shape(&expression("--9223372036854775808")),
            "(Neg -9223372036854775808)"
        );
    }

    #[test]
    fn rejects_out_of_range_literals_without_wrapping() {
        for value in [
            "9223372036854775808",
            "-9223372036854775809",
            "18446744073709551616",
            "99999999999999999999999999999999999999999999999999",
        ] {
            let source = format!("def main() {{ print({value}) }}");
            let errors = parse(&source).unwrap_err();
            assert_eq!(errors[0].code, "P002", "{value}");
            assert!(errors[0].message.contains("signed 64-bit range"));
            let digits = value.trim_start_matches('-');
            assert_eq!(&source[errors[0].span.start..errors[0].span.end], digits);
        }
    }

    #[test]
    fn newlines_semicolons_and_closing_braces_terminate_statements() {
        let statements = body("var a = 1\nvar b = 2; a = b\nreturn");
        assert_eq!(statements.len(), 4);
        assert!(matches!(statements[2].kind, StmtKind::Assign { .. }));
        assert!(matches!(statements[3].kind, StmtKind::Return(None)));
        assert!(parse("extern def exit_code() -> int").is_ok());
    }

    #[test]
    fn return_never_absorbs_an_expression_on_the_next_line() {
        for gap in ["\n", "# comment\n", "// comment\r\n", ";"] {
            let statements = body(&format!("return{gap}42"));
            assert_eq!(statements.len(), 2);
            assert!(matches!(statements[0].kind, StmtKind::Return(None)));
            assert!(matches!(
                statements[1].kind,
                StmtKind::Expr(Expr {
                    kind: ExprKind::Int(42),
                    ..
                })
            ));
        }
    }

    #[test]
    fn comments_preserve_statement_boundaries() {
        let statements = body("var a = 1 // comment\r\nvar b = 2 # comment\r\nprint(a + b)");
        assert_eq!(statements.len(), 3);
    }

    #[test]
    fn newlines_inside_parentheses_allow_multiline_expressions_and_calls() {
        let program = parse("def add(\na: int,\n# comment\nb: int,\n) -> int\n{ return (\na +\nb\n) }\ndef main() { print(\nadd(\n1,\n2,\n),\n) }").unwrap();
        assert_eq!(program.functions[0].params.len(), 2);
        assert_eq!(program.functions[1].body.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn newlines_outside_parentheses_do_not_continue_expressions() {
        for source in [
            "var a = 1 +\n2",
            "var a = 1\n+ 2",
            "print\n(1)",
            "var a =\n1",
        ] {
            if source == "print\n(1)" {
                // Two expression statements, never a call across the newline.
                let statements = body(source);
                assert_eq!(statements.len(), 2);
                assert!(matches!(
                    statements[0].kind,
                    StmtKind::Expr(Expr {
                        kind: ExprKind::Name(_),
                        ..
                    })
                ));
            } else {
                assert!(
                    parse(&format!("def main() {{ {source} }}")).is_err(),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn accepts_newlines_before_blocks_and_else() {
        let statements =
            body("if true\n{ return }\n# comment\nelse\n{ while false\n{ print(1) } }");
        assert_eq!(statements.len(), 1);
        assert!(matches!(
            statements[0].kind,
            StmtKind::If {
                else_block: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn spans_are_utf8_byte_offsets_and_cover_complete_syntax() {
        let source = "def main() {\n    print(\"भाषा\");\n    return\n}";
        let program = parse(source).unwrap();
        let main = &program.functions[0];
        assert_eq!(main.span, Span::new(0, source.len()));
        let statements = main.body.as_ref().unwrap();
        assert_eq!(
            &source[statements[0].span.start..statements[0].span.end],
            "print(\"भाषा\")"
        );
        let StmtKind::Expr(Expr {
            kind: ExprKind::Call { args, .. },
            ..
        }) = &statements[0].kind
        else {
            panic!("expected call")
        };
        assert_eq!(&source[args[0].span.start..args[0].span.end], "\"भाषा\"");
        assert_eq!(
            &source[statements[1].span.start..statements[1].span.end],
            "return"
        );
    }

    #[test]
    fn diagnostic_after_unicode_uses_the_offending_token_span() {
        let source = "def main() { print(\"भाषा\") var a = 1 }";
        let error = parse(source).unwrap_err().remove(0);
        assert!(error.message.contains("newline or ';'"));
        assert_eq!(&source[error.span.start..error.span.end], "var");
    }

    #[test]
    fn rejects_malformed_declarations_expressions_and_blocks() {
        for source in [
            "var top_level = 1",
            "def () {}",
            "def main {}",
            "def f(a) {}",
            "def f(a int) {}",
            "def f() -> {}",
            "def f();",
            "extern f()",
            "extern def f() {}",
            "def main() {",
            "def main() { var x }",
            "def main() { var x: int }",
            "def main() { x = }",
            "def main() { print(,1) }",
            "def main() { print(1 2) }",
            "def main() { print(1; 2) }",
            "def main() { (1 + 2 }",
            "def main() { if true return }",
            "def main() { if true {} else }",
            "def main() { else {} }",
            "def main() { while {} }",
            "def main() { def nested() {} }",
            "def main() { 1 = 2 }",
            "def main() { (x) = 1 }",
            "def main() {} }",
        ] {
            let errors = parse(source).unwrap_err();
            assert!(errors[0].code.starts_with('P'), "{source}: {errors:?}");
            assert!(errors[0].span.start <= errors[0].span.end);
            assert!(errors[0].span.end <= source.len());
        }
    }

    #[test]
    fn missing_block_closer_points_at_eof() {
        let source = "def main() { return";
        let errors = parse(source).unwrap_err();
        assert!(errors[0].message.contains("expected '}'"));
        assert_eq!(errors[0].span, Span::new(source.len(), source.len()));
    }

    #[test]
    fn lexical_diagnostics_reach_the_public_parser() {
        for source in ["def main() { print(\"\\q\") }", "def main() { @ }"] {
            assert!(parse(source).unwrap_err()[0].code.starts_with('L'));
        }
    }

    #[test]
    fn excessive_nesting_has_a_diagnostic() {
        let parentheses = format!(
            "def main() {{ return {}1{} }}",
            "(".repeat(256),
            ")".repeat(256)
        );
        let unary = format!("def main() {{ return {}true }}", "!".repeat(256));
        let blocks = format!(
            "def main() {{ {}return{} }}",
            "if true {".repeat(256),
            "}".repeat(256)
        );
        for source in [parentheses, unary] {
            assert_eq!(parse(&source).unwrap_err()[0].code, "P003");
        }
        assert_eq!(parse(&blocks).unwrap_err()[0].code, "P004");
    }

    #[test]
    fn flat_binary_chains_are_limited_before_building_a_deep_ast() {
        let source = format!("def main() {{ print(1{}) }}", " + 1".repeat(4096));
        let errors = parse(&source).unwrap_err();
        assert_eq!(errors[0].code, "P004");
        assert!(errors[0]
            .message
            .contains("split the expression into variables"));
    }

    #[test]
    fn expression_depth_includes_nested_calls_unary_and_binary_nodes() {
        let chain = format!("1{}", " + 1".repeat(126));
        assert!(parse(&format!("def f() {{ return {chain} }}")).is_ok());
        assert!(parse(&format!("def f() {{ return -({chain}) }}")).is_ok());
        for source in [
            format!("def f() {{ return -({chain}) + 1 }}"),
            format!("def f() {{ return g({chain}) + 1 }}"),
            format!("def f() {{ return 1 + (1 + ({chain})) }}"),
        ] {
            assert_eq!(parse(&source).unwrap_err()[0].code, "P004");
        }
    }

    #[test]
    fn expression_depth_does_not_limit_the_number_of_sibling_arguments() {
        let arguments = std::iter::repeat_n("1", 512).collect::<Vec<_>>().join(",");
        assert!(parse(&format!("def f() {{ g({arguments}) }}")).is_ok());
    }

    #[test]
    fn nested_statement_boundaries_fit_the_default_test_stack() {
        for (opening, closing) in [
            ("if true {", "}"),
            ("while true {", "}"),
            ("match value { case E.unit {", "}}"),
        ] {
            for (depth, valid) in [(128, true), (129, false)] {
                let source = format!(
                    "def main() {{ {}{} }}",
                    opening.repeat(depth),
                    closing.repeat(depth),
                );
                let result = parse(&source);
                if valid {
                    assert!(result.is_ok(), "{opening}, depth {depth}: {result:?}");
                } else {
                    assert_eq!(result.unwrap_err()[0].code, "P004", "{opening}");
                }
            }
        }
    }

    #[test]
    fn block_and_expression_recursion_share_the_same_limit() {
        let source = format!(
            "def main() {{ {}return {}1{}{} }}",
            "if true {".repeat(128),
            "(".repeat(128),
            ")".repeat(128),
            "}".repeat(128),
        );
        assert_eq!(parse(&source).unwrap_err()[0].code, "P003");
    }

    #[test]
    fn block_depth_also_reduces_the_expression_tree_budget() {
        let valid = format!(
            "def main() {{ {}print(1{}){} }}",
            "if true {".repeat(100),
            " + 1".repeat(26),
            "}".repeat(100),
        );
        assert!(parse(&valid).is_ok());
        let invalid = valid.replace("print(1", "print(1 + 1");
        assert_eq!(parse(&invalid).unwrap_err()[0].code, "P004");
    }

    #[test]
    fn mixed_match_and_loop_blocks_share_the_expression_budget() {
        let mut opening = String::new();
        let mut closing = String::new();
        for index in 0..100 {
            let (start, end) = match index % 3 {
                0 => ("if true {", "}"),
                1 => ("while true {", "}"),
                _ => ("match value { case E.unit {", "}}"),
            };
            opening.push_str(start);
            closing.insert_str(0, end);
        }
        let valid = format!(
            "def main() {{ {opening} print(1{}) {closing} }}",
            " + 1".repeat(26),
        );
        assert!(parse(&valid).is_ok());
        let invalid = valid.replace("print(1", "print(1 + 1");
        assert_eq!(parse(&invalid).unwrap_err()[0].code, "P004");
    }

    #[test]
    fn leaf_expression_fast_paths_obey_the_combined_depth_budget() {
        for leaf in [
            "value",
            "f()",
            "-9223372036854775808",
            "1",
            "true",
            "\"str\"",
        ] {
            let source = format!(
                "def main() {{ {}return {leaf}{} }}",
                "if true {".repeat(128),
                "}".repeat(128),
            );
            assert_eq!(parse(&source).unwrap_err()[0].code, "P004", "{leaf}");
        }
    }
}
