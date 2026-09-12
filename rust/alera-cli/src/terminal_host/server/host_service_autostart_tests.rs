use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::DeferredAdmission;
use super::ServerActor;
use crate::automation_autostart::{test_override, AutostartPaths};
use crate::terminal_host::client::{ClientFrame, ClientHandle};

async fn next_response(responses: &mut UnboundedReceiver<ClientFrame>, request_id: i64) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = responses.recv().await.unwrap().as_json().unwrap();
            if response["id"] == request_id {
                return response;
            }
        }
    })
    .await
    .expect("a response should arrive")
}

async fn update_automation(
    actor: &mut ServerActor,
    responses: &mut UnboundedReceiver<ClientFrame>,
    autostart: bool,
) -> Value {
    actor
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
    next_response(responses, 1).await
}

async fn status_is_answered(
    actor: &mut ServerActor,
    responses: &mut UnboundedReceiver<ClientFrame>,
) {
    actor
        .handle_line(
            1,
            json!({"id": 2, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = next_response(responses, 2).await;
    assert_eq!(status["ok"], true);
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
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let paths = AutostartPaths {
        file: dir.path().join("startup").join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    };
    let _override = test_override::begin(paths.clone());

    let response = update_automation(&mut actor, &mut responses, true).await;
    assert_eq!(response["ok"], true, "{response}");
    assert!(
        !paths.file.exists(),
        "autostart reconcile must not run on the actor mailbox"
    );
    status_is_answered(&mut actor, &mut responses).await;

    actor.deferred_admission.add_test_permits(1);
    wait_for_path(&paths.file, true).await;
    assert_eq!(std::fs::read_to_string(&paths.file).unwrap(), "enabled");
}

#[tokio::test]
async fn autostart_reconcile_failure_does_not_fail_the_persisted_update() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let _override = test_override::begin(AutostartPaths {
        file: blocker.join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    });

    let response = update_automation(&mut actor, &mut responses, true).await;
    assert_eq!(
        response["ok"], true,
        "the update owns the persisted setting; reconcile is best-effort: {response}"
    );
    assert_eq!(
        response["payload"]["automation"]["autostart"], true,
        "{response}"
    );
    let persisted = actor.runtime_store.automation_settings().await.unwrap();
    assert!(persisted.autostart);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(blocker.is_file());
}

#[tokio::test]
async fn disabled_autostart_removes_the_login_entry_off_mailbox() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let paths = AutostartPaths {
        file: dir.path().join("startup").join("alera-autostart.cmd"),
        content: "enabled".to_string(),
    };
    std::fs::create_dir_all(paths.file.parent().unwrap()).unwrap();
    std::fs::write(&paths.file, b"enabled").unwrap();
    let _override = test_override::begin(paths.clone());

    let response = update_automation(&mut actor, &mut responses, false).await;
    assert_eq!(response["ok"], true, "{response}");
    assert!(
        paths.file.exists(),
        "autostart removal must not run on the actor mailbox"
    );

    actor.deferred_admission.add_test_permits(1);
    wait_for_path(&paths.file, false).await;
}
