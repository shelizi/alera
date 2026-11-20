//! Native view model for Classic's Host-owned agent quota snapshots.
//! Neither credentials nor quota provider requests are implemented here.
use serde::Deserialize;
use serde_json::Value;

pub const REFRESH_INTERVAL_SECS: u64 = 5 * 60;
/// Keep Classic/GPUI quota pin visibility in sync while its popover is open.
/// Avoid the more frequent Host settings polling when neither UI is visible.
pub const fn shared_quota_pin_poll_seconds(settings_open: bool, quota_popover_open: bool) -> u64 {
    if settings_open || quota_popover_open {
        3
    } else {
        15
    }
}

const MAX_SNAPSHOTS: usize = 128;

const PROVIDERS: &[(&str, &str)] = &[
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("kimi", "Kimi"),
    ("grok", "Grok Build"),
    ("cursor", "Cursor"),
    ("agy", "Antigravity"),
    ("minimax", "MiniMax"),
    ("zai", "Z.ai"),
    ("devin", "Devin"),
    ("opencode", "OpenCode"),
];

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaMetric {
    #[serde(default, alias = "name")]
    pub label: String,
    #[serde(default)]
    pub used_percent: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
    #[serde(default)]
    pub reset_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAmount {
    pub label: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub spent_amount: Option<f64>,
    #[serde(default)]
    pub remaining_amount: Option<f64>,
    #[serde(default)]
    pub limit_amount: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
    #[serde(default)]
    pub reset_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub provider: String,
    #[serde(default = "default_account")]
    pub account_id: String,
    #[serde(default)]
    pub display_name: String,
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub windows: Vec<QuotaMetric>,
    #[serde(default)]
    pub buckets: Vec<QuotaMetric>,
    #[serde(default)]
    pub amounts: Vec<QuotaAmount>,
    #[serde(default)]
    pub data_quality: Option<String>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

fn default_account() -> String {
    "default".to_owned()
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaReading {
    pub label: String,
    pub full_label: String,
    pub value: String,
    pub remaining_percent: Option<f64>,
    pub reset: Option<String>,
    pub detail: Option<String>,
    pub order: i32,
}

/// Native GPUI progress bar values from Claude's provider-reported monthly extra usage.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeExtraUsageProgress {
    pub used_fraction: f32,
    pub spent_amount: f64,
    pub limit_amount: f64,
    pub remaining_amount: f64,
    pub currency: String,
}

impl ClaudeExtraUsageProgress {
    pub fn spent_text(&self) -> String {
        format_quota_amount(self.spent_amount, &self.currency)
    }

    pub fn limit_text(&self) -> String {
        format_quota_amount(self.limit_amount, &self.currency)
    }

    pub fn remaining_text(&self) -> String {
        format_quota_amount(self.remaining_amount, &self.currency)
    }
}

/// Classic's quota hover card uses the Host's Unix-millisecond reset time
/// in preference to an optional provider-supplied description. Format a
/// compact relative duration rather than silently dropping the reset ETA.
fn quota_reset_text(
    resets_at: Option<i64>,
    description: Option<&str>,
    now_millis: i64,
) -> Option<String> {
    if let Some(resets_at) = resets_at {
        let millis = resets_at.saturating_sub(now_millis);
        if millis <= 0 {
            return Some("Resets in <1m".to_owned());
        }
        let minutes = millis.saturating_add(59_999) / 60_000;
        let days = minutes / 1_440;
        let hours = (minutes % 1_440) / 60;
        let minutes = minutes % 60;
        let mut parts = Vec::with_capacity(3);
        if days > 0 {
            parts.push(format!("{days}d"));
        }
        if hours > 0 {
            parts.push(format!("{hours}h"));
        }
        if minutes > 0 || parts.is_empty() {
            parts.push(format!("{minutes}m"));
        }
        return Some(format!("Resets in {}", parts.join(" ")));
    }
    description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn unix_time_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn format_quota_amount(value: f64, currency: &str) -> String {
    match currency {
        "USD" => format!("US${value:.2}"),
        "credits" => format!("{value:.2} credits"),
        currency if !currency.is_empty() => format!("{currency} {value:.2}"),
        _ => format!("{value:.2}"),
    }
}

fn is_claude_extra_amount(provider: &str, label: &str) -> bool {
    provider == "claude"
        && (label.eq_ignore_ascii_case("Extra Usage")
            || label.eq_ignore_ascii_case("Usage credits"))
}

fn claude_extra_remaining(amount: &QuotaAmount) -> Option<f64> {
    amount
        .remaining_amount
        .filter(|value| value.is_finite() && *value >= 0.0)
        .or_else(|| {
            let (cap, used) = (amount.limit_amount?, amount.spent_amount?);
            (cap.is_finite() && used.is_finite() && cap >= 0.0 && used >= 0.0)
                .then(|| (cap - used).max(0.0))
        })
}

impl QuotaSnapshot {
    /// No progress bar without valid provider spending and positive monthly cap.
    /// The bar measures used (not remaining) and clamps over-limit usage to 100%.
    pub fn claude_extra_usage_progress(&self) -> Option<ClaudeExtraUsageProgress> {
        if self.provider != "claude" {
            return None;
        }
        self.amounts.iter().find_map(|amount| {
            if !is_claude_extra_amount(&self.provider, &amount.label) {
                return None;
            }
            let spent = amount
                .spent_amount
                .filter(|value| value.is_finite() && *value >= 0.0)?;
            let limit = amount
                .limit_amount
                .filter(|value| value.is_finite() && *value > 0.0)?;
            Some(ClaudeExtraUsageProgress {
                used_fraction: (spent / limit).clamp(0.0, 1.0) as f32,
                spent_amount: spent,
                limit_amount: limit,
                remaining_amount: (limit - spent).max(0.0),
                currency: amount.currency.clone(),
            })
        })
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.account_id)
    }

    /// Exact pin key used by Classic's AgentQuotaHostSettings.quotaPinKey.
    pub fn pin_key(&self) -> String {
        match self.provider.as_str() {
            "claude" | "opencode" => format!("{}:{}", self.provider, self.account_id),
            "codex" if self.account_id != "default" => {
                format!("codex:{}", self.account_id)
            }
            _ => self.provider.clone(),
        }
    }

    pub fn provider_label(&self) -> &'static str {
        PROVIDERS
            .iter()
            .find(|(id, _)| *id == self.provider)
            .map(|(_, name)| *name)
            .unwrap_or("Agent")
    }

    pub fn title(&self) -> String {
        let provider = self.provider_label();
        if self.provider == "opencode" {
            return format!(
                "{provider} {}",
                if self.account_id == "go" { "Go" } else { "Zen" }
            );
        }
        if self.account_id == "default" || self.account_id.is_empty() {
            provider.to_owned()
        } else {
            format!("{provider} {}", self.display_name.trim())
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.status == "ok"
    }

    pub fn readings(&self) -> Vec<QuotaReading> {
        let mut readings = Vec::new();
        let now_millis = unix_time_millis();
        for metric in self.windows.iter().chain(self.buckets.iter()) {
            let remaining = metric
                .used_percent
                .filter(|v| v.is_finite())
                .map(|v| (100.0 - v).clamp(0.0, 100.0));
            readings.push(QuotaReading {
                label: short_label(&self.provider, &metric.label),
                full_label: metric.label.clone(),
                value: remaining
                    .map(|v| format!("{}%", v.round() as u32))
                    .unwrap_or_else(|| "-".into()),
                remaining_percent: remaining,
                reset: quota_reset_text(
                    metric.resets_at,
                    metric.reset_description.as_deref(),
                    now_millis,
                ),
                detail: None,
                order: reading_order(&self.provider, &metric.label),
            });
        }
        for amount in &self.amounts {
            let is_extra = is_claude_extra_amount(&self.provider, &amount.label);
            let remaining = is_extra.then(|| claude_extra_remaining(amount)).flatten();
            let number = if is_extra {
                remaining
            } else {
                amount
                    .spent_amount
                    .or(amount.remaining_amount)
                    .or(amount.limit_amount)
                    .filter(|v| v.is_finite())
            };
            let value = number
                .map(|number| format_quota_amount(number, &amount.currency))
                .unwrap_or_else(|| "-".into());
            let value = if is_extra && number.is_some() {
                format!("{value} left")
            } else {
                value
            };
            let detail = if is_extra {
                match (amount.spent_amount, amount.limit_amount) {
                    (Some(used), Some(cap)) if used.is_finite() && cap.is_finite() => {
                        Some(format!(
                            "Extra Usage: {} / {} used",
                            format_quota_amount(used, &amount.currency),
                            format_quota_amount(cap, &amount.currency)
                        ))
                    }
                    _ => Some("Extra Usage remaining balance unavailable from provider".into()),
                }
            } else {
                None
            };
            let remaining_percent = if is_extra {
                remaining.zip(amount.limit_amount).and_then(|(left, cap)| {
                    (cap.is_finite() && cap > 0.0).then(|| (left / cap * 100.0).clamp(0.0, 100.0))
                })
            } else {
                None
            };
            readings.push(QuotaReading {
                label: if is_extra {
                    "Extra".into()
                } else {
                    short_label(&self.provider, &amount.label)
                },
                full_label: if is_extra {
                    "Extra Usage".into()
                } else {
                    amount.label.clone()
                },
                value,
                remaining_percent,
                reset: quota_reset_text(
                    amount.resets_at,
                    amount.reset_description.as_deref(),
                    now_millis,
                ),
                detail,
                order: 30,
            });
        }
        readings.sort_by_key(|reading| reading.order);
        readings
    }
}

/// Classic shows the provider icon, not its full name, in the status row.
/// Only account aliases disambiguate multi-account or OpenCode entries.
pub fn status_bar_account_label(snapshot: &QuotaSnapshot, all: &[QuotaSnapshot]) -> Option<String> {
    if snapshot.provider == "opencode" {
        return Some(
            if snapshot.account_id == "go" {
                "Go"
            } else {
                "Zen"
            }
            .to_owned(),
        );
    }
    let peers = all
        .iter()
        .filter(|row| row.provider == snapshot.provider)
        .count();
    if snapshot.account_id == "default" || snapshot.account_id.is_empty() {
        return (peers > 1).then(|| "Default".to_owned());
    }
    let alias = snapshot.display_name.trim();
    Some(if alias.is_empty() {
        snapshot.account_id.clone()
    } else {
        alias.to_owned()
    })
}

/// Classic status-bar visibility follows the shared per-account pin list
/// and the Host's enabled provider order; the overview shows every row.
pub fn status_bar_quotas<'a>(
    snapshots: &'a [QuotaSnapshot],
    enabled_order: Option<&[String]>,
    unpinned_keys: &[String],
) -> Vec<&'a QuotaSnapshot> {
    let mut visible: Vec<&QuotaSnapshot> = snapshots
        .iter()
        .filter(|row| {
            enabled_order.is_none_or(|order| order.iter().any(|p| p == &row.provider))
                && !unpinned_keys.iter().any(|key| key == &row.pin_key())
        })
        .collect();
    visible.sort_by_key(|row| {
        let index = enabled_order
            .and_then(|order| order.iter().position(|id| id == &row.provider))
            .or_else(|| PROVIDERS.iter().position(|(id, _)| *id == row.provider))
            .unwrap_or(usize::MAX);
        (index, row.account_id != "default", row.account_id.clone())
    });
    visible
}

