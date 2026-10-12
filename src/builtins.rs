//! Checked interfaces for the native runtime. No implicit imports or raw pointers.
use crate::ast::Type;
use Type::*;

pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "int"
            | "float"
            | "spawn"
            | "join"
            | "async_spawn"
            | "await"
            | "buffer"
            | "ints"
            | "len"
            | "get"
            | "set"
            | "push"
            | "append"
            | "slice"
            | "same"
            | "to_buffer"
            | "arg_count"
            | "arg"
            | "read_file"
            | "write_file"
            | "read_stdin"
            | "write_stdout"
            | "write_stderr"
            | "args_push"
            | "process_run"
            | "gpu_available"
            | "gpu_dispatch"
            | "utf8_valid"
            | "utf8_count"
            | "utf8_get"
            | "utf8_slice"
            | "utf8_append"
            | "monotonic_millis"
            | "tcp_listen"
            | "tcp_listen_on"
            | "tcp_connect"
            | "tcp_accept"
            | "tcp_listener_status"
            | "tcp_socket_status"
            | "tcp_local_port"
            | "tcp_read"
            | "tcp_write"
            | "tcp_connect_timeout"
            | "tcp_accept_timeout"
            | "tcp_read_timeout"
            | "tcp_write_timeout"
            | "tls_connect"
            | "tls_accept"
            | "tls_status"
            | "tls_read"
            | "tls_write"
            | "tls_shutdown"
            | "stream_read"
            | "stream_write"
    )
}

pub fn must_use(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "write_file"
            | "read_stdin"
            | "write_stdout"
            | "write_stderr"
            | "process_run"
            | "gpu_dispatch"
            | "tcp_read"
            | "tcp_write"
            | "tcp_read_timeout"
            | "tcp_write_timeout"
            | "utf8_append"
            | "stream_read"
            | "stream_write"
            | "tls_read"
            | "tls_write"
            | "tls_shutdown"
    )
}

