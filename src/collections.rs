//! Concrete collection schemas emitted by the generic expansion pass.
use crate::ast::{RecordDecl, Type};

#[derive(Clone, Copy, Debug)]
pub enum Schema {
    List(Type),
    Map(Type, Type),
}

pub fn is_collection(name: &str) -> bool {
    name.starts_with("__bhg_list_") || name.starts_with("__bhg_map_")
}

pub fn schema(record: &RecordDecl) -> Result<Option<Schema>, String> {
    let (prefix, count) = if record.name.starts_with("__bhg_list_") {
        ("__bhg_list_", 1)
    } else if record.name.starts_with("__bhg_map_") {
        ("__bhg_map_", 2)
    } else {
        return Ok(None);
    };
    let suffix = &record.name[prefix.len()..];
    if suffix.is_empty()
        || !suffix.bytes().all(|b| b.is_ascii_digit())
        || record.fields.len() != count
        || !record.methods.is_empty()
        || record
            .fields
            .iter()
            .any(|field| matches!(field.ty, Type::Void | Type::Task))
    {
        return Err("invalid generated collection schema".into());
    }
    if count == 1 && record.fields[0].name == "__element" {
        Ok(Some(Schema::List(record.fields[0].ty)))
    } else if count == 2 && record.fields[0].name == "__key" && record.fields[1].name == "__value" {
        if !matches!(record.fields[0].ty, Type::Int | Type::Str) {
            return Err("Map keys must be int or str".into());
        }
        Ok(Some(Schema::Map(record.fields[0].ty, record.fields[1].ty)))
    } else {
        Err("invalid generated collection fields".into())
    }
}

impl Schema {
    pub fn kind(self) -> &'static str {
        match self {
            Self::List(_) => "list",
            Self::Map(_, _) => "map",
        }
    }
    pub fn elements(self) -> Vec<Type> {
        match self {
            Self::List(value) => vec![value],
            Self::Map(key, value) => vec![key, value],
        }
    }
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Self::List(_) => &["push", "get", "set", "count", "clear"],
            Self::Map(_, _) => &["put", "get", "contains", "count", "remove", "clear"],
        }
    }
    pub fn method(self, name: &str) -> Option<(Vec<Type>, Type)> {
        Some(match (self, name) {
            (_, "count") => (vec![], Type::Int),
            (_, "clear") => (vec![], Type::Void),
            (Self::List(value), "push") => (vec![value], Type::Void),
            (Self::List(value), "get") => (vec![Type::Int], value),
            (Self::List(value), "set") => (vec![Type::Int, value], Type::Void),
            (Self::Map(key, value), "put") => (vec![key, value], Type::Void),
            (Self::Map(key, value), "get") => (vec![key], value),
            (Self::Map(key, _), "contains" | "remove") => (vec![key], Type::Bool),
            _ => return None,
        })
    }
}