fn short_label(provider: &str, label: &str) -> String {
    let lower = label.to_ascii_lowercase();
    if provider == "claude" && lower.contains("fable") {
        return "Fable".into();
    }
    let window = if lower.contains("5 hour") || lower.contains("five hour") || lower.contains("5h")
    {
        "5H"
    } else if lower.contains("week") {
        "W"
    } else if lower.contains("month") {
        "M"
    } else if lower.contains("day") {
        "D"
    } else {
        "Q"
    };
    if provider == "agy" {
        let model = if lower.contains("argon") {
            "Ar"
        } else if lower.contains("gemini") {
            "G"
        } else {
            "C/G"
        };
        return format!("{model}·{window}");
    }
    if provider == "zai" && lower.contains("mcp") {
        return "MCP".into();
    }
    if provider == "minimax" {
        let model = if lower.ends_with(" weekly") {
            &label[..label.len() - " weekly".len()]
        } else {
            label
        };
        let model = model.strip_prefix("MiniMax-").unwrap_or(model).trim();
        let compact = match model.to_ascii_lowercase().as_str() {
            "general" => "G".to_owned(),
            "video" => "V".to_owned(),
            _ => model.chars().take(6).collect::<String>(),
        };
        return format!("{compact}·{window}");
    }
    window.into()
}

