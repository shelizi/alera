//! GPUI presentation of Classic's Host-owned Agent Quota snapshots.
//! Every query is read-only and performed on the background executor.
use super::*;
use alera_gpui_desktop::agent_quota::{
    mark_failed_quota_refresh, merge_quota_snapshots, parse_agent_usage_summary,
    parse_quota_snapshots, scoped_quota_refresh, status_bar_account_label,
    ClaudeExtraUsageProgress, QuotaReading, QuotaSnapshot,
};
use alera_gpui_desktop::agent_status_visual::{AgentStateSummary, AgentVisualState};
use alera_gpui_desktop::native_i18n::tr;
use alera_gpui_desktop::visual_tokens::FAINT;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::Sizable as _;
use std::io;
use std::sync::OnceLock;

/// Display provider names as Classic does, instead of Host abbreviations.
/// Unknown providers keep their original identity and artwork lookup.
pub(super) fn classic_agent_display_name(provider: &str) -> &str {
    match provider {
        "agy" => "Antigravity",
        "claude" => "Claude",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "devin" => "Devin",
        "copilot" => "GitHub Copilot",
        "opencode" | "opencode2" => "OpenCode",
        "grok" => "Grok",
        "gemini" => "Gemini",
        "minimax" => "MiniMax",
        "kimi" => "Kimi",
        "zai" => "Z.ai",
        "pi" => "Pi",
        "amp" => "Amp",
        _ => provider,
    }
}

/// Classic uses one animated glyph for a working Agent and colored static
/// status dots for other states. GPUI Kit's spinner animates its own element
/// (not the entire Workbench) and follows OS reduced-motion preferences.
pub(super) fn agent_run_indicator(state: AgentVisualState, size: f32) -> gpui::AnyElement {
    if state == AgentVisualState::Working {
        div()
            .w(px(size))
            .h(px(size))
            .text_color(rgb(state.run_indicator_color()))
            .child(Spinner::new().with_size(px(size)))
            .into_any_element()
    } else {
        div()
            .w(px(size))
            .h(px(size))
            .rounded_full()
            .bg(rgb(state.run_indicator_color()))
            .into_any_element()
    }
}

/// Render the same live Host Agent counts in the left sidebar and footer.
/// Each item is a Classic-color status dot + count, with translated tooltip;
/// neither surface owns or subscribes to an additional Host connection.
pub(super) fn agent_state_summary_chips(
    summary: AgentStateSummary,
    locale: UiLocale,
    labeled: bool,
) -> gpui::Div {
    let mut row = div().flex().items_center().gap_2().text_xs();
    if labeled {
        row = row.child(div().text_color(rgb(MUTED)).child(tr(locale, "Agents")));
    }
    for (state, count) in summary.groups() {
        if count == 0 {
            continue;
        }
        let tooltip = format!("{}: {}", tr(locale, state.label()), count);
        row = row.child(
            div()
                .id(("agent-summary-count", state as usize))
                .flex()
                .items_center()
                .gap_1()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(
                    div()
                        .w(px(12.0))
                        .h(px(12.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(agent_run_indicator(
                            state,
                            if state == AgentVisualState::Working {
                                11.0
                            } else {
                                TAB_ATTENTION_DOT_SIZE
                            },
                        )),
                )
                .child(div().text_color(rgb(MUTED)).child(count.to_string())),
        );
    }
    row
}

/// Embed the exact provider artwork already shipped by Classic. Avoid
/// hand-drawn monograms or runtime file paths that break portable builds.
/// Some upstream SVG assets use CSS currentColor, which defaults to black
/// in GPUI's direct SVG renderer; replace just that CSS token for dark UI.
fn light_current_color_icon(
    original: &'static str,
    cache: &'static OnceLock<Vec<u8>>,
) -> &'static [u8] {
    cache
        .get_or_init(|| original.replace("currentColor", "#EAEAEA").into_bytes())
        .as_slice()
}

/// Classic's compact quota gauge, rather than a generic bullet.
fn quota_gauge_icon() -> gpui::AnyElement {
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#A1A1A1" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 17a8.5 8.5 0 1 1 17 0"/><path d="M12 17l4.5-5"/><circle cx="12" cy="17" r="1.5" fill="#A1A1A1" stroke="none"/></svg>"##;
    gpui::svg().data(SVG).size(px(13.0)).into_any_element()
}

/// Embed the exact SVG artwork that Classic uses for AgentIdentityIcon.
/// The quota panel and + menu share these static buffers, including corrected
/// currentColor values for dark surfaces; no runtime asset lookup is needed.
pub(super) fn classic_agent_svg_bytes(provider: &str) -> Option<&'static [u8]> {
    match provider {
        "claude" => Some(include_bytes!("../../assets/agents/claude.svg")),
        "codex" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            Some(light_current_color_icon(
                include_str!("../../assets/agents/codex.svg"),
                &SVG,
            ))
        }
        "copilot" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            Some(light_current_color_icon(
                include_str!("../../assets/agents/copilot.svg"),
                &SVG,
            ))
        }
        "pi" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            Some(light_current_color_icon(
                include_str!("../../assets/agents/pi.svg"),
                &SVG,
            ))
        }
        "fx" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            Some(light_current_color_icon(
                include_str!("../../assets/agents/fx.svg"),
                &SVG,
            ))
        }
        "grok" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            Some(light_current_color_icon(
                include_str!("../../assets/agents/grok.svg"),
                &SVG,
            ))
        }
        "kimi" => Some(include_bytes!("../../assets/agents/kimi.svg")),
        "minimax" => Some(include_bytes!("../../assets/agents/minimax.svg")),
        "zai" => Some(include_bytes!("../../assets/agents/zai.svg")),
        "devin" => Some(include_bytes!("../../assets/agents/devin.svg")),
        _ => None,
    }
}

