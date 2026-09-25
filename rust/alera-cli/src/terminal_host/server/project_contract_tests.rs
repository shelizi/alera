//! Operation-contract coverage for `project.*` requests: the matrix in
//! `docs/project-operation-contract.md` names the replay and recovery
//! guarantees each operation keeps. These tests pin the cells the deferred
//! request suite does not already cover.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use alera_core::runtime::{ProjectCloneJob, ProjectCloneJobPhase, ProjectCloneJobStatus};
use chrono::Utc;
use serde_json::{json, Value};

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::DeferredAdmission;
use super::runtime_mutations::{run_runtime_mutation, RuntimeMutationRequest};
use crate::project_management::prepare_project_registration;
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::host_error::HostError;

fn stored_clone_job(
    id: &str,
    parent_path: &str,
    directory_name: &str,
    status: ProjectCloneJobStatus,
) -> ProjectCloneJob {
    let now = Utc::now();
    ProjectCloneJob {
        id: id.to_string(),
        source: "https://example.invalid/repo.git".to_string(),
        parent_path: parent_path.to_string(),
        directory_name: directory_name.to_string(),
        destination_path: Path::new(parent_path)
            .join(directory_name)
            .to_string_lossy()
            .to_string(),
        project_name: None,
        status,
        phase: ProjectCloneJobPhase::Cloning,
        progress_percent: None,
        message: None,
        error: None,
        project_id: None,
        workspace_id: None,
        created_at: now,
        updated_at: now,
        finished_at: status.is_terminal().then_some(now),
    }
}

async fn read_response(
    responses: &mut tokio::sync::mpsc::UnboundedReceiver<crate::terminal_host::client::ClientFrame>,
    request_id: i64,
) -> Value {
    for _ in 0..8 {
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), responses.recv())
            .await
            .expect("response should arrive")
            .unwrap()
            .as_json()
            .unwrap();
        if response["id"] == request_id {
            return response;
        }
    }
    panic!("no response for request {request_id}");
}

#[tokio::test]
async fn project_register_retry_returns_the_same_project() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let mut project_ids = Vec::new();
    for request_id in [10, 11] {
        let prepared =
            prepare_project_registration(project_dir.path().to_str().unwrap(), Some("Demo"))
                .unwrap();
        actor
            .finish_project_registration(1, request_id, Ok(prepared))
            .await;
        let response = read_response(&mut responses, request_id).await;
        assert_eq!(response["ok"], true, "register {request_id}: {response}");
        project_ids.push(response["payload"]["project"]["id"].clone());
    }
    assert_eq!(project_ids[0], project_ids[1]);

    let projects = actor.runtime_store.list_projects().await.unwrap();
    assert_eq!(projects.len(), 1, "a retried register must not duplicate");
}

#[tokio::test]
async fn project_remove_is_idempotent_for_retries() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let registration = crate::project_management::register_project(
        &actor.runtime_store,
        project_dir.path().to_str().unwrap(),
        Some("Demo"),
    )
    .await
    .unwrap();

    for _ in 0..2 {
        let outcome = run_runtime_mutation(
            actor.runtime_store.clone(),
            RuntimeMutationRequest::RemoveProject {
                project_id: registration.project.id.clone(),
            },
        )
        .await;
        assert!(
            outcome.result.is_ok(),
            "repeated project.remove should stay a no-op success"
        );
    }
    assert!(actor
        .runtime_store
        .list_projects()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn clone_cancel_on_a_terminal_job_returns_the_stored_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let now = Utc::now();
    let job = actor
        .runtime_store
        .insert_project_clone_job(ProjectCloneJob {
            id: "job-1".to_string(),
            source: "https://example.invalid/repo.git".to_string(),
            parent_path: dir.path().to_string_lossy().to_string(),
            directory_name: "repo".to_string(),
            destination_path: dir.path().join("repo").to_string_lossy().to_string(),
            project_name: None,
            status: ProjectCloneJobStatus::Completed,
            phase: ProjectCloneJobPhase::Registering,
            progress_percent: Some(100),
            message: None,
            error: None,
            project_id: None,
            workspace_id: None,
            created_at: now,
            updated_at: now,
            finished_at: Some(now),
        })
        .await
        .unwrap();

    let response = actor
        .project_clone_cancel_request(&json!({ "id": job.id }))
        .await
        .unwrap();
    assert_eq!(
        response["id"].as_str(),
        Some(job.id.as_str()),
        "cancelling a finished clone should return the job, not error"
    );
}

