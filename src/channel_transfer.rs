//! Conservative channel payload check, to be called after nominal normalization
//! and concrete generic specialization, and again before descriptor emission.
use crate::ast::{Program, Type};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    TaskOrVoid,
    UnapprovedResource,
    MissingDeclaration,
    RecursiveOrTooDeep,
}

#[cfg(test)]
pub fn check_channel_transfer(program: &Program, ty: Type) -> Result<(), TransferError> {
    check_channel_transfer_excluding(program, ty, &[])
}

/// Supply normalized file/foreign/scoped resource types from the compiler's
/// ownership metadata. Exclusions apply transitively, including alias targets.
pub fn check_channel_transfer_excluding(
    program: &Program,
    ty: Type,
    excluded: &[Type],
) -> Result<(), TransferError> {
    check_with_visits(program, ty, excluded, None).0
}

/// Worker transfer has a distinct context. Authenticated endpoints retain shared
/// channel state, but their payload must pass the stricter channel payload check.
pub fn check_worker_transfer(
    program: &Program,
    registry: &crate::channel_registry::Registry,
    context: &crate::channel_registry::ExpansionContext,
    ty: Type,
    excluded: &[Type],
) -> Result<(), TransferError> {
    let view = registry
        .validated(program, context)
        .map_err(|_| TransferError::UnapprovedResource)?;
    check_with_visits(program, ty, excluded, Some(&view)).0
}

const MAX_NESTING: usize = 128;

#[derive(Default)]
struct VisitState {
    active: BTreeSet<(bool, usize)>,
    successful: std::collections::BTreeMap<(bool, usize), usize>,
    visits: usize,
}

/// Successful aggregate subtrees memoize their height, not merely membership.
/// Cached reuse therefore still rejects a subtree at an excessively deep site.
fn check_with_visits(
    program: &Program,
    ty: Type,
    excluded: &[Type],
    worker: Option<&crate::channel_registry::ValidatedRegistry<'_>>,
) -> (Result<(), TransferError>, usize) {
    fn visit(
        program: &Program,
        ty: Type,
        depth: usize,
        state: &mut VisitState,
        excluded: &[Type],
        worker: Option<&crate::channel_registry::ValidatedRegistry<'_>>,
    ) -> Result<usize, TransferError> {
        state.visits = state.visits.saturating_add(1);
        if excluded.contains(&ty) {
            return Err(TransferError::UnapprovedResource);
        }
        let registered = worker.and_then(|view| view.schema(ty));
        if let Some(crate::channels::Schema::Endpoint { payload, .. }) = registered {
            let Type::Record(id) = ty else { unreachable!() };
            let key = (false, id);
            if state.successful.get(&key) == Some(&0) {
                return Ok(0);
            }
            check_channel_transfer_excluding(program, payload, excluded)?;
            state.successful.insert(key, 0);
            return Ok(0);
        }
        match ty {
            Type::Task | Type::Void => Err(TransferError::TaskOrVoid),
            Type::FileHandle => Err(TransferError::UnapprovedResource),
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::Str
            | Type::Buffer
            | Type::Ints
            | Type::TcpListener
            | Type::TcpSocket
            | Type::TlsSocket => Ok(0),
            Type::Record(id) | Type::Enum(id) => {
                if let Some(name) = program.type_names.get(id) {
                    // Conservative payload exclusion only. Prefixes never grant
                    // endpoint dispatch/transfer authority; use the registry.
                    if registered.is_none()
                        && (name.starts_with("__bhg_sender_")
                            || name.starts_with("__bhg_receiver_")
                            || name.starts_with("__bhg_channel_")
                            || name == "file_handle")
                    {
                        return Err(TransferError::UnapprovedResource);
                    }
                }
                let key = (matches!(ty, Type::Enum(_)), id);
                if depth >= MAX_NESTING || state.active.contains(&key) {
                    return Err(TransferError::RecursiveOrTooDeep);
                }
                if let Some(&height) = state.successful.get(&key) {
                    return if depth + height <= MAX_NESTING {
                        Ok(height)
                    } else {
                        Err(TransferError::RecursiveOrTooDeep)
                    };
                }
                state.active.insert(key);
                let children: Result<Vec<Type>, TransferError> = match ty {
                    Type::Record(_) => program
                        .record(id)
                        .ok_or(TransferError::MissingDeclaration)
                        .map(|r| r.fields.iter().map(|f| f.ty).collect()),
                    Type::Enum(_) => program
                        .enum_decl(id)
                        .ok_or(TransferError::MissingDeclaration)
                        .map(|e| {
                            e.variants
                                .iter()
                                .flat_map(|v| &v.fields)
                                .map(|f| f.ty)
                                .collect()
                        }),
                    _ => unreachable!(),
                };
                let result = children
                    .and_then(|children| {
                        children.into_iter().try_fold(0, |height, child| {
                            visit(program, child, depth + 1, state, excluded, worker)
                                .map(|child_height| height.max(child_height))
                        })
                    })
                    .map(|height| height + 1);
                state.active.remove(&key);
                if let Ok(height) = result {
                    state.successful.insert(key, height);
                }
                result
            }
        }
    }
    let mut state = VisitState::default();
    let result = visit(program, ty, 0, &mut state, excluded, worker).map(|_| ());
    (result, state.visits)
}

