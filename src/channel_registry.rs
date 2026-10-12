//! Immutable channel authority. Consumers cannot construct registry entries,
//! expansion contexts or tokens from source spelling, raw IDs or paths.
//!
//! The production issuer is intentionally not exported in this corrective draft.
//! Root must place issuance in the trusted module/generic expansion boundary and
//! ratify its loader/template evidence before adding a production call site.
use crate::ast::{Program, Span, Type};
use crate::channels::{validate_declaration_shape, Schema};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModuleRole {
    #[cfg(test)]
    Ordinary,
    RegisteredChannelStd,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleIdentity {
    canonical_file: PathBuf,
    role: ModuleRole,
    exact_source: Arc<str>,
    template_span: Span,
}
#[derive(Debug)]
struct ExpansionIdentity {
    module: ModuleIdentity,
}
/// Opaque provenance issued alongside the registry by validated expansion.
/// A clone preserves authority; reconstructing identity from strings cannot.
#[derive(Debug, Clone)]
pub struct ExpansionContext {
    identity: Arc<ExpansionIdentity>,
}
type FieldSnapshot = (String, Type, Span);
type RecordSnapshot = (String, Span, Vec<FieldSnapshot>);
type VariantSnapshot = (String, Span, Vec<FieldSnapshot>);
type EnumSnapshot = (String, Span, Vec<VariantSnapshot>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeSnapshot {
    names: Vec<String>,
    records: Vec<RecordSnapshot>,
    enums: Vec<EnumSnapshot>,
}
impl TypeSnapshot {
    fn capture(program: &Program) -> Self {
        let fields = |fields: &[crate::ast::RecordField]| {
            fields
                .iter()
                .map(|field| (field.name.clone(), field.ty, field.span))
                .collect()
        };
        Self {
            names: program.type_names.clone(),
            records: program
                .records
                .iter()
                .map(|r| (r.name.clone(), r.span, fields(&r.fields)))
                .collect(),
            enums: program
                .enums
                .iter()
                .map(|e| {
                    (
                        e.name.clone(),
                        e.span,
                        e.variants
                            .iter()
                            .map(|v| (v.name.clone(), v.span, fields(&v.fields)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}
#[derive(Debug, Clone)]
struct IssuedChannel {
    pair: usize,
    sender: usize,
    receiver: usize,
    payload: Type,
    send_result: usize,
    receive_result: usize,
    receipt: usize,
    send_failure: usize,
    receive_error: usize,
}
/// Immutable after issuance. No public/default/raw-ID constructor or deserializer.
#[derive(Debug)]
pub struct Registry {
    identity: Arc<ExpansionIdentity>,
    snapshot: TypeSnapshot,
    channels: Vec<IssuedChannel>,
}
/// A token borrows an authenticated entry. It cannot outlive its registry.
#[derive(Debug)]
pub struct ChannelToken<'a> {
    channel: &'a IssuedChannel,
    _program: &'a Program,
}
impl ChannelToken<'_> {
    pub fn pair(&self) -> usize {
        self.channel.pair
    }
    pub fn sender(&self) -> usize {
        self.channel.sender
    }
    pub fn receiver(&self) -> usize {
        self.channel.receiver
    }
    pub fn payload(&self) -> Type {
        self.channel.payload
    }
    pub fn send_result(&self) -> usize {
        self.channel.send_result
    }
    pub fn receive_result(&self) -> usize {
        self.channel.receive_result
    }
}
/// A stage-local read-only view. Its Program borrow prevents mutation after
/// snapshot/origin revalidation, and repeated type lookups do not rescan the AST.
pub struct ValidatedRegistry<'a> {
    registry: &'a Registry,
    _program: &'a Program,
}
impl ValidatedRegistry<'_> {
    pub fn schema(&self, ty: Type) -> Option<Schema> {
        self.registry.lookup_schema(ty)
    }
}
impl Registry {
    pub fn validated<'a>(
        &'a self,
        program: &'a Program,
        context: &ExpansionContext,
    ) -> Result<ValidatedRegistry<'a>, &'static str> {
        self.revalidate(program, context)?;
        Ok(ValidatedRegistry {
            registry: self,
            _program: program,
        })
    }
    fn lookup_schema(&self, ty: Type) -> Option<Schema> {
        let Type::Record(id) = ty else {
            return None;
        };
        for entry in &self.channels {
            if id == entry.pair {
                return Some(Schema::Channel {
                    payload: entry.payload,
                    sender: entry.sender,
                    receiver: entry.receiver,
                });
            }
            if id == entry.sender || id == entry.receiver {
                return Some(Schema::Endpoint {
                    payload: entry.payload,
                    direction: if id == entry.sender {
                        crate::channels::Direction::Sender
                    } else {
                        crate::channels::Direction::Receiver
                    },
                });
            }
        }
        None
    }
    pub fn revalidate(
        &self,
        program: &Program,
        context: &ExpansionContext,
    ) -> Result<(), &'static str> {
        if !Arc::ptr_eq(&self.identity, &context.identity) {
            return Err("channel expansion provenance mismatch");
        }
        if self.identity.module.role != ModuleRole::RegisteredChannelStd {
            return Err("module lacks trusted channel builtin registration");
        }
        if TypeSnapshot::capture(program) != self.snapshot {
            return Err("channel declaration or nominal ID snapshot changed");
        }
        for entry in &self.channels {
            let pair = unique_record(program, entry.pair)?;
            let Some(Schema::Channel {
                payload,
                sender,
                receiver,
            }) = validate_declaration_shape(program, pair)?
            else {
                return Err("issued channel pair lost its validated schema");
            };
            if payload != entry.payload || sender != entry.sender || receiver != entry.receiver {
                return Err("issued channel specialization changed");
            }
            for id in [sender, receiver] {
                if !unique_record(program, id)?.methods.is_empty() {
                    return Err("issued endpoint acquired source methods");
                }
            }
            unique_enum(program, entry.send_result)?;
            unique_enum(program, entry.receive_result)?;
            for id in [entry.receipt, entry.send_failure, entry.receive_error] {
                unique_enum(program, id)?;
            }
            let send = program
                .enum_decl(entry.send_result)
                .and_then(|e| e.result_shape())
                .ok_or("issued SendResult lost its shape")?;
            let receive = program
                .enum_decl(entry.receive_result)
                .and_then(|e| e.result_shape())
                .ok_or("issued ReceiveResult lost its shape")?;
            if send.success != Type::Enum(entry.receipt)
                || send.error != Type::Enum(entry.send_failure)
                || receive.success != entry.payload
                || receive.error != Type::Enum(entry.receive_error)
            {
                return Err("issued result family or normalized payload mismatch");
            }
        }
        Ok(())
    }
    pub fn channel<'a>(
        &'a self,
        program: &'a Program,
        context: &ExpansionContext,
        pair: usize,
    ) -> Result<ChannelToken<'a>, &'static str> {
        self.revalidate(program, context)?;
        let channel = self
            .channels
            .iter()
            .find(|entry| entry.pair == pair)
            .ok_or("channel nominal ID was not issued by expansion")?;
        Ok(ChannelToken {
            channel,
            _program: program,
        })
    }
    /// Only authenticated nominal IDs dispatch. Unissued generated-looking names
    /// are ordinary records, with no channel clone/drop/method/field privileges.
    pub fn schema(
        &self,
        program: &Program,
        context: &ExpansionContext,
        ty: Type,
    ) -> Result<Option<Schema>, &'static str> {
        self.revalidate(program, context)?;
        Ok(self.lookup_schema(ty))
    }
}
fn unique_record(program: &Program, id: usize) -> Result<&crate::ast::RecordDecl, &'static str> {
    let name = program
        .type_names
        .get(id)
        .ok_or("missing issued nominal ID")?;
    if program
        .type_names
        .iter()
        .filter(|candidate| *candidate == name)
        .count()
        != 1
    {
        return Err("duplicate issued nominal name");
    }
    let mut declarations = program.records.iter().filter(|r| &r.name == name);
    let declaration = declarations
        .next()
        .ok_or("missing issued record declaration")?;
    if declarations.next().is_some() || program.enums.iter().any(|e| &e.name == name) {
        return Err("ambiguous issued record declaration");
    }
    Ok(declaration)
}
fn unique_enum(program: &Program, id: usize) -> Result<(), &'static str> {
    let name = program
        .type_names
        .get(id)
        .ok_or("missing issued result ID")?;
    if program
        .type_names
        .iter()
        .filter(|candidate| *candidate == name)
        .count()
        != 1
        || program.enums.iter().filter(|e| &e.name == name).count() != 1
        || program.records.iter().any(|r| &r.name == name)
    {
        return Err("ambiguous issued result enum");
    }
    Ok(())
}

