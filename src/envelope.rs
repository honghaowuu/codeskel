use serde_json::{json, Map, Value};
use std::process::exit;

/// Build a success envelope `{"ok": true, ...fields}` as a compact JSON string.
/// `value` MUST be a JSON object so its fields can be flattened. An `"ok"`
/// field already present in `value` is overwritten.
pub fn format_ok(value: Value) -> String {
    serde_json::to_string(&Value::Object(merge_ok(value, None)))
        .expect("serializing JSON object never fails")
}

/// Build a success envelope with a `warnings` array as a compact JSON string.
/// Used by commands that legitimately partial-succeed (`rescan`, README contract
/// requires exit code 2 in that case).
pub fn format_ok_with_warnings(value: Value, warnings: Vec<String>) -> String {
    serde_json::to_string(&Value::Object(merge_ok(value, Some(warnings))))
        .expect("serializing JSON object never fails")
}

/// Print a failure envelope `{"ok": false, "error": {"message": ...}}` to
/// stdout and exit 1.
pub fn print_err(message: &str, hint: Option<&str>) -> ! {
    emit_err(None, message, hint)
}

/// Print a failure envelope with a stable `code` and exit 1.
pub fn print_err_coded(code: &str, message: &str, hint: Option<&str>) -> ! {
    emit_err(Some(code), message, hint)
}

fn merge_ok(value: Value, warnings: Option<Vec<String>>) -> Map<String, Value> {
    let mut map = match value {
        Value::Object(m) => m,
        other => panic!("envelope helpers require a JSON object, got {other:?}"),
    };
    let mut out = Map::new();
    out.insert("ok".into(), Value::Bool(true));
    if let Some(ws) = warnings {
        out.insert(
            "warnings".into(),
            Value::Array(ws.into_iter().map(Value::String).collect()),
        );
    }
    map.remove("ok");
    map.remove("warnings");
    for (k, v) in map {
        out.insert(k, v);
    }
    out
}

fn emit_err(code: Option<&str>, message: &str, hint: Option<&str>) -> ! {
    let mut err_obj = Map::new();
    err_obj.insert("message".into(), Value::String(message.to_string()));
    if let Some(c) = code {
        err_obj.insert("code".into(), Value::String(c.to_string()));
    }
    if let Some(h) = hint {
        err_obj.insert("hint".into(), Value::String(h.to_string()));
    }
    let env = json!({ "ok": false, "error": Value::Object(err_obj) });
    println!(
        "{}",
        serde_json::to_string(&env).expect("serializing JSON object never fails")
    );
    exit(1);
}
