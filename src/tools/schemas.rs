use serde_json::{json, Value};

pub fn string_schema(desc: &str) -> Value {
    json!({ "type": "string", "description": desc })
}

pub fn optional_string_schema(desc: &str) -> Value {
    json!({ "type": "string", "description": desc })
}

pub fn integer_schema(desc: &str) -> Value {
    json!({ "type": "integer", "description": desc })
}

pub fn optional_integer_schema(desc: &str) -> Value {
    json!({ "type": "integer", "description": desc })
}

pub fn boolean_schema(desc: &str) -> Value {
    json!({ "type": "boolean", "description": desc })
}

pub fn enum_schema(desc: &str, variants: &[&str]) -> Value {
    json!({ "type": "string", "enum": variants, "description": desc })
}