// Private issuer accepts an expansion receipt, not a parsed source Program's
// names. IssuedChannel entries must be minted by trusted normalized expansion.
// No public constructor is added until that production boundary is integrated.
fn seal_expansion(
    program: &Program,
    module: ModuleIdentity,
    entries: Vec<IssuedChannel>,
) -> Result<(Registry, ExpansionContext), &'static str> {
    if module.role != ModuleRole::RegisteredChannelStd {
        return Err("module lacks trusted channel builtin registration");
    }
    if !module.canonical_file.is_absolute()
        || module.exact_source.is_empty()
        || module.template_span.start >= module.template_span.end
        || module.template_span.end > module.exact_source.len()
    {
        return Err("invalid loaded module identity");
    }
    let mut issued_payloads = Vec::new();
    let mut owned_ids = std::collections::BTreeSet::new();
    for entry in &entries {
        if issued_payloads.contains(&entry.payload) {
            return Err("payload already has a canonical channel family");
        }
        issued_payloads.push(entry.payload);
        if [entry.pair, entry.sender, entry.receiver]
            .iter()
            .any(|id| !owned_ids.insert(*id))
        {
            return Err("channel issuance contains overlapping nominal IDs");
        }
    }
    let identity = Arc::new(ExpansionIdentity { module });
    let context = ExpansionContext {
        identity: Arc::clone(&identity),
    };
    let registry = Registry {
        identity,
        snapshot: TypeSnapshot::capture(program),
        channels: entries,
    };
    registry.revalidate(program, &context)?;
    Ok((registry, context))
}

