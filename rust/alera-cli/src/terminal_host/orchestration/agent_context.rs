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
        if next.goal.is_empty() && !prompt.is_empty() {
            next.goal = prompt.clone();
            next.record("goal", clean(&prompt, EVENT_LIMIT), at);
        } else if !prompt.is_empty()
            && previous_prompt.is_some_and(|old| clean(old, GOAL_LIMIT) != prompt)
        {
            // An iterative instruction is a milestone, not a silent rewrite
            // of the overarching objective.
            next.record("instruction", clean(&prompt, EVENT_LIMIT), at);
        }
        if let Some(message) = assistant {
            let report = clean(message, PROGRESS_LIMIT);
            if !report.is_empty()
                && previous_assistant.is_none_or(|old| clean(old, PROGRESS_LIMIT) != report)
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
                next.record("report", clean(&report, EVENT_LIMIT), at);
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
    fn followup_preserves_goal_and_report_progress_is_idempotent() {
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
        assert_eq!(next.goal, first.goal);
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
