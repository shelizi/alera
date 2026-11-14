use crate::agent_descriptor::canonical_agent_id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const CURRENT_PROVIDER_DEFAULTS_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAgentQuotaSettings {
    #[serde(default = "default_quota_providers")]
    pub enabled_providers: Vec<String>,
    #[serde(default = "legacy_provider_defaults_version")]
    pub provider_defaults_version: u32,
    #[serde(default = "default_true")]
    pub claude_default_enabled: bool,
    #[serde(default = "default_true")]
    pub claude_default_show_in_usage: bool,
    #[serde(default)]
    pub claude_profiles: Vec<RuntimeClaudeQuotaProfile>,
    #[serde(default)]
    pub codex_profiles: Vec<RuntimeCodexQuotaProfile>,
    /// Display-only account registrations. AGY uses an OS user-scoped
    /// keyring; these names are NOT proof of independent authentication.
    #[serde(default)]
    pub agy_profiles: Vec<RuntimeAgyQuotaProfile>,
    /// Per-host account history policy; never carries credentials.
    #[serde(default)]
    pub history_sync: RuntimeAgentHistorySyncSettings,
    #[serde(default)]
    pub environment: RuntimeAgentQuotaEnvironment,
}

impl Default for RuntimeAgentQuotaSettings {
    fn default() -> Self {
        Self {
            enabled_providers: default_quota_providers(),
            provider_defaults_version: CURRENT_PROVIDER_DEFAULTS_VERSION,
            claude_default_enabled: true,
            claude_default_show_in_usage: true,
            claude_profiles: Vec::new(),
            codex_profiles: Vec::new(),
            agy_profiles: Vec::new(),
            history_sync: RuntimeAgentHistorySyncSettings::default(),
            environment: RuntimeAgentQuotaEnvironment::default(),
        }
    }
}

impl RuntimeAgentQuotaSettings {
    pub fn normalized(mut self) -> Self {
        const SUPPORTED: [&str; 10] = [
            "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai", "devin",
            "opencode",
        ];
        let mut seen = std::collections::HashSet::new();
        self.enabled_providers = self
            .enabled_providers
            .into_iter()
            .filter_map(|provider| {
                let canonical = canonical_agent_id(&provider)
                    .map(str::to_owned)
                    .unwrap_or(provider);
                if SUPPORTED.contains(&canonical.as_str()) && seen.insert(canonical.clone()) {
                    Some(canonical)
                } else {
                    None
                }
            })
            .collect();
        if self.provider_defaults_version < CURRENT_PROVIDER_DEFAULTS_VERSION {
            if !self.enabled_providers.is_empty()
                && !self
                    .enabled_providers
                    .iter()
                    .any(|provider| provider == "devin")
            {
                self.enabled_providers.push("devin".to_string());
            }
            self.provider_defaults_version = CURRENT_PROVIDER_DEFAULTS_VERSION;
        }
        for profile in &mut self.claude_profiles {
            profile.alias = profile.alias.trim().to_string();
            profile.profile = profile.profile.trim().to_string();
            profile.usage_display_name = profile
                .usage_display_name
                .take()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
        }
        self.claude_profiles
            .retain(|profile| !profile.alias.is_empty() && !profile.profile.is_empty());
        self.environment.normalize();
        let mut seen_profiles = std::collections::HashSet::new();
        for profile in &mut self.codex_profiles {
            profile.alias = profile.alias.trim().to_string();
            profile.profile = profile.profile.trim().to_string();
        }
        self.codex_profiles.retain(|entry| {
            !entry.alias.is_empty()
                && std::path::Path::new(&entry.profile).is_absolute()
                && seen_profiles.insert(entry.profile.clone())
        });
        let mut seen_agy = std::collections::HashSet::new();
        let mut seen_agy_aliases = std::collections::HashSet::new();
        for profile in &mut self.agy_profiles {
            profile.alias = profile.alias.trim().to_string();
            profile.account_id = profile.account_id.trim().to_string();
        }
        self.agy_profiles.retain(|entry| {
            !entry.alias.is_empty()
                && entry.alias.chars().count() <= 80
                && !entry.alias.chars().any(char::is_control)
                && entry.account_id != "default"
                && !entry.account_id.is_empty()
                && entry.account_id.len() <= 64
                && entry
                    .account_id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                && !seen_agy.contains(&entry.account_id)
                && !seen_agy_aliases.contains(&entry.alias)
                && seen_agy.insert(entry.account_id.clone())
                && seen_agy_aliases.insert(entry.alias.clone())
        });
        self.agy_profiles.truncate(32);
        self.history_sync.normalize();
        self
    }

