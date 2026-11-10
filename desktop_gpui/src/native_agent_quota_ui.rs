//! GPUI presentation of Classic's Host-owned Agent Quota snapshots.
//! Every query is read-only and performed on the background executor.
use super::*;
use alera_gpui_desktop::agent_quota::{merge_quota_snapshots, parse_quota_snapshots, QuotaReading};
use std::io;
use std::sync::OnceLock;

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

fn quota_provider_icon(provider: &str, size: f32) -> gpui::AnyElement {
    match provider {
        "claude" => gpui::svg()
            .data(include_bytes!("../../assets/agents/claude.svg"))
            .size(px(size))
            .into_any_element(),
        "codex" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            gpui::svg()
                .data(light_current_color_icon(
                    include_str!("../../assets/agents/codex.svg"),
                    &SVG,
                ))
                .size(px(size))
                .into_any_element()
        }
        "kimi" => gpui::svg()
            .data(include_bytes!("../../assets/agents/kimi.svg"))
            .size(px(size))
            .into_any_element(),
        "grok" => {
            static SVG: OnceLock<Vec<u8>> = OnceLock::new();
            gpui::svg()
                .data(light_current_color_icon(
                    include_str!("../../assets/agents/grok.svg"),
                    &SVG,
                ))
                .size(px(size))
                .into_any_element()
        }
        "minimax" => gpui::svg()
            .data(include_bytes!("../../assets/agents/minimax.svg"))
            .size(px(size))
            .into_any_element(),
        "zai" => gpui::svg()
            .data(include_bytes!("../../assets/agents/zai.svg"))
            .size(px(size))
            .into_any_element(),
        "devin" => gpui::svg()
            .data(include_bytes!("../../assets/agents/devin.svg"))
            .size(px(size))
            .into_any_element(),
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
        "opencode" => {
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
        "Last refresh failed; previous readings are stale"
    } else if !has_entries {
        "No quota data returned from Runtime Host"
    } else {
        "Provider quotas · remaining percent"
    }
}

#[cfg(test)]
mod quota_ui_tests {
    use super::{light_current_color_icon, quota_overview_status};
    use std::sync::OnceLock;

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
    if status != "ok" {
        return MUTED;
    }
    match reading.remaining_percent {
        Some(value) if value <= 10.0 => 0xeb9a9a,
        Some(value) if value <= 30.0 => 0xe9bd80,
        _ => TEXT,
    }
}

