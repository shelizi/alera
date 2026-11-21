//! Global, read-only projection of Host-confirmed Agent sessions.
//!
//! This view is intentionally not tied to the currently selected workspace.
//! It neither starts a Host nor attaches to idle Terminal output. Navigation
//! reuses the existing verified Agent notice / draft-safe workspace switch.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GlobalAgentPromptTarget {
    handle: String,
    workspace_id: String,
    tab_id: String,
    agent_type: String,
}

impl GlobalAgentPromptTarget {
    fn is_live(&self, sessions: &[alera_gpui_desktop::terminal::TerminalSession]) -> bool {
        sessions.iter().any(|session| {
            session.running
                && session.handle == self.handle
                && session.workspace_id == self.workspace_id
                && session.tab_id == self.tab_id
                && session.agent_type.as_deref() == Some(self.agent_type.as_str())
        })
    }
}

impl Workbench {
    fn submit_global_agent_prompt(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_submit_busy || self.terminals.session_list_is_stale() {
            self.global_agent_submit_status =
                "Host inventory is stale or another send is in progress".into();
            cx.notify();
            return;
        }
        let Some(target) = self.global_agent_target.clone() else {
            self.global_agent_submit_status = "Choose a verified Agent row first".into();
            cx.notify();
            return;
        };
        if !target.is_live(&self.terminals.sessions) {
            self.global_agent_submit_status = "Agent identity changed. Select it again".into();
            self.global_agent_target = None;
            cx.notify();
            return;
        }
        let prompt = self.global_agent_prompt_input.read(cx).query().to_owned();
        if prompt.trim().is_empty() || prompt.len() > 4096 || prompt.contains('\0') {
            self.global_agent_submit_status = "Prompt must be nonempty and at most 4 KiB".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_submit_status = "Existing Runtime Host CLI is unavailable".into();
            cx.notify();
            return;
        };
        self.global_agent_submit_busy = true;
        self.global_agent_submit_status = "Sending to verified Host Agent…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let sent_target = target.clone();
            let sent_prompt = prompt.clone();
            let result = cx
                .background_spawn(async move {
                    bridge.submit_verified_agent_prompt(
                        &target.handle,
                        &target.workspace_id,
                        &target.tab_id,
                        &prompt,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                view.global_agent_submit_busy = false;
                if view.global_agent_target.as_ref() != Some(&sent_target) {
                    cx.notify();
                    return;
                }
                view.global_agent_submit_status = if result.is_ok() {
                    // Only clear the input if it wasn't edited during the send.
                    view.global_agent_prompt_input.update(cx, |input, cx| {
                        if input.query() == sent_prompt {
                            input.clear(cx);
                        }
                    });
                    "Host acknowledged Agent input".into()
                } else {
                    // The Host might have enqueued before a response was lost.
                    // Never retry automatically.
                    "Delivery unconfirmed. Refresh Host status before retrying".into()
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

pub(super) fn panel(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    let fresh = !view.terminals.session_list_is_stale();
    let sessions = view
        .terminals
        .sessions
        .iter()
        .filter(|session| session.running && session.agent_type.is_some())
        .take(128)
        .cloned()
        .collect::<Vec<_>>();

    let mut list = div()
        .id("global-live-agent-sessions")
        .h(px(340.0))
        .flex_shrink_0()
        .overflow_y_scroll()
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .child(format!("All Agent Sessions ({})", sessions.len())),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Global Host inventory, all registered workspaces"),
        )
        .child(div().text_xs().text_color(rgb(MUTED)).child(
            view.global_agent_target.as_ref().map_or_else(
                || "Select an Agent's Steer button to target its live session".to_owned(),
                |target| format!("Target: {} / {}", target.workspace_id, target.tab_id),
            ),
        ))
        .child(
            div()
                .id("global-agent-launch-current-workspace")
                .cursor_pointer()
                .text_xs()
                .child("+ Agent · current worktree launcher")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.runtime_details_open = true;
                    view.agent_status =
                        "Select a Host Agent profile in the launcher for this worktree".into();
                    cx.notify();
                })),
        )
        .child(view.global_agent_prompt_input.clone())
        .child(
            div()
                .id("global-agent-send-instruction")
                .cursor_pointer()
                .text_xs()
                .text_color(rgb(if view.global_agent_submit_busy {
                    MUTED
                } else {
                    TEXT
                }))
                .child(if view.global_agent_submit_busy {
                    "Sending…"
                } else {
                    "Send instruction to selected Agent"
                })
                .on_click(cx.listener(|view, _, _, cx| {
                    view.submit_global_agent_prompt(cx);
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(view.global_agent_submit_status.clone()),
        );

    if !fresh {
        return list
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("Host inventory is stale. Refresh before focusing an Agent."),
            )
            .into_any_element();
    }

    if sessions.is_empty() {
        return list
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("No running Agent sessions in the connected Host."),
            )
            .into_any_element();
    }

    for (index, session) in sessions.iter().enumerate() {
        let agent = session.agent_type.clone().unwrap_or_default();
        let state = session.agent_state;
        let state_text = if let Some(state) = state {
            if session.interrupted {
                "interrupted".to_owned()
            } else {
                state.label().to_owned()
            }
        } else {
            "state unavailable".to_owned()
        };
        let goal = session
            .agent_context
            .as_ref()
            .map(|context| context.goal.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("No overall goal reported")
            .to_owned();
        let progress = session
            .agent_context
            .as_ref()
            .map(|context| context.progress.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("No progress report")
            .to_owned();
        let session_id = session.handle.clone();
        let workspace_id = session.workspace_id.clone();
        let tab_id = session.tab_id.clone();
        let agent_type = agent.clone();
        let steer_id = session_id.clone();
        let steer_workspace = workspace_id.clone();
        let steer_tab = tab_id.clone();
        let steer_agent = agent_type.clone();

        list = list.child(
            div()
                .id(("global-agent-session-row", index))
                .rounded_md()
                .bg(rgb(ELEVATED))
                .px_2()
                .py_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(format!("{agent} - {state_text}")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("Workspace: {workspace_id}")),
                )
                .child(div().text_sm().child(goal))
                .child(div().text_xs().text_color(rgb(MUTED)).child(progress))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if state.is_some() {
                            "Click to open this Agent"
                        } else {
                            "Waiting for verified Agent state"
                        }),
                )
                .child(
                    div()
                        .id(("global-agent-select-steer", index))
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Steer · select target")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            let target = GlobalAgentPromptTarget {
                                handle: steer_id.clone(),
                                workspace_id: steer_workspace.clone(),
                                tab_id: steer_tab.clone(),
                                agent_type: steer_agent.clone(),
                            };
                            if view.terminals.session_list_is_stale()
                                || !target.is_live(&view.terminals.sessions)
                            {
                                view.global_agent_target = None;
                                view.global_agent_submit_status =
                                    "Agent identity is stale; refresh Host inventory".into();
                            } else {
                                view.global_agent_target = Some(target);
                                view.global_agent_submit_status =
                                    "Target selected; type an instruction above".into();
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id(("global-agent-open", index))
                        .cursor_pointer()
                        .text_xs()
                        .child("Open original Agent tab")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            // Read fresh Host state again; never navigate from a stale
                            // copied row or a recycled handle.
                            if view.terminals.session_list_is_stale() {
                                view.terminals.status =
                                    "Host Agent inventory is stale; refresh before opening".into();
                                cx.notify();
                                return;
                            }
                            let current = view.terminals.sessions.iter().find(|candidate| {
                                candidate.running
                                    && candidate.handle == session_id
                                    && candidate.workspace_id == workspace_id
                                    && candidate.tab_id == tab_id
                                    && candidate.agent_type.as_deref() == Some(agent_type.as_str())
                            });
                            if let Some(current) = current {
                                if let Some(current_state) = current.agent_state {
                                    view.activate_agent_notice(
                                        AgentNotice {
                                            session_id: session_id.clone(),
                                            workspace_id: workspace_id.clone(),
                                            tab_id: tab_id.clone(),
                                            agent_type: agent_type.clone(),
                                            state: current_state,
                                        },
                                        window,
                                        cx,
                                    );
                                } else {
                                    view.terminals.status =
                                        "Agent state is unverified; refresh Host status".into();
                                    cx.notify();
                                }
                            } else {
                                view.terminals.status =
                                    "Global Agent selection no longer matches the Host".into();
                                cx.notify();
                            }
                        })),
                ),
        );
    }
    list.into_any_element()
}
