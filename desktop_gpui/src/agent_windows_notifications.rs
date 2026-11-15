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
    build_windows_toast_xml_with_launch(notices, None)
}

fn build_windows_toast_xml_with_launch(
    notices: &[AgentNotice],
    launch_token: Option<&str>,
) -> Option<String> {
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
    let launch = match launch_token {
        Some(token)
            if token.len() == "alera-native:".len() + 32
                && token.starts_with("alera-native:")
                && token["alera-native:".len()..]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()) =>
        {
            format!(" launch=\"{token}\"")
        }
        Some(_) => return None,
        None => String::new(),
    };
    Some(format!(
        "<toast{launch} duration=\"short\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_xml_text(&title), escape_xml_text(&body)
    ))
}

#[cfg(windows)]
mod windows_toast {
    use super::*;
    use crate::native_toast_activation::NativeToastActivationStore;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};
    use windows::core::{IInspectable, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        NotificationSetting, ToastFailedEventArgs, ToastNotification, ToastNotificationManager,
    };

    /// The UI thread resolves a one-time opaque token against the per-user
    /// DPAPI store, then revalidates the live Host identity before focusing.
    pub struct WindowsAgentToastPresenter {
        activation_tx: Sender<(String, AgentNotice)>,
        activation_rx: Receiver<(String, AgentNotice)>,
        failure_tx: Sender<String>,
        failure_rx: Receiver<String>,
        activation_store: Option<NativeToastActivationStore>,
        cold_start_fallback_after: Option<Instant>,
        activation_tokens: VecDeque<(String, AgentNotice)>,
        live_toasts: VecDeque<ToastNotification>,
    }

    impl WindowsAgentToastPresenter {
        pub fn new() -> Self {
            let state_dir = crate::session::PreviewSessionStore::platform_default()
                .map(|store| store.state_dir().to_path_buf());
            let mut presenter = Self::with_state_dir(state_dir);
            // Only a Windows LocalServer32 cold launch can use the unique
            // pending-notice fallback. First give normal COM a chance to
            // deliver the exact launch token; ordinary app starts never guess.
            let launched_from_toast = std::env::args_os().skip(1).any(|arg| {
                arg.to_string_lossy()
                    .eq_ignore_ascii_case("-ToastActivated")
            });
            if launched_from_toast {
                presenter.cold_start_fallback_after =
                    Instant::now().checked_add(Duration::from_secs(4));
            }
            presenter
        }

        pub fn with_state_dir(state_dir: Option<PathBuf>) -> Self {
            let (activation_tx, activation_rx) = mpsc::channel();
            let (failure_tx, failure_rx) = mpsc::channel();
            Self {
                activation_tx,
                activation_rx,
                failure_tx,
                failure_rx,
                activation_store: state_dir.map(NativeToastActivationStore::new),
                cold_start_fallback_after: None,
                activation_tokens: VecDeque::new(),
                live_toasts: VecDeque::new(),
            }
        }

        pub fn show(&mut self, notices: &[AgentNotice]) -> Result<(), String> {
            // A randomly generated per-toast key is all Windows sees. Never
            // serialize raw Host session/workspace/tab identities in XML.
            let launch_token = if notices.len() == 1 {
                Some(
                    crate::native_toast_activation::new_token()
                        .map_err(|error| format!("Native notice token: {error}"))?,
                )
            } else {
                None
            };
            let xml = build_windows_toast_xml_with_launch(notices, launch_token.as_deref())
                .ok_or("Invalid attention notice")?;
            let doc = XmlDocument::new().map_err(|error| format!("Toast XML: {error}"))?;
            doc.LoadXml(&HSTRING::from(xml))
                .map_err(|error| format!("Toast markup: {error}"))?;
            let toast = ToastNotification::CreateToastNotification(&doc)
                .map_err(|error| format!("Toast creation: {error}"))?;
            // Only an unambiguous single-Agent toast can directly navigate.
            // Grouped notices open Native without secretly selecting a Tab.
            if notices.len() == 1 {
                let target = notices[0].clone();
                let token = launch_token
                    .clone()
                    .expect("single Agent toast has a launch token");
                let sender = self.activation_tx.clone();
                let on_click =
                    TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
                        let _ = sender.send((token.clone(), target.clone()));
                        Ok(())
                    });
                toast
                    .Activated(&on_click)
                    .map_err(|error| format!("Toast activation: {error}"))?;
            }
            let failure_sender = self.failure_tx.clone();
            let on_failure = TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(
                move |_, args| {
                    let code = args
                        .as_ref()
                        .and_then(|event| event.ErrorCode().ok())
                        .map(|value| format!("{value:?}"))
                        .unwrap_or_else(|| "unknown toast error".to_owned());
                    let _ = failure_sender.send(code);
                    Ok(())
                },
            );
            toast
                .Failed(&on_failure)
                .map_err(|error| format!("Toast failure callback: {error}"))?;
            // A separate Native AUMID prevents writing to Classic's
            // notification history; Windows may still require a registered
            // Start Menu shortcut, which is an explicit H5 acceptance gate.
            let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
                crate::edition::NATIVE_WINDOWS_APP_ID,
            ))
            .map_err(|error| format!("Native toast identity: {error}"))?;
            let setting = notifier
                .Setting()
                .map_err(|error| format!("Windows notification setting: {error}"))?;
            if setting != NotificationSetting::Enabled {
                return Err(format!("Windows blocked Native notifications: {setting:?}"));
            }
            let durable_token = if let (Some(token), Some(store)) =
                (launch_token.as_deref(), self.activation_store.as_ref())
            {
                store
                    .store_notice(token, &notices[0])
                    .map_err(|error| format!("Native toast activation store: {error}"))?;
                true
            } else {
                false
            };
            if let Err(error) = notifier.Show(&toast) {
                if durable_token {
                    if let (Some(token), Some(store)) =
                        (launch_token.as_deref(), self.activation_store.as_ref())
                    {
                        let _ = store.remove_notice(token);
                    }
                }
                return Err(format!("Windows toast display: {error}"));
            }
            if !durable_token {
                if let Some(token) = launch_token {
                    self.activation_tokens
                        .push_back((token, notices[0].clone()));
                }
            }
            self.live_toasts.push_back(toast);
            while self.live_toasts.len() > 16 {
                self.live_toasts.pop_front();
            }
            while self.activation_tokens.len() > 16 {
                self.activation_tokens.pop_front();
            }
            Ok(())
        }

        pub fn take_activation(&mut self) -> Option<AgentNotice> {
            if let Ok((token, notice)) = self.activation_rx.try_recv() {
                if let Some(store) = &self.activation_store {
                    let _ = store.remove_notice(&token);
                }
                self.activation_tokens
                    .retain(|(existing, _)| existing != &token);
                return Some(notice);
            }
            // COM can launch a fresh GPUI process after the original one exits.
            // Resolve the opaque launch token through the DPAPI-protected
            // per-user store; a claimed token is consumed exactly once.
            while let Some(token) = crate::native_toast_activation::take_token() {
                if let Some(store) = &self.activation_store {
                    match store.take_notice(&token) {
                        Ok(Some(notice)) => return Some(notice),
                        Ok(None) => continue,
                        Err(error) => {
                            eprintln!("Alera Native: toast activation lookup failed: {error}");
                            continue;
                        }
                    }
                }
                if let Some(index) = self
                    .activation_tokens
                    .iter()
                    .position(|(existing, _)| existing == &token)
                {
                    return self
                        .activation_tokens
                        .remove(index)
                        .map(|(_, notice)| notice);
                }
            }
            if self
                .cold_start_fallback_after
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                self.cold_start_fallback_after = None;
                if let Some(store) = &self.activation_store {
                    match store.take_unique_pending_notice() {
                        Ok(Some(notice)) => {
                            eprintln!("Alera Native: recovered a unique cold-start Windows toast");
                            return Some(notice);
                        }
                        Ok(None) => {
                            eprintln!(
                                "Alera Native: no unique pending toast; not guessing Agent tab"
                            );
                        }
                        Err(error) => {
                            eprintln!("Alera Native: cold-start toast recovery failed: {error}");
                        }
                    }
                }
            }
            None
        }

        /// WinRT may reject a toast asynchronously after Show() returned Ok.
        /// Report only the OS error code, never Host identifiers or prompt text.
        pub fn take_failure(&self) -> Option<String> {
            self.failure_rx.try_recv().ok()
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
    #[test]
    fn h03_toast_launch_contains_only_random_token_and_no_host_identity() {
        let notes = vec![AgentNotice {
            session_id: "h03-secret-session".into(),
            workspace_id: "h03-secret-workspace".into(),
            tab_id: "h03-secret-tab".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        }];
        let token = "alera-native:0123456789abcdef0123456789abcdef";
        let xml = build_windows_toast_xml_with_launch(&notes, Some(token)).expect("safe launch");
        assert!(xml.contains(&format!("launch=\"{token}\"")));
        assert!(!xml.contains("h03-secret"));
        assert!(build_windows_toast_xml_with_launch(&notes, Some("host:sensitive")).is_none());
    }

    /// Opt-in OS end-to-end test. It intentionally does NOT register a COM
    /// class factory in this Cargo process: clicking the toast must launch the
    /// actual Native LocalServer32 EXE, whose new process consumes the DPAPI
    /// token mapping. It never starts a Runtime Host or Agent.
    #[cfg(windows)]
    #[test]
    #[ignore = "Requires interactive Windows desktop, real Native LocalServer32 registration, and manual Action Center click"]
    fn h03_winrt_smoke_toast_show_and_activation_for_isolated_agent() {
        assert_eq!(
            std::env::var("ALERA_GPUI_AGENT_TOAST_SMOKE").as_deref(),
            Ok("1"),
            "opt in explicitly; this test displays a synthetic Windows Toast"
        );
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
        let state_dir = crate::session::PreviewSessionStore::platform_default()
            .expect("per-user Native state directory")
            .state_dir()
            .to_path_buf();
        let store = crate::native_toast_activation::NativeToastActivationStore::new(state_dir);
        let token = crate::native_toast_activation::new_token().expect("opaque launch token");
        store
            .store_notice(&token, &notice)
            .expect("persist DPAPI activation payload before Toast.Show");

        let xml = build_windows_toast_xml_with_launch(&[notice], Some(&token))
            .expect("safe synthetic toast");
        let doc = windows::Data::Xml::Dom::XmlDocument::new().expect("H03 XML document");
        doc.LoadXml(&windows::core::HSTRING::from(xml))
            .expect("H03 Toast XML");
        let toast = windows::UI::Notifications::ToastNotification::CreateToastNotification(&doc)
            .expect("H03 Toast");
        let notifier =
            windows::UI::Notifications::ToastNotificationManager::CreateToastNotifierWithId(
                &windows::core::HSTRING::from(crate::edition::NATIVE_WINDOWS_APP_ID),
            )
            .expect("H03 notifier");
        assert_eq!(
            notifier.Setting().expect("H03 setting"),
            windows::UI::Notifications::NotificationSetting::Enabled,
            "Windows must allow Native Toast delivery"
        );
        notifier.Show(&toast).expect("H03 Toast.Show");

        let history = windows::UI::Notifications::ToastNotificationManager::History()
            .and_then(|history| {
                history.GetHistoryWithId(&windows::core::HSTRING::from(
                    crate::edition::NATIVE_WINDOWS_APP_ID,
                ))
            })
            .and_then(|entries| entries.Size());
        eprintln!("H03 Action Center history after Show: {history:?}");
        eprintln!(
            "H03 test: click the synthetic Waiting Toast; the registered Native EXE must launch and consume the DPAPI token within 60 seconds"
        );
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(60) {
            if !store.contains(&token).expect("check pending DPAPI token") {
                eprintln!("H03 COM LocalServer process consumed the one-time DPAPI token");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let _ = store.remove_notice(&token);
        panic!("Action Center click did not launch Native LocalServer or consume its DPAPI activation token");
    }
}
