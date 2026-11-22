//! Bounded Hook-derived task context for one verified Agent session.
//! A report is an observation, not proof of task completion. Only user
//! prompts and Agent reports are retained; tool input/output is ignored.
use chrono::{DateTime, Utc};
use serde::Serialize;

const GOAL_LIMIT: usize = 600;
const PROGRESS_LIMIT: usize = 420;
const EVENT_LIMIT: usize = 260;
const TIMELINE_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextMilestone {
    pub kind: String,
    pub text: String,
    pub at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextOverview {
    pub goal: String,
    pub progress: String,
    /// Task anchor retained across turns, even if the recent focus changes.
    pub initial_goal: String,
    /// Most recent substantive user change; never a mere 'continue' request.
    pub current_focus: String,
    /// Prevent delayed Hook events from rolling back the task focus.
    pub focus_updated_at: Option<String>,
    /// Newest first. The fixed bound avoids growing memory with long sessions.
    pub timeline: Vec<AgentContextMilestone>,
}

fn clean(text: &str, limit: usize) -> String {
    let mut output = String::new();
    let mut pending_space = false;
    let mut count = 0;
    for ch in text.trim().chars() {
        if ch.is_control() || ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() && count < limit {
            output.push(' ');
            count += 1;
        }
        pending_space = false;
        if count >= limit {
            break;
        }
        output.push(ch);
        count += 1;
    }
    output
}

fn substantive_instruction(prompt: &str) -> bool {
    let normalized = prompt
        .trim_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | '，' | ',')
        })
        .to_lowercase();
    !matches!(
        normalized.as_str(),
        "" | "繼續"
            | "繼續吧"
            | "嗯"
            | "好"
            | "好的"
            | "做吧"
            | "進度"
            | "ok"
            | "okay"
            | "continue"
            | "go on"
            | "next"
    )
}

fn changes_overall_objective(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    [
        "改成",
        "改為",
        "新目標",
        "新的目標",
        "改做",
        "switch to",
        "change the goal",
        "instead",
    ]
    .iter()
    .any(|marker| lower.starts_with(marker))
}

fn evolving_objective(base: &str, focus: &str) -> String {
    if focus.is_empty() || focus == base {
        clean(base, GOAL_LIMIT)
    } else {
        format!("{}｜目前重點：{}", clean(base, 370), clean(focus, 200))
            .chars()
            .take(GOAL_LIMIT)
            .collect()
    }
}

/// Compact a model-authored final response without invoking another model.
/// Keep both the response opening and its conclusion so lengthy work reports
/// do not show only boilerplate while dropping the latest verification result.
fn report_excerpt(response: &str, limit: usize) -> String {
    let text = clean(response, 8_000);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text;
    }
    let head = limit * 3 / 5;
    let tail = limit.saturating_sub(head + 3);
    let mut summary: String = chars.iter().take(head).collect();
    summary.push_str(" … ");
    summary.extend(chars[chars.len().saturating_sub(tail)..].iter());
    summary
}

impl AgentContextOverview {
    pub fn observe(
        previous: Option<&Self>,
        previous_prompt: Option<&str>,
        previous_assistant: Option<&str>,
        prompt: &str,
        assistant: Option<&str>,
        at: DateTime<Utc>,
    ) -> Self {
        let mut next = previous.cloned().unwrap_or_default();
        let prompt = clean(prompt, GOAL_LIMIT);
        if next.initial_goal.is_empty() && !next.goal.is_empty() {
            // Reconcile older in-memory views built before the evolving goal.
            next.initial_goal = next.goal.clone();
        }
        if next.initial_goal.is_empty() && !prompt.is_empty() {
            next.initial_goal = prompt.clone();
            next.goal = prompt.clone();
            next.record("goal", clean(&prompt, EVENT_LIMIT), at);
        } else if !prompt.is_empty()
            && previous_prompt.is_none_or(|old| clean(old, GOAL_LIMIT) != prompt)
            && prompt != next.initial_goal
            && prompt != next.current_focus
        {
            if substantive_instruction(&prompt) {
                let latest = next
                    .focus_updated_at
                    .as_deref()
                    .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                if latest.is_none_or(|time| at >= time) {
                    if changes_overall_objective(&prompt) {
                        next.initial_goal = prompt.clone();
                        next.current_focus.clear();
                        next.record("goal", clean(&prompt, EVENT_LIMIT), at);
                    } else {
                        next.current_focus = prompt.clone();
                        next.record("instruction", clean(&prompt, EVENT_LIMIT), at);
                    }
                    next.focus_updated_at = Some(at.to_rfc3339());
                    next.goal = evolving_objective(&next.initial_goal, &next.current_focus);
                } else {
                    next.record("instruction", clean(&prompt, EVENT_LIMIT), at);
                }
            }
        }
        if let Some(message) = assistant {
            let report = report_excerpt(message, PROGRESS_LIMIT);
            if !report.is_empty()
                && previous_assistant.is_none_or(|old| old.trim() != message.trim())
            {
                let newest_report = next
                    .timeline
                    .iter()
                    .filter(|event| event.kind == "report")
                    .filter_map(|event| DateTime::parse_from_rfc3339(&event.at).ok())
                    .map(|event| event.with_timezone(&Utc))
                    .max();
                // A delayed Hook must not replace the newest progress report.
                if newest_report.is_none_or(|previous| at >= previous) {
                    next.progress = report.clone();
                }
                next.record("report", report_excerpt(message, EVENT_LIMIT), at);
            }
        }
        next
    }