/// Test-only issuer. Never compiled into the production compiler. Its path and
/// exact source snapshot identify the real draft std module, not a fake label.
#[cfg(test)]
pub(crate) fn fixture(
    program: &Program,
    issue: bool,
) -> Result<(Registry, ExpansionContext), &'static str> {
    let path = std::path::Path::new(file!())
        .parent()
        .unwrap()
        .join("../std/channels.bh");
    let canonical_file = std::fs::canonicalize(&path).map_err(|_| "missing fixture module")?;
    let source =
        std::fs::read_to_string(&canonical_file).map_err(|_| "unreadable fixture module")?;
    let start = source
        .find("struct Channel[T]")
        .ok_or("missing fixture Channel template")?;
    let end = source[start..]
        .find("\ndef channel[T]")
        .map(|i| start + i)
        .ok_or("missing fixture channel factory")?;
    let entries = if issue {
        vec![IssuedChannel {
            pair: 2,
            sender: 0,
            receiver: 1,
            payload: program
                .records
                .first()
                .and_then(|r| r.fields.first())
                .ok_or("missing fixture payload")?
                .ty,
            send_result: 6,
            receive_result: 7,
            receipt: 3,
            send_failure: 4,
            receive_error: 5,
        }]
    } else {
        vec![]
    };
    seal_expansion(
        program,
        ModuleIdentity {
            canonical_file,
            role: ModuleRole::RegisteredChannelStd,
            exact_source: Arc::from(source),
            template_span: Span::new(start, end),
        },
        entries,
    )
}

