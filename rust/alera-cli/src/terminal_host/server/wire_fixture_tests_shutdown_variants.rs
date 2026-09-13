//! Live verification of shutdown and restart edge response fixtures.

use std::collections::HashMap;

use serde_json::json;

use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::session::Session;

use super::super::actor_test_harness::{local_client, test_actor};
use super::super::wire_fixture_tests_lifecycle::{assert_fixture, exchange, wire_fixture};

#[tokio::test]
async fn host_restart_busy_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), Session::driver_test_stub("s1", 80, 24))]),
    )
    .await;
    let (response, events, markers) = exchange(
        &mut actor,
        1,
        &wire_fixture("request.host_restart.busy.json"),
        &mut receiver,
    )
    .await;
    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(
        &response,
        &wire_fixture("response.error.host_restart_busy.json"),
    );
}

#[tokio::test]
async fn forced_host_restart_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), Session::driver_test_stub("s1", 80, 24))]),
    )
    .await;
    let (response, events, markers) = exchange(
        &mut actor,
        1,
        &wire_fixture("request.host_restart.force.json"),
        &mut receiver,
    )
    .await;
    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(
        &response,
        &wire_fixture("response.ok.host_restart.force.json"),
    );
}

#[tokio::test]
async fn unauthenticated_shutdown_and_restart_responses_match_shared_fixtures() {
    for (id, request_type) in [(42, "host.shutdown"), (43, "host.restart")] {
        let dir = tempfile::tempdir().unwrap();
        let (handle, mut receiver) = ClientHandle::test_channels();
        let mut client = local_client(handle);
        client.authenticated = false;
        let mut actor = test_actor(&dir, HashMap::from([(1, client)]), HashMap::new()).await;
        let request_name = if request_type == "host.shutdown" {
            "request.host_shutdown.unauthenticated.json"
        } else {
            "request.host_restart.unauthenticated.json"
        };
        let response_name = if request_type == "host.shutdown" {
            "response.error.host_shutdown.unauthenticated.json"
        } else {
            "response.error.host_restart.unauthenticated.json"
        };
        let request = wire_fixture(request_name);
        assert_eq!(request["id"], id);
        let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;
        assert!(events.is_empty());
        assert!(markers.is_empty());
        assert_fixture(&response, &wire_fixture(response_name));
    }
}

#[tokio::test]
async fn shutdown_and_restart_count_a_queued_mutation_in_shared_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.park_runtime_mutations();
    actor
        .handle_line(
            1,
            json!({
                "id": 90,
                "type": "tab.remove",
                "payload": {"id": "missing-tab"}
            })
            .to_string(),
        )
        .await;
    for (request_name, response_name) in [
        (
            "request.host_shutdown.mutation_busy.json",
            "response.error.host_shutdown.mutation_busy.json",
        ),
        (
            "request.host_restart.mutation_busy.json",
            "response.error.host_restart.mutation_busy.json",
        ),
    ] {
        let request = wire_fixture(request_name);
        let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;
        assert!(events.is_empty());
        assert!(markers.is_empty());
        assert_fixture(&response, &wire_fixture(response_name));
    }
}