/// Shared Classic AgentIdentityIcon artwork for quota panels and Native
/// New Tab menu. Keep SVG/PNG decoding cached and never load assets from disk
/// on each popup open, preserving portable EXE behavior.
pub(super) fn quota_provider_icon(provider: &str, size: f32) -> gpui::AnyElement {
    if let Some(svg) = classic_agent_svg_bytes(provider) {
        return gpui::svg().data(svg).size(px(size)).into_any_element();
    }
    match provider {
        "cursor" => {
            static IMG: OnceLock<Arc<gpui::Image>> = OnceLock::new();
            gpui::img(
                IMG.get_or_init(|| {
                    Arc::new(gpui::Image::from_bytes(
                        gpui::ImageFormat::Png,
                        include_bytes!("../../assets/agents/cursor.png").to_vec(),
                    ))
                })
                .clone(),
            )
            .size(px(size))
            .into_any_element()
        }
        "agy" => {
            static IMG: OnceLock<Arc<gpui::Image>> = OnceLock::new();
            gpui::img(
                IMG.get_or_init(|| {
                    Arc::new(gpui::Image::from_bytes(
                        gpui::ImageFormat::Png,
                        include_bytes!("../../assets/agents/agy.png").to_vec(),
                    ))
                })
                .clone(),
            )
            .size(px(size))
            .into_any_element()
        }
        "amp" => {
            static IMG: OnceLock<Arc<gpui::Image>> = OnceLock::new();
            gpui::img(
                IMG.get_or_init(|| {
                    Arc::new(gpui::Image::from_bytes(
                        gpui::ImageFormat::Png,
                        include_bytes!("../../assets/agents/amp.png").to_vec(),
                    ))
                })
                .clone(),
            )
            .size(px(size))
            .into_any_element()
        }
        "opencode" | "opencode2" => {
            static IMG: OnceLock<Arc<gpui::Image>> = OnceLock::new();
            gpui::img(
                IMG.get_or_init(|| {
                    Arc::new(gpui::Image::from_bytes(
                        gpui::ImageFormat::Png,
                        include_bytes!("../../assets/agents/opencode.png").to_vec(),
                    ))
                })
                .clone(),
            )
            .size(px(size))
            .into_any_element()
        }
        _ => icon(UiIcon::Terminal, size, MUTED).into_any_element(),
    }
}

fn quota_overview_status(loading: bool, failed: bool, has_entries: bool) -> &'static str {
    if loading {
        "Refreshing from the existing Host…"
    } else if failed && !has_entries {
        "Quota unavailable · connect to the existing Runtime Host"
    } else if failed {
        "Last refresh failed; affected readings may be stale"
    } else if !has_entries {
        "No quota data returned from Runtime Host"
    } else {
        "Provider quotas · remaining percent"
    }
}

#[cfg(test)]
mod quota_ui_tests {
    use super::{
        classic_agent_svg_bytes, light_current_color_icon, quota_badge_fill,
        quota_hover_status_label, quota_hover_value_label, quota_lowest_remaining_percent,
        quota_overview_name, quota_overview_name_color, quota_overview_status, quota_pin_icon,
        quota_pin_tooltip, quota_reading_color, quota_remaining_bar_fraction, toggle_unpinned_keys,
        QuotaReading, QuotaSnapshot, UiIcon,
    };
    use alera_gpui_desktop::visual_tokens::{
        AGENT_BLOCKED, AGENT_DONE, AGENT_WAITING, ELEVATED, FAINT, TEXT,
    };
    use std::sync::OnceLock;

    #[test]
    fn nested_sidebar_agent_names_match_classic_without_losing_unknown_provider() {
        assert_eq!(super::classic_agent_display_name("agy"), "Antigravity");
        assert_eq!(super::classic_agent_display_name("claude"), "Claude");
        assert_eq!(super::classic_agent_display_name("opencode2"), "OpenCode");
        assert_eq!(
            super::classic_agent_display_name("custom-agent"),
            "custom-agent"
        );
    }

    #[test]
    fn status_bar_remaining_colors_follow_classic_thresholds_without_stale_green() {
        let mut reading = QuotaReading {
            label: "W".into(),
            full_label: "Weekly".into(),
            value: "19%".into(),
            remaining_percent: Some(19.0),
            reset: None,
            detail: None,
            order: 1,
        };
        assert_eq!(quota_reading_color("ok", &reading), AGENT_BLOCKED);
        reading.remaining_percent = Some(20.0);
        assert_eq!(quota_reading_color("ok", &reading), AGENT_WAITING);
        reading.remaining_percent = Some(49.0);
        assert_eq!(quota_reading_color("ok", &reading), AGENT_WAITING);
        reading.remaining_percent = Some(50.0);
        assert_eq!(quota_reading_color("ok", &reading), AGENT_DONE);
        assert_eq!(quota_reading_color("stale", &reading), FAINT);
        assert_eq!(quota_reading_color("error", &reading), AGENT_BLOCKED);
        reading.remaining_percent = None;
        assert_eq!(quota_reading_color("ok", &reading), FAINT);
    }

    #[test]
    fn classic_agent_artwork_is_shared_by_installed_menu_and_runtime_profiles() {
        let popup = include_str!("native_popup_ui.rs");
        assert!(popup.contains("branded_agent_popup_item(agent_id.clone(), label)"));
        // The Classic-compatible + menu only lists installed Agents.
        // Managed Profiles retain the same provider artwork in Runtime / Agents.
        assert!(!popup.contains("Host Agent Profiles"));
        let workbench = include_str!("main.rs");
        assert!(workbench.contains("agent-profile-row"));
        assert!(workbench.contains("native_agent_quota_ui::quota_provider_icon(&brand, 16.0)"));
        assert!(popup.contains("PopupMenuItem::new(label).icon(Icon::default().data(svg))"));
        assert!(popup.contains("PopupMenuItem::element"));
        // GPUI Kit inserts a blank xsmall icon in ElementItem rows:
        // compensate exactly once so PNG and SVG icons align with File/Terminal.
        assert!(popup.contains(".ml(px(-16.0))"));

        let artwork = include_str!("native_agent_quota_ui.rs");
        for asset in [
            "agents/codex.svg",
            "agents/claude.svg",
            "agents/copilot.svg",
            "agents/cursor.png",
            "agents/agy.png",
            "agents/opencode.png",
            "agents/pi.svg",
            "agents/amp.png",
            "agents/grok.svg",
            "agents/devin.svg",
            "agents/fx.svg",
        ] {
            assert!(artwork.contains(asset), "missing Classic artwork: {asset}");
        }
        assert!(artwork.contains("\"opencode\" | \"opencode2\""));
        for agent_id in ["codex", "claude", "copilot", "grok", "devin", "pi", "fx"] {
            let svg = classic_agent_svg_bytes(agent_id)
                .unwrap_or_else(|| panic!("missing native SVG logo for {agent_id}"));
            assert!(svg.starts_with(b"<svg"), "{agent_id} not a standalone SVG");
            assert!(
                !std::str::from_utf8(svg).unwrap().contains("currentColor"),
                "{agent_id} icon would render black on dark menu"
            );
        }
        for agent_id in ["agy", "cursor", "opencode", "opencode2", "amp"] {
            assert!(classic_agent_svg_bytes(agent_id).is_none());
        }
    }