fn reading_order(provider: &str, label: &str) -> i32 {
    let lower = label.to_ascii_lowercase();
    let base = if lower.contains("5 hour") || lower.contains("five hour") || lower.contains("5h") {
        0
    } else if lower.contains("week") {
        1
    } else if lower.contains("month") {
        2
    } else {
        3
    };
    if provider == "agy" {
        let group = if lower.contains("argon") {
            20
        } else if lower.contains("gemini") {
            0
        } else {
            10
        };
        return group + base;
    }
    if provider == "claude" && lower.contains("fable") {
        return 2;
    }
    base
}

/// Unknown future providers are ignored just as in Classic. An invalid row
/// cannot suppress healthy rows from another provider.
pub fn parse_quota_snapshots(payload: &Value) -> Result<Vec<QuotaSnapshot>, &'static str> {
    let snapshots = payload
        .get("snapshots")
        .and_then(Value::as_array)
        .ok_or("Host quota response is missing snapshots")?;
    if snapshots.len() > MAX_SNAPSHOTS {
        return Err("Host quota response contains too many accounts");
    }
    let mut result = Vec::new();
    for item in snapshots {
        let Ok(mut snapshot) = serde_json::from_value::<QuotaSnapshot>(item.clone()) else {
            continue;
        };
        if snapshot.provider == "antigravity" {
            snapshot.provider = "agy".to_owned();
        }
        if !PROVIDERS.iter().any(|(id, _)| *id == snapshot.provider) {
            continue;
        }
        result.push(snapshot);
    }
    result.sort_by_key(|s| {
        let order = PROVIDERS
            .iter()
            .position(|(id, _)| *id == s.provider)
            .unwrap_or(usize::MAX);
        (order, s.account_id.clone())
    });
    result.dedup_by(|right, left| right.key() == left.key());
    Ok(result)
}

