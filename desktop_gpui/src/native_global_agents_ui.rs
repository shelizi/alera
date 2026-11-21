//! Global, read-only projection of Host-confirmed Agent sessions.
//!
//! This view is intentionally not tied to the currently selected workspace.
//! It neither starts a Host nor attaches to idle Terminal output. Navigation
//! reuses the existing verified Agent notice / draft-safe workspace switch.
use super::*;

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
        .h(px(262.0))
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
        );
    }
    list.into_any_element()
}
