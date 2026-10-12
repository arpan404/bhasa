//! Isolated LLVM helper renderer. The coordinator must register every dependency
//! with Module and mirror schemas/checks in the native frontend before wiring it.
use crate::ast::{Program, Type};
use crate::channel_registry::{ExpansionContext, Registry};
use crate::channel_transfer::check_channel_transfer_excluding;

#[derive(Debug)]
pub struct Helpers {
    #[cfg(test)]
    pub descriptor_symbol: String,
    pub descriptor: String,
    pub definitions: String,
    pub declarations: Vec<String>,
    pub required_owned_helpers: Vec<Type>,
}
fn llvm(ty: Type) -> &'static str {
    match ty {
        Type::Int => "i64",
        Type::Float => "double",
        Type::Bool => "i1",
        Type::Void => "void",
        _ => "ptr",
    }
}
fn owned_helper(ty: Type, clone: bool) -> Option<String> {
    let operation = if clone { "clone" } else { "drop" };
    Some(match ty {
        Type::Record(id) => format!("@bhasa.record.{id}.{operation}"),
        Type::Enum(id) => format!("@bhasa.enum.{id}.{operation}"),
        Type::Buffer | Type::Ints | Type::TcpListener | Type::TcpSocket | Type::TlsSocket => {
            let stem = match ty {
                Type::Buffer => "buffer",
                Type::Ints => "ints",
                Type::TcpListener => "tcp_listener",
                Type::TcpSocket => "tcp_socket",
                Type::TlsSocket => "tls_socket",
                _ => unreachable!(),
            };
            format!("@bhasa_rt_{stem}_{}", if clone { "clone" } else { "free" })
        }
        _ => return None,
    })
}
fn payload_key(ty: Type) -> String {
    match ty {
        Type::Record(id) => format!("record.{id}"),
        Type::Enum(id) => format!("enum.{id}"),
        _ => ty.to_string(),
    }
}
fn function(out: &mut String, name: &str, result: &str, params: &str, body: &str) {
    out.push_str(&format!(
        "define internal {result} {name}({params}) {{\n{body}}}\n\n"
    ));
}
fn variant(program: &Program, id: usize, name: &str, fields: &[Type]) -> Result<usize, String> {
    let declaration = program.enum_decl(id).ok_or("missing channel result enum")?;
    let matches: Vec<_> = declaration
        .variants
        .iter()
        .enumerate()
        .filter(|(_, v)| v.name == name)
        .collect();
    let [(index, value)] = matches.as_slice() else {
        return Err(format!("missing/duplicate channel variant {name}"));
    };
    if value.fields.iter().map(|f| f.ty).collect::<Vec<_>>() != fields {
        return Err(format!("wrong channel variant payload {name}"));
    }
    Ok(*index)
}
fn result_shape(program: &Program, id: usize) -> Result<(usize, usize, Type, Type), String> {
    let shape = program
        .enum_decl(id)
        .and_then(|d| d.result_shape())
        .ok_or("channel result must have checked ok/error shape")?;
    Ok((
        shape.ok_index,
        shape.error_index,
        shape.success,
        shape.error,
    ))
}
fn enum_value(
    out: &mut String,
    prefix: &str,
    id: usize,
    index: usize,
    field: Option<(&str, &str)>,
) {
    let payload = if let Some((ty, value)) = field {
        out.push_str(&format!("  %{prefix}.payload = call ptr @bhasa.enum.{id}.variant.{index}.new()\n  %{prefix}.slot = getelementptr %bhasa.enum.{id}.variant.{index}, ptr %{prefix}.payload, i32 0, i32 0\n  store {ty} {value}, ptr %{prefix}.slot\n"));
        format!("%{prefix}.payload")
    } else {
        "null".into()
    };
    out.push_str(&format!(
        "  %{prefix} = call ptr @bhasa.enum.{id}.new(i64 {index}, ptr {payload})\n"
    ));
}

/// Use the existing target layer’s resolved-target and 128-byte ABI policy.
fn native_target(target: &str) -> Result<(), String> {
    crate::target::require_native_posix64(target)
}

