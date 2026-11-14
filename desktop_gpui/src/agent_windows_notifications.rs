//! Native-only desktop Agent Hook Toast presenter. Windows builds use a
//! different AppUserModelID from Classic; other OSes must explicitly fail.
//! The presenter is *never* allowed to decide delivery ownership itself.

use crate::agent_status_notifications::AgentNotice;
use crate::terminal::AgentActivityState;

fn escape_xml_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len().min(256));
    for character in text.chars().take(160) {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            control if control.is_control() => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Do not put Host IDs, prompt text, ToolInput or process launch commands in
/// Toast XML. Activation uses an in-process callback bound to a typed notice.
/// Native is a separate AUMID; never register Classic's icon/notification ID.
pub fn build_windows_toast_xml(notices: &[AgentNotice]) -> Option<String> {
    if notices.is_empty() || notices.len() > 128 {
        return None;
    }
    if notices
        .iter()
        .any(|notice| notice.state == AgentActivityState::Working)
    {
        return None;
    }
    let title = if notices.len() > 1 {
        format!("{} agents need attention", notices.len())
    } else {
        "Alera Native Agent".to_owned()
    };
    let body = if notices.len() > 1 {
        let waiting = notices
            .iter()
            .filter(|notice| notice.state == AgentActivityState::Waiting)
            .count();
        let blocked = notices
            .iter()
            .filter(|notice| notice.state == AgentActivityState::Blocked)
            .count();
        let done = notices.len() - waiting - blocked;
        format!("{waiting} waiting, {blocked} blocked, {done} finished")
    } else {
        let notice = &notices[0];
        let status = match notice.state {
            AgentActivityState::Working => return None,
            AgentActivityState::Waiting => "is waiting for you",
            AgentActivityState::Blocked => "needs approval or is blocked",
            AgentActivityState::Done => "has finished",
        };
        format!("{} {status}", notice.agent_type)
    };
    Some(format!(
        "<toast duration=\"short\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_xml_text(&title), escape_xml_text(&body)
    ))
}

#[cfg(windows)]
mod windows_toast {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::mpsc::{self, Receiver, Sender};
    use windows::core::{IInspectable, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    /// The UI thread holds this presenter while open; the WinRT activation
    /// callback sends only the typed, identity-checked Host location.
    pub struct WindowsAgentToastPresenter {
        activation_tx: Sender<AgentNotice>,
        activation_rx: Receiver<AgentNotice>,
        live_toasts: VecDeque<ToastNotification>,
    }

    impl WindowsAgentToastPresenter {
        pub fn new() -> Self {
            let (activation_tx, activation_rx) = mpsc::channel();
            Self {
                activation_tx,
                activation_rx,
                live_toasts: VecDeque::new(),
            }
        }

        pub fn show(&mut self, notices: &[AgentNotice]) -> Result<(), String> {
            let xml = build_windows_toast_xml(notices).ok_or("Invalid attention notice")?;
            let doc = XmlDocument::new().map_err(|error| format!("Toast XML: {error}"))?;
            doc.LoadXml(&HSTRING::from(xml))
                .map_err(|error| format!("Toast markup: {error}"))?;
            let toast = ToastNotification::CreateToastNotification(&doc)
                .map_err(|error| format!("Toast creation: {error}"))?;
            // Only an unambiguous single-Agent toast can directly navigate.
            // Grouped notices open Native without secretly selecting a Tab.
            if notices.len() == 1 {
                let target = notices[0].clone();
                let sender = self.activation_tx.clone();
                let on_click =
                    TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
                        let _ = sender.send(target.clone());
                        Ok(())
                    });
                toast
                    .Activated(&on_click)
                    .map_err(|error| format!("Toast activation: {error}"))?;
            }
            // A separate Native AUMID prevents writing to Classic's
            // notification history; Windows may still require a registered
            // Start Menu shortcut, which is an explicit H5 acceptance gate.
            let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
                crate::edition::NATIVE_WINDOWS_APP_ID,
            ))
            .map_err(|error| format!("Native toast identity: {error}"))?;
            notifier
                .Show(&toast)
                .map_err(|error| format!("Windows toast display: {error}"))?;
            self.live_toasts.push_back(toast);
            while self.live_toasts.len() > 16 {
                self.live_toasts.pop_front();
            }
            Ok(())
        }

        pub fn take_activation(&self) -> Option<AgentNotice> {
            self.activation_rx.try_recv().ok()
        }
    }
}