#[cfg(test)]
use crate::channel_model as worker_model;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{EnumDecl, EnumVariant, RecordDecl, RecordField, Span};
    fn field(ty: Type) -> RecordField {
        RecordField {
            name: "value".into(),
            ty,
            span: Span::default(),
        }
    }
    fn record(program: &mut Program, name: &str, fields: Vec<RecordField>) -> Type {
        let id = program.intern_type_name(name);
        program.records.push(RecordDecl {
            name: name.into(),
            fields,
            methods: vec![],
            span: Span::default(),
        });
        Type::Record(id)
    }
    #[test]
    fn concrete_nested_payloads_and_resource_semantics() {
        let mut p = Program::default();
        let inner = record(&mut p, "Text", vec![field(Type::Buffer)]);
        let outer = record(&mut p, "List[Text]", vec![field(inner), field(inner)]);
        assert_eq!(check_channel_transfer(&p, outer), Ok(()));
        for ty in [
            Type::Int,
            Type::Float,
            Type::Bool,
            Type::Str,
            Type::Buffer,
            Type::Ints,
            Type::TcpListener,
            Type::TcpSocket,
            Type::TlsSocket,
        ] {
            assert_eq!(check_channel_transfer(&p, ty), Ok(()));
        }
    }
    #[test]
    fn rejected_payload_in_any_enum_variant() {
        let mut p = Program::default();
        let id = p.intern_type_name("IoResult[Task]");
        p.enums.push(EnumDecl {
            name: "IoResult[Task]".into(),
            span: Span::default(),
            variants: vec![
                EnumVariant {
                    name: "ok".into(),
                    fields: vec![field(Type::Buffer)],
                    span: Span::default(),
                },
                EnumVariant {
                    name: "error".into(),
                    fields: vec![field(Type::Task)],
                    span: Span::default(),
                },
            ],
        });
        assert_eq!(
            check_channel_transfer(&p, Type::Enum(id)),
            Err(TransferError::TaskOrVoid)
        );
        for ty in [Type::Task, Type::Void] {
            assert_eq!(
                check_channel_transfer(&p, ty),
                Err(TransferError::TaskOrVoid)
            );
        }
    }
    #[test]
    fn file_and_endpoint_resources_are_rejected_transitively() {
        let mut p = Program::default();
        let file = record(&mut p, "ForeignFile", vec![field(Type::Int)]);
        let wrapper = record(&mut p, "IoResult[ForeignFile]", vec![field(file)]);
        assert_eq!(
            check_channel_transfer_excluding(&p, wrapper, &[file]),
            Err(TransferError::UnapprovedResource)
        );
        let sender = record(&mut p, "__bhg_sender_0", vec![field(Type::Int)]);
        let outer = record(&mut p, "EndpointBox", vec![field(sender)]);
        assert_eq!(
            check_channel_transfer(&p, outer),
            Err(TransferError::UnapprovedResource)
        );
    }
    #[test]
    fn worker_endpoint_eligibility_is_distinct_from_payload_nesting() {
        let mut p = worker_model::model(Type::Buffer);
        let frame = record(
            &mut p,
            "WorkerFrame",
            vec![field(Type::Record(0)), field(Type::Record(1))],
        );
        let (registry, context) = crate::channel_registry::fixture(&p, true).unwrap();
        for ty in [Type::Record(0), Type::Record(1), Type::Record(2), frame] {
            assert_eq!(
                check_worker_transfer(&p, &registry, &context, ty, &[]),
                Ok(())
            );
            assert_eq!(
                check_channel_transfer(&p, ty),
                Err(TransferError::UnapprovedResource)
            );
            assert_eq!(
                check_worker_transfer(&p, &registry, &context, ty, &[Type::Buffer]),
                Err(TransferError::UnapprovedResource)
            );
        }
        let (unissued, unissued_context) = crate::channel_registry::fixture(&p, false).unwrap();
        assert_eq!(
            check_worker_transfer(&p, &unissued, &unissued_context, Type::Record(0), &[]),
            Err(TransferError::UnapprovedResource)
        );
        assert_eq!(
            check_worker_transfer(&p, &registry, &unissued_context, frame, &[]),
            Err(TransferError::UnapprovedResource)
        );
    }
    #[test]
    fn legal_shared_dag_has_linear_visit_bound() {
        let mut p = Program::default();
        let mut ty = record(&mut p, "R0", vec![field(Type::Int), field(Type::Int)]);
        for i in 1..=30 {
            ty = record(&mut p, &format!("R{i}"), vec![field(ty), field(ty)]);
        }
        let (result, visits) = check_with_visits(&p, ty, &[], None);
        assert_eq!(result, Ok(()));
        assert!(
            visits <= 2 * p.records.len() + 3,
            "shared DAG visited {visits} nodes"
        );
    }
    #[test]
    fn memoized_height_enforces_depth_at_every_reuse_site() {
        let mut p = Program::default();
        let mut shared = Type::Int;
        for i in 0..10 {
            shared = record(&mut p, &format!("Shared{i}"), vec![field(shared)]);
        }
        let mut deep = shared;
        for i in 0..118 {
            deep = record(&mut p, &format!("Ancestor{i}"), vec![field(deep)]);
        }
        // Shared subtree is first cached at shallow depth, then reused at depth
        // 119. Ignoring its cached height would incorrectly accept this graph.
        let root = record(&mut p, "Root", vec![field(shared), field(deep)]);
        assert_eq!(
            check_channel_transfer(&p, root),
            Err(TransferError::RecursiveOrTooDeep)
        );
        let mut p = Program::default();
        let mut exact = Type::Int;
        for i in 0..128 {
            exact = record(&mut p, &format!("Exact{i}"), vec![field(exact)]);
        }
        assert_eq!(check_channel_transfer(&p, exact), Ok(()));
        let too_deep = record(&mut p, "TooDeep", vec![field(exact)]);
        assert_eq!(
            check_channel_transfer(&p, too_deep),
            Err(TransferError::RecursiveOrTooDeep)
        );
    }
    #[test]
    fn corrupt_missing_recursive_and_deep_types_fail_closed() {
        let mut p = Program::default();
        assert_eq!(
            check_channel_transfer(&p, Type::Record(100)),
            Err(TransferError::MissingDeclaration)
        );
        assert_eq!(
            check_channel_transfer(&p, Type::Enum(100)),
            Err(TransferError::MissingDeclaration)
        );
        let id = p.intern_type_name("Recursive");
        let recursive = record(&mut p, "Recursive", vec![field(Type::Record(id))]);
        assert_eq!(
            check_channel_transfer(&p, recursive),
            Err(TransferError::RecursiveOrTooDeep)
        );
        let mut ty = Type::Int;
        for i in 0..129 {
            ty = record(&mut p, &format!("Depth{i}"), vec![field(ty)]);
        }
        assert_eq!(
            check_channel_transfer(&p, ty),
            Err(TransferError::RecursiveOrTooDeep)
        );
    }
}