    pub fn claude_profiles_for_usage(&self) -> Vec<RuntimeClaudeQuotaProfile> {
        self.claude_profiles
            .iter()
            .filter(|profile| profile.show_in_usage)
            .cloned()
            .map(|mut profile| {
                profile.alias = profile.usage_label().to_string();
                profile
            })
            .collect()
    }
}

/// Only the same provider's histories can be mirrored as native files.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeAgentHistorySyncMode {
    All,
    #[default]
    Separate,
    Groups,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAgentHistorySyncSettings {
    #[serde(default)]
    pub mode: RuntimeAgentHistorySyncMode,
    /// Keys are `provider:accountId`. Group names are scoped per provider.
    #[serde(default)]
    pub account_groups: BTreeMap<String, String>,
}

impl RuntimeAgentHistorySyncSettings {
    pub fn normalize(&mut self) {
        self.account_groups = std::mem::take(&mut self.account_groups)
            .into_iter()
            .filter_map(|(account, group)| {
                let account = account.trim();
                let group = group.trim();
                let (provider, id) = account.split_once(':')?;
                if !matches!(provider, "codex" | "claude" | "agy")
                    || id.trim().is_empty()
                    || group.is_empty()
                {
                    return None;
                }
                Some((account.to_owned(), group.to_owned()))
            })
            .collect();
    }

