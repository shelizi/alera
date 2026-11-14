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
    pub value: String,
    pub remaining_percent: Option<f64>,
    pub reset: Option<String>,
    pub order: i32,
}

impl QuotaSnapshot {
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
        for metric in self.windows.iter().chain(self.buckets.iter()) {
            let remaining = metric
                .used_percent
                .filter(|v| v.is_finite())
                .map(|v| (100.0 - v).clamp(0.0, 100.0));
            readings.push(QuotaReading {
                label: short_label(&self.provider, &metric.label),
                value: remaining
                    .map(|v| format!("{}%", v.round() as u32))
                    .unwrap_or_else(|| "-".into()),
                remaining_percent: remaining,
                reset: metric.reset_description.clone(),
                order: reading_order(&self.provider, &metric.label),
            });
        }
        for amount in &self.amounts {
            let number = amount
                .spent_amount
                .or(amount.remaining_amount)
                .or(amount.limit_amount)
                .filter(|v| v.is_finite());
            let value = match (number, amount.currency.as_str()) {
                (Some(value), "USD") => format!("US${value:.2}"),
                (Some(value), "credits") => format!("{value:.2} credits"),
                (Some(value), currency) if !currency.is_empty() => format!("{currency} {value:.2}"),
                _ => "-".into(),
            };
            readings.push(QuotaReading {
                label: short_label(&self.provider, &amount.label),
                value,
                remaining_percent: None,
                reset: amount.reset_description.clone(),
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