    #[test]
    fn original_codex_svg_current_color_is_converted_for_dark_ui() {
        static SVG: OnceLock<Vec<u8>> = OnceLock::new();
        let result = std::str::from_utf8(light_current_color_icon(
            include_str!("../../assets/agents/codex.svg"),
            &SVG,
        ))
        .unwrap();
        assert!(result.contains("#EAEAEA"));
        assert!(!result.contains("currentColor"));
    }

    #[test]
    fn hover_status_and_overview_name_match_classic() {
        assert_eq!(quota_hover_status_label("ok", None), "Live");
        assert_eq!(
            quota_hover_status_label("ok", Some("estimated")),
            "Estimated"
        );
        assert_eq!(quota_hover_status_label("ok", Some("measured")), "Live");
        assert_eq!(quota_hover_status_label("stale", None), "Stale");
        assert_eq!(quota_hover_status_label("error", None), "Error");
        assert_eq!(quota_hover_status_label("unavailable", None), "Unavailable");
        assert_eq!(quota_overview_name_color("ok"), TEXT);
        assert_eq!(quota_overview_name_color("stale"), FAINT);
        assert_eq!(quota_overview_name_color("error"), AGENT_BLOCKED);
        assert_eq!(quota_overview_name("Claude Code", None), "Claude Code");
        assert_eq!(
            quota_overview_name("Claude Code", Some("Work")),
            "Claude Code Work"
        );
        assert_eq!(quota_overview_name("Codex", Some("  ")), "Codex");
        let snapshot = QuotaSnapshot {
            provider: "claude".into(),
            account_id: "default".into(),
            display_name: String::new(),
            status: "ok".into(),
            error: None,
            windows: vec![
                alera_gpui_desktop::agent_quota::QuotaMetric {
                    label: "5h".into(),
                    used_percent: Some(20.0),
                    resets_at: None,
                    reset_description: None,
                },
                alera_gpui_desktop::agent_quota::QuotaMetric {
                    label: "week".into(),
                    used_percent: Some(40.0),
                    resets_at: None,
                    reset_description: None,
                },
            ],
            buckets: Vec::new(),
            amounts: Vec::new(),
            data_quality: None,
            updated_at: None,
        };
        assert_eq!(quota_lowest_remaining_percent(&snapshot), Some(60.0));
        let fill = quota_badge_fill(AGENT_DONE);
        assert_ne!(fill, AGENT_DONE);
        assert_ne!(fill, ELEVATED);
    }

    #[test]
    fn quota_pin_toggle_matches_classic_status_bar_list() {
        assert_eq!(
            toggle_unpinned_keys(&["agy".into(), "codex".into()], "agy"),
            vec!["codex".to_owned()]
        );
        assert_eq!(
            toggle_unpinned_keys(&["codex".into()], "claude:default"),
            vec!["codex".to_owned(), "claude:default".to_owned()]
        );
        assert_eq!(quota_pin_icon(true), UiIcon::Pin);
        assert_eq!(quota_pin_icon(false), UiIcon::PinOff);
        assert_eq!(quota_pin_tooltip(true, false), "Unpin From Status Bar");
        assert_eq!(quota_pin_tooltip(false, false), "Pin To Status Bar");
        assert_eq!(quota_pin_tooltip(true, true), "Shown in status bar");
        assert_eq!(
            quota_pin_tooltip(false, true),
            "Hidden from status bar - available in the quota panel"
        );
    }

    #[test]
    fn quota_usage_bar_fill_follows_classic_remaining_percent() {
        assert_eq!(quota_remaining_bar_fraction(80.0), Some(0.8));
        assert_eq!(quota_remaining_bar_fraction(0.0), Some(0.0));
        assert_eq!(quota_remaining_bar_fraction(100.0), Some(1.0));
        assert_eq!(quota_remaining_bar_fraction(f64::NAN), None);
        assert_eq!(quota_remaining_bar_fraction(-1.0), None);
        let mut reading = QuotaReading {
            label: "5H".into(),
            full_label: "5-hour limit".into(),
            value: "72%".into(),
            remaining_percent: Some(72.4),
            reset: None,
            detail: None,
            order: 0,
        };
        assert_eq!(quota_hover_value_label(&reading), "72% Left");
        reading.remaining_percent = None;
        reading.value = "US$1.00 left".into();
        assert_eq!(quota_hover_value_label(&reading), "US$1.00 left");
    }

    #[test]
    fn missing_host_without_previous_readings_does_not_claim_stale_data() {
        assert!(quota_overview_status(false, true, false).contains("unavailable"));
        assert!(!quota_overview_status(false, true, false).contains("previous readings"));
    }

    #[test]
    fn prior_readings_are_only_stale_after_a_failed_refresh() {
        assert!(quota_overview_status(false, true, true).contains("stale"));
        assert!(quota_overview_status(true, true, true).starts_with("Refreshing"));
        assert!(quota_overview_status(false, false, false).starts_with("No quota data"));
    }
}

fn quota_reading_color(status: &str, reading: &QuotaReading) -> u32 {
    quota_remaining_color(status, reading.remaining_percent)
}

fn quota_remaining_color(status: &str, remaining_percent: Option<f64>) -> u32 {
    match status {
        "stale" => alera_gpui_desktop::visual_tokens::FAINT,
        "ok" => match remaining_percent {
            Some(value) if value.is_finite() && value < 20.0 => AGENT_BLOCKED,
            Some(value) if value.is_finite() && value < 50.0 => AGENT_WAITING,
            Some(value) if value.is_finite() => AGENT_DONE,
            _ => alera_gpui_desktop::visual_tokens::FAINT,
        },
        _ => AGENT_BLOCKED,
    }
}

/// Classic hover badge. Ok stays "Live" unless the host marked the sample estimated.
fn quota_hover_status_label(status: &str, data_quality: Option<&str>) -> &'static str {
    if status == "ok" && data_quality == Some("estimated") {
        return "Estimated";
    }
    match status {
        "ok" => "Live",
        "stale" => "Stale",
        "error" => "Error",
        _ => "Unavailable",
    }
}

/// Overview names use the provider color, not the remaining-percent color.
fn quota_overview_name_color(status: &str) -> u32 {
    match status {
        "ok" => TEXT,
        "stale" => FAINT,
        _ => AGENT_BLOCKED,
    }
}

fn quota_overview_name(provider_label: &str, profile: Option<&str>) -> String {
    match profile.map(str::trim).filter(|label| !label.is_empty()) {
        Some(profile) => format!("{provider_label} {profile}"),
        None => provider_label.to_owned(),
    }
}

/// Lowest window or bucket remaining, matching Classic's snapshot badge.
fn quota_lowest_remaining_percent(snapshot: &QuotaSnapshot) -> Option<f64> {
    snapshot
        .windows
        .iter()
        .chain(snapshot.buckets.iter())
        .filter_map(|metric| {
            metric
                .used_percent
                .filter(|value| value.is_finite())
                .map(|used| (100.0 - used).clamp(0.0, 100.0))
        })
        .reduce(f64::min)
}

