//! GPUI presentation of Classic's Host-owned Agent Quota snapshots.
//! Every query is read-only and performed on the background executor.
use super::*;
use alera_gpui_desktop::agent_quota::{merge_quota_snapshots, parse_quota_snapshots, QuotaReading};
use std::io;

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
    use super::quota_overview_status;

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
    /// Preserve previously fetched rows on failures; a single-account
    /// refresh cannot drop other provider/account rows.
    pub(super) fn refresh_agent_quotas(
        &mut self,
        force_refresh: bool,
        account: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
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
                    bridge.agent_quota_snapshot(
                        force_refresh,
                        query_account
                            .as_ref()
                            .map(|(provider, _)| provider.as_str()),
                        query_account.as_ref().map(|(_, id)| id.as_str()),
                    )
                })
                .await
                .and_then(|data| {
                    parse_quota_snapshots(&data)
                        .map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))
                });
            let _ = view.update(cx, |view, cx| {
                if view.quota_request != request {
                    return;
                }
                view.quota_loading = false;
                match result {
                    Ok(fresh) => {
                        view.quota_snapshots =
                            merge_quota_snapshots(&view.quota_snapshots, fresh, account.is_some());
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
        let chips = self
            .quota_snapshots
            .iter()
            .enumerate()
            .map(|(index, snapshot)| {
                let readings = snapshot.readings();
                div()
                    .id(("native-quota-status-provider", index))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .border_r_1()
                    .border_color(rgb(BORDER))
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
            })
            .collect::<Vec<_>>();
        div()
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
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_native_quota_dialog(window, cx);
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
            .into_any_element()
    }

    fn open_native_quota_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let viewport = window.viewport_size();
            let width = (f32::from(viewport.width) * 0.83)
                .clamp(300.0, 800.0)
                .min((f32::from(viewport.width) - 30.0).max(280.0));
            let height = (f32::from(viewport.height) * 0.62).clamp(160.0, 530.0);
            let content = owner
                .upgrade()
                .map(|entity| {
                    entity.update(cx, |view, cx| view.render_native_quota_overview(height, cx))
                })
                .unwrap_or_else(|| div().into_any_element());
            dialog
                .title("Agent Quotas · Runtime Host")
                .w(px(width))
                .centered()
                .close_button(true)
                .overlay_closable(true)
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| false)
                .child(content)
        });
    }

    fn render_native_quota_overview(
        &self,
        height: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let owner = cx.entity().downgrade();
        let refresh_owner = owner.clone();
        let loading = self.quota_loading;
        let entries = self.quota_snapshots.clone();
        let status = quota_overview_status(
            loading,
            self.quota_error.is_some(),
            !entries.is_empty(),
        );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_xs().text_color(rgb(MUTED)).child(status))
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
                    ),
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
                                            .id(("native-quota-refresh-account", index))
                                            .cursor_pointer()
                                            .px_2()
                                            .py_1()
                                            .text_xs()
                                            .text_color(rgb(if loading { MUTED } else { TEXT }))
                                            .child("↻")
                                            .on_click(move |_, _, cx| {
                                                let _ = row_owner.update(cx, |view, cx| {
                                                    view.refresh_agent_quotas(
                                                        true,
                                                        Some((provider.clone(), account.clone())),
                                                        cx,
                                                    );
                                                });
                                            }),
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
