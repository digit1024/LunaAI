//! Utility helper functions
//!
//! Helper functions for JSON formatting, value coercion, etc.

use serde_json::Value;

/// Format a JSON string for display
pub fn format_json_string(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}

/// Coerce a string to a JSON Value
pub fn coerce_value(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.to_string()))
}