    fn record(&mut self, kind: &str, text: String, at: DateTime<Utc>) {
        if text.is_empty() {
            return;
        }
        if self
            .timeline
            .first()
            .is_some_and(|last| last.kind == kind && last.text == text)
        {
            return;
        }
        let index = self
            .timeline
            .iter()
            .position(|event| {
                DateTime::parse_from_rfc3339(&event.at)
                    .ok()
                    .is_none_or(|previous| previous.with_timezone(&Utc) <= at)
            })
            .unwrap_or(self.timeline.len());
        self.timeline.insert(
            index,
            AgentContextMilestone {
                kind: kind.to_owned(),
                text,
                at: at.to_rfc3339(),
            },
        );
        self.timeline.truncate(TIMELINE_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evolving_goal_keeps_original_scope_and_tracks_latest_meaningful_focus() {
        let at = Utc::now();
        let first = AgentContextOverview::observe(
            None,
            None,
            None,
            "讓 GPUI 與 Classic 設定完整對齊",
            None,
            at,
        );
        let next = AgentContextOverview::observe(
            Some(&first),
            Some("讓 GPUI 與 Classic 設定完整對齊"),
            None,
            "修正字體選單與 Terminal 主題",
            Some("字體選單已完成，Terminal 主題待檢查"),
            at + chrono::Duration::minutes(1),
        );
        assert!(next.goal.contains("讓 GPUI 與 Classic 設定完整對齊"));
        assert!(next.goal.contains("修正字體選單與 Terminal 主題"));
        assert_eq!(next.progress, "字體選單已完成，Terminal 主題待檢查");
        assert_eq!(next.timeline[0].kind, "report");
        let continuation = AgentContextOverview::observe(
            Some(&next),
            Some("修正字體選單與 Terminal 主題"),
            Some("字體選單已完成，Terminal 主題待檢查"),
            "繼續",
            None,
            at + chrono::Duration::minutes(2),
        );
        assert_eq!(continuation.goal, next.goal);
        let switch = AgentContextOverview::observe(
            Some(&continuation),
            Some("繼續"),
            None,
            "改成修正 Agent Dashboard 的歷史訊息",
            None,
            at + chrono::Duration::minutes(3),
        );
        assert!(switch.goal.contains("Agent Dashboard"));
        assert!(!switch.goal.contains("Terminal 主題"));
    }

    #[test]
    fn followup_preserves_task_anchor_and_report_progress_is_idempotent() {
        let at = Utc::now();
        let first = AgentContextOverview::observe(
            None,
            None,
            None,
            "對齊 GPUI 與 Classic 的全部設定",
            None,
            at,
        );
        let next = AgentContextOverview::observe(
            Some(&first),
            Some("對齊 GPUI 與 Classic 的全部設定"),
            None,
            "接著修正儲存按鈕",
            Some("已補上共享設定驗證"),
            at,
        );
        assert!(next.goal.contains(&first.goal));
        assert!(next.goal.contains("接著修正儲存按鈕"));
        assert_eq!(next.progress, "已補上共享設定驗證");
        assert_eq!(next.timeline.len(), 3);
        assert_eq!(next.timeline[0].kind, "report");
        let same = AgentContextOverview::observe(
            Some(&next),
            Some("接著修正儲存按鈕"),
            Some("已補上共享設定驗證"),
            "接著修正儲存按鈕",
            Some("已補上共享設定驗證"),
            at,
        );
        assert_eq!(same, next);
    }

    #[test]
    fn long_model_report_excerpt_shows_recent_conclusion_not_just_opening() {
        let at = Utc::now();
        let first_reply = format!(
            "已開始驗收。{}最後結論：第一輪通過",
            "中間處理紀錄".repeat(150)
        );
        let first =
            AgentContextOverview::observe(None, None, None, "驗收專案", Some(&first_reply), at);
        assert!(first.progress.contains("最後結論：第一輪通過"));
        assert!(first.timeline[0].text.contains("最後結論：第一輪通過"));
        let updated_reply = format!(
            "已開始驗收。{}最後結論：第二輪修正完成",
            "中間處理紀錄".repeat(150)
        );
        let second = AgentContextOverview::observe(
            Some(&first),
            Some("驗收專案"),
            Some(&first_reply),
            "驗收專案",
            Some(&updated_reply),
            at + chrono::Duration::seconds(1),
        );
        assert!(second.progress.contains("第二輪修正完成"));
        assert_eq!(second.timeline.len(), 3);
        assert_eq!(second.timeline[0].kind, "report");
    }

    #[test]
    fn late_hook_keeps_newest_report_and_chronological_timeline() {
        let start = Utc::now();
        let later = start + chrono::Duration::minutes(5);
        let first = AgentContextOverview::observe(
            None,
            None,
            None,
            "完成跨版本設定對齊",
            Some("最新進度"),
            later,
        );
        let updated = AgentContextOverview::observe(
            Some(&first),
            Some("完成跨版本設定對齊"),
            Some("最新進度"),
            "完成跨版本設定對齊",
            Some("稍早的回報"),
            start,
        );
        assert_eq!(updated.progress, "最新進度");
        assert_eq!(updated.timeline[0].text, "最新進度");
        assert_eq!(updated.timeline[2].text, "稍早的回報");
    }

    #[test]
    fn bounded_unicode_history_and_empty_events() {
        let at = Utc::now();
        let mut overview = AgentContextOverview::default();
        for i in 0..50 {
            let report = format!("第{i}步：{}", "測".repeat(700));
            overview = AgentContextOverview::observe(
                Some(&overview),
                None,
                None,
                "工作目標",
                Some(&report),
                at,
            );
        }
        assert_eq!(overview.timeline.len(), TIMELINE_LIMIT);
        assert!(overview.progress.chars().count() <= PROGRESS_LIMIT);
        assert!(overview.timeline[0].text.chars().count() <= EVENT_LIMIT);
        let empty = AgentContextOverview::observe(None, None, None, "", None, at);
        assert!(empty.timeline.is_empty());
    }
}
