//! GPUI presentation for history sync policy stored by the shared Runtime Host.
//! Native's own settings JSON is intentionally never read or written here.
use super::*;

enum HistorySyncChange {
    Mode(HistorySyncMode),
    Group(HistorySyncAccount, String),
}

impl Workbench {
    pub(super) fn refresh_shared_history_sync(&mut self, cx: &mut Context<Self>) {
        if self.history_sync_loading || self.history_sync_busy {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.history_sync_status =
                "Alera CLI is unavailable; shared Host history settings cannot be loaded".into();
            cx.notify();
            return;
        };
        self.history_sync_loading = true;
        self.history_sync_status = "Reading Classic's Runtime Host history settings…".into();
        self.history_sync_request = self.history_sync_request.wrapping_add(1);
        let request = self.history_sync_request;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.read_history_sync() })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.history_sync_request != request {
                    return;
                }
                view.history_sync_loading = false;
                match result {
                    Ok(snapshot) => {
                        view.history_sync_snapshot = Some(snapshot);
                        view.history_sync_status =
                            "Shared Runtime Host history settings loaded".into();
                    }
                    Err(error) => {
                        view.history_sync_snapshot = None;
                        view.history_sync_status = format!("History settings unavailable: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn change_shared_history_sync(&mut self, change: HistorySyncChange, cx: &mut Context<Self>) {
        if self.history_sync_busy || self.history_sync_loading {
            return;
        }
        let Some(snapshot) = self.history_sync_snapshot.as_ref() else {
            self.history_sync_status = "Refresh the Runtime Host settings first".into();
            cx.notify();
            return;
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.history_sync_status = "Alera CLI unavailable".into();
            cx.notify();
            return;
        };
        let revision = snapshot.revision;
        let generation = self.workspace_generation;
        self.history_sync_busy = true;
        self.history_sync_request = self.history_sync_request.wrapping_add(1);
        let request = self.history_sync_request;
        self.history_sync_status = "Saving shared history policy on Runtime Host…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match change {
                        HistorySyncChange::Mode(mode) => {
                            bridge.update_history_sync_mode(revision, mode)
                        }
                        HistorySyncChange::Group(account, group) => {
                            bridge.update_history_sync_group(revision, &account, &group)
                        }
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.history_sync_request != request {
                    return;
                }
                view.history_sync_busy = false;
                match result {
                    Ok(snapshot) => {
                        view.history_sync_snapshot = Some(snapshot);
                        view.history_sync_status =
                            "Saved to the shared Runtime Host (Classic and GPUI)".into();
                        view.history_sync_edit = None;
                        view.history_sync_input
                            .update(cx, |input, cx| input.clear(cx));
                    }
                    Err(error) => {
                        // Keep the edit and last visible value, but never silently
                        // overwrite a concurrent Classic/GPUI settings update.
                        view.history_sync_status =
                            format!("Not saved: {error}. Refresh and retry.");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn begin_shared_history_group_edit(
        &mut self,
        account: HistorySyncAccount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history_sync_loading || self.history_sync_busy {
            return;
        }
        let value = self
            .history_sync_snapshot
            .as_ref()
            .map(|snapshot| snapshot.group_for(&account))
            .unwrap_or("");
        let input = self.history_sync_input.clone();
        input.update(cx, |input, cx| {
            input.set_query(value, cx);
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.history_sync_edit = Some(account);
        cx.notify();
    }

    fn save_shared_history_group(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.history_sync_edit.clone() else {
            return;
        };
        let input = self.history_sync_input.read(cx);
        if input.composing() {
            self.history_sync_status = "Finish IME composition before saving group".into();
            cx.notify();
            return;
        }
        let group = input.query().trim().to_owned();
        if group.len() > 64 || group.chars().any(char::is_control) {
            self.history_sync_status =
                "Group name must be 64 bytes or fewer without control characters".into();
            cx.notify();
            return;
        }
        self.change_shared_history_sync(HistorySyncChange::Group(account, group), cx);
    }

    pub(super) fn render_history_sync_settings(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Shared Agent History (Runtime Host)"))
                    .child(
                        native_components::action_button(
                            "native-history-sync-refresh", "Refresh", false, false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.refresh_shared_history_sync(cx);
                        })),
                    ),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "This setting is shared with Classic. Account authentication and Native UI preferences stay separate.",
            ))
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                if self.history_sync_busy {
                    "Saving to Host…"
                } else if self.history_sync_loading {
                    "Loading from Host…"
                } else {
                    self.history_sync_status.as_str()
                }.to_owned()
            ));
        let Some(snapshot) = self.history_sync_snapshot.as_ref() else {
            return content
                .child(div().p_3().bg(rgb(PANEL)).text_sm()
                    .child("Connect an existing Runtime Host and click Refresh. No local fallback will be written."))
                .into_any_element();
        };
        let pending_google_accounts = snapshot
            .accounts
            .iter()
            .filter(|account| account.provider == "agy" && account.account_id != "default")
            .count();
        if pending_google_accounts > 0 {
            content = content.child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "{pending_google_accounts} additional Google account registration(s) are pending a verified isolated login. Quota refresh, AGY history mirroring and account switching remain disabled."
            )));
        }
        if let Some(archive) = &snapshot.archive_summary {
            content = content.child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "Host-private updated history snapshots: {} sessions, {} versions{}",
                archive.sessions,
                archive.versions,
                if archive.truncated {
                    " (limited scan)"
                } else {
                    ""
                }
            )));
        }
        let modes = [
            (
                HistorySyncMode::Separate,
                "Keep Separate (default)",
                "Accounts do not share future history. Previously copied history is not deleted.",
            ),
            (
                HistorySyncMode::All,
                "Sync All (opt-in)",
                "Explicitly share completed native transcripts across same-provider accounts.",
            ),
            (
                HistorySyncMode::Groups,
                "Sync by Group (opt-in)",
                "Only accounts with the same nonempty group share history.",
            ),
        ];
        content =
            content
                .child(div().text_sm().child("Sharing mode"))
                .children(modes.into_iter().enumerate().map(
                    |(index, (mode, title, description))| {
                        let selected = snapshot.history_sync.mode == mode;
                        div()
                            .id(("native-history-sync-mode", index))
                            .cursor_pointer()
                            .rounded_md()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .bg(rgb(if selected { ACTIVE } else { PANEL }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(format!("{} {title}", if selected { "●" } else { "○" })),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).child(description))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if !selected {
                                    view.change_shared_history_sync(
                                        HistorySyncChange::Mode(mode),
                                        cx,
                                    );
                                }
                            }))
                    },
                ));
        if snapshot.history_sync.mode == HistorySyncMode::Groups {
            content = content
                .child(div().text_sm().child("Account groups"))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Assign the same group name to accounts of the same Agent. Empty means independent.",
                ));
            content = content.children(snapshot.accounts.iter().cloned().enumerate().map(
                |(index, account)| {
                    let group = snapshot.group_for(&account).to_owned();
                    let selected = self
                        .history_sync_edit
                        .as_ref()
                        .is_some_and(|editing| editing == &account);
                    let for_edit = account.clone();
                    let can_mirror = account.supported;
                    let mut row = div()
                        .id(("native-history-sync-account", index))
                        .rounded_md()
                        .p_3()
                        .bg(rgb(PANEL))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(div().text_sm().child(account.label))
                                        .child(div().text_xs().text_color(rgb(MUTED)).child(
                                            format!(
                                                "Group: {}",
                                                if group.is_empty() {
                                                    "Independent"
                                                } else {
                                                    &group
                                                }
                                            ),
                                        ))
                                        .child(div().text_xs().text_color(rgb(MUTED)).child(
                                            if can_mirror {
                                                "Native transcript mirroring available"
                                            } else {
                                                "Native AGY history mirroring disabled; group labels are metadata only"
                                            }
                                        )),
                                )
                                .child(
                                    native_components::action_button(
                                        ("native-history-sync-edit", index),
                                        "Edit group",
                                        false,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        move |view, _, window, cx| {
                                            view.begin_shared_history_group_edit(
                                                for_edit.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                ),
                        );
                    if selected {
                        row = row.child(self.history_sync_input.clone()).child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    native_components::action_button(
                                        ("native-history-sync-save", index),
                                        "Save",
                                        false,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        |view, _, _, cx| {
                                            view.save_shared_history_group(cx);
                                        },
                                    )),
                                )
                                .child(
                                    native_components::action_button(
                                        ("native-history-sync-cancel", index),
                                        "Cancel",
                                        false,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        |view, _, _, cx| {
                                            view.history_sync_edit = None;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        );
                    }
                    row
                },
            ));
        }
        content
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "Codex/Claude share quiet native transcripts. Continued conversations are archived as private Alera snapshots without overwriting native files; snapshots may contain sensitive chat text. AGY mirroring and cross-account Resume need verification.",
            ))
            .into_any_element()
    }
}