#[tokio::test]
async fn clone_start_rejects_non_terminal_same_destination_but_allows_other_destinations() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(1, 4, 0));
    let parent_path = dunce::canonicalize(dir.path())
        .unwrap()
        .to_string_lossy()
        .to_string();
    let existing = stored_clone_job(
        "pending-job",
        &parent_path,
        "repo",
        ProjectCloneJobStatus::Queued,
    );
    actor
        .runtime_store
        .insert_project_clone_job(existing)
        .await
        .unwrap();

    let duplicate = actor
        .project_clone_start_request(&json!({
            "url": "https://example.invalid/repo.git",
            "parentPath": parent_path,
            "directoryName": "repo",
        }))
        .await;
    match duplicate {
        Err(HostError::Conflict { code, details, .. }) => {
            assert_eq!(code, "project_clone_destination_conflict");
            assert_eq!(details["parentPath"], parent_path);
            assert_eq!(details["directoryName"], "repo");
            assert_eq!(details["existingJobId"], "pending-job");
        }
        other => panic!("expected a typed clone-destination conflict, got {other:?}"),
    }

    let started = actor
        .project_clone_start_request(&json!({
            "url": "https://example.invalid/repo.git",
            "parentPath": parent_path,
            "directoryName": "other-repo",
        }))
        .await
        .unwrap();
    assert_eq!(started["directoryName"], "other-repo");
    assert_eq!(started["status"], "queued");
}

#[tokio::test]
async fn clone_start_allows_terminal_job_for_same_destination() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(1, 4, 0));
    let parent_path = dunce::canonicalize(dir.path())
        .unwrap()
        .to_string_lossy()
        .to_string();
    actor
        .runtime_store
        .insert_project_clone_job(stored_clone_job(
            "completed-job",
            &parent_path,
            "repo",
            ProjectCloneJobStatus::Completed,
        ))
        .await
        .unwrap();

    let started = actor
        .project_clone_start_request(&json!({
            "url": "https://example.invalid/repo.git",
            "parentPath": parent_path,
            "directoryName": "repo",
        }))
        .await
        .unwrap();
    assert_eq!(started["directoryName"], "repo");
    assert_eq!(started["status"], "queued");
}

#[tokio::test]
async fn clone_start_marks_job_failed_when_deferred_admission_rejects() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = Arc::new(DeferredAdmission::with_limits(1, 0, 0));
    let parent_path = dir.path().to_string_lossy().to_string();

    let result = actor
        .project_clone_start_request(&json!({
            "url": "https://example.invalid/repo.git",
            "parentPath": parent_path,
            "directoryName": "repo",
        }))
        .await;
    match result {
        Err(HostError::Conflict { code, .. }) => {
            assert_eq!(code, "deferred_request_backpressure");
        }
        other => panic!("expected deferred-admission rejection, got {other:?}"),
    }
    assert!(actor.project_clone_jobs.is_empty());

    let rows = actor.runtime_store.list_project_clone_jobs().await.unwrap();
    assert_eq!(rows.len(), 1);
    let failed = actor
        .runtime_store
        .find_project_clone_job(&rows[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, ProjectCloneJobStatus::Failed);
    assert_eq!(failed.message.as_deref(), Some("Clone Rejected"));
    assert_eq!(
        failed.error.as_deref(),
        Some("The runtime host is busy. Retry the request.")
    );
}
