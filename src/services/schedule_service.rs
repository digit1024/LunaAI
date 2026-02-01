//! Schedule service: parse run_at, validate cron schedule, insert scheduled jobs.

use crate::storage::{ScheduledJob, Storage};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Normalize cron string: 5-field (min hour dom month dow) -> 6-field (sec min hour dom month dow).
fn normalize_cron(s: &str) -> String {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", s.trim())
    } else {
        s.trim().to_string()
    }
}

/// Validate schedule: null/empty/"once" = one-shot (ok). Otherwise parse as cron.
pub fn validate_schedule(schedule: Option<&str>) -> Result<()> {
    let s = match schedule {
        None => return Ok(()),
        Some(t) if t.trim().is_empty() => return Ok(()),
        Some(t) if t.trim().eq_ignore_ascii_case("once") => return Ok(()),
        Some(t) => t.trim(),
    };
    let cron_str = normalize_cron(s);
    Schedule::from_str(&cron_str).map_err(|e| {
        anyhow!(
            "Invalid schedule: expected 5-field cron (min hour dom month dow) or 'once'. Details: {}",
            e
        )
    })?;
    Ok(())
}

/// Parse run_at: relative ("in 30 minutes", "in 2 hours") or absolute (ISO 8601).
pub fn parse_run_at(s: &str) -> Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow!("run_at cannot be empty"));
    }

    // Relative: "in N minutes", "in N hours", "in N days"
    let lower = s.to_lowercase();
    if lower.starts_with("in ") {
        let rest = lower["in ".len()..].trim();
        let (num_str, unit) = rest
            .split_once(|c: char| c.is_whitespace() || c == '_')
            .ok_or_else(|| anyhow!("Invalid relative time: expected e.g. 'in 30 minutes'"))?;
        let num: i64 = num_str.parse().context("Invalid number in relative time")?;
        let dur = match unit.trim().trim_end_matches('s') {
            "minute" => Duration::minutes(num),
            "hour" => Duration::hours(num),
            "day" => Duration::days(num),
            "week" => Duration::weeks(num),
            _ => return Err(anyhow!("Unknown unit: use minute(s), hour(s), day(s), week(s)")),
        };
        let t = Utc::now() + dur;
        return Ok(t.timestamp());
    }

    // Absolute: ISO 8601 or Unix timestamp
    if let Ok(ts) = s.parse::<i64>() {
        if ts > 0 {
            return Ok(ts);
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc).timestamp());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp());
    }

    Err(anyhow!(
        "Invalid run_at: use relative (e.g. 'in 30 minutes') or ISO 8601 / Unix timestamp"
    ))
}

/// Compute next run from cron expression (after `after_utc_secs`).
pub fn next_run_from_cron(schedule: &str, after_utc_secs: i64) -> Result<i64> {
    let cron_str = normalize_cron(schedule);
    let sched = Schedule::from_str(&cron_str).map_err(|e| anyhow!("Invalid cron: {}", e))?;
    let after = DateTime::from_timestamp(after_utc_secs, 0).unwrap_or_else(Utc::now);
    let next = sched
        .after(&after)
        .next()
        .ok_or_else(|| anyhow!("No next run for cron"))?;
    Ok(next.timestamp())
}

pub struct ScheduleService {
    storage: Arc<Mutex<Storage>>,
}

impl ScheduleService {
    pub fn new(storage: Arc<Mutex<Storage>>) -> Self {
        Self { storage }
    }

    /// Create and persist a scheduled job. Validates schedule (cron or "once").
    /// When new_conversation is true, conversation_id is stored as None (fresh conversation at run time).
    pub async fn schedule_task(
        &self,
        conversation_id: Option<Uuid>,
        message: String,
        run_at_str: &str,
        schedule: Option<String>,
        profile_name: Option<String>,
        title: Option<String>,
        new_conversation: bool,
    ) -> Result<ScheduledJob> {
        validate_schedule(schedule.as_deref())?;
        let run_at_utc_secs = parse_run_at(run_at_str)?;

        let now = Utc::now().timestamp();
        let stored_conv_id = if new_conversation {
            None
        } else {
            conversation_id.map(|u| u.to_string())
        };
        let job = ScheduledJob {
            id: Uuid::new_v4().to_string(),
            conversation_id: stored_conv_id,
            run_at_utc_secs,
            message,
            profile_name,
            title,
            status: "pending".to_string(),
            created_at_utc_secs: now,
            updated_at_utc_secs: now,
            error_message: None,
            schedule,
        };

        let guard = self.storage.lock().await;
        guard.insert_scheduled_job(&job).context("Failed to insert scheduled job")?;
        Ok(job)
    }

    /// Cancel (delete) a scheduled job by id. Returns true if the job existed and was removed.
    pub async fn cancel_scheduled_task(&self, job_id: &str) -> Result<bool> {
        let guard = self.storage.lock().await;
        guard
            .delete_scheduled_job(job_id)
            .context("Failed to delete scheduled job")
    }
}
