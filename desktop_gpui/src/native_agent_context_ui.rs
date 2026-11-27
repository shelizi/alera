//! Bounded Agent Context overview in the Native GPUI right sidebar.
//! Host inventory is authoritative. No PTY transcript, tool input, or
//! untrusted status-only sidecar text is parsed inside this UI component.
use super::*;
use alera_gpui_desktop::terminal::AgentContextOverview;
use alera_gpui_desktop::visual_tokens::ACCENT;

/// Bounded display projection shared by sidebar Session rows and Tab chips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AgentTaskPresentation {
    pub(super) goal: String,
    pub(super) progress: String,
    pub(super) overall: String,
    pub(super) is_summary: bool,
}

impl AgentTaskPresentation {
    pub(super) fn from_context(context: Option<&AgentContextOverview>) -> Self {
        context.map(|value| Self {
            goal: value.goal.trim().to_owned(),
            progress: value.progress.trim().to_owned(),
            overall: value.overall_summary.trim().to_owned(),
            is_summary: value.is_summary,
        }).unwrap_or_default()
    }

    pub(super) fn title(&self, show: bool, fallback: &str) -> String {
        if show && !self.goal.is_empty() { self.goal.clone() } else { fallback.to_owned() }
    }

    pub(super) fn inline_progress(&self, show: bool) -> Option<String> {
        (show && !self.progress.is_empty()).then(|| self.progress.clone())
    }

    pub(super) fn tooltip(&self, status: &str, locale: UiLocale) -> String {
        let mut text = status.to_owned();
        if !self.goal.is_empty() {
            text.push_str(&format!("\n\n{}: {}", tr(locale, "Overall Objective"), self.goal));
        }
        if !self.overall.is_empty() {
            text.push_str(&format!("\n\n{}: {}", tr(locale, "Overall Progress Summary"), self.overall));
        }
        if !self.progress.is_empty() {
            let label = if self.is_summary { "Latest Report · AI Summary" } else { "Latest Report · Original Reply" };
            text.push_str(&format!("\n\n{}: {}", tr(locale, label), self.progress));
        }
        text
    }
}

pub(super) fn panel(
    context: Option<&AgentContextOverview>,
    expanded: &HashSet<String>,
    selection: Option<&(String, String, String, String)>,
    pending: &HashSet<String>,
    feedback: Option<&str>,
    cx: &mut Context<Workbench>,
) -> gpui::AnyElement {
    let body =
        div()
            .id("agent-context-overview-scroll")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .px_3()
            .py_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Agent Context"),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "AI summaries take precedence. Original replies are shown when no summary is available. Reports are not independently verified.",
            ));

    let Some(context) = context else {
        return body
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("Select a running Agent tab to see its overall task and timeline."),
            )
            .into_any_element();
    };

    let goal = if context.goal.is_empty() {
        "No overall task goal received yet".to_owned()
    } else {
        context.goal.clone()
    };
    let progress = if context.progress.is_empty() {
        "Waiting for an Agent work report".to_owned()
    } else {
        context.progress.clone()
    };
    let latest = context.timeline.iter().find(|event| event.kind == "report");
    let overall = if context.overall_summary.is_empty() {
        div().text_xs().text_color(rgb(MUTED)).child("No cumulative summary yet. Generate from retained reports.").into_any_element()
    } else {
        summary_text("overall".to_owned(), context.overall_summary.clone(), "summary", expanded, cx)
    };
    let mut overall_body = div().flex().flex_col().gap_1().child(overall);
    if latest.is_some() {
        overall_body = overall_body.child(summary_action("overall", selection, None, !context.overall_summary.is_empty(), pending, cx));
    }
    let latest_body = if context.progress.is_empty() {
        div().text_sm().child(progress).into_any_element()
    } else {
        summary_text("latest".to_owned(), progress, if context.is_summary { "summary" } else { "reply" }, expanded, cx)
    };
    let mut latest_body = div().flex().flex_col().gap_1().child(latest_body);
    if let Some(event) = latest {
        latest_body = latest_body.child(summary_action("latest", selection, Some(&event.at), context.is_summary, pending, cx));
    }
    let mut body = body
        .child(section("OVERALL OBJECTIVE", summary_text("goal".to_owned(), goal, "objective", expanded, cx)))
        .when(latest.is_some() || !context.overall_summary.is_empty(), |body| body.child(section("CUMULATIVE PROGRESS SUMMARY", overall_body.into_any_element())))
        .child(section(if context.is_summary { "LATEST AGENT REPORT · AI SUMMARY" } else if context.has_report { "LATEST AGENT REPORT · ORIGINAL REPLY" } else { "LATEST AGENT REPORT" }, latest_body.into_any_element()))
        .when_some(feedback.map(str::to_owned), |body, message| body.child(div().text_xs().text_color(rgb(MUTED)).child(message)))
        .when(!context.is_summary || context.timeline.iter().any(|event| event.kind == "report" && !event.is_summary), |body| body.child(
            div().text_xs().text_color(rgb(MUTED)).child("Enable automatic summaries in Settings → AI Assist → Agent Work Report → Auto-Summarize Agent Work Reports. Generated summaries appear automatically."),
        ))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("TIMELINE · NEWEST FIRST"),
        );
    if context.timeline.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("No meaningful updates yet."),
        );
    } else {
        for (index, event) in context.timeline.iter().enumerate() {
            let label = match event.kind.as_str() {
                "goal" => "Goal",
                "instruction" => "Follow-up instruction",
                "report" if event.is_summary => "Agent report · AI summary",
                "report" => "Agent report · original reply",
                _ => "Update",
            };
            body = body.child(
                div()
                    .id(("agent-context-event", index))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .rounded_md()
                    .bg(rgb(ELEVATED))
                    .px_2()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(event.at.clone()),
                            ),
                    )
                    .child(if event.kind == "report" {
                        if event.text.is_empty() {
                            div().text_xs().text_color(rgb(MUTED)).child("Report received · no summary").into_any_element()
                        } else {
                            summary_text(format!("event-{index}-{}", event.at), event.text.clone(), if event.is_summary { "summary" } else { "reply" }, expanded, cx)
                        }
                    } else {
                        div().min_w_0().text_sm().child(event.text.clone()).into_any_element()
                    })
                    .when(event.kind == "report", |row| row.child(summary_action(&format!("event-{index}"), selection, Some(&event.at), event.is_summary, pending, cx))),
            );
        }
    }
    body.into_any_element()
}