/// Classic `AleraBadge` fills the status color at alpha 28 over `surfaceElevated`.
fn quota_badge_fill(color: u32) -> u32 {
    const ALPHA: u32 = 28;
    let mix = |channel: u32, base: u32| (channel * ALPHA + base * (255 - ALPHA)) / 255;
    let r = mix((color >> 16) & 0xFF, (ELEVATED >> 16) & 0xFF);
    let g = mix((color >> 8) & 0xFF, (ELEVATED >> 8) & 0xFF);
    let b = mix(color & 0xFF, ELEVATED & 0xFF);
    (r << 16) | (g << 8) | b
}

fn quota_mono_label(size: f32, weight: gpui::FontWeight, color: u32, text: String) -> gpui::Div {
    div()
        .font_family("JetBrains Mono")
        .text_size(px(size))
        .font_weight(weight)
        .text_color(rgb(color))
        .whitespace_nowrap()
        .child(text)
}

fn quota_pin_icon(pinned: bool) -> UiIcon {
    if pinned {
        UiIcon::Pin
    } else {
        UiIcon::PinOff
    }
}

/// Overview copy matches Classic's quota card. Settings copy matches
/// `_QuotaPinButton` beside each provider switch.
fn quota_pin_tooltip(pinned: bool, settings_row: bool) -> &'static str {
    match (settings_row, pinned) {
        (false, true) => "Unpin From Status Bar",
        (false, false) => "Pin To Status Bar",
        (true, true) => "Shown in status bar",
        (true, false) => "Hidden from status bar - available in the quota panel",
    }
}

fn toggle_unpinned_keys(unpinned: &[String], key: &str) -> Vec<String> {
    let mut next = unpinned.to_vec();
    if let Some(index) = next.iter().position(|existing| existing == key) {
        next.remove(index);
    } else {
        next.push(key.to_owned());
    }
    next
}

/// Classic's hover card fills the bar with remaining percent, not used percent.
fn quota_remaining_bar_fraction(remaining_percent: f64) -> Option<f32> {
    (remaining_percent.is_finite() && (0.0..=100.0).contains(&remaining_percent))
        .then_some((remaining_percent / 100.0) as f32)
}

fn quota_hover_value_label(reading: &QuotaReading) -> String {
    match reading.remaining_percent {
        Some(percent) if percent.is_finite() => format!("{}% Left", percent.round() as u32),
        _ => reading.value.clone(),
    }
}

/// 4px pill. Classic draws the track in `surfaceVariant` on the elevated card.
fn quota_usage_bar(fraction: f32, color: u32) -> gpui::Div {
    div()
        .w_full()
        .h(px(4.0))
        .rounded_full()
        .overflow_hidden()
        .bg(rgb(PANEL))
        .child(
            div()
                .w(gpui::relative(fraction.clamp(0.0, 1.0)))
                .h_full()
                .rounded_full()
                .bg(rgb(color)),
        )
}

/// Status bar uses 8px labels. The all-accounts overview uses 9px. Neither draws a bar.
fn quota_inline_reading(status: &str, reading: &QuotaReading, label_size: f32) -> gpui::AnyElement {
    let color = quota_reading_color(status, reading);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(2.0))
        .child(quota_mono_label(
            label_size,
            gpui::FontWeight::MEDIUM,
            FAINT,
            reading.label.clone(),
        ))
        .child(quota_mono_label(
            10.0,
            gpui::FontWeight::SEMIBOLD,
            color,
            reading.value.clone(),
        ))
        .into_any_element()
}

fn quota_hover_reading(status: &str, reading: &QuotaReading) -> gpui::AnyElement {
    let color = quota_reading_color(status, reading);
    let fraction = reading
        .remaining_percent
        .and_then(quota_remaining_bar_fraction);
    let reset = reading
        .reset
        .clone()
        .unwrap_or_else(|| "Reset time unavailable".to_owned());
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(MUTED))
                        .whitespace_nowrap()
                        .child(reading.full_label.clone()),
                )
                .child(quota_mono_label(
                    11.0,
                    gpui::FontWeight::SEMIBOLD,
                    color,
                    quota_hover_value_label(reading),
                )),
        )
        .when_some(fraction, |column, fraction| {
            column.child(quota_usage_bar(fraction, color))
        })
        .child(quota_mono_label(
            9.0,
            gpui::FontWeight::NORMAL,
            FAINT,
            reset,
        ))
        .into_any_element()
}

/// The fill tracks actual extra spend (not remaining), like Classic.
/// Unknown or invalid spending/cap does not render this element at all.
fn claude_extra_usage_progress_element(
    status: &str,
    progress: &ClaudeExtraUsageProgress,
    reset: Option<&str>,
) -> gpui::AnyElement {
    let used_percentage = progress.spent_amount / progress.limit_amount * 100.0;
    let remaining_percentage = (1.0 - progress.used_fraction as f64) * 100.0;
    let color = quota_remaining_color(status, Some(remaining_percentage));
    div()
        .flex()
        .flex_col()
        .gap_1()
        .mt_1()
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .child("Extra Usage"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(format!("Used {}", progress.spent_text())),
                )
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(format!("Monthly limit {}", progress.limit_text())),
                ),
        )
        .child(quota_usage_bar(progress.used_fraction, color))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(format!("{used_percentage:.2}% used")),
                )
                .child(
                    div()
                        .text_color(rgb(color))
                        .child(format!("{} remaining", progress.remaining_text())),
                ),
        )
        .when_some(reset, |column, reset| {
            column.child(
                div()
                    .text_xs()
                    .text_color(rgb(FAINT))
                    .child(reset.to_owned()),
            )
        })
        .into_any_element()
}

impl Workbench {
    /// Test-only local preview. Requires BOTH an explicit feature and an
    /// environment opt-in in a debug executable; release binaries never
    /// substitute fixture percentages for Host-owned measurements.
    #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
    fn quota_visual_fixture_enabled() -> bool {
        std::env::var_os("ALERA_GPUI_QUOTA_VISUAL_FIXTURE").is_some()
    }