#[cfg(windows)]
pub use windows_toast::WindowsAgentToastPresenter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::AgentActivityState;

    #[test]
    fn windows_toast_xml_has_no_prompt_tool_input_or_unescaped_agent_text() {
        let notes = vec![AgentNotice {
            session_id: "h1".into(),
            workspace_id: "w1".into(),
            tab_id: "t1".into(),
            agent_type: "codex<&\"".into(),
            state: AgentActivityState::Waiting,
        }];
        let xml = build_windows_toast_xml(&notes).expect("safe toast");
        assert!(xml.contains("ToastGeneric"));
        assert!(xml.contains("codex&lt;&amp;&quot;"));
        assert!(!xml.contains("codex<&\""));
        assert!(!xml.contains("prompt"));
        assert!(!xml.contains("toolInput"));
        assert!(!xml.contains("API_KEY"));
        assert!(
            !xml.contains("sessionId="),
            "no raw Host identity in Windows toast launch"
        );
        assert_eq!(
            crate::edition::NATIVE_WINDOWS_APP_ID,
            "dev.leynier.alera.native"
        );
        assert_ne!(
            crate::edition::NATIVE_WINDOWS_APP_ID,
            crate::edition::CLASSIC_WINDOWS_APP_ID
        );
    }

    #[test]
    fn toast_only_accepts_one_or_more_bounded_attention_items() {
        assert!(build_windows_toast_xml(&[]).is_none());
        let mut notes = vec![];
        for index in 0..129 {
            notes.push(AgentNotice {
                session_id: format!("h{index}"),
                workspace_id: "w".into(),
                tab_id: format!("t{index}"),
                agent_type: "codex".into(),
                state: AgentActivityState::Blocked,
            });
        }
        assert!(
            build_windows_toast_xml(&notes).is_none(),
            "oversized Host batch refused"
        );
        notes.truncate(2);
        let xml = build_windows_toast_xml(&notes).unwrap();
        assert!(
            xml.contains("2 agents"),
            "batch grouped, not 2 separate toasts"
        );
    }
    /// Opt-in only: tests a real Windows notification, but never starts a
    /// Runtime Host, changes Hooks or emits a real Agent status.
    #[cfg(windows)]
    #[test]
    #[ignore = "Requires interactive Windows desktop, registered Native AUMID, and manual Toast click"]
    fn h03_winrt_smoke_toast_show_and_activation_for_isolated_agent() {
        assert_eq!(
            std::env::var("ALERA_GPUI_AGENT_TOAST_SMOKE").as_deref(),
            Ok("1"),
            "opt in explicitly; this test displays a synthetic Windows Toast"
        );
        // The real Native app calls this before starting GPUI. A cargo test
        // harness must explicitly assume the same AppUserModelID; otherwise
        // CreateToastNotifierWithId can return success without OS delivery.
        #[link(name = "shell32")]
        extern "system" {
            fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
        }
        let app_id = crate::edition::NATIVE_WINDOWS_APP_ID
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let hr = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
        assert!(
            hr >= 0,
            "Windows Native test AppUserModelID setup failed: {hr:#x}"
        );

        let notice = AgentNotice {
            session_id: "h03-isolated-session".into(),
            workspace_id: "h03-isolated-workspace".into(),
            tab_id: "h03-isolated-tab".into(),
            agent_type: "H03 test Agent".into(),
            state: AgentActivityState::Waiting,
        };
        let mut presenter = WindowsAgentToastPresenter::new();
        presenter.show(&[notice.clone()]).expect(
            "WinRT Toast show requires a registered Native AUMID and Windows notification permission",
        );
        eprintln!("H03 test: click the synthetic Windows Toast within 45 seconds");
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(45) {
            if let Some(activated) = presenter.take_activation() {
                assert_eq!(
                    activated, notice,
                    "the Toast callback must preserve exact in-process Agent identity"
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        panic!("WinRT reported Toast.show success but no activation callback was received");
    }
}