fn summary_action(
    id: &str,
    selection: Option<&(String, String, String, String)>,
    report_at: Option<&str>,
    exists: bool,
    pending: &HashSet<String>,
    cx: &mut Context<Workbench>,
) -> gpui::AnyElement {
    let Some(selection) = selection.filter(|identity| !identity.3.is_empty()).cloned() else {
        return div().into_any_element();
    };
    let report_at = report_at.map(str::to_owned);
    let key = report_at.clone().unwrap_or_else(|| "overall".into());
    let busy = pending.contains(&key);
    let button = div()
        .id(gpui::SharedString::from(format!("agent-context-generate-{id}")))
        .text_xs().text_color(rgb(if busy { MUTED } else { ACCENT }))
        .child(if busy { "Submitting…" } else if exists { "Regenerate summary" } else { "Generate summary" });
    if busy {
        button.into_any_element()
    } else {
        button.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
            this.generate_context_summary(selection.clone(), report_at.clone(), cx);
        })).into_any_element()
    }
}

impl Workbench {
    fn generate_context_summary(&mut self, identity: (String, String, String, String), report_at: Option<String>, cx: &mut Context<Self>) {
        if self.agent_context_selection.as_ref() != Some(&identity) || self.terminals.session_list_is_stale() {
            self.agent_context_feedback = Some("Agent identity changed; refresh the Host inventory".into());
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_context_feedback = Some("Connect the existing Runtime Host to generate a summary".into());
            cx.notify();
            return;
        };
        let key = report_at.clone().unwrap_or_else(|| "overall".into());
        if !self.agent_context_pending.insert(key.clone()) { return; }
        self.agent_context_feedback = Some("Submitting summary request…".into());
        cx.notify();
        let request_identity = identity.clone();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.summarize_agent_context(&request_identity.1, &request_identity.2,
                    &request_identity.0, &request_identity.3, report_at.as_deref())
            }).await;
            view.update(cx, |view, cx| {
                if view.agent_context_selection.as_ref() != Some(&identity) { return; }
                view.agent_context_pending.remove(&key);
                view.agent_context_feedback = Some(match result {
                    Ok(()) => "Summary queued; the report updates when generation completes".into(),
                    Err(error) => format!("Could not queue summary: {error}"),
                });
                view.refresh_terminals(cx);
                cx.notify();
            }).ok();
        }).detach();
    }
}

fn section(title: &'static str, value: gpui::AnyElement) -> gpui::AnyElement {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_2()
        .rounded_md()
        .bg(rgb(ELEVATED))
        .px_3()
        .py_2()
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(value)
        .into_any_element()
}

/// Collapse long generated summaries and original report excerpts; never label raw text as a summary.
fn summary_text(
    id: String,
    value: String,
    label: &'static str,
    expanded: &HashSet<String>,
    cx: &mut Context<Workbench>,
) -> gpui::AnyElement {
    let long = value.chars().count() > 120;
    let key = format!("{id}:{label}:{value}");
    let open = expanded.contains(&key);
    let text = if long && !open {
        format!("{}…", value.chars().take(117).collect::<String>())
    } else {
        value
    };
    // Match Classic: the preview or expanded text comes before its control.
    let mut body = div().min_w_0().flex().flex_col().gap_1()
        .child(div().min_w_0().text_sm().child(text));
    if long {
        body = body.child(
            div()
                .id(gpui::SharedString::from(format!("agent-context-toggle-{id}")))
                .text_xs()
                .text_color(rgb(ACCENT))
                .cursor_pointer()
                .child(format!("{} {label} {}", if open { "Collapse" } else { "Expand" },
                    if open { "▴" } else { "▾" }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.agent_context_expanded.remove(&key) {
                        if this.agent_context_expanded.len() >= 32 {
                            this.agent_context_expanded.clear();
                        }
                        this.agent_context_expanded.insert(key.clone());
                    }
                    cx.notify();
                })),
        );
    }
    body.into_any_element()
}

/// Sidebar summaries are opt-in. Missing, stale, or non-boolean Host values
/// must not display task text in the left project/session list.
pub(super) fn task_summary_sidebar_enabled(
    host_settings_ready: bool,
    value: Option<&serde_json::Value>,
) -> bool {
    host_settings_ready && value.and_then(serde_json::Value::as_bool) == Some(true)
}

#[cfg(test)]
mod task_summary_toggle_tests {
    use super::*;

    #[test]
    fn default_off_and_only_explicit_host_true_enables_summary() {
        assert!(!task_summary_sidebar_enabled(true, None));
        assert!(!task_summary_sidebar_enabled(
            true,
            Some(&serde_json::json!(false))
        ));
        assert!(!task_summary_sidebar_enabled(
            true,
            Some(&serde_json::json!("true"))
        ));
        assert!(!task_summary_sidebar_enabled(
            false,
            Some(&serde_json::json!(true))
        ));
        assert!(task_summary_sidebar_enabled(
            true,
            Some(&serde_json::json!(true))
        ));
    }
}