#[cfg(test)]
use crate::channel_model as model;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_module_registry_accepts_only_issued_normalized_nominal_ids() {
        let p = model::model(Type::Buffer);
        let (registry, context) = fixture(&p, true).unwrap();
        let token = registry.channel(&p, &context, 2).unwrap();
        assert_eq!(
            (token.payload(), token.send_result(), token.receive_result()),
            (Type::Buffer, 6, 7)
        );
        assert!(registry
            .schema(&p, &context, Type::Record(0))
            .unwrap()
            .is_some());
        assert!(registry.identity.module.canonical_file.is_absolute());
        assert!(registry
            .identity
            .module
            .exact_source
            .contains("private __sender: Sender[T]"));
        assert!(registry.channel(&p, &context, 0).is_err());
        assert!(registry.channel(&p, &context, usize::MAX).is_err());
    }
    #[test]
    fn exact_fake_prefix_shapes_and_raw_ids_have_no_dispatch_authority() {
        let p = model::model(Type::Buffer);
        let (unissued, context) = fixture(&p, false).unwrap();
        for id in 0..3 {
            assert_eq!(unissued.schema(&p, &context, Type::Record(id)), Ok(None));
        }
        assert!(crate::channel_codegen::emit_helpers(
            &p,
            &unissued,
            &context,
            2,
            "aarch64-apple-darwin",
            &[]
        )
        .is_err());
    }
    #[test]
    fn stale_origin_nonce_and_nominal_remapping_are_rejected() {
        let p = model::model(Type::Buffer);
        let (registry, context) = fixture(&p, true).unwrap();
        let (_, other_context) = fixture(&p, true).unwrap();
        assert!(registry.channel(&p, &other_context, 2).is_err());
        let mut forged = p.clone();
        forged.type_names.swap(0, 1);
        assert!(registry.channel(&forged, &context, 2).is_err());
        forged = p.clone();
        forged.records[2].span = Span::new(10, 20);
        assert!(registry.channel(&forged, &context, 2).is_err());
        forged = p.clone();
        forged.records[0].fields[0].ty = Type::Ints;
        assert!(registry.channel(&forged, &context, 2).is_err());
        forged = p.clone();
        forged.enums[3].variants[0].fields[0].ty = Type::Enum(5);
        assert!(registry.channel(&forged, &context, 2).is_err());
    }
    #[test]
    fn real_filename_without_builtin_role_and_duplicate_payload_family_are_rejected() {
        let p = model::model(Type::Buffer);
        let (registry, _) = fixture(&p, true).unwrap();
        let mut untrusted = registry.identity.module.clone();
        untrusted.role = ModuleRole::Ordinary;
        assert!(seal_expansion(&p, untrusted, registry.channels.clone()).is_err());
        let duplicate = vec![registry.channels[0].clone(), registry.channels[0].clone()];
        assert_eq!(
            seal_expansion(&p, registry.identity.module.clone(), duplicate).unwrap_err(),
            "payload already has a canonical channel family"
        );
    }
    #[test]
    fn source_getter_bodies_or_declarations_are_never_substituted() {
        use crate::ast::{Function, Param, Stmt, StmtKind};
        let p = model::model(Type::Buffer);
        let (registry, context) = fixture(&p, true).unwrap();
        for body in [
            None,
            Some(vec![]),
            Some(vec![Stmt {
                kind: StmtKind::Return(None),
                span: Span::default(),
            }]),
        ] {
            let mut forged = p.clone();
            forged.records[2].methods.push(Function {
                name: "sender".into(),
                params: vec![Param {
                    mode: crate::ast::ParamMode::Borrowed,
                    name: "self".into(),
                    ty: Type::Record(2),
                    span: Span::default(),
                }],
                return_type: Type::Record(0),
                body,
                span: Span::default(),
            });
            assert!(registry.channel(&forged, &context, 2).is_err());
            assert!(fixture(&forged, true).is_err());
            assert!(crate::channel_codegen::emit_helpers(
                &forged,
                &registry,
                &context,
                2,
                "aarch64-apple-darwin",
                &[]
            )
            .is_err());
        }
    }
}