    /// Membership only: file I/O must separately enforce safe path/liveness.
    pub fn should_sync(&self, provider: &str, first: &str, second: &str) -> bool {
        if first == second || !matches!(provider, "codex" | "claude" | "agy") {
            return false;
        }
        match self.mode {
            RuntimeAgentHistorySyncMode::All => true,
            RuntimeAgentHistorySyncMode::Separate => false,
            RuntimeAgentHistorySyncMode::Groups => {
                let one = self.account_groups.get(&format!("{provider}:{first}"));
                let two = self.account_groups.get(&format!("{provider}:{second}"));
                one.is_some_and(|group| !group.is_empty() && Some(group) == two)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCodexQuotaProfile {
    pub alias: String,
    pub profile: String,
}

/// Metadata only: do not add credentials or an unverified keyring path here.
/// Non-default AGY profiles cannot be launched or queried until an isolated
/// identity backend is explicitly validated on the target OS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeAgyQuotaProfile {
    pub alias: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeClaudeQuotaProfile {
    pub alias: String,
    pub profile: String,
    #[serde(default = "default_true")]
    pub show_in_usage: bool,
    #[serde(default)]
    pub usage_display_name: Option<String>,
}

impl RuntimeClaudeQuotaProfile {
    pub fn usage_label(&self) -> &str {
        self.usage_display_name.as_deref().unwrap_or(&self.alias)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAgentQuotaEnvironment {
    #[serde(default = "default_kimi_api_key")]
    pub kimi_api_key: String,
    #[serde(default = "default_zai_api_key")]
    pub zai_api_key: String,
    #[serde(default = "default_zai_base_url")]
    pub zai_base_url: String,
    #[serde(default = "default_minimax_api_key")]
    pub minimax_api_key: String,
    #[serde(default = "default_minimax_api_host")]
    pub minimax_api_host: String,
}

impl Default for RuntimeAgentQuotaEnvironment {
    fn default() -> Self {
        Self {
            kimi_api_key: default_kimi_api_key(),
            zai_api_key: default_zai_api_key(),
            zai_base_url: default_zai_base_url(),
            minimax_api_key: default_minimax_api_key(),
            minimax_api_host: default_minimax_api_host(),
        }
    }
}

impl RuntimeAgentQuotaEnvironment {
    fn normalize(&mut self) {
        self.kimi_api_key = non_blank_or(&self.kimi_api_key, default_kimi_api_key);
        self.zai_api_key = non_blank_or(&self.zai_api_key, default_zai_api_key);
        self.zai_base_url = non_blank_or(&self.zai_base_url, default_zai_base_url);
        self.minimax_api_key = non_blank_or(&self.minimax_api_key, default_minimax_api_key);
        self.minimax_api_host = non_blank_or(&self.minimax_api_host, default_minimax_api_host);
    }
}

fn default_quota_providers() -> Vec<String> {
    [
        "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai", "devin", "opencode",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn legacy_provider_defaults_version() -> u32 {
    1
}

fn non_blank_or(value: &str, fallback: fn() -> String) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback()
    } else {
        value.to_string()
    }
}

fn default_kimi_api_key() -> String {
    "KIMI_API_KEY".to_string()
}

fn default_zai_api_key() -> String {
    "ZAI_API_KEY".to_string()
}

fn default_zai_base_url() -> String {
    "ZAI_BASE_URL".to_string()
}

fn default_minimax_api_key() -> String {
    "MINIMAX_API_KEY".to_string()
}

fn default_minimax_api_host() -> String {
    "MINIMAX_API_HOST".to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agy_accounts_are_optional_non_secret_metadata_and_normalize_unique_ids() {
        let legacy: RuntimeAgentQuotaSettings =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(legacy.agy_profiles.is_empty());

        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "agyProfiles": [
                {"alias": " Research ", "accountId": "research"},
                {"alias": "Duplicate", "accountId": "research"},
                {"alias": "Reserved", "accountId": "default"},
                {"alias": "Wrong path", "accountId": "../escape"},
                {"alias": " Personal ", "accountId": "home_2"},
                {"alias": "Research", "accountId": "different_id"}
            ]
        }))
        .unwrap();
        let normalized = settings.normalized();
        assert_eq!(normalized.agy_profiles.len(), 2);
        assert_eq!(normalized.agy_profiles[0].alias, "Research");
        assert_eq!(normalized.agy_profiles[1].account_id, "home_2");
        let encoded = serde_json::to_value(normalized).unwrap();
        assert!(encoded["agyProfiles"][0].get("credentials").is_none());
        assert!(
            serde_json::from_value::<RuntimeAgyQuotaProfile>(serde_json::json!({
                "alias": "Other", "accountId": "other", "token": "not-storable"
            }))
            .is_err()
        );
    }

    #[test]
    fn history_sync_defaults_to_separate_for_legacy_settings() {
        let settings: RuntimeAgentQuotaSettings =
            serde_json::from_value(serde_json::json!({"codexProfiles": []})).unwrap();
        assert_eq!(
            settings.history_sync.mode,
            RuntimeAgentHistorySyncMode::Separate
        );
        assert!(!settings
            .history_sync
            .should_sync("codex", "default", "work"));
        assert!(!settings
            .history_sync
            .should_sync("codex", "default", "default"));
        assert!(!settings.history_sync.should_sync("grok", "default", "work"));
    }

    #[test]
    fn history_sync_only_merges_accounts_after_explicit_mode_selection() {
        let fresh = RuntimeAgentQuotaSettings::default();
        assert_eq!(
            fresh.history_sync.mode,
            RuntimeAgentHistorySyncMode::Separate
        );
        let omitted_mode: RuntimeAgentHistorySyncSettings =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!omitted_mode.should_sync("claude", "default", "work"));
        let explicit_all: RuntimeAgentQuotaSettings =
            serde_json::from_value(serde_json::json!({"historySync": {"mode": "all"}})).unwrap();
        assert!(explicit_all
            .history_sync
            .should_sync("codex", "default", "work"));
        let serialized = serde_json::to_value(explicit_all).unwrap();
        assert_eq!(serialized["historySync"]["mode"], "all");
    }

    #[test]
    fn history_sync_separate_mode_blocks_all_cross_account_copies() {
        let settings: RuntimeAgentHistorySyncSettings =
            serde_json::from_value(serde_json::json!({"mode": "separate"})).unwrap();
        assert!(!settings.should_sync("codex", "default", "work"));
        assert!(!settings.should_sync("claude", "default", "work"));
        assert!(!settings.should_sync("agy", "default", "work"));
    }

    #[test]
    fn history_sync_groups_require_explicit_membership_within_provider() {
        let mut settings: RuntimeAgentHistorySyncSettings =
            serde_json::from_value(serde_json::json!({
                "mode": "groups",
                "accountGroups": {
                    "codex:default": "work",
                    "codex:C:\\codex-work": "work",
                    "codex:private": "personal",
                    "claude:default": "work",
                    "claude:second": "work",
                    "agy:default": "research",
                    "agy:second": "research",
                    "claude:unused": "  ",
                    "unknown:foo": "work"
                }
            }))
            .unwrap();
        settings.normalize();
        assert!(settings.should_sync("codex", "default", "C:\\codex-work"));
        assert!(!settings.should_sync("codex", "default", "private"));
        assert!(!settings.should_sync("codex", "default", "missing"));
        assert!(settings.should_sync("claude", "default", "second"));
        assert!(settings.should_sync("agy", "default", "second"));
        assert!(!settings.account_groups.contains_key("claude:unused"));
        assert!(!settings.account_groups.contains_key("unknown:foo"));
    }

    #[test]
    fn history_sync_json_missing_mode_is_also_separate() {
        let policy: RuntimeAgentHistorySyncSettings = serde_json::from_value(
            serde_json::json!({"accountGroups": {"codex:a": "team", "codex:b": "team"}}),
        )
        .unwrap();
        assert_eq!(policy.mode, RuntimeAgentHistorySyncMode::Separate);
        assert!(!policy.should_sync("codex", "a", "b"));
    }

    #[test]
    fn history_sync_round_trip_retains_selection_and_groups() {
        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "historySync": {
                "mode": "groups",
                "accountGroups": {"codex:default": "team", "codex:b": "team"}
            }
        }))
        .unwrap();
        let restored: RuntimeAgentQuotaSettings =
            serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();
        assert_eq!(
            restored.history_sync.mode,
            RuntimeAgentHistorySyncMode::Groups
        );
        assert!(restored.history_sync.should_sync("codex", "default", "b"));
        assert!(!restored.history_sync.should_sync("claude", "default", "b"));
    }

    #[test]
    fn legacy_claude_profiles_are_visible_in_usage_by_default() {
        let profile: RuntimeClaudeQuotaProfile = serde_json::from_value(serde_json::json!({
            "alias": "ccdev",
            "profile": "dev"
        }))
        .expect("legacy profile");

        assert!(profile.show_in_usage);
        assert_eq!(profile.usage_label(), "ccdev");

        let settings: RuntimeAgentQuotaSettings =
            serde_json::from_value(serde_json::json!({ "claudeDefaultEnabled": false }))
                .expect("legacy settings");
        assert!(!settings.claude_default_enabled);
        assert!(settings.claude_default_show_in_usage);
    }

    #[test]
    fn legacy_provider_defaults_add_devin_once() {
        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "enabledProviders": ["claude", "codex"]
        }))
        .expect("legacy settings");
        let migrated = settings.normalized();

        assert_eq!(
            migrated.provider_defaults_version,
            CURRENT_PROVIDER_DEFAULTS_VERSION
        );
        assert!(migrated
            .enabled_providers
            .iter()
            .any(|provider| provider == "devin"));
    }

    #[test]
    fn current_provider_defaults_preserve_explicit_devin_disable() {
        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "enabledProviders": ["claude", "codex"],
            "providerDefaultsVersion": CURRENT_PROVIDER_DEFAULTS_VERSION
        }))
        .expect("current settings");
        let normalized = settings.normalized();

        assert!(!normalized
            .enabled_providers
            .iter()
            .any(|provider| provider == "devin"));
    }

    #[test]
    fn legacy_empty_provider_list_stays_empty() {
        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "enabledProviders": []
        }))
        .expect("legacy settings");
        let migrated = settings.normalized();

        assert!(migrated.enabled_providers.is_empty());
        assert_eq!(
            migrated.provider_defaults_version,
            CURRENT_PROVIDER_DEFAULTS_VERSION
        );
    }

    #[test]
    fn legacy_antigravity_provider_is_canonicalized_and_deduplicated() {
        let settings: RuntimeAgentQuotaSettings = serde_json::from_value(serde_json::json!({
            "enabledProviders": ["antigravity", "agy", "kimi"],
            "providerDefaultsVersion": CURRENT_PROVIDER_DEFAULTS_VERSION
        }))
        .expect("legacy settings");

        assert_eq!(
            settings.normalized().enabled_providers,
            vec!["agy".to_string(), "kimi".to_string()]
        );
    }

    #[test]
    fn default_quota_providers_use_canonical_agy_id() {
        assert!(RuntimeAgentQuotaSettings::default()
            .enabled_providers
            .contains(&"agy".to_string()));
        assert!(!RuntimeAgentQuotaSettings::default()
            .enabled_providers
            .contains(&"antigravity".to_string()));
    }

    #[test]
    fn usage_profiles_are_filtered_renamed_and_normalized() {
        let settings = RuntimeAgentQuotaSettings {
            claude_profiles: vec![
                RuntimeClaudeQuotaProfile {
                    alias: " ccdev ".to_string(),
                    profile: " dev ".to_string(),
                    show_in_usage: true,
                    usage_display_name: Some(" Engineering ".to_string()),
                },
                RuntimeClaudeQuotaProfile {
                    alias: "ccshared".to_string(),
                    profile: "shared".to_string(),
                    show_in_usage: false,
                    usage_display_name: Some("Shared".to_string()),
                },
            ],
            ..RuntimeAgentQuotaSettings::default()
        }
        .normalized();

        let profiles = settings.claude_profiles_for_usage();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile, "dev");
        assert_eq!(profiles[0].alias, "Engineering");
    }
}
