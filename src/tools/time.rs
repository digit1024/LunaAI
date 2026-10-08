//! Embedded `get_current_time` / `convert_time` tools.
//!
//! Port of `mcp-server-time`: returns the same JSON shapes the MCP server did.

use super::time_format::{parse_tz, resolve_configured_tz};
use super::{TIME_CONVERT_TOOL_NAME, TIME_GET_TOOL_NAME};
use crate::config::TimeToolConfig;
use crate::llm::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;
use chrono::{NaiveTime, Offset, SecondsFormat, TimeZone, Utc};
use chrono_tz::{OffsetComponents, Tz};

pub struct TimeTool {
    /// Resolved IANA zone name used for the tool descriptions/fallback.
    local_tz_name: String,
}

impl TimeTool {
    pub fn new(config: TimeToolConfig) -> Self {
        let local_tz_name = resolve_configured_tz(&config.local_timezone);
        Self { local_tz_name }
    }

    fn get_current_time_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: TIME_GET_TOOL_NAME.to_string(),
            description: "Get current time in a specific timezone".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "timezone": {
                        "type": "string",
                        "description": format!(
                            "IANA timezone name (e.g., 'America/New_York', 'Europe/London'). Use '{}' as local timezone if no timezone provided by the user.",
                            self.local_tz_name
                        )
                    }
                },
                "required": ["timezone"]
            }),
        }
    }

    fn convert_time_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: TIME_CONVERT_TOOL_NAME.to_string(),
            description: "Convert time between timezones".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "source_timezone": {
                        "type": "string",
                        "description": format!(
                            "Source IANA timezone name (e.g., 'America/New_York', 'Europe/London'). Use '{}' as local timezone if no source timezone provided by the user.",
                            self.local_tz_name
                        )
                    },
                    "time": {
                        "type": "string",
                        "description": "Time to convert in 24-hour format (HH:MM)"
                    },
                    "target_timezone": {
                        "type": "string",
                        "description": format!(
                            "Target IANA timezone name (e.g., 'Asia/Tokyo', 'America/San_Francisco'). Use '{}' as local timezone if no target timezone provided by the user.",
                            self.local_tz_name
                        )
                    }
                },
                "required": ["source_timezone", "time", "target_timezone"]
            }),
        }
    }

    fn current_time(&self, timezone: &str) -> Result<serde_json::Value, String> {
        let tz = parse_tz(timezone)?;
        let now = Utc::now().with_timezone(&tz);
        Ok(time_result(timezone, &now))
    }

    fn convert_time(
        &self,
        source_tz_name: &str,
        time_str: &str,
        target_tz_name: &str,
    ) -> Result<serde_json::Value, String> {
        let source_tz = parse_tz(source_tz_name)?;
        let target_tz = parse_tz(target_tz_name)?;

        let parsed = NaiveTime::parse_from_str(time_str, "%H:%M")
            .map_err(|_| "Invalid time format. Expected HH:MM [24-hour format]".to_string())?;

        let today = Utc::now().with_timezone(&source_tz).date_naive();
        let naive = today.and_time(parsed);
        let source_time = match source_tz.from_local_datetime(&naive) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None => {
                return Err(format!(
                    "Nonexistent local time {} in {}",
                    time_str, source_tz_name
                ))
            }
        };

        let target_time = source_time.with_timezone(&target_tz);

        let source_offset = source_time.offset().fix().local_minus_utc() as f64;
        let target_offset = target_time.offset().fix().local_minus_utc() as f64;
        let hours_difference = (target_offset - source_offset) / 3600.0;

        let time_diff_str = if hours_difference.fract() == 0.0 {
            format!("{:+.1}h", hours_difference)
        } else {
            let mut s = format!("{:+.2}", hours_difference);
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
            format!("{}h", s)
        };

        Ok(serde_json::json!({
            "source": time_result(source_tz_name, &source_time),
            "target": time_result(target_tz_name, &target_time),
            "time_difference": time_diff_str
        }))
    }

    fn run(&self, call: &ToolCall) -> Result<serde_json::Value, String> {
        let params = &call.parameters;
        match call.name.as_str() {
            TIME_GET_TOOL_NAME => {
                let timezone = params
                    .get("timezone")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required argument: timezone")?;
                self.current_time(timezone)
            }
            TIME_CONVERT_TOOL_NAME => {
                let source = params
                    .get("source_timezone")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required arguments")?;
                let time = params
                    .get("time")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required arguments")?;
                let target = params
                    .get("target_timezone")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required arguments")?;
                self.convert_time(source, time, target)
            }
            other => Err(format!("Unknown time tool: {}", other)),
        }
    }
}

fn time_result(name: &str, dt: &chrono::DateTime<Tz>) -> serde_json::Value {
    let offset = dt.offset();
    let is_dst = offset.base_utc_offset() != offset.dst_offset();
    serde_json::json!({
        "timezone": name,
        "datetime": dt.to_rfc3339_opts(SecondsFormat::Secs, true),
        "day_of_week": dt.format("%A").to_string(),
        "is_dst": is_dst
    })
}

#[async_trait]
impl super::EmbeddedTool for TimeTool {
    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![
            self.get_current_time_definition(),
            self.convert_time_definition(),
        ]
    }

    fn names(&self) -> Vec<&'static str> {
        vec![TIME_GET_TOOL_NAME, TIME_CONVERT_TOOL_NAME]
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.run(call) {
            Ok(value) => ToolResult {
                content: serde_json::to_string_pretty(&value)
                    .unwrap_or_else(|_| value.to_string()),
                is_error: false,
            },
            Err(message) => ToolResult {
                content: format!("Error processing time query: {}", message),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EmbeddedTool;

    fn tool() -> TimeTool {
        TimeTool::new(TimeToolConfig {
            enabled: true,
            local_timezone: "local".to_string(),
        })
    }

    fn call(name: &str, params: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "t".to_string(),
            name: name.to_string(),
            parameters: params,
        }
    }

    #[tokio::test]
    async fn gets_current_time() {
        let t = tool();
        let r = t
            .execute(&call(
                TIME_GET_TOOL_NAME,
                serde_json::json!({"timezone": "Europe/Warsaw"}),
            ))
            .await;
        assert!(!r.is_error, "{}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["timezone"], "Europe/Warsaw");
        assert!(v["datetime"].as_str().unwrap().contains('T'));
    }

    #[tokio::test]
    async fn rejects_invalid_timezone() {
        let t = tool();
        let r = t
            .execute(&call(
                TIME_GET_TOOL_NAME,
                serde_json::json!({"timezone": "Not/AZone"}),
            ))
            .await;
        assert!(r.is_error);
    }

    #[tokio::test]
    async fn converts_time() {
        let t = tool();
        let r = t
            .execute(&call(
                TIME_CONVERT_TOOL_NAME,
                serde_json::json!({
                    "source_timezone": "Europe/Warsaw",
                    "time": "12:00",
                    "target_timezone": "America/New_York"
                }),
            ))
            .await;
        assert!(!r.is_error, "{}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["source"]["timezone"], "Europe/Warsaw");
        assert!(v["time_difference"].is_string());
    }

    #[tokio::test]
    async fn rejects_bad_time_format() {
        let t = tool();
        let r = t
            .execute(&call(
                TIME_CONVERT_TOOL_NAME,
                serde_json::json!({
                    "source_timezone": "UTC",
                    "time": "25:99",
                    "target_timezone": "UTC"
                }),
            ))
            .await;
        assert!(r.is_error);
    }
}