fn quota_reading_element(status: &str, reading: &QuotaReading) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(reading.label.clone()),
        )
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(quota_reading_color(status, reading)))
                .child(reading.value.clone()),
        )
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
        if !self
            .settings_extra_values
            .contains_key("agents.quotaUnpinnedKeys")
        {
            self.model.set_status(
                "Quota pins require the shared Runtime Host; refresh after reconnecting",
            );
            cx.notify();
            return;
        }
        let mut unpinned = self.quota_unpinned_keys();
        if unpinned.iter().any(|value| value == &key) {
            unpinned.retain(|value| value != &key);
        } else {
            unpinned.push(key);
        }
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
                        // No new numbers were validated. Keep the old values
                        // but mark them stale rather than claiming freshness.
                        for item in &mut view.quota_snapshots {
                            if item.status == "ok" {
                                item.status = "stale".to_owned();
                            }
                        }
                        view.quota_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_native_quota_status(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let has_data = !self.quota_snapshots.is_empty();
        let label = if self.quota_loading && !has_data {
            "Quotas: loading…"
        } else if self.quota_error.is_some() && !has_data {
            "Quotas: unavailable"
        } else if !has_data {
            "Quotas"
        } else {
            "◎"
        };
        #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
        let label = if Self::quota_visual_fixture_enabled() {
            "DEMO quotas"
        } else {
            label
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
            div()
                .id(("native-quota-status-provider", index))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .px_2()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(ACTIVE)))
                .border_r_1()
                .border_color(rgb(BORDER))
                .child(quota_provider_icon(&snapshot.provider, 14.0))
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(if snapshot.status == "ok" { TEXT } else { MUTED }))
                        .child(snapshot.title()),
                )
                .children(
                    readings
                        .iter()
                        .map(|reading| quota_reading_element(&snapshot.status, reading)),
                )
                .on_hover(cx.listener(move |view, hovering, _, cx| {
                    if *hovering {
                        view.quota_popover_open = true;
                        view.quota_popover_account = Some(account_key.clone());
                        cx.notify();
                    }
                }))
        })
        .collect::<Vec<_>>();
        div()
            .relative()
            .flex()
            .min_w_0()
            .flex_1()
            .items_center()
            .gap_1()
            .h(px(STATUS_BAR_HEIGHT))
            .child(
                div()
                    .id("native-quota-overview")
                    .cursor_pointer()
                    .flex_shrink_0()
                    .px_2()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .hover(|style| style.bg(rgb(ACTIVE)))
                    .child(label)
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
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("native-quota-status-scroll")
                    .min_w_0()
                    .flex_1()
                    .overflow_x_scroll()
                    .flex()
                    .items_center()
                    .children(chips),
            )
            .when(self.quota_popover_open, |root| {
                root.child(
                    div()
                        .id("native-quota-anchored-popover")
                        .absolute()
                        .bottom(px(STATUS_BAR_HEIGHT))
                        .left(px(0.0))
                        .w(px(410.0))
                        .max_h(px(490.0))
                        .overflow_y_scroll()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(ELEVATED))
                        .shadow_lg()
                        .p_2()
                        .child(self.render_native_quota_overview(440.0, cx))
                        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
                            view.quota_popover_open = false;
                            view.quota_popover_account = None;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn render_native_quota_overview(
        &self,
        height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let owner = cx.entity().downgrade();
        let refresh_owner = owner.clone();
        let loading = self.quota_loading;
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
        if let Some(account) = self.quota_popover_account.as_ref() {
            entries.retain(|row| &row.key() == account);
        }
        let unpinned = self.quota_unpinned_keys();
        let status =
            quota_overview_status(loading, self.quota_error.is_some(), !entries.is_empty());
        #[cfg(all(feature = "quota-visual-fixture", debug_assertions))]
        let status = if Self::quota_visual_fixture_enabled() {
            "SYNTHETIC TEST DATA - not actual quota"
        } else {
            status
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
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
                                    .child("Agent Quotas"),
                            )
                            .child(
                                div()
                                    .id("native-quota-refresh-all")
                                    .cursor_pointer()
                                    .rounded_md()
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(PANEL))
                                    .text_xs()
                                    .text_color(rgb(if loading { MUTED } else { TEXT }))
                                    .child(if loading {
                                        "Refreshing…"
                                    } else {
                                        "Refresh all"
                                    })
                                    .on_click(move |_, _, cx| {
                                        let _ = refresh_owner.update(cx, |view, cx| {
                                            view.refresh_agent_quotas(true, None, cx);
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .id("native-quota-close-popover")
                                    .cursor_pointer()
                                    .px_2()
                                    .text_color(rgb(MUTED))
                                    .child("×")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.quota_popover_open = false;
                                        view.quota_popover_account = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED)).child(status)),
            )
            .child(
                div()
                    .id("native-quota-overview-scroll")
                    .max_h(px(height))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(entries.into_iter().enumerate().map(|(index, snapshot)| {
                        let provider = snapshot.provider.clone();
                        let account = snapshot.account_id.clone();
                        let row_owner = owner.clone();
                        let pin_owner = owner.clone();
                        let pin_key = snapshot.pin_key();
                        let pinned = !unpinned.iter().any(|key| key == &pin_key);
                        let readings = snapshot.readings();
                        let status_text = if snapshot.status == "ok" {
                            snapshot
                                .data_quality
                                .as_deref()
                                .unwrap_or("Current")
                                .to_owned()
                        } else {
                            snapshot.status.clone()
                        };
                        div()
                            .id(("native-quota-overview-row", index))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(quota_provider_icon(&snapshot.provider, 16.0))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(if snapshot.status == "ok" {
                                                        TEXT
                                                    } else {
                                                        MUTED
                                                    }))
                                                    .child(snapshot.title()),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(status_text),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id(("native-quota-pin-account", index))
                                                    .cursor_pointer()
                                                    .px_2()
                                                    .py_1()
                                                    .text_xs()
                                                    .text_color(rgb(if pinned {
                                                        TEXT
                                                    } else {
                                                        MUTED
                                                    }))
                                                    .child(if pinned { "Unpin" } else { "Pin" })
                                                    .on_click(move |_, _, cx| {
                                                        let _ = pin_owner.update(cx, |view, cx| {
                                                            view.toggle_quota_pin(
                                                                pin_key.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id(("native-quota-refresh-account", index))
                                                    .cursor_pointer()
                                                    .px_2()
                                                    .py_1()
                                                    .text_xs()
                                                    .text_color(rgb(if loading {
                                                        MUTED
                                                    } else {
                                                        TEXT
                                                    }))
                                                    .child("↻")
                                                    .on_click(move |_, _, cx| {
                                                        let _ = row_owner.update(cx, |view, cx| {
                                                            view.refresh_agent_quotas(
                                                                true,
                                                                Some((
                                                                    provider.clone(),
                                                                    account.clone(),
                                                                )),
                                                                cx,
                                                            );
                                                        });
                                                    }),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id(("native-quota-reading-scroll", index))
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .overflow_x_scroll()
                                    .children(readings.iter().map(|reading| {
                                        quota_reading_element(&snapshot.status, reading)
                                    })),
                            )
                            .children(readings.into_iter().filter_map(|reading| {
                                reading.reset.map(|reset| {
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(format!("{}: {}", reading.label, reset))
                                })
                            }))
                            .when(
                                snapshot
                                    .error
                                    .as_ref()
                                    .is_some_and(|error| !error.is_empty()),
                                |row| {
                                    row.child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(snapshot.error.unwrap_or_default()),
                                    )
                                },
                            )
                    })),
            )
            .into_any_element()
    }
}
