//! Structural validation only. Never use generated spelling as dispatch authority.
//! Registry authentication is required before this private validator is called. Marker fields describe
//! types only for endpoints; they are never exposed as storage or constructors.
use crate::ast::{Program, RecordDecl, Type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Sender,
    Receiver,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schema {
    Endpoint {
        payload: Type,
        direction: Direction,
    },
    Channel {
        payload: Type,
        sender: usize,
        receiver: usize,
    },
}

fn generated(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}
fn is_channel_record(name: &str) -> bool {
    ["__bhg_sender_", "__bhg_receiver_", "__bhg_channel_"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}
pub(crate) fn validate_declaration_shape(
    program: &Program,
    record: &RecordDecl,
) -> Result<Option<Schema>, &'static str> {
    if !is_channel_record(&record.name) {
        return Ok(None);
    }
    for (prefix, direction) in [
        ("__bhg_sender_", Direction::Sender),
        ("__bhg_receiver_", Direction::Receiver),
    ] {
        if generated(&record.name, prefix) {
            if !record.methods.is_empty() {
                return Err("endpoint methods must be compiler-issued");
            }
            let [field] = record.fields.as_slice() else {
                return Err("invalid endpoint metadata");
            };
            if field.name != "__payload"
                || matches!(field.ty, Type::Task | Type::Void | Type::FileHandle)
            {
                return Err("invalid endpoint payload metadata");
            }
            return Ok(Some(Schema::Endpoint {
                payload: field.ty,
                direction,
            }));
        }
    }
    if !generated(&record.name, "__bhg_channel_") {
        return Err("invalid channel record identity");
    }
    let [sender_field, receiver_field] = record.fields.as_slice() else {
        return Err("invalid channel pair metadata");
    };
    if sender_field.name != "__sender" || receiver_field.name != "__receiver" {
        return Err("invalid channel pair fields");
    }
    let (Type::Record(sender), Type::Record(receiver)) = (sender_field.ty, receiver_field.ty)
    else {
        return Err("channel pair requires typed endpoint records");
    };
    let sender_record = program.record(sender).ok_or("missing sender declaration")?;
    let receiver_record = program
        .record(receiver)
        .ok_or("missing receiver declaration")?;
    /* Read endpoint metadata directly so a corrupt pair->pair cycle never recurses. */
    fn endpoint_payload(record: &RecordDecl, prefix: &str) -> Result<Type, &'static str> {
        let [field] = record.fields.as_slice() else {
            return Err("invalid pair endpoint metadata");
        };
        if !generated(&record.name, prefix)
            || field.name != "__payload"
            || !record.methods.is_empty()
            || matches!(field.ty, Type::Task | Type::Void | Type::FileHandle)
        {
            return Err("invalid pair endpoint direction or payload");
        }
        Ok(field.ty)
    }
    let payload = endpoint_payload(sender_record, "__bhg_sender_")?;
    if endpoint_payload(receiver_record, "__bhg_receiver_")? != payload {
        return Err("channel endpoint payloads disagree");
    }
    if !record.methods.is_empty() {
        return Err(
            "Channel getters are compiler-issued; source method declarations are forbidden",
        );
    }
    Ok(Some(Schema::Channel {
        payload,
        sender,
        receiver,
    }))
}
