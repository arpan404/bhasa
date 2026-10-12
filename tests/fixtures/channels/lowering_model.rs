use crate::ast::*;
type VariantShape = (&'static str, Option<Type>);
type EnumShape = (usize, Vec<VariantShape>);
pub fn model(payload: Type) -> Program {
    let span = Span::default();
    let field = |name: &str, ty| RecordField {
        name: name.into(),
        ty,
        span,
    };
    let mut p = Program::default();
    for name in [
        "__bhg_sender_0",
        "__bhg_receiver_1",
        "__bhg_channel_2",
        "ChannelSent",
        "SendFailure",
        "ReceiveError",
        "SendResult",
        "ReceiveResult",
    ] {
        p.intern_type_name(name);
    }
    p.records = vec![
        RecordDecl {
            name: p.type_names[0].clone(),
            fields: vec![field("__payload", payload)],
            methods: vec![],
            span,
        },
        RecordDecl {
            name: p.type_names[1].clone(),
            fields: vec![field("__payload", payload)],
            methods: vec![],
            span,
        },
        RecordDecl {
            name: p.type_names[2].clone(),
            fields: vec![
                field("__sender", Type::Record(0)),
                field("__receiver", Type::Record(1)),
            ],
            methods: vec![],
            span,
        },
    ];
    let shapes: Vec<EnumShape> = vec![
        (3, vec![("sent", None)]),
        (4, vec![("full", Some(payload)), ("closed", Some(payload))]),
        (5, vec![("empty", None), ("closed", None)]),
        (
            6,
            vec![("error", Some(Type::Enum(4))), ("ok", Some(Type::Enum(3)))],
        ),
        (
            7,
            vec![("error", Some(Type::Enum(5))), ("ok", Some(payload))],
        ),
    ];
    for (id, variants) in shapes {
        p.enums.push(EnumDecl {
            name: p.type_names[id].clone(),
            span,
            variants: variants
                .into_iter()
                .map(|(name, ty)| EnumVariant {
                    name: name.into(),
                    fields: ty.map(|ty| vec![field("value", ty)]).unwrap_or_default(),
                    span,
                })
                .collect(),
        });
    }
    p
}