// Production loader and receipt pipeline. All construction stays crate-private;
// public AST consumers receive only the sealed bound authority.
#[derive(Debug, Clone)]
pub(crate) struct LoadedChannelModule {
    module: ModuleIdentity,
    owner: usize,
}
impl LoadedChannelModule {
    pub(crate) fn owner(&self) -> usize {
        self.owner
    }
}
struct ShippedRegistration {
    canonical_file: PathBuf,
    reviewed: &'static str,
}
impl ShippedRegistration {
    fn repository() -> Option<Self> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("std/channels.bh");
        Some(Self {
            canonical_file: std::fs::canonicalize(path).ok()?,
            reviewed: include_str!("../std/channels.bh"),
        })
    }
}
pub(crate) fn registered_module(
    files: &[crate::project::SourceFile],
) -> Result<Option<LoadedChannelModule>, crate::Diagnostic> {
    let Some(registration) = ShippedRegistration::repository() else {
        return Ok(None);
    };
    let mut found = None;
    for (owner, file) in files.iter().enumerate() {
        if std::fs::canonicalize(&file.path).ok().as_ref() != Some(&registration.canonical_file) {
            continue;
        }
        if file.source != registration.reviewed {
            return Err(crate::Diagnostic::new("C101", "registered channel module differs from reviewed shipped bytes; intrinsic issuance refused", Span::new(file.base, file.base + file.source.len())));
        }
        if found.is_some() {
            return Err(crate::Diagnostic::new(
                "C101",
                "duplicate registered channel module",
                Span::new(file.base, file.base),
            ));
        }
        let start = registration
            .reviewed
            .find("struct Channel[T]")
            .expect("reviewed Channel template");
        let end = registration.reviewed[start..]
            .find("\ndef channel[T]")
            .map(|i| start + i)
            .expect("reviewed channel factory");
        found = Some(LoadedChannelModule {
            owner,
            module: ModuleIdentity {
                canonical_file: registration.canonical_file.clone(),
                role: ModuleRole::RegisteredChannelStd,
                exact_source: Arc::from(file.source.as_str()),
                template_span: Span::new(start, end),
            },
        });
    }
    Ok(found)
}
#[derive(Debug, Clone)]
pub(crate) struct ModuleEvidence {
    loaded: LoadedChannelModule,
    names: std::collections::BTreeMap<String, String>,
    source: Arc<str>,
    boundaries: Vec<usize>,
}
impl ModuleEvidence {
    pub(crate) fn names(&self) -> &std::collections::BTreeMap<String, String> {
        &self.names
    }
    pub(crate) fn validate(
        &self,
        source: &str,
        boundaries: &[usize],
    ) -> Result<(), crate::Diagnostic> {
        if self.source.as_ref() != source || self.boundaries != boundaries {
            return Err(crate::Diagnostic::new(
                "C101",
                "channel module lowering receipt does not match source",
                Span::default(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn lowered_receipt(
    loaded: LoadedChannelModule,
    names: std::collections::BTreeMap<String, String>,
    source: &str,
    boundaries: &[usize],
) -> Result<ModuleEvidence, crate::Diagnostic> {
    for name in [
        "Sender",
        "Receiver",
        "Channel",
        "SendResult",
        "SendFailure",
        "ReceiveResult",
        "ChannelSent",
        "ReceiveError",
        "channel",
    ] {
        if !names.contains_key(name) {
            return Err(crate::Diagnostic::new(
                "C101",
                "registered channel export missing",
                Span::default(),
            ));
        }
    }
    Ok(ModuleEvidence {
        loaded,
        names,
        source: Arc::from(source),
        boundaries: boundaries.to_vec(),
    })
}
#[derive(Debug, Clone)]
pub(crate) struct FamilyNames {
    pub payload: String,
    pub pair: String,
    pub sender: String,
    pub receiver: String,
    pub send_result: String,
    pub send_failure: String,
    pub receive_result: String,
    pub receipt: String,
    pub receive_error: String,
    pub factory: String,
}
#[derive(Debug, Clone)]
pub(crate) struct ExpansionReceipts {
    module: ModuleEvidence,
    families: Vec<FamilyNames>,
    source: Arc<str>,
    boundaries: Vec<usize>,
}
impl ExpansionReceipts {
    pub(crate) fn validate(
        &self,
        source: &str,
        boundaries: &[usize],
    ) -> Result<(), crate::Diagnostic> {
        if self.source.as_ref() != source || self.boundaries != boundaries {
            return Err(crate::Diagnostic::new(
                "C101",
                "channel specialization receipt does not match source",
                Span::default(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn expanded_receipts(
    module: ModuleEvidence,
    families: Vec<FamilyNames>,
    source: &str,
    boundaries: &[usize],
) -> ExpansionReceipts {
    ExpansionReceipts {
        module,
        families,
        source: Arc::from(source),
        boundaries: boundaries.to_vec(),
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct FactoryShape {
    name: String,
    pair: usize,
    parameter: String,
    span: Span,
    call_span: Span,
}
fn factory_shape(program: &Program, name: &str, pair: usize) -> Result<FactoryShape, &'static str> {
    let matches: Vec<_> = program
        .functions
        .iter()
        .filter(|f| f.name == name)
        .collect();
    let [function] = matches.as_slice() else {
        return Err("registered factory declaration missing/ambiguous");
    };
    let [parameter] = function.params.as_slice() else {
        return Err("registered factory arity changed");
    };
    if parameter.ty != Type::Int
        || parameter.mode != crate::ast::ParamMode::Borrowed
        || function.return_type != Type::Record(pair)
    {
        return Err("registered factory signature changed");
    }
    let Some(body) = &function.body else {
        return Err("registered factory became external");
    };
    let [statement] = body.as_slice() else {
        return Err("registered factory body changed");
    };
    let crate::ast::StmtKind::Return(Some(call)) = &statement.kind else {
        return Err("registered factory return changed");
    };
    let crate::ast::ExprKind::Call {
        name: constructor,
        args,
    } = &call.kind
    else {
        return Err("registered factory constructor changed");
    };
    if program.type_names.get(pair) != Some(constructor)
        || args.len() != 1
        || !matches!(&args[0].kind, crate::ast::ExprKind::Name(name) if name == &parameter.name)
    {
        return Err("registered factory argument or pair identity changed");
    }
    Ok(FactoryShape {
        name: name.into(),
        pair,
        parameter: parameter.name.clone(),
        span: function.span,
        call_span: call.span,
    })
}
#[derive(Debug)]
pub struct BoundAuthority {
    registry: Registry,
    context: ExpansionContext,
    factories: Vec<FactoryShape>,
}
impl BoundAuthority {
    pub fn validate(&self, program: &Program) -> Result<(), &'static str> {
        self.registry.revalidate(program, &self.context)?;
        for shape in &self.factories {
            if &factory_shape(program, &shape.name, shape.pair)? != shape {
                return Err("registered factory mutation");
            }
        }
        Ok(())
    }
    pub fn schema(&self, ty: Type) -> Option<Schema> {
        self.registry.lookup_schema(ty)
    }
    pub fn context(&self) -> &ExpansionContext {
        &self.context
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    pub fn factory_allowed(&self, pair: usize, function: &str, span: Span) -> bool {
        self.factories
            .iter()
            .any(|f| f.pair == pair && f.name == function && f.call_span == span)
    }
    pub fn pairs(&self) -> impl Iterator<Item = usize> + '_ {
        self.registry.channels.iter().map(|f| f.pair)
    }
}
fn nominal(program: &Program, name: &str) -> Result<usize, &'static str> {
    program
        .type_names
        .iter()
        .position(|candidate| candidate == name)
        .ok_or("receipt nominal declaration missing")
}
fn concrete(program: &Program, name: &str) -> Result<Type, &'static str> {
    Ok(match name {
        "int" => Type::Int,
        "bool" => Type::Bool,
        "float" => Type::Float,
        "str" => Type::Str,
        "buffer" => Type::Buffer,
        "ints" => Type::Ints,
        "void" => Type::Void,
        "task" => Type::Task,
        "tcp_listener" => Type::TcpListener,
        "tcp_socket" => Type::TcpSocket,
        "tls_socket" => Type::TlsSocket,
        "file_handle" => Type::FileHandle,
        _ => {
            let id = nominal(program, name)?;
            if program.enum_decl(id).is_some() {
                Type::Enum(id)
            } else {
                Type::Record(id)
            }
        }
    })
}
pub(crate) fn bind_expansion(
    program: &mut Program,
    receipts: &ExpansionReceipts,
) -> Result<(), crate::Diagnostic> {
    let issue = || -> Result<BoundAuthority, String> {
        let mut entries = Vec::new();
        let mut factories = Vec::new();
        for names in &receipts.families {
            let pair = nominal(program, &names.pair)?;
            let payload = concrete(program, &names.payload)?;
            crate::channel_transfer::check_channel_transfer_excluding(program, payload, &[])
                .map_err(|e| format!("channel payload cannot transfer: {e:?}"))?;
            entries.push(IssuedChannel {
                pair,
                sender: nominal(program, &names.sender)?,
                receiver: nominal(program, &names.receiver)?,
                payload,
                send_result: nominal(program, &names.send_result)?,
                receive_result: nominal(program, &names.receive_result)?,
                receipt: nominal(program, &names.receipt)?,
                send_failure: nominal(program, &names.send_failure)?,
                receive_error: nominal(program, &names.receive_error)?,
            });
            factories.push(factory_shape(program, &names.factory, pair)?);
        }
        let (registry, context) =
            seal_expansion(program, receipts.module.loaded.module.clone(), entries)?;
        Ok(BoundAuthority {
            registry,
            context,
            factories,
        })
    };
    let authority =
        issue().map_err(|message| crate::Diagnostic::new("C101", message, Span::default()))?;
    program.channels = Some(Arc::new(authority));
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Sender,
    Receiver,
    Send,
    TrySend,
    Recv,
    TryRecv,
    Close,
}
#[derive(Debug, Clone)]
pub struct ResolvedOperation {
    pub pair: usize,
    pub owner: usize,
    pub operation: Operation,
    pub params: Vec<Type>,
    pub result: Type,
    pub helper: String,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sender => "sender",
            Self::Receiver => "receiver",
            Self::Send => "send",
            Self::TrySend => "try_send",
            Self::Recv => "recv",
            Self::TryRecv => "try_recv",
            Self::Close => "close",
        }
    }
}
impl BoundAuthority {
    /// Read-only operation metadata from this issued authority, after snapshot validation.
    pub fn operations(&self, program: &Program) -> Result<Vec<ResolvedOperation>, &'static str> {
        self.validate(program)?;
        let mut operations = Vec::new();
        for family in &self.registry.channels {
            for (owner, names) in [
                (family.pair, &["sender", "receiver"][..]),
                (family.sender, &["send", "try_send", "close"][..]),
                (family.receiver, &["recv", "try_recv", "close"][..]),
            ] {
                let owner = program
                    .type_names
                    .get(owner)
                    .ok_or("issued owner missing")?;
                for name in names {
                    operations.push(
                        self.operation(program, &format!("{owner}.{name}"))
                            .ok_or("issued operation missing")?,
                    );
                }
            }
        }
        Ok(operations)
    }
}
impl ResolvedOperation {
    pub fn blocking(&self) -> bool {
        matches!(self.operation, Operation::Send | Operation::Recv)
    }
}
impl BoundAuthority {
    pub fn operation(&self, program: &Program, key: &str) -> Option<ResolvedOperation> {
        let (owner_name, method) = key.split_once('.')?;
        let owner = program
            .type_names
            .iter()
            .position(|name| name == owner_name)?;
        for f in &self.registry.channels {
            let (operation, params, result, helper) = if owner == f.pair {
                match method {
                    "sender" => (
                        Operation::Sender,
                        vec![Type::Record(owner)],
                        Type::Record(f.sender),
                        format!("@bhasa.channel.{}.sender", f.pair),
                    ),
                    "receiver" => (
                        Operation::Receiver,
                        vec![Type::Record(owner)],
                        Type::Record(f.receiver),
                        format!("@bhasa.channel.{}.receiver", f.pair),
                    ),
                    _ => continue,
                }
            } else if owner == f.sender {
                match method {
                    "send" | "try_send" => (
                        if method == "send" {
                            Operation::Send
                        } else {
                            Operation::TrySend
                        },
                        vec![Type::Record(owner), f.payload],
                        Type::Enum(f.send_result),
                        format!("@bhasa.channel.sender.{owner}.{method}"),
                    ),
                    "close" => (
                        Operation::Close,
                        vec![Type::Record(owner)],
                        Type::Void,
                        format!("@bhasa.channel.endpoint.{owner}.close"),
                    ),
                    _ => continue,
                }
            } else if owner == f.receiver {
                match method {
                    "recv" | "try_recv" => (
                        if method == "recv" {
                            Operation::Recv
                        } else {
                            Operation::TryRecv
                        },
                        vec![Type::Record(owner)],
                        Type::Enum(f.receive_result),
                        format!("@bhasa.channel.receiver.{owner}.{method}"),
                    ),
                    "close" => (
                        Operation::Close,
                        vec![Type::Record(owner)],
                        Type::Void,
                        format!("@bhasa.channel.endpoint.{owner}.close"),
                    ),
                    _ => continue,
                }
            } else {
                continue;
            };
            return Some(ResolvedOperation {
                pair: f.pair,
                owner,
                operation,
                params,
                result,
                helper,
            });
        }
        None
    }
}

/// Borrowed native input issued only by the authenticated expansion producer.
/// No decoder/constructor accepts caller-provided rows. Source and context live
/// with the manifest until the synchronous native call finishes.
// Reference builds retain this private producer for source-level ABI probes.
#[cfg_attr(not(feature = "bhasa-frontend"), allow(dead_code))]
#[derive(Debug)]
pub(crate) struct NativeManifest {
    words: Vec<i64>,
    receipts: ExpansionReceipts,
    nonce: i64,
}
#[cfg_attr(not(feature = "bhasa-frontend"), allow(dead_code))]
impl NativeManifest {
    pub(crate) fn words(&self) -> &[i64] {
        &self.words
    }
    pub(crate) fn validate(
        &self,
        source: &str,
        boundaries: &[usize],
    ) -> Result<(), crate::Diagnostic> {
        self.receipts.validate(source, boundaries)?;
        if self.words.get(1) != Some(&self.nonce) || self.nonce <= 0 {
            return Err(crate::Diagnostic::new(
                "C101",
                "invalid native channel compilation context",
                Span::default(),
            ));
        }
        Ok(())
    }
}
impl ExpansionReceipts {
    #[cfg_attr(not(feature = "bhasa-frontend"), allow(dead_code))]
    pub(crate) fn native_manifest(
        &self,
        source: &str,
        boundaries: &[usize],
        target: &str,
    ) -> Result<NativeManifest, Vec<crate::Diagnostic>> {
        self.validate(source, boundaries).map_err(|e| vec![e])?;
        let bad = |message: &str| vec![crate::Diagnostic::new("C101", message, Span::default())];
        crate::target::require_native_posix64(target).map_err(|message| bad(&message))?;
        // Parse exact emitted bytes before remapping spans to original files.
        let mut program = Program::default();
        let mut start = 0;
        let mut ends = boundaries.to_vec();
        if ends.last().copied() != Some(source.len()) {
            ends.push(source.len());
        }
        for end in ends {
            if end < start
                || end > source.len()
                || !source.is_char_boundary(start)
                || !source.is_char_boundary(end)
            {
                return Err(bad("invalid native channel source boundary"));
            }
            let mut tokens = crate::lexer::lex(&source[start..end])?;
            for token in &mut tokens {
                token.span.start += start;
                token.span.end += start;
            }
            program.merge(crate::parser::parse_tokens(tokens)?);
            start = end.saturating_add(1);
        }
        crate::sema::normalize_enum_types(&mut program);
        // Authenticate the same generated schemas and factory bodies, with raw
        // expanded declaration spans for the native parser's nominal binding.
        bind_expansion(&mut program, self).map_err(|e| vec![e])?;
        let anchor = |name: &str| -> Result<[i64; 2], Vec<crate::Diagnostic>> {
            let span = program
                .records
                .iter()
                .find(|d| d.name == name)
                .map(|d| d.span)
                .or_else(|| {
                    program
                        .enums
                        .iter()
                        .find(|d| d.name == name)
                        .map(|d| d.span)
                })
                .or_else(|| {
                    program
                        .functions
                        .iter()
                        .find(|d| d.name == name)
                        .map(|d| d.span)
                })
                .ok_or_else(|| bad("native manifest declaration anchor missing"))?;
            Ok([
                i64::try_from(span.start).map_err(|_| bad("native manifest span overflow"))?,
                i64::try_from(span.end).map_err(|_| bad("native manifest span overflow"))?,
            ])
        };
        static NEXT_CONTEXT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
        let nonce = NEXT_CONTEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| bad("native channel compilation contexts exhausted"))?;
        let mut words = vec![
            1,
            nonce,
            i64::try_from(source.len()).map_err(|_| bad("native source too large"))?,
            i64::try_from(self.families.len()).map_err(|_| bad("too many channel families"))?,
            21,
        ];
        for family in &self.families {
            for name in [&family.pair, &family.sender, &family.receiver] {
                words.extend(anchor(name)?);
            }
            let ty = concrete(&program, &family.payload).map_err(&bad)?;
            let tag = match ty {
                Type::Void => 0,
                Type::Int => 1,
                Type::Bool => 2,
                Type::Str => 3,
                Type::Buffer => 4,
                Type::Ints => 5,
                Type::Task => 6,
                Type::TcpListener => 7,
                Type::TcpSocket => 8,
                Type::TlsSocket => 9,
                Type::Float => 10,
                Type::FileHandle => return Err(bad("file_handle cannot be a channel payload")),
                Type::Record(_) => -1,
                Type::Enum(_) => -2,
            };
            words.push(tag);
            if tag < 0 {
                words.extend(anchor(&family.payload)?);
            } else {
                words.extend([0, 0]);
            }
            for name in [
                &family.send_result,
                &family.send_failure,
                &family.receipt,
                &family.receive_result,
                &family.receive_error,
                &family.factory,
            ] {
                words.extend(anchor(name)?);
            }
        }
        Ok(NativeManifest {
            words,
            receipts: self.clone(),
            nonce,
        })
    }
}