pub fn signature(name: &str, args: &[Type]) -> Option<Result<Type, String>> {
    let result = match (name, args) {
        ("float", [Int]) => Float,
        ("int", [Float]) => Int,
        ("buffer", [] | [Str]) => Buffer,
        ("ints", []) => Ints,
        ("len", [Buffer | Ints]) => Int,
        ("get", [Buffer | Ints, Int]) => Int,
        ("set", [Buffer | Ints, Int, Int]) | ("push", [Buffer | Ints, Int]) => Void,
        ("append" | "args_push", [Buffer, Buffer | Str]) => Void,
        ("slice", [Buffer, Int, Int]) => Buffer,
        ("same", [Buffer, Buffer | Str]) => Bool,
        ("to_buffer" | "arg", [Int]) => Buffer,
        ("arg_count", []) => Int,
        ("read_file" | "write_file", [Buffer | Str, Buffer]) => Int,
        ("read_stdin" | "write_stdout" | "write_stderr" | "process_run", [Buffer]) => Int,
        ("gpu_available", []) => Bool,
        ("gpu_dispatch", [Buffer | Str, Buffer | Str, Buffer]) => Int,
        ("utf8_valid", [Buffer]) => Bool,
        ("utf8_count", [Buffer]) | ("utf8_get", [Buffer, Int]) => Int,
        ("utf8_slice", [Buffer, Int, Int]) => Buffer,
        ("utf8_append", [Buffer, Int]) | ("monotonic_millis", []) => Int,
        ("tcp_listen", [Int]) => TcpListener,
        ("tcp_listen_on", [Buffer | Str, Int]) => TcpListener,
        ("tcp_connect", [Buffer | Str, Int]) | ("tcp_accept", [TcpListener]) => TcpSocket,
        ("tcp_connect_timeout", [Buffer | Str, Int, Int])
        | ("tcp_accept_timeout", [TcpListener, Int]) => TcpSocket,
        ("tcp_listener_status" | "tcp_local_port", [TcpListener]) => Int,
        ("tcp_socket_status", [TcpSocket]) => Int,
        ("tcp_read", [TcpSocket, Buffer, Int]) | ("tcp_write", [TcpSocket, Buffer]) => Int,
        ("tcp_read_timeout", [TcpSocket, Buffer, Int, Int])
        | ("tcp_write_timeout", [TcpSocket, Buffer, Int]) => Int,
        ("stream_read", [TcpSocket | TlsSocket, Buffer, Int, Int])
        | ("stream_write", [TcpSocket | TlsSocket, Buffer, Int]) => Int,
        ("print", [Int | Float | Bool | Str | Buffer]) => Void,
        ("join" | "await", [Task]) => Int,
        ("tls_connect", [Buffer | Str, Int, Buffer | Str, Buffer | Str, Int])
        | ("tls_accept", [TcpListener, Buffer | Str, Buffer | Str, Int]) => TlsSocket,
        ("tls_status", [TlsSocket])
        | ("tls_read", [TlsSocket, Buffer, Int, Int])
        | ("tls_write", [TlsSocket, Buffer, Int])
        | ("tls_shutdown", [TlsSocket, Int]) => Int,
        _ if is_builtin(name) => {
            return Some(Err(format!(
                "invalid arguments to builtin '{name}': ({})",
                args.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
        _ => return None,
    };
    Some(Ok(result))
}

/// Called only after signature validation. Types select overloaded C symbols.
pub fn runtime_name(name: &str, args: &[Type]) -> Option<String> {
    let stem = match name {
        "stream_read" => if args.first() == Some(&TlsSocket) {
            "tls_read"
        } else {
            "tcp_read_timeout"
        }
        .to_owned(),
        "stream_write" => if args.first() == Some(&TlsSocket) {
            "tls_write"
        } else {
            "tcp_write_timeout"
        }
        .to_owned(),
        "buffer" => if args.is_empty() {
            "buffer_new"
        } else {
            "buffer_from_str"
        }
        .to_owned(),
        "ints" => "ints_new".to_owned(),
        "len" | "get" | "set" | "push" => format!(
            "{}_{}",
            if args.first() == Some(&Ints) {
                "ints"
            } else {
                "buffer"
            },
            name
        ),
        "append" | "same" => format!(
            "buffer_{name}{}",
            if args.get(1) == Some(&Str) {
                "_str"
            } else {
                ""
            }
        ),
        "slice" => "buffer_slice".to_owned(),
        "read_file" | "write_file" => format!(
            "{name}{}",
            if args.first() == Some(&Str) {
                "_str"
            } else {
                ""
            }
        ),
        "args_push" => format!(
            "args_push{}",
            if args.get(1) == Some(&Str) {
                "_str"
            } else {
                ""
            }
        ),
        "to_buffer" | "arg_count" | "arg" | "read_stdin" | "write_stdout" | "write_stderr"
        | "process_run" | "gpu_available" | "gpu_dispatch" => name.to_owned(),
        "tcp_connect" | "tcp_connect_timeout" | "tcp_listen_on" => format!(
            "{name}{}",
            if args.first() == Some(&Str) {
                "_str"
            } else {
                ""
            }
        ),
        "tcp_listen"
        | "tcp_accept"
        | "tcp_listener_status"
        | "tcp_socket_status"
        | "tcp_local_port"
        | "tcp_read"
        | "tcp_write"
        | "tcp_accept_timeout"
        | "tcp_read_timeout"
        | "tcp_write_timeout"
        | "utf8_valid"
        | "utf8_count"
        | "utf8_get"
        | "utf8_slice"
        | "utf8_append"
        | "monotonic_millis"
        | "tls_connect"
        | "tls_accept"
        | "tls_status"
        | "tls_read"
        | "tls_write"
        | "tls_shutdown" => name.to_owned(),
        _ => return None,
    };
    Some(format!("bhasa_rt_{stem}"))
}
