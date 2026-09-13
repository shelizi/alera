use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::DeferredAdmission;
use super::{ServerActor, ServerCommand};
use crate::automation_autostart::{test_override, AutostartPaths};
use crate::terminal_host::client::{ClientFrame, ClientHandle};

struct Fixture {
    _root: tempfile::TempDir,
    actor: ServerActor,
    responses: UnboundedReceiver<ClientFrame>,
    commands: UnboundedReceiver<ServerCommand>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let (handle, responses) = ClientHandle::test_channels();
        let mut actor = test_actor(
            &root,
            HashMap::from([(1, local_client(handle))]),
            HashMap::new(),
        )
        .await;
        let (inbox, commands) = tokio::sync::mpsc::unbounded_channel();
        actor.inbox = inbox;
        Self {
            _root: root,
            actor,
            responses,
            commands,
        }
    }

    fn pause_admission(&mut self) {
        self.actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
            usize::MAX,
            usize::MAX,
            0,
        ));
    }

    async fn next_response(&mut self, request_id: i64) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    command = self.commands.recv() => {
                        self.actor.handle(command.unwrap()).await;
                    }
                    frame = self.responses.recv() => {
                        let response = frame.unwrap().as_json().unwrap();
                        if response["id"] == request_id {
                            return response;
                        }
                    }
                }
            }
        })
        .await
        .expect("a response should arrive")
    }

    async fn update_automation(&mut self, autostart: bool) -> Value {
        self.actor
            .handle_line(
                1,
                json!({
                    "id": 1,
                    "type": "runtimeSettings.update",
                    "payload": {"automation": {"autostart": autostart}},
                })
                .to_string(),
            )
            .await;
        self.next_response(1).await
    }

    async fn status_is_answered(&mut self) {
        self.actor
            .handle_line(
                1,
                json!({"id": 2, "type": "status.get", "payload": {}}).to_string(),
            )
            .await;
        let status = self.next_response(2).await;
        assert_eq!(status["ok"], true);
    }
}

async fn wait_for_path(path: &std::path::Path, present: bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while path.exists() != present {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("path {} never reached the expected state", path.display()));
}

#[tokio::test]
async fn autostart_reconcile_runs_off_the_actor_mailbox() {
    let mut fixture = Fixture::new().await;
    fixture.pause_admission();
    let paths = AutostartPaths {
        file: fixture
            ._root
            .path()
            .join("startup")
            .join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    };
    let _override = test_override::begin(paths.clone());

    fixture
        .actor
        .handle_line(
            1,
            json!({
                "id": 1,
                "type": "runtimeSettings.update",
                "payload": {"automation": {"autostart": true}},
            })
            .to_string(),
        )
        .await;

    assert!(
        tokio::time::timeout(Duration::from_millis(50), fixture.responses.recv())
            .await
            .is_err(),
        "runtimeSettings.update reply must wait for reconcile completion under contract C"
    );
    fixture.status_is_answered().await;

    fixture.actor.deferred_admission.add_test_permits(1);
    let response = fixture.next_response(1).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        response["payload"]["automation"]["autostart"], true,
        "{response}"
    );

    wait_for_path(&paths.file, true).await;
    assert_eq!(std::fs::read_to_string(&paths.file).unwrap(), "enabled");
}

#[tokio::test]
async fn autostart_reconcile_failure_returns_error_reply_and_keeps_persisted_setting() {
    let mut fixture = Fixture::new().await;
    let blocker = fixture._root.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let _override = test_override::begin(AutostartPaths {
        file: blocker.join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    });

    let response = fixture.update_automation(true).await;
    assert_eq!(
        response["ok"], false,
        "under contract C reconcile failure returns error reply: {response}"
    );
    let persisted = fixture
        .actor
        .runtime_store
        .automation_settings()
        .await
        .unwrap();
    assert!(
        persisted.autostart,
        "persisted setting must remain true even when reconcile fails"
    );
    assert!(blocker.is_file());
}

#[tokio::test]
async fn disabled_autostart_removes_the_login_entry_off_mailbox() {
    let mut fixture = Fixture::new().await;
    fixture.pause_admission();
    let paths = AutostartPaths {
        file: fixture
            ._root
            .path()
            .join("startup")
            .join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    };
    std::fs::create_dir_all(paths.file.parent().unwrap()).unwrap();
    std::fs::write(&paths.file, b"enabled").unwrap();
    let _override = test_override::begin(paths.clone());

    fixture
        .actor
        .handle_line(
            1,
            json!({
                "id": 1,
                "type": "runtimeSettings.update",
                "payload": {"automation": {"autostart": false}},
            })
            .to_string(),
        )
        .await;

    assert!(
        tokio::time::timeout(Duration::from_millis(50), fixture.responses.recv())
            .await
            .is_err(),
        "disabling autostart reply must wait for reconcile completion under contract C"
    );
    fixture.status_is_answered().await;

    fixture.actor.deferred_admission.add_test_permits(1);
    let response = fixture.next_response(1).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        response["payload"]["automation"]["autostart"], false,
        "{response}"
    );

    wait_for_path(&paths.file, false).await;
}
