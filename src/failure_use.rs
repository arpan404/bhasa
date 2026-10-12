//! Conservative local failure obligations, run only after ordinary checking.
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;

#[derive(Clone)]
struct Binding {
    pending: bool,
}
type State = Vec<BTreeMap<String, Binding>>;

pub(crate) fn check(program: &Program) -> Vec<Diagnostic> {
    let results = program
        .enums
        .iter()
        .filter_map(|declaration| {
            declaration.result_shape()?;
            program
                .type_names
                .iter()
                .position(|name| name == &declaration.name)
        })
        .collect();
    let mut checker = FailureUse {
        results,
        diagnostics: Vec::new(),
        loops: Vec::new(),
    };
    for function in program
        .functions
        .iter()
        .chain(program.records.iter().flat_map(|record| &record.methods))
    {
        if let Some(body) = &function.body {
            let mut state = vec![BTreeMap::new()];
            // Parameters retain the existing API contract in this increment.
            for parameter in &function.params {
                checker.bind(&mut state, &parameter.name, false, parameter.span);
            }
            if checker.block(body, &mut state) {
                checker.pending(&state, 0, function.span, "function exit");
            }
        }
    }
    checker.diagnostics
}

struct FailureUse {
    results: BTreeSet<usize>,
    diagnostics: Vec<Diagnostic>,
    // First scope belonging to each active loop iteration.
    loops: Vec<usize>,
}
impl FailureUse {
    fn result(&self, ty: Option<Type>) -> bool {
        matches!(ty, Some(Type::Enum(id)) if self.results.contains(&id))
    }
    fn bind(&self, state: &mut State, name: &str, pending: bool, _span: Span) {
        state
            .last_mut()
            .unwrap()
            .insert(name.into(), Binding { pending });
    }
    fn scope(state: &State, name: &str) -> Option<usize> {
        state.iter().rposition(|scope| scope.contains_key(name))
    }
    fn discharge(&self, state: &mut State, value: &Expr) {
        if let ExprKind::Name(name) = &value.kind {
            if let Some(index) = Self::scope(state, name) {
                state[index].get_mut(name).unwrap().pending = false;
            }
        }
    }
    fn pending(&mut self, state: &State, first: usize, exit: Span, reason: &str) {
        for scope in &state[first..] {
            for (name, binding) in scope {
                if binding.pending {
                    self.diagnostics.push(Diagnostic::new("S037", format!(
                        "result binding '{name}' is pending at {reason}; match, return or propagate it before discarding it"), exit));
                }
            }
        }
    }
    fn scoped(&mut self, block: &Block, state: &mut State, span: Span) -> bool {
        state.push(BTreeMap::new());
        let next = self.block(block, state);
        if next {
            self.pending(state, state.len() - 1, span, "scope exit");
        }
        state.pop();
        next
    }
    fn merge(target: &mut State, branches: Vec<State>) -> bool {
        let mut branches = branches.into_iter();
        let Some(first) = branches.next() else {
            return false;
        };
        *target = first;
        for branch in branches {
            for (scope, other) in target.iter_mut().zip(branch) {
                for (name, binding) in scope {
                    binding.pending |= other[name].pending;
                }
            }
        }
        true
    }
    fn block(&mut self, block: &Block, state: &mut State) -> bool {
        for statement in block {
            match &statement.kind {
                StmtKind::Var { name, value, .. } => {
                    self.bind(state, name, self.result(value.ty), statement.span);
                }
                StmtKind::TryVar {
                    name,
                    value,
                    success_type,
                    try_span,
                    ..
                } => {
                    self.discharge(state, value);
                    // The implicit error return discards every other local.
                    self.pending(state, 0, *try_span, "try error exit");
                    self.bind(state, name, self.result(*success_type), statement.span);
                }
                StmtKind::Assign { name, value } => {
                    if let Some(index) = Self::scope(state, name) {
                        let binding = &state[index][name];
                        if binding.pending {
                            self.diagnostics.push(Diagnostic::new("S037", format!(
                                "cannot overwrite pending result binding '{name}'; match, return or propagate it first"), statement.span));
                        }
                        let pending = self.result(value.ty);
                        // No loop fixed point yet. Reject new outer obligations,
                        // including inside a nested loop, rather than losing them.
                        if pending && self.loops.last().is_some_and(|first| index < *first) {
                            self.diagnostics.push(Diagnostic::new("S037", format!(
                                "result assignment to outer binding '{name}' inside a loop requires failure-flow analysis; use a local result and handle it in this iteration"), statement.span));
                        }
                        state[index].get_mut(name).unwrap().pending = pending;
                    }
                }
                StmtKind::Return(value) => {
                    if let Some(value) = value {
                        self.discharge(state, value);
                    }
                    self.pending(state, 0, statement.span, "return");
                    return false;
                }
                StmtKind::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    let mut branches = Vec::new();
                    let mut then_state = state.clone();
                    if self.scoped(then_block, &mut then_state, statement.span) {
                        branches.push(then_state);
                    }
                    let mut else_state = state.clone();
                    if else_block
                        .as_ref()
                        .is_none_or(|body| self.scoped(body, &mut else_state, statement.span))
                    {
                        branches.push(else_state);
                    }
                    if !Self::merge(state, branches) {
                        return false;
                    }
                }
                StmtKind::Match { value, arms } => {
                    // Ordinary checking already proved this match exhaustive.
                    self.discharge(state, value);
                    let mut branches = Vec::new();
                    for arm in arms {
                        let mut branch = state.clone();
                        branch.push(BTreeMap::new());
                        for binding in &arm.bindings {
                            self.bind(
                                &mut branch,
                                &binding.name,
                                self.result(binding.ty),
                                binding.span,
                            );
                        }
                        if self.block(&arm.body, &mut branch) {
                            self.pending(&branch, branch.len() - 1, arm.span, "match arm exit");
                            branch.pop();
                            branches.push(branch);
                        }
                    }
                    if !Self::merge(state, branches) {
                        return false;
                    }
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    let mut iteration = state.clone();
                    let first = iteration.len();
                    iteration.push(BTreeMap::new());
                    if let StmtKind::For {
                        name,
                        name_span,
                        element_type,
                        ..
                    } = &statement.kind
                    {
                        self.bind(&mut iteration, name, self.result(*element_type), *name_span);
                    }
                    self.loops.push(first);
                    if self.block(body, &mut iteration) {
                        self.pending(&iteration, first, statement.span, "loop iteration exit");
                    }
                    self.loops.pop();
                    // A loop may execute zero times. Handling an outer value in
                    // its body does not discharge the obligation after the loop.
                }
                StmtKind::Break | StmtKind::Continue => {
                    if let Some(&first) = self.loops.last() {
                        self.pending(state, first, statement.span, "loop control exit");
                    }
                    return false;
                }
                StmtKind::FieldAssign { .. } | StmtKind::Expr(_) => {}
            }
        }
        true
    }
}