/// Renderer validates schemas/results and transfer eligibility before issuing a
/// descriptor. Exclusions come from normalized foreign/file ownership metadata.
/// Emit descriptor+payload definitions once per descriptor_symbol in the module.
/// For now one Channel specialization exists per concrete payload in a program.
pub fn emit_helpers(
    program: &Program,
    registry: &Registry,
    context: &ExpansionContext,
    pair_id: usize,
    target: &str,
    excluded: &[Type],
) -> Result<Helpers, String> {
    native_target(target)?;
    let token = registry
        .channel(program, context, pair_id)
        .map_err(str::to_owned)?;
    let channel_id = token.pair();
    let sender = token.sender();
    let receiver = token.receiver();
    let payload = token.payload();
    let send_result = token.send_result();
    let receive_result = token.receive_result();
    check_channel_transfer_excluding(program, payload, excluded)
        .map_err(|e| format!("unchecked channel payload: {e:?}"))?;
    let (send_ok, send_error, receipt, failure) = result_shape(program, send_result)?;
    let (recv_ok, recv_error, recv_value, reason) = result_shape(program, receive_result)?;
    if recv_value != payload {
        return Err("receive result payload disagrees with endpoint".into());
    }
    let (Type::Enum(receipt), Type::Enum(failure), Type::Enum(reason)) = (receipt, failure, reason)
    else {
        return Err("channel result status must be typed enums".into());
    };
    if program
        .enum_decl(receipt)
        .ok_or("missing ChannelSent declaration")?
        .variants
        .len()
        != 1
        || program
            .enum_decl(failure)
            .ok_or("missing SendFailure declaration")?
            .variants
            .len()
            != 2
        || program
            .enum_decl(reason)
            .ok_or("missing ReceiveError declaration")?
            .variants
            .len()
            != 2
    {
        return Err("channel status enums must have exact exhaustive variants".into());
    }
    let sent = variant(program, receipt, "sent", &[])?;
    let closed = variant(program, failure, "closed", &[payload])?;
    let full = variant(program, failure, "full", &[payload])?;
    let recv_closed = variant(program, reason, "closed", &[])?;
    let empty = variant(program, reason, "empty", &[])?;
    let key = payload_key(payload);
    let descriptor_symbol = format!("@bhasa.channel.type.{key}");
    let pack = format!("@bhasa.channel.payload.{key}.pack_copy");
    let drop = format!("@bhasa.channel.payload.{key}.drop");
    let descriptor =
        format!("{descriptor_symbol} = internal constant {{ ptr, i64 }} {{ ptr {drop}, i64 1 }}\n");
    let value_type = llvm(payload);
    let mut definitions = String::new();
    let mut body = "entry:\n".to_owned();
    let copy = if let Some(helper) = owned_helper(payload, true) {
        body.push_str(&format!("  %copy = call ptr {helper}(ptr %value)\n"));
        "%copy"
    } else {
        "%value"
    };
    body.push_str(&format!("  %frame = call ptr @bhasa_rt_task_alloc(i64 ptrtoint (ptr getelementptr ({value_type}, ptr null, i32 1) to i64))\n  store {value_type} {copy}, ptr %frame\n  ret ptr %frame\n"));
    function(
        &mut definitions,
        &pack,
        "ptr",
        &format!("{value_type} %value"),
        &body,
    );
    body = "entry:\n".into();
    if let Some(helper) = owned_helper(payload, false) {
        body.push_str(&format!(
            "  %value = load ptr, ptr %frame\n  call void {helper}(ptr %value)\n"
        ));
    }
    body.push_str("  call void @bhasa_rt_task_free(ptr %frame)\n  ret void\n");
    function(&mut definitions, &drop, "void", "ptr %frame", &body);
    for id in [sender, receiver] {
        function(
            &mut definitions,
            &format!("@bhasa.record.{id}.clone"),
            "ptr",
            "ptr %endpoint",
            "entry:\n  %copy = call ptr @bhasa_rt_channel_clone(ptr %endpoint)\n  ret ptr %copy\n",
        );
        function(
            &mut definitions,
            &format!("@bhasa.record.{id}.drop"),
            "void",
            "ptr %endpoint",
            "entry:\n  call void @bhasa_rt_channel_drop(ptr %endpoint)\n  ret void\n",
        );
        function(
            &mut definitions,
            &format!("@bhasa.channel.endpoint.{id}.close"),
            "void",
            "ptr %endpoint",
            "entry:\n  call void @bhasa_rt_channel_close(ptr %endpoint)\n  ret void\n",
        );
    }
    body = format!("entry:\n  %out = alloca ptr, align 8\n  %sender = call ptr @bhasa_rt_channel_new(ptr {descriptor_symbol}, i64 %capacity, ptr %out)\n  %receiver = load ptr, ptr %out\n  %pair = call ptr @bhasa.record.{channel_id}.new()\n  %sender.slot = getelementptr %bhasa.record.{channel_id}, ptr %pair, i32 0, i32 0\n  store ptr %sender, ptr %sender.slot\n  %receiver.slot = getelementptr %bhasa.record.{channel_id}, ptr %pair, i32 0, i32 1\n  store ptr %receiver, ptr %receiver.slot\n  ret ptr %pair\n");
    function(
        &mut definitions,
        &format!("@bhasa.channel.{channel_id}.new"),
        "ptr",
        "i64 %capacity",
        &body,
    );
    for (index, name) in [(0, "sender"), (1, "receiver")] {
        body = format!("entry:\n  %slot = getelementptr %bhasa.record.{channel_id}, ptr %pair, i32 0, i32 {index}\n  %endpoint = load ptr, ptr %slot\n  %copy = call ptr @bhasa_rt_channel_clone(ptr %endpoint)\n  ret ptr %copy\n");
        function(
            &mut definitions,
            &format!("@bhasa.channel.{channel_id}.{name}"),
            "ptr",
            "ptr %pair",
            &body,
        );
    }
    for (method, runtime) in [("send", "send"), ("try_send", "try_send")] {
        body = format!("entry:\n  %frame = call ptr {pack}({value_type} %value)\n  %status = call i64 @bhasa_rt_channel_{runtime}(ptr %endpoint, ptr {descriptor_symbol}, ptr %frame)\n  switch i64 %status, label %invalid [ i64 0, label %sent i64 1, label %closed i64 2, label %full ]\ninvalid:\n  call void @llvm.trap()\n  unreachable\nsent:\n");
        enum_value(&mut body, "receipt", receipt, sent, None);
        enum_value(
            &mut body,
            "success",
            send_result,
            send_ok,
            Some(("ptr", "%receipt")),
        );
        body.push_str("  ret ptr %success\n");
        for (label, tag) in [("closed", closed), ("full", full)] {
            body.push_str(&format!("{label}:\n  %{label}.value = load {value_type}, ptr %frame\n  call void @bhasa_rt_task_free(ptr %frame)\n"));
            enum_value(
                &mut body,
                &format!("{label}.failure"),
                failure,
                tag,
                Some((value_type, &format!("%{label}.value"))),
            );
            enum_value(
                &mut body,
                &format!("{label}.result"),
                send_result,
                send_error,
                Some(("ptr", &format!("%{label}.failure"))),
            );
            body.push_str(&format!("  ret ptr %{label}.result\n"));
        }
        function(
            &mut definitions,
            &format!("@bhasa.channel.sender.{sender}.{method}"),
            "ptr",
            &format!("ptr %endpoint, {value_type} %value"),
            &body,
        );
    }
    for (method, runtime) in [("recv", "recv"), ("try_recv", "try_recv")] {
        body = format!("entry:\n  %out = alloca ptr, align 8\n  %status = call i64 @bhasa_rt_channel_{runtime}(ptr %endpoint, ptr {descriptor_symbol}, ptr %out)\n  switch i64 %status, label %invalid [ i64 0, label %value i64 1, label %closed i64 2, label %empty ]\ninvalid:\n  call void @llvm.trap()\n  unreachable\nvalue:\n  %frame = load ptr, ptr %out\n  %received = load {value_type}, ptr %frame\n  call void @bhasa_rt_task_free(ptr %frame)\n");
        enum_value(
            &mut body,
            "success",
            receive_result,
            recv_ok,
            Some((value_type, "%received")),
        );
        body.push_str("  ret ptr %success\n");
        for (label, tag) in [("closed", recv_closed), ("empty", empty)] {
            body.push_str(&format!("{label}:\n"));
            enum_value(&mut body, &format!("{label}.reason"), reason, tag, None);
            enum_value(
                &mut body,
                &format!("{label}.result"),
                receive_result,
                recv_error,
                Some(("ptr", &format!("%{label}.reason"))),
            );
            body.push_str(&format!("  ret ptr %{label}.result\n"));
        }
        function(
            &mut definitions,
            &format!("@bhasa.channel.receiver.{receiver}.{method}"),
            "ptr",
            "ptr %endpoint",
            &body,
        );
    }
    let mut declarations = vec![
        "declare ptr @bhasa_rt_task_alloc(i64)",
        "declare void @bhasa_rt_task_free(ptr)",
        "declare ptr @bhasa_rt_channel_new(ptr, i64, ptr)",
        "declare ptr @bhasa_rt_channel_clone(ptr)",
        "declare void @bhasa_rt_channel_drop(ptr)",
        "declare void @bhasa_rt_channel_close(ptr)",
        "declare void @llvm.trap() cold noreturn nounwind",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for name in ["send", "try_send", "recv", "try_recv"] {
        declarations.push(format!(
            "declare i64 @bhasa_rt_channel_{name}(ptr, ptr, ptr)"
        ));
    }
    if let Some(helper) = owned_helper(payload, true) {
        if !matches!(payload, Type::Record(_) | Type::Enum(_)) {
            declarations.push(format!("declare ptr {helper}(ptr)"));
            declarations.push(format!(
                "declare void {}(ptr)",
                owned_helper(payload, false).unwrap()
            ));
        }
    }
    Ok(Helpers {
        #[cfg(test)]
        descriptor_symbol,
        descriptor,
        definitions,
        declarations,
        required_owned_helpers: vec![
            payload,
            Type::Record(channel_id),
            Type::Enum(receipt),
            Type::Enum(failure),
            Type::Enum(reason),
            Type::Enum(send_result),
            Type::Enum(receive_result),
        ],
    })
}

#[cfg(test)]
use crate::channel_model as lowering_model;
#[cfg(test)]
mod tests {
    use super::*;
    fn render(program: &Program, excluded: &[Type]) -> Result<Helpers, String> {
        let (registry, context) =
            crate::channel_registry::fixture(program, true).map_err(str::to_owned)?;
        emit_helpers(
            program,
            &registry,
            &context,
            2,
            "aarch64-apple-darwin",
            excluded,
        )
    }
    #[test]
    fn typed_outcomes_reversed_variants_and_failed_receive_do_not_read_output() {
        let p = lowering_model::model(Type::Buffer);
        let h = render(&p, &[]).unwrap();
        assert_eq!(h.descriptor_symbol, "@bhasa.channel.type.buffer");
        assert!(h.descriptor.contains("i64 1"));
        assert!(h
            .definitions
            .contains("call ptr @bhasa_rt_buffer_clone(ptr %value)"));
        assert!(h
            .definitions
            .contains("call void @bhasa_rt_buffer_free(ptr %value)"));
        assert!(h
            .definitions
            .contains("@bhasa.enum.6.new(i64 1, ptr %success.payload)"));
        let receiver = h
            .definitions
            .split("@bhasa.channel.receiver.1.try_recv")
            .nth(1)
            .unwrap();
        let closed = receiver
            .split("closed:\n")
            .nth(1)
            .unwrap()
            .split("empty:\n")
            .next()
            .unwrap();
        assert!(!closed.contains("load ptr, ptr %out"));
        assert!(h.required_owned_helpers.contains(&Type::Enum(6)));
        assert!(h
            .declarations
            .iter()
            .any(|s| s.contains("@bhasa_rt_channel_try_recv")));
    }
    #[test]
    fn descriptor_is_never_issued_for_excluded_or_corrupt_outcomes() {
        let mut p = lowering_model::model(Type::Buffer);
        assert!(render(&p, &[Type::Buffer]).is_err());
        p.enums[1].variants[0].fields[0].ty = Type::Int;
        assert!(render(&p, &[]).is_err());
        let mut missing = lowering_model::model(Type::Buffer);
        missing
            .enums
            .retain(|declaration| declaration.name != "ChannelSent");
        assert!(render(&missing, &[]).is_err());
        let p = lowering_model::model(Type::Task);
        assert!(render(&p, &[]).is_err());
    }
    #[test]
    fn unsupported_targets_are_rejected_before_descriptor_emission() {
        let p = lowering_model::model(Type::Buffer);
        let (registry, context) = crate::channel_registry::fixture(&p, true).unwrap();
        for target in [
            "wasm32-unknown-unknown",
            "x86_64-pc-windows-msvc",
            "i686-unknown-linux-gnu",
        ] {
            assert!(emit_helpers(&p, &registry, &context, 2, target, &[]).is_err());
        }
    }
    #[test]
    fn scalar_pack_does_not_issue_aggregate_callbacks() {
        for payload in [Type::Int, Type::Bool, Type::Float, Type::Str] {
            let p = lowering_model::model(payload);
            let h = render(&p, &[]).unwrap();
            assert!(!h.definitions.contains("@bhasa_rt_buffer_clone"));
            assert!(h
                .definitions
                .contains(&format!("store {} %value, ptr %frame", llvm(payload))));
        }
    }
}