/// An account-scoped refresh must update exactly the requested account.
/// Some Host/provider adapters return an empty set (or another account)
/// without a transport error. Treat that as failure so the old percentage
/// becomes stale instead of being misreported as freshly checked.
pub fn scoped_quota_refresh(
    fresh: Vec<QuotaSnapshot>,
    account: Option<(&str, &str)>,
) -> Result<Vec<QuotaSnapshot>, &'static str> {
    let Some((provider, account_id)) = account else {
        return Ok(fresh);
    };
    let matched = fresh
        .into_iter()
        .filter(|snapshot| snapshot.provider == provider && snapshot.account_id == account_id)
        .collect::<Vec<_>>();
    if matched.is_empty() {
        return Err("Host quota refresh did not return the requested account");
    }
    Ok(matched)
}

/// Read-only, Host-aggregated usage projection for the Native quota popover.
/// No raw transcript, credential, account identifier or model metadata is kept.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageRow {
    pub provider: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageDay {
    pub day: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageSummary {
    pub since_day: String,
    pub until_day: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub unpriced_records: u64,
    pub incomplete_sources: bool,
    pub providers: Vec<AgentUsageRow>,
    pub days: Vec<AgentUsageDay>,
}

pub fn parse_agent_usage_summary(payload: &Value) -> Result<AgentUsageSummary, &'static str> {
    let summary: AgentUsageSummary =
        serde_json::from_value(payload.clone()).map_err(|_| "Invalid Host usage summary")?;
    if summary.since_day.len() != 10
        || summary.until_day.len() != 10
        || summary.providers.len() > 3
        || summary.days.len() > 30
        || !summary.cost_usd.is_finite()
        || summary.cost_usd < 0.0
        || summary.providers.iter().any(|row| {
            !matches!(row.provider.as_str(), "claude" | "codex" | "grok")
                || !row.cost_usd.is_finite()
                || row.cost_usd < 0.0
        })
        || summary.days.iter().any(|row| {
            row.day.len() != 10
                || row.day < summary.since_day
                || row.day > summary.until_day
                || !row.cost_usd.is_finite()
                || row.cost_usd < 0.0
        })
    {
        return Err("Host usage summary exceeds limits or has invalid values");
    }
    Ok(summary)
}

