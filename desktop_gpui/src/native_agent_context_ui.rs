//! Bounded Agent Context overview in the Native GPUI right sidebar.
//! Host inventory is authoritative. No PTY transcript, tool input, or
//! untrusted status-only sidecar text is parsed inside this UI component.
use super::*;
use alera_gpui_desktop::terminal::AgentContextOverview;

pub(super) fn panel(context: Option<&AgentContextOverview>) -> gpui::AnyElement {
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
                "Overall goal follows task changes. Agent replies are excerpts, not independently verified results.",
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
        "Waiting for an Agent progress report".to_owned()
    } else {
        context.progress.clone()
    };
    let mut body = body
        .child(section("OVERALL OBJECTIVE", goal))
        .child(section("LATEST AGENT REPORT", progress))
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
                "report" => "Agent report",
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
                    .child(div().min_w_0().text_sm().child(event.text.clone())),
            );
        }
    }
    body.into_any_element()
}

fn section(title: &'static str, value: String) -> gpui::AnyElement {
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
        .child(div().min_w_0().text_sm().child(value))
        .into_any_element()
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