    #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
    fn refresh_quota_visual_fixture(&mut self, cx: &mut Context<Self>) {
        let payload: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/agent-quotas-visual.json"))
                .expect("fixed test fixture must contain valid JSON");
        self.quota_snapshots = parse_quota_snapshots(&payload)
            .expect("fixed test fixture must have quota snapshot rows");
        self.quota_enabled_order = Some(
            [
                "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai", "devin",
                "opencode",
            ]
            .iter()
            .map(|id| id.to_string())
            .collect(),
        );
        self.quota_error = None;
        self.quota_loading = false;
        self.quota_last_refresh = Instant::now();
        self.quota_popover_open = true;
        self.quota_popover_account = (std::env::var("ALERA_GPUI_QUOTA_VISUAL_FIXTURE").as_deref()
            == Ok("agy"))
        .then(|| "agy:default".to_owned());
        // Seed sample unpinned profiles for visual review. This is not
        // persisted and never sent to a Runtime Host.
        self.settings_extra_values
            .entry("agents.quotaUnpinnedKeys")
            .or_insert_with(|| serde_json::json!(["claude:work", "grok"]));
        cx.notify();
    }

    fn quota_unpinned_keys(&self) -> Vec<String> {
        self.settings_extra_values
            .get("agents.quotaUnpinnedKeys")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    /// Classic's pin / pin-off control. A missing host field still means
    /// every row is pinned; the click writes that default through the
    /// shared Runtime Host instead of showing a dead label.
    fn quota_pin_button(
        &self,
        id: impl Into<gpui::ElementId>,
        pin_key: String,
        pinned: bool,
        settings_row: bool,
        owner: gpui::WeakEntity<Self>,
    ) -> gpui::AnyElement {
        let tooltip = quota_pin_tooltip(pinned, settings_row);
        div()
            .id(id)
            .cursor_pointer()
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .hover(|style| style.bg(rgb(ACTIVE)))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
            })
            .child(icon(
                quota_pin_icon(pinned),
                12.0,
                if pinned {
                    MUTED
                } else {
                    alera_gpui_desktop::visual_tokens::FAINT
                },
            ))
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |view, cx| {
                    view.toggle_quota_pin(pin_key.clone(), cx);
                });
            })
            .into_any_element()
    }

    pub(super) fn render_quota_settings_pin_rows(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        if self.quota_snapshots.is_empty() {
            return vec![div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(
                    "Pin controls appear on each quota row after the Runtime Host reports usage.",
                )
                .into_any_element()];
        }
        let owner = cx.entity().downgrade();
        let unpinned = self.quota_unpinned_keys();
        let mut entries = self.quota_snapshots.clone();
        entries.sort_by_key(|row| {
            let rank = self
                .quota_enabled_order
                .as_deref()
                .and_then(|order| order.iter().position(|id| id == &row.provider))
                .unwrap_or(usize::MAX);
            (rank, row.account_id != "default", row.account_id.clone())
        });
        entries
            .into_iter()
            .enumerate()
            .map(|(index, snapshot)| {
                let pin_key = snapshot.pin_key();
                let pinned = !unpinned.iter().any(|key| key == &pin_key);
                div()
                    .id(("native-settings-quota-pin-row", index))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(quota_provider_icon(&snapshot.provider, 16.0))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(snapshot.title()),
                            ),
                    )
                    .child(self.quota_pin_button(
                        ("native-settings-quota-pin", index),
                        pin_key,
                        pinned,
                        true,
                        owner.clone(),
                    ))
                    .into_any_element()
            })
            .collect()
    }

    fn toggle_quota_pin(&mut self, key: String, cx: &mut Context<Self>) {
        #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
        if Self::quota_visual_fixture_enabled() {
            let mut unpinned = self.quota_unpinned_keys();
            if unpinned.iter().any(|existing| existing == &key) {
                unpinned.retain(|existing| existing != &key);
            } else {
                unpinned.push(key);
            }
            self.settings_extra_values.insert(
                "agents.quotaUnpinnedKeys".into(),
                serde_json::json!(unpinned),
            );
            cx.notify();
            return;
        }
        if !self.settings_native_profile_migrated || self.runtime_bridge.is_none() {
            self.refresh_shared_desktop_settings(cx);
            self.model.set_status(
                "Quota pins require the shared Runtime Host; refresh after reconnecting",
            );
            cx.notify();
            return;
        }
        let unpinned = toggle_unpinned_keys(&self.quota_unpinned_keys(), &key);
        self.change_shared_desktop_setting(
            "agents.quotaUnpinnedKeys",
            serde_json::json!(unpinned),
            cx,
        );
    }

    /// Preserve previously fetched rows on failures; a single-account
    /// refresh cannot drop other provider/account rows.
    pub(super) fn refresh_agent_quotas(
        &mut self,
        force_refresh: bool,
        account: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
        if Self::quota_visual_fixture_enabled() {
            self.refresh_quota_visual_fixture(cx);
            return;
        }
        if self.quota_loading {
            return;
        }
        let bridge = self.runtime_bridge.clone();
        self.quota_request = self.quota_request.wrapping_add(1);
        let request = self.quota_request;
        self.quota_loading = true;
        self.quota_last_refresh = Instant::now();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let query_account = account.clone();
            let result = cx
                .background_spawn(async move {
                    let bridge = bridge.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotConnected, "Alera CLI is unavailable")
                    })?;
                    let data = bridge.agent_quota_snapshot(
                        force_refresh,
                        query_account
                            .as_ref()
                            .map(|(provider, _)| provider.as_str()),
                        query_account.as_ref().map(|(_, id)| id.as_str()),
                    )?;
                    let order = bridge.agent_quota_enabled_order().ok();
                    Ok::<_, io::Error>((data, order))
                })
                .await
                .and_then(|(data, order)| {
                    parse_quota_snapshots(&data)
                        .and_then(|snapshots| {
                            scoped_quota_refresh(
                                snapshots,
                                account
                                    .as_ref()
                                    .map(|(provider, id)| (provider.as_str(), id.as_str())),
                            )
                        })
                        .map(|snapshots| (snapshots, order))
                        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))
                });
            let _ = view.update(cx, |view, cx| {
                if view.quota_request != request {
                    return;
                }
                view.quota_loading = false;
                match result {
                    Ok((fresh, order)) => {
                        view.quota_snapshots =
                            merge_quota_snapshots(&view.quota_snapshots, fresh, account.is_some());
                        if let Some(order) = order {
                            view.quota_enabled_order = Some(order);
                        }
                        view.quota_error = None;
                    }
                    Err(error) => {
                        // A failed single-account refresh must never stale
                        // unrelated healthy providers in the status bar.
                        mark_failed_quota_refresh(
                            &mut view.quota_snapshots,
                            account
                                .as_ref()
                                .map(|(provider, id)| (provider.as_str(), id.as_str())),
                        );
                        view.quota_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Read a Host-owned, secret-free usage summary in a background task.
    /// Selection changes invalidate older requests so 7d/30d cannot race.
    fn refresh_agent_usage(&mut self, days: u8, cx: &mut Context<Self>) {
        if !matches!(days, 7 | 30) || (self.quota_usage_loading && self.quota_usage_days == days) {
            return;
        }
        self.quota_usage_days = days;
        self.quota_usage_loading = true;
        self.quota_usage_error = None;
        self.quota_usage_summary = None;
        self.quota_usage_request = self.quota_usage_request.wrapping_add(1);
        let request = self.quota_usage_request;
        let bridge = self.runtime_bridge.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let bridge = bridge.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotConnected, "Alera CLI is unavailable")
                    })?;
                    let payload = bridge.agent_usage_summary(days)?;
                    parse_agent_usage_summary(&payload)
                        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.quota_usage_request != request {
                    return;
                }
                view.quota_usage_loading = false;
                match result {
                    Ok(summary) => {
                        view.quota_usage_summary = Some(summary);
                        view.quota_usage_error = None;
                    }
                    Err(error) => {
                        view.quota_usage_summary = None;
                        view.quota_usage_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_native_quota_status(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let has_data = !self.quota_snapshots.is_empty();
        let empty_status = if self.quota_loading {
            "Refreshing quotas"
        } else if self.quota_error.is_some() {
            "Quota refresh failed"
        } else {
            "No quota data"
        };
        let unpinned_keys = self.quota_unpinned_keys();
        let chips = alera_gpui_desktop::agent_quota::status_bar_quotas(
            &self.quota_snapshots,
            self.quota_enabled_order.as_deref(),
            &unpinned_keys,
        )
        .into_iter()
        .enumerate()
        .map(|(index, snapshot)| {
            let readings = snapshot.readings();
            let account_key = snapshot.key();
            let click_account_key = account_key.clone();
            let profile_label = status_bar_account_label(snapshot, &self.quota_snapshots);
            let reading_chips = if readings.is_empty() {
                vec![quota_mono_label(
                    10.0,
                    gpui::FontWeight::NORMAL,
                    quota_remaining_color(&snapshot.status, None),
                    "-".into(),
                )
                .into_any_element()]
            } else {
                readings
                    .iter()
                    .map(|reading| quota_inline_reading(&snapshot.status, reading, 8.0))
                    .collect::<Vec<_>>()
            };
            div()
                .id(("native-quota-status-provider", index))
                .h(px(STATUS_BAR_HEIGHT))
                .flex()
                .flex_shrink_0()
                .items_center()
                .px(px(6.0))
                .overflow_hidden()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(ACTIVE)))
                .border_r_1()
                .border_color(rgb(BORDER))
                .child(quota_provider_icon(&snapshot.provider, 12.0))
                // Classic puts the provider identity in the icon and only
                // adds an account alias when it disambiguates accounts.
                .children(profile_label.map(|label| {
                    quota_mono_label(8.0, gpui::FontWeight::SEMIBOLD, MUTED, label).ml(px(4.0))
                }))
                .child(
                    div()
                        .ml(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .children(reading_chips),
                )
                .on_hover(cx.listener(move |view, hovering, _, cx| {
                    if *hovering {
                        view.quota_popover_open = true;
                        view.quota_popover_account = Some(account_key.clone());
                        view.quota_usage_open = false;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.quota_popover_open = true;
                    view.quota_popover_account = Some(click_account_key.clone());
                    view.quota_usage_open = false;
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();
        let hover_card = self.quota_popover_account.is_some() && !self.quota_usage_open;
        div()
            .relative()
            .flex()
            .min_w_0()
            .flex_1()
            .items_center()
            .h(px(STATUS_BAR_HEIGHT))
            .child(
                div()
                    .id("native-quota-overview")
                    .h(px(STATUS_BAR_HEIGHT))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .px(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACTIVE)))
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .child(quota_gauge_icon())
                    .on_hover(cx.listener(|view, hovering, _, cx| {
                        if *hovering {
                            view.quota_popover_open = true;
                            view.quota_popover_account = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.quota_popover_open = !view.quota_popover_open;
                        view.quota_popover_account = None;
                        view.quota_usage_open = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("native-quota-status-scroll")
                    .min_w_0()
                    .flex_1()
                    .h(px(STATUS_BAR_HEIGHT))
                    .overflow_x_scroll()
                    .flex()
                    .items_center()
                    .children(chips)
                    .when(!has_data, |row| {
                        row.child(
                            quota_mono_label(
                                10.0,
                                gpui::FontWeight::NORMAL,
                                MUTED,
                                empty_status.into(),
                            )
                            .px(px(8.0)),
                        )
                    })
                    .child(
                        div()
                            .id("native-quota-status-refresh")
                            .h(px(STATUS_BAR_HEIGHT))
                            .flex_shrink_0()
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(ACTIVE)))
                            .tooltip({
                                let label = if self.quota_loading {
                                    "Refreshing Quotas"
                                } else {
                                    "Refresh Quotas - Automatically Refreshes Every 5 Minutes"
                                };
                                move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(label)
                                        .build(window, cx)
                                }
                            })
                            .child(icon(
                                UiIcon::Refresh,
                                13.0,
                                if self.quota_loading { FAINT } else { MUTED },
                            ))
                            .on_click(cx.listener(|view, _, _, cx| {
                                if !view.quota_loading {
                                    view.refresh_agent_quotas(true, None, cx);
                                }
                            })),
                    ),
            )
            .when(self.quota_popover_open, |root| {
                root.child(
                    div()
                        .id("native-quota-anchored-popover")
                        .absolute()
                        .bottom(px(STATUS_BAR_HEIGHT))
                        .left(px(0.0))
                        .w(px(if hover_card { 360.0 } else { 380.0 }))
                        .max_h(px(480.0))
                        .overflow_hidden()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgb(BORDER_ACTIVE))
                        .bg(rgb(ELEVATED))
                        .shadow_lg()
                        .child(if self.quota_usage_open {
                            self.render_native_usage_overview(404.0, cx)
                        } else {
                            self.render_native_quota_overview(cx)
                        })
                        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                            view.quota_popover_open = false;
                            view.quota_popover_account = None;
                            view.quota_usage_open = false;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn render_native_usage_overview(
        &self,
        height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let loading = self.quota_usage_loading;
        let period = self.quota_usage_days;
        let summary = self.quota_usage_summary.clone();
        let error = self.quota_usage_error.clone();
        let empty = !loading && summary.is_none() && error.is_none();
        div()
            .w_full()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child("Agent Usage"),
                    )
                    .child(
                        div()
                            .id("native-usage-back-to-quotas")
                            .cursor_pointer()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .child("Quotas")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.quota_usage_open = false;
                                view.quota_popover_account = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("native-usage-close")
                            .cursor_pointer()
                            .px_2()
                            .text_color(rgb(MUTED))
                            .child("×")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.quota_usage_open = false;
                                view.quota_popover_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("native-usage-7days")
                            .cursor_pointer()
                            .rounded_md()
                            .bg(rgb(if period == 7 { ACTIVE } else { PANEL }))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .child("7 days")
                            .on_click(cx.listener(|view, _, _, cx| {
                                if view.quota_usage_days != 7 {
                                    view.refresh_agent_usage(7, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("native-usage-30days")
                            .cursor_pointer()
                            .rounded_md()
                            .bg(rgb(if period == 30 { ACTIVE } else { PANEL }))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .child("30 days")
                            .on_click(cx.listener(|view, _, _, cx| {
                                if view.quota_usage_days != 30 {
                                    view.refresh_agent_usage(30, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("native-usage-refresh")
                            .cursor_pointer()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(rgb(if loading { MUTED } else { TEXT }))
                            .child(if loading { "Loading…" } else { "Refresh" })
                            .on_click(cx.listener(|view, _, _, cx| {
                                if !view.quota_usage_loading {
                                    let days = view.quota_usage_days;
                                    view.refresh_agent_usage(days, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("native-usage-scroll")
                    .max_h(px(height))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when(loading, |list| {
                        list.child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Reading usage from existing Runtime Host…"),
                        )
                    })
                    .children(error.into_iter().map(|message| {
                        div()
                            .text_xs()
                            .text_color(rgb(AGENT_BLOCKED))
                            .child(message)
                    }))
                    .when(empty, |list| {
                        list.child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("No usage data loaded"),
                        )
                    })
                    .children(summary.into_iter().map(|usage| {
                        let has_usage = usage.total_tokens > 0;
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(PANEL))
                                    .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                                        "{} – {}",
                                        usage.since_day, usage.until_day
                                    )))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT))
                                            .child(format!(
                                                "{} tokens · USD {:.2}",
                                                usage.total_tokens, usage.cost_usd
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child("Estimated cost; not actual billed charges"),
                                    )
                                    .when(
                                        usage.incomplete_sources || usage.unpriced_records > 0,
                                        |panel| {
                                            panel.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(AGENT_WAITING))
                                                    .child(format!(
                                                        "{} unpriced records{}",
                                                        usage.unpriced_records,
                                                        if usage.incomplete_sources {
                                                            " · some sources unavailable"
                                                        } else {
                                                            ""
                                                        },
                                                    )),
                                            )
                                        },
                                    ),
                            )
                            .when(!has_usage, |panel| {
                                panel.child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child("No recorded Agent usage for this period"),
                                )
                            })
                            .children(usage.providers.into_iter().enumerate().map(|(i, row)| {
                                div()
                                    .id(("native-usage-provider", i))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .p_2()
                                    .rounded_sm()
                                    .bg(rgb(PANEL))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(quota_provider_icon(&row.provider, 16.0))
                                            .child(row.provider),
                                    )
                                    .child(div().text_xs().text_color(rgb(TEXT)).child(format!(
                                        "{} tokens · USD {:.2}",
                                        row.tokens, row.cost_usd
                                    )))
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(MUTED))
                                    .child("Daily usage"),
                            )
                            .children(usage.days.into_iter().rev().enumerate().map(|(i, day)| {
                                div()
                                    .id(("native-usage-day", i))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .px_2()
                                    .py_1()
                                    .child(div().text_xs().text_color(rgb(MUTED)).child(day.day))
                                    .child(div().text_xs().text_color(rgb(TEXT)).child(format!(
                                        "{} tokens · USD {:.2}",
                                        day.tokens, day.cost_usd
                                    )))
                            }))
                    })),
            )
            .into_any_element()
    }

    fn sorted_quota_entries(&self) -> Vec<QuotaSnapshot> {
        let mut entries = self.quota_snapshots.clone();
        entries.sort_by_key(|row| {
            let rank = self
                .quota_enabled_order
                .as_deref()
                .and_then(|order| order.iter().position(|id| id == &row.provider))
                .or_else(|| {
                    [
                        "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai",
                        "devin", "opencode",
                    ]
                    .iter()
                    .position(|id| *id == row.provider)
                })
                .unwrap_or(usize::MAX);
            (rank, row.account_id != "default", row.account_id.clone())
        });
        entries
    }

    fn quota_snapshot_refresh_button(
        &self,
        id: impl Into<gpui::ElementId>,
        loading: bool,
        provider: String,
        account: String,
        owner: gpui::WeakEntity<Self>,
    ) -> gpui::AnyElement {
        let tooltip = if loading {
            "Refreshing Agent"
        } else {
            "Refresh Agent"
        };
        div()
            .id(id)
            .flex_shrink_0()
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(ACTIVE)))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
            })
            .child(icon(
                UiIcon::Refresh,
                13.0,
                if loading { FAINT } else { MUTED },
            ))
            .on_click(move |_, _, cx| {
                if loading {
                    return;
                }
                let _ = owner.update(cx, |view, cx| {
                    view.refresh_agent_quotas(true, Some((provider.clone(), account.clone())), cx);
                });
            })
            .into_any_element()
    }

    /// Gauge hover: one text row per account, pin, and Open Usage. No bars.
    fn render_native_quota_overview(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut entries = self.sorted_quota_entries();
        if let Some(account) = self.quota_popover_account.as_ref() {
            entries.retain(|row| &row.key() == account);
            return self.render_native_quota_hover_card(entries, cx);
        }
        let loading = self.quota_loading;
        let unpinned = self.quota_unpinned_keys();
        let all = self.quota_snapshots.clone();
        let owner = cx.entity().downgrade();
        let empty_message = if entries.is_empty() {
            Some(if loading {
                "Refreshing quotas".to_owned()
            } else {
                self.quota_error
                    .clone()
                    .unwrap_or_else(|| "No quota data".to_owned())
            })
        } else {
            None
        };
        let rows = entries
            .into_iter()
            .enumerate()
            .map(|(index, snapshot)| {
                self.quota_overview_row(index, &snapshot, &all, &unpinned, loading, owner.clone())
            })
            .collect::<Vec<_>>();
        div()
            .id("native-quota-overview-scroll")
            .w_full()
            .max_h(px(480.0))
            .overflow_y_scroll()
            .py(px(4.0))
            .flex()
            .flex_col()
            .children(empty_message.map(|message| {
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED))
                    .child(message)
            }))
            .children(rows)
            .child(div().h(px(1.0)).w_full().bg(rgb(BORDER)))
            .child(
                div().p(px(8.0)).child(
                    div()
                        .id("native-quota-open-usage")
                        .w_full()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .rounded_md()
                        .bg(rgb(PANEL))
                        .px(px(12.0))
                        .py(px(8.0))
                        .hover(|style| style.bg(rgb(ACTIVE)))
                        .child(quota_gauge_icon())
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT))
                                .child("Open Usage"),
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.quota_usage_open = true;
                            view.quota_popover_account = None;
                            view.refresh_agent_usage(7, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn quota_overview_row(
        &self,
        index: usize,
        snapshot: &QuotaSnapshot,
        all: &[QuotaSnapshot],
        unpinned: &[String],
        loading: bool,
        owner: gpui::WeakEntity<Self>,
    ) -> gpui::AnyElement {
        let profile = status_bar_account_label(snapshot, all);
        let name = quota_overview_name(snapshot.provider_label(), profile.as_deref());
        let tip = quota_row_tooltip(snapshot, &name);
        let readings = snapshot.readings();
        let reading_chips = if readings.is_empty() {
            vec![quota_mono_label(
                10.0,
                gpui::FontWeight::NORMAL,
                quota_remaining_color(&snapshot.status, None),
                "-".into(),
            )
            .into_any_element()]
        } else {
            readings
                .iter()
                .map(|reading| quota_inline_reading(&snapshot.status, reading, 9.0))
                .collect::<Vec<_>>()
        };
        let pin_key = snapshot.pin_key();
        let pinned = !unpinned.iter().any(|key| key == &pin_key);
        div()
            .id(("native-quota-overview-row", index))
            .px(px(8.0))
            .py(px(2.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(quota_provider_icon(&snapshot.provider, 14.0))
            .child(
                div()
                    .id(("native-quota-overview-name", index))
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .tooltip({
                        let tip = tip.clone();
                        move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone())
                                .build(window, cx)
                        }
                    })
                    .child(
                        quota_mono_label(
                            10.0,
                            gpui::FontWeight::SEMIBOLD,
                            quota_overview_name_color(&snapshot.status),
                            name,
                        )
                        .overflow_hidden()
                        .truncate(),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .children(reading_chips),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(self.quota_snapshot_refresh_button(
                        ("native-quota-refresh-account", index),
                        loading,
                        snapshot.provider.clone(),
                        snapshot.account_id.clone(),
                        owner.clone(),
                    ))
                    .child(self.quota_pin_button(
                        ("native-quota-pin-account", index),
                        pin_key,
                        pinned,
                        false,
                        owner,
                    )),
            )
            .into_any_element()
    }

    /// Provider-chip hover: remaining bars, status badge, and per-account refresh.
    fn render_native_quota_hover_card(
        &self,
        entries: Vec<QuotaSnapshot>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if entries.is_empty() {
            return div()
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(rgb(MUTED))
                .child("Quota data unavailable")
                .into_any_element();
        }
        let loading = self.quota_loading;
        let all = self.quota_snapshots.clone();
        let owner = cx.entity().downgrade();
        div()
            .id("native-quota-hover-scroll")
            .w_full()
            .max_h(px(480.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .children(entries.into_iter().enumerate().map(|(index, snapshot)| {
                let profile = status_bar_account_label(&snapshot, &all);
                let readings = snapshot.readings();
                let extra_progress = snapshot.claude_extra_usage_progress();
                let reset = readings.iter().find_map(|reading| {
                    (reading.label == "Extra")
                        .then_some(reading.reset.as_deref())
                        .flatten()
                        .map(str::to_owned)
                });
                let visible_readings = readings
                    .iter()
                    .filter(|reading| extra_progress.is_none() || reading.label != "Extra")
                    .cloned()
                    .collect::<Vec<_>>();
                let status_color = quota_remaining_color(
                    &snapshot.status,
                    quota_lowest_remaining_percent(&snapshot),
                );
                let status_label =
                    quota_hover_status_label(&snapshot.status, snapshot.data_quality.as_deref());
                let empty = visible_readings.is_empty() && extra_progress.is_none();
                let empty_message = snapshot
                    .error
                    .clone()
                    .filter(|error| !error.is_empty())
                    .unwrap_or_else(|| "Quota data unavailable".to_owned());
                div()
                    .id(("native-quota-hover-section", index))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .when(index > 0, |section| {
                        section.border_t_1().border_color(rgb(BORDER_ACTIVE))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(quota_provider_icon(&snapshot.provider, 18.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .overflow_hidden()
                                            .text_size(px(14.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT))
                                            .whitespace_nowrap()
                                            .child(snapshot.provider_label().to_owned()),
                                    )
                                    .children(profile.map(|label| {
                                        quota_mono_label(
                                            10.0,
                                            gpui::FontWeight::NORMAL,
                                            MUTED,
                                            label,
                                        )
                                    })),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded_full()
                                    .bg(rgb(quota_badge_fill(status_color)))
                                    .text_size(px(11.0))
                                    .text_color(rgb(status_color))
                                    .whitespace_nowrap()
                                    .child(status_label),
                            )
                            .child(self.quota_snapshot_refresh_button(
                                ("native-quota-refresh-account", index),
                                loading,
                                snapshot.provider.clone(),
                                snapshot.account_id.clone(),
                                owner.clone(),
                            )),
                    )
                    .when(empty, |section| {
                        section.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(MUTED))
                                .child(empty_message),
                        )
                    })
                    .when(!empty, |section| {
                        section.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .children(
                                    visible_readings.iter().map(|reading| {
                                        quota_hover_reading(&snapshot.status, reading)
                                    }),
                                )
                                .children(extra_progress.iter().map(|progress| {
                                    claude_extra_usage_progress_element(
                                        &snapshot.status,
                                        progress,
                                        reset.as_deref(),
                                    )
                                })),
                        )
                    })
            }))
            .into_any_element()
    }
}

fn quota_row_tooltip(snapshot: &QuotaSnapshot, name: &str) -> String {
    let mut lines = vec![name.to_owned()];
    let readings = snapshot.readings();
    for reading in &readings {
        let reset = reading.reset.as_deref().unwrap_or("Reset time unavailable");
        if let Some(percent) = reading.remaining_percent.filter(|value| value.is_finite()) {
            lines.push(format!(
                "{}: {}% Left - {}",
                reading.full_label,
                percent.round() as u32,
                reset
            ));
        } else {
            lines.push(format!(
                "{}: {} - {}",
                reading.full_label, reading.value, reset
            ));
        }
    }
    if let Some(error) = snapshot.error.as_ref().filter(|error| !error.is_empty()) {
        lines.push(format!("Error: {error}"));
    } else if readings.is_empty() {
        lines.push("Quota data unavailable".to_owned());
    }
    lines.join("\n")
}
