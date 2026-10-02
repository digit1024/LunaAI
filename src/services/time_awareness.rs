//! Time-awareness context injection.
//!
//! Builds a short, ephemeral system block that tells the agent how much wall-clock
//! time has passed since the last message. The block is injected into the in-memory
//! message list immediately before the current user turn and is **never persisted**
//! to conversation history.
//!
//! Mirrors the memory-block pattern in `memory_rag`: a dynamic `Role::System` message
//! that exists only for the duration of a single LLM request.

use crate::config::TimeAwarenessConfig;
use crate::llm::{Message as LlmMessage, Role};
use chrono::{DateTime, Local, Utc};

/// Build the time-context block, or `None` when there is nothing worth injecting.
///
/// * `last_at` — creation time of the most recent stored message preceding the current
///   turn. `None` means this is the first message of the conversation.
/// * `now` — current wall-clock time (injectable for deterministic tests).
pub fn build_time_context(
    last_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    cfg: &TimeAwarenessConfig,
) -> Option<String> {
    if !cfg.enabled {
        return None;
    }

    let display_local = cfg.timezone.eq_ignore_ascii_case("local");

    let mut lines: Vec<String> = vec!["[Time context]".to_string()];

    if let Some(last) = last_at {
        let elapsed_secs = (now - last).num_seconds().max(0);
        let min_secs = (cfg.min_gap_hours.max(0.0) * 3600.0) as i64;
        if elapsed_secs < min_secs {
            return None;
        }
        lines.push(format!(
            "Elapsed since last message: {}",
            humanize_duration(elapsed_secs)
        ));
        if cfg.include_last_message_time {
            lines.push(format!("Last message: {}", format_dt(last, display_local)));
        }
    }

    if cfg.include_current_time {
        lines.push(format!("Current time: {}", format_dt(now, display_local)));
    }

    if lines.len() == 1 {
        return None;
    }

    Some(lines.join("\n"))
}

/// Insert a pre-built time-context block into `messages`.
///
/// When `at_end` is false the block is placed immediately before the last user turn
/// (the current turn); otherwise it is appended at the end (resume / no fresh user turn).
pub fn insert_time_context(messages: &mut Vec<LlmMessage>, body: String, at_end: bool) {
    let pos = if at_end {
        messages.len()
    } else {
        messages
            .iter()
            .rposition(|m| m.role == Role::User)
            .unwrap_or(messages.len())
    };
    messages.insert(pos, LlmMessage::new(Role::System, body));
}

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Humanize a duration as `Nd Nh Nm` (days/hours/minutes), skipping zero components.
fn humanize_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{} day{}", days, plural(days)));
    }
    if hours > 0 {
        parts.push(format!("{} hour{}", hours, plural(hours)));
    }
    if minutes > 0 {
        parts.push(format!("{} minute{}", minutes, plural(minutes)));
    }

    if parts.is_empty() {
        "less than a minute".to_string()
    } else {
        parts.join(" ")
    }
}

fn format_dt(dt: DateTime<Utc>, local: bool) -> String {
    if local {
        dt.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %Z (%A)")
            .to_string()
    } else {
        dt.format("%Y-%m-%d %H:%M UTC (%A)").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cfg() -> TimeAwarenessConfig {
        TimeAwarenessConfig {
            enabled: true,
            min_gap_hours: 6.0,
            include_current_time: true,
            include_last_message_time: true,
            timezone: "utc".to_string(),
        }
    }

    fn t(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn disabled_returns_none() {
        let mut c = cfg();
        c.enabled = false;
        assert!(build_time_context(Some(t(0)), t(100_000), &c).is_none());
    }

    #[test]
    fn below_threshold_returns_none() {
        let now = t(10_000);
        let last = t(10_000 - 5 * 3600); // 5h < 6h
        assert!(build_time_context(Some(last), now, &cfg()).is_none());
    }

    #[test]
    fn formats_hours_and_minutes() {
        let now = t(1_000_000);
        let last = t(1_000_000 - (6 * 3600 + 2 * 60));
        let body = build_time_context(Some(last), now, &cfg()).unwrap();
        assert!(body.contains("Elapsed since last message: 6 hours 2 minutes"));
        assert!(body.contains("Current time:"));
    }

    #[test]
    fn first_message_emits_current_time_only() {
        let body = build_time_context(None, t(1_000_000), &cfg()).unwrap();
        assert!(body.contains("[Time context]"));
        assert!(body.contains("Current time:"));
        assert!(!body.contains("Elapsed since"));
        assert!(!body.contains("Last message:"));
    }

    #[test]
    fn first_message_without_current_time_is_none() {
        let mut c = cfg();
        c.include_current_time = false;
        assert!(build_time_context(None, t(1_000_000), &c).is_none());
    }

    #[test]
    fn future_timestamp_clamps_to_zero() {
        let now = t(1_000);
        let last = t(10_000); // in the future
        // min_gap_hours = 0 -> always emits; negative elapsed clamps to "less than a minute".
        let mut c = cfg();
        c.min_gap_hours = 0.0;
        let body = build_time_context(Some(last), now, &c).unwrap();
        assert!(body.contains("Elapsed since last message: less than a minute"));
    }

    #[test]
    fn zero_gap_threshold_always_emits() {
        let now = t(10_000);
        let last = t(10_000);
        let mut c = cfg();
        c.min_gap_hours = 0.0;
        let body = build_time_context(Some(last), now, &c).unwrap();
        assert!(body.contains("Elapsed since last message: less than a minute"));
    }

    #[test]
    fn day_granularity() {
        assert_eq!(humanize_duration(90_000), "1 day 1 hour");
        assert_eq!(humanize_duration(3 * 86_400 + 2), "3 days");
        assert_eq!(humanize_duration(0), "less than a minute");
    }

    #[test]
    fn inserts_before_last_user_turn() {
        let mut msgs = vec![
            LlmMessage::new(Role::System, "sys".into()),
            LlmMessage::new(Role::Assistant, "hi".into()),
            LlmMessage::new(Role::User, "question".into()),
        ];
        insert_time_context(&mut msgs, "TIME".into(), false);
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[2].role, Role::System);
        assert_eq!(msgs[2].content, "TIME");
        assert_eq!(msgs[3].role, Role::User);
    }

    #[test]
    fn inserts_at_end_when_requested() {
        let mut msgs = vec![
            LlmMessage::new(Role::System, "sys".into()),
            LlmMessage::new(Role::User, "old".into()),
            LlmMessage::new(Role::Assistant, "tool tail".into()),
        ];
        insert_time_context(&mut msgs, "TIME".into(), true);
        assert_eq!(msgs.last().unwrap().content, "TIME");
    }
}