/// Keep the last successful quota if a provider fails during a full refresh;
/// partial refreshes replace only the targeted provider/account.
pub fn merge_quota_snapshots(
    old: &[QuotaSnapshot],
    fresh: Vec<QuotaSnapshot>,
    partial: bool,
) -> Vec<QuotaSnapshot> {
    let mut next = if partial { old.to_vec() } else { Vec::new() };
    for mut snapshot in fresh {
        if let Some(previous) = old.iter().find(|old| old.key() == snapshot.key()) {
            if snapshot.status != "ok" && previous.is_healthy() {
                snapshot = previous.clone();
                snapshot.status = "stale".to_owned();
            }
        }
        next.retain(|old| old.key() != snapshot.key());
        next.push(snapshot);
    }
    next.sort_by_key(|s| {
        (
            PROVIDERS
                .iter()
                .position(|(id, _)| *id == s.provider)
                .unwrap_or(usize::MAX),
            s.account_id.clone(),
        )
    });
    next
}

/// A failed account-scoped refresh only invalidates that one account's
/// last readings; an all-provider refresh failure invalidates every row.
/// This matches Classic and keeps unrelated healthy quota chips current.
pub fn mark_failed_quota_refresh(snapshots: &mut [QuotaSnapshot], account: Option<(&str, &str)>) {
    for snapshot in snapshots {
        let affected = account.is_none_or(|(provider, account_id)| {
            snapshot.provider == provider && snapshot.account_id == account_id
        });
        if affected && snapshot.status == "ok" {
            snapshot.status = "stale".to_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn u05_classic_reset_countdown_uses_host_timestamp_and_fallback_description() {
        let now = 1_800_000_000_000_i64;
        assert_eq!(
            quota_reset_text(Some(now + 30_000), None, now).as_deref(),
            Some("Resets in 1m")
        );
        assert_eq!(
            quota_reset_text(Some(now + 3_600_000), None, now).as_deref(),
            Some("Resets in 1h")
        );
        assert_eq!(
            quota_reset_text(Some(now + 90_060_000), Some("wrong fallback"), now).as_deref(),
            Some("Resets in 1d 1h 1m")
        );
        assert_eq!(
            quota_reset_text(Some(now - 100), None, now).as_deref(),
            Some("Resets in <1m")
        );
        assert_eq!(
            quota_reset_text(None, Some("  resets tomorrow  "), now).as_deref(),
            Some("resets tomorrow")
        );
        assert_eq!(quota_reset_text(None, Some("   "), now), None);
        assert_eq!(quota_reset_text(None, None, now), None);
        let values = parse_quota_snapshots(&json!({"snapshots":[{
            "provider":"claude","status":"ok",
            "windows":[{"label":"weekly","usedPercent":30,"resetsAt":1_900_000_000_000_i64}]
        }]}))
        .unwrap();
        assert!(values[0].readings()[0]
            .reset
            .as_ref()
            .is_some_and(|text| text.starts_with("Resets in ")));
    }

    #[test]
    fn u05_usage_summary_is_bounded_and_cannot_inject_provider_metadata() {
        let sample = json!({
            "sinceDay":"2026-11-08","untilDay":"2026-11-14",
            "totalTokens":111,"costUsd":1.25,"unpricedRecords":1,
            "incompleteSources":false,
            "providers":[{"provider":"claude","tokens":111,"costUsd":1.25}],
            "days":[{"day":"2026-11-14","tokens":111,"costUsd":1.25}]
        });
        let parsed = parse_agent_usage_summary(&sample).unwrap();
        assert_eq!(parsed.total_tokens, 111);
        assert_eq!(parsed.providers[0].provider, "claude");
        let mut invalid = sample.clone();
        invalid["providers"][0]["provider"] = json!("private");
        assert!(parse_agent_usage_summary(&invalid).is_err());
        invalid = sample.clone();
        invalid["costUsd"] = json!(-1.0);
        assert!(parse_agent_usage_summary(&invalid).is_err());
        invalid = sample;
        invalid["days"][0]["day"] = json!("2099-01-01");
        assert!(parse_agent_usage_summary(&invalid).is_err());
    }

    #[test]
    fn u05_account_scoped_refresh_rejects_missing_result_and_ignores_other_accounts() {
        let rows = parse_quota_snapshots(&json!({"snapshots": [
            {"provider":"claude","accountId":"default","status":"ok","windows":[{"label":"5h","usedPercent":20}]},
            {"provider":"claude","accountId":"work","status":"ok","windows":[{"label":"5h","usedPercent":40}]},
            {"provider":"codex","accountId":"default","status":"ok","windows":[{"label":"week","usedPercent":30}]}
        ]})).unwrap();
        let matching = scoped_quota_refresh(rows.clone(), Some(("claude", "work"))).unwrap();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].key(), "claude:work");
        assert_eq!(
            scoped_quota_refresh(rows.clone(), Some(("claude", "missing"))).unwrap_err(),
            "Host quota refresh did not return the requested account"
        );
        assert_eq!(
            scoped_quota_refresh(Vec::new(), Some(("claude", "work"))).unwrap_err(),
            "Host quota refresh did not return the requested account"
        );
        assert_eq!(scoped_quota_refresh(rows.clone(), None).unwrap(), rows);
    }

    #[test]
    fn failing_one_account_refresh_does_not_stale_unrelated_provider_readings() {
        let fixtures = json!({"snapshots":[
            {"provider":"claude","accountId":"default","status":"ok","windows":[{"label":"5h","usedPercent":20}]},
            {"provider":"claude","accountId":"work","status":"ok","windows":[{"label":"5h","usedPercent":40}]},
            {"provider":"codex","accountId":"default","status":"ok","windows":[{"label":"week","usedPercent":30}]}
        ]});
        let mut rows = parse_quota_snapshots(&fixtures).unwrap();
        mark_failed_quota_refresh(&mut rows, Some(("claude", "work")));
        assert_eq!(
            rows.iter().map(|r| r.status.as_str()).collect::<Vec<_>>(),
            vec!["ok", "stale", "ok"]
        );
        assert_eq!(rows[1].readings()[0].value, "60%");
        mark_failed_quota_refresh(&mut rows, Some(("claude", "missing")));
        assert_eq!(rows[0].status, "ok");
        assert_eq!(rows[2].status, "ok");
        mark_failed_quota_refresh(&mut rows, None);
        assert!(rows.iter().all(|r| r.status == "stale"));
    }

    #[test]
    fn quota_popover_prioritizes_shared_classic_pin_sync_without_fast_idle_polling() {
        assert_eq!(shared_quota_pin_poll_seconds(false, false), 15);
        assert_eq!(shared_quota_pin_poll_seconds(true, false), 3);
        assert_eq!(shared_quota_pin_poll_seconds(false, true), 3);
        assert_eq!(shared_quota_pin_poll_seconds(true, true), 3);
    }

    #[test]
    fn classic_status_bar_refreshes_five_minutes_and_uses_compact_account_labels() {
        assert_eq!(REFRESH_INTERVAL_SECS, 5 * 60);
        let snapshots = parse_quota_snapshots(&json!({"snapshots": [
            {"provider":"claude","accountId":"default","status":"ok"},
            {"provider":"claude","accountId":"work","displayName":"Work","status":"ok"},
            {"provider":"codex","accountId":"default","status":"ok"},
            {"provider":"agy","accountId":"default","status":"ok"},
            {"provider":"opencode","accountId":"go","status":"ok"}
        ]}))
        .unwrap();
        assert_eq!(
            status_bar_account_label(&snapshots[0], &snapshots),
            Some("Default".into())
        );
        assert_eq!(
            status_bar_account_label(&snapshots[1], &snapshots),
            Some("Work".into())
        );
        assert_eq!(status_bar_account_label(&snapshots[2], &snapshots), None);
        assert_eq!(status_bar_account_label(&snapshots[3], &snapshots), None);
        assert_eq!(
            status_bar_account_label(&snapshots[4], &snapshots),
            Some("Go".into())
        );
    }

    #[test]
    fn visual_fixture_covers_ten_provider_logos_multiple_accounts_and_agy_five_metrics() {
        let raw: Value =
            serde_json::from_str(include_str!("../tests/fixtures/agent-quotas-visual.json"))
                .unwrap();
        let snapshots = parse_quota_snapshots(&raw).unwrap();
        let mut unique = std::collections::HashSet::new();
        for row in &snapshots {
            unique.insert(row.provider.as_str());
            assert!(!row.readings().is_empty());
        }
        assert_eq!(unique.len(), PROVIDERS.len());
        assert_eq!(snapshots.len(), 13);
        let agy = snapshots.iter().find(|s| s.provider == "agy").unwrap();
        assert_eq!(agy.readings().len(), 5);
        assert_eq!(agy.readings()[0].label, "G·5H");
        let unpinned = vec!["claude:work".to_owned(), "grok".to_owned()];
        let visible = status_bar_quotas(&snapshots, None, &unpinned);
        assert_eq!(visible.len(), 11);
        assert!(!visible.iter().any(|row| row.pin_key() == "claude:work"));
        assert!(!visible.iter().any(|row| row.pin_key() == "grok"));
    }

    #[test]
    fn retains_five_antigravity_buckets_and_clamps_remaining() {
        let values = parse_quota_snapshots(&json!({ "snapshots": [{
            "provider": "antigravity", "accountId": "default", "displayName": "AGY", "status": "ok",
            "buckets": [
                {"name": "Gemini Five Hour", "usedPercent": 10},
                {"name": "Gemini Weekly", "usedPercent": 25},
                {"name": "Claude and GPT Five Hour", "usedPercent": 100},
                {"name": "Claude and GPT Weekly", "usedPercent": 0},
                {"name": "Argon Weekly", "usedPercent": 99.5}
            ]
        }]}))
        .unwrap();
        assert_eq!(values[0].provider_label(), "Antigravity");
        let readings = values[0].readings();
        assert_eq!(readings.len(), 5);
        assert_eq!(readings[0].label, "G·5H");
        assert_eq!(readings[0].value, "90%");
        assert_eq!(readings[4].label, "Ar·W");
        assert_eq!(readings[4].value, "1%");
    }

    #[test]
    fn claude_extra_usage_remaining_is_distinct_from_amount_spent() {
        for label in ["Usage credits", "Extra Usage"] {
            let snapshots = parse_quota_snapshots(&json!({"snapshots": [{
                "provider":"claude", "accountId":"default", "status":"ok",
                "amounts":[{"label":label,"currency":"USD","spentAmount":2.83,"limitAmount":50.0}]
            }]}))
            .unwrap();
            let readings = snapshots[0].readings();
            assert_eq!(readings.len(), 1);
            assert_eq!(readings[0].label, "Extra");
            assert_eq!(readings[0].value, "US$47.17 left");
            assert!(readings[0]
                .detail
                .as_ref()
                .unwrap()
                .contains("US$2.83 / US$50.00 used"));
            assert!((readings[0].remaining_percent.unwrap() - 94.34).abs() < 0.00001);
        }
        let snapshots = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"claude", "status":"ok",
            "amounts":[{"label":"Extra Usage","currency":"USD","spentAmount":2.83}]
        }]}))
        .unwrap();
        assert_eq!(snapshots[0].readings()[0].value, "-");
        assert!(snapshots[0].readings()[0].remaining_percent.is_none());
    }

    #[test]
    fn claude_extra_progress_requires_actual_spending_and_positive_monthly_cap() {
        for label in ["Extra Usage", "Usage credits"] {
            let snapshots = parse_quota_snapshots(&json!({"snapshots": [{
                "provider":"claude", "status":"ok",
                "amounts":[{"label":label,"currency":"USD","spentAmount":2.83,"limitAmount":50.0}]
            }]}))
            .unwrap();
            let progress = snapshots[0].claude_extra_usage_progress().unwrap();
            assert!((progress.used_fraction as f64 - 0.0566).abs() < 0.00001);
            assert_eq!(progress.spent_text(), "US$2.83");
            assert_eq!(progress.limit_text(), "US$50.00");
            assert_eq!(progress.remaining_text(), "US$47.17");
        }
        for amount in [
            json!({"label":"Extra Usage","currency":"USD","limitAmount":50.0}),
            json!({"label":"Extra Usage","currency":"USD","spentAmount":2.83}),
            json!({"label":"Extra Usage","currency":"USD","spentAmount":2.83,"limitAmount":0}),
            json!({"label":"Extra Usage","currency":"USD","spentAmount":-1,"limitAmount":50}),
        ] {
            let snapshots = parse_quota_snapshots(&json!({"snapshots": [{
                "provider":"claude","status":"ok","amounts":[amount]
            }]}))
            .unwrap();
            assert!(snapshots[0].claude_extra_usage_progress().is_none());
        }
        let over = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"claude","status":"ok",
            "amounts":[{"label":"Extra Usage","currency":"USD","spentAmount":70,"limitAmount":50}]
        }]}))
        .unwrap();
        let progress = over[0].claude_extra_usage_progress().unwrap();
        assert_eq!(progress.used_fraction, 1.0);
        assert_eq!(progress.remaining_text(), "US$0.00");
        let unrelated = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"opencode","status":"ok",
            "amounts":[{"label":"Extra Usage","currency":"USD","spentAmount":10,"limitAmount":50}]
        }]}))
        .unwrap();
        assert!(unrelated[0].claude_extra_usage_progress().is_none());
    }

    #[test]
    fn supports_multi_accounts_amounts_unknown_provider_and_partial_updates() {
        let old = parse_quota_snapshots(&json!({"snapshots": [
            {"provider":"claude","accountId":"one","displayName":"Work","status":"ok","windows":[{"label":"5 hour","usedPercent":20}]},
            {"provider":"codex","accountId":"default","status":"ok","windows":[{"label":"Weekly","usedPercent":30}]},
            {"provider":"future","status":"ok"}
        ]})).unwrap();
        assert_eq!(old.len(), 2);
        assert_eq!(old[0].title(), "Claude Code Work");
        let fresh = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"claude","accountId":"one","status":"error","error":"offline",
            "amounts":[{"label":"Spend","currency":"USD","spentAmount":3.25}]
        }]}))
        .unwrap();
        let merged = merge_quota_snapshots(&old, fresh, true);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].status, "stale");
        assert_eq!(merged[0].readings()[0].value, "80%");
        assert_eq!(merged[1].provider, "codex");
        let amount = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"opencode","accountId":"go","status":"ok",
            "amounts":[{"label":"Spend","currency":"USD","spentAmount":3.25}]
        }]}))
        .unwrap();
        assert_eq!(amount[0].title(), "OpenCode Go");
        assert_eq!(amount[0].readings()[0].value, "US$3.25");
    }

    #[test]
    fn invalid_host_payload_and_invalid_metric_do_not_invent_remaining() {
        assert!(parse_quota_snapshots(&json!({"environment":{}})).is_err());
        let values = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"codex","status":"unavailable","windows":[{"label":"Weekly"}]
        }]}))
        .unwrap();
        assert_eq!(values[0].readings()[0].value, "-");
    }

    #[test]
    fn classic_pin_keys_and_provider_order_are_account_scoped() {
        let snapshots = parse_quota_snapshots(&json!({"snapshots":[
            {"provider":"claude","accountId":"default","status":"ok"},
            {"provider":"claude","accountId":"work","displayName":"Work","status":"ok"},
            {"provider":"codex","accountId":"default","status":"ok"},
            {"provider":"agy","accountId":"default","status":"ok"},
            {"provider":"opencode","accountId":"go","status":"ok"}
        ]}))
        .unwrap();
        assert_eq!(snapshots[0].pin_key(), "claude:default");
        assert_eq!(snapshots[1].pin_key(), "claude:work");
        assert_eq!(snapshots[2].pin_key(), "codex");
        assert_eq!(snapshots[3].pin_key(), "agy");
        assert_eq!(snapshots[4].pin_key(), "opencode:go");
        let ordered = status_bar_quotas(
            &snapshots,
            Some(&[
                "agy".into(),
                "claude".into(),
                "codex".into(),
                "opencode".into(),
            ]),
            &["claude:default".into(), "opencode:go".into()],
        );
        assert_eq!(
            ordered.iter().map(|row| row.key()).collect::<Vec<_>>(),
            vec!["agy:default", "claude:work", "codex:default"]
        );
        assert!(status_bar_quotas(&snapshots, Some(&[]), &[]).is_empty());
        assert_eq!(status_bar_quotas(&snapshots, None, &[]).len(), 5);
    }

    #[test]
    fn minimax_buckets_keep_model_identity_like_classic() {
        let values = parse_quota_snapshots(&json!({"snapshots": [{
            "provider":"minimax","status":"ok","buckets":[
                {"name":"MiniMax-general Weekly","usedPercent":25},
                {"name":"MiniMax-video Weekly","usedPercent":50}
            ]
        }]}))
        .unwrap();
        let readings = values[0].readings();
        assert_eq!(readings[0].label, "G·W");
        assert_eq!(readings[1].label, "V·W");
    }
}
