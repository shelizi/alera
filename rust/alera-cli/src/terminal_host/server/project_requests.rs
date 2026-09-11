use std::process::Stdio;

use alera_core::child_process::windowless_async_command;
use alera_core::git as core_git;
use alera_core::runtime::{
    ProjectCloneJob, ProjectCloneJobPhase, ProjectCloneJobStatus, ProjectConfig, RuntimeStore,
};
use chrono::Utc;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::project_management::{
    commit_project_registration, effective_project_config, host_directory_roots,
    list_host_directory, prepare_project_registration, register_project,
    remove_owned_clone_destination, rename_project, validate_clone_destination,
    PreparedProjectRegistration,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::{ServerActor, ServerCommand};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRegisterRequest {
    path: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRenameRequest {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCloneStartRequest {
    url: String,
    parent_path: String,
    directory_name: String,
    name: Option<String>,
}

impl ServerActor {
    pub(super) fn start_project_registration(
        &self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let request: ProjectRegisterRequest = parse(payload)?;
        let path = request.path;
        let name = request.name;
        let inbox = self.inbox.clone();
        let slots = self.deferred_request_slots.clone();
        tokio::spawn(async move {
            let _permit = slots
                .acquire_owned()
                .await
                .expect("deferred request semaphore must remain open");
            let result = tokio::task::spawn_blocking(move || {
                prepare_project_registration(&path, name.as_deref()).map_err(state_error)
            })
            .await
            .unwrap_or_else(|error| {
                Err(HostError::state(format!(
                    "Project registration preparation failed: {error}"
                )))
            });
            let _ = inbox.send(ServerCommand::ProjectRegistrationPrepared {
                client_id,
                request_id,
                result,
            });
        });
        Ok(())
    }

    pub(super) async fn finish_project_registration(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<PreparedProjectRegistration>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        let result = match result {
            Ok(prepared) => commit_project_registration(&self.runtime_store, prepared)
                .await
                .map_err(state_error)
                .and_then(|registration| serde_json::to_value(registration).map_err(state_error)),
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => {
                self.broadcast_project_state_changed();
                self.client_write(client_id, ok_response(request_id, value));
            }
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }

    pub(super) async fn project_register_request(&mut self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRegisterRequest = parse(payload)?;
        let result = register_project(&self.runtime_store, &request.path, request.name.as_deref())
            .await
            .map_err(state_error)?;
        self.broadcast_project_state_changed();
        serde_json::to_value(result).map_err(state_error)
    }

    pub(super) async fn project_rename_request(&mut self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRenameRequest = parse(payload)?;
        let project = rename_project(&self.runtime_store, &request.id, &request.name)
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("projectsChanged", json!({})));
        serde_json::to_value(project).map_err(state_error)
    }

    pub(super) async fn project_remove_preview_request(
        &self,
        payload: &Value,
    ) -> HostResult<Value> {
        let id = string_key(payload, "id")?;
        let workspaces = self
            .runtime_store
            .list_workspaces(&id)
            .await
            .map_err(state_error)?;
        let mut tab_count = 0usize;
        let mut session_count = 0usize;
        for workspace in &workspaces {
            tab_count += self
                .runtime_store
                .list_workspace_tabs(&workspace.id)
                .await
                .map_err(state_error)?
                .len();
            session_count += self
                .sessions
                .values()
                .filter(|session| session.workspace_id() == workspace.id)
                .count();
        }
        let has_config_override = self
            .runtime_store
            .find_project_config(&id)
            .await
            .map_err(state_error)?
            .is_some();
        Ok(json!({
            "projectId": id,
            "workspaceCount": workspaces.len(),
            "tabCount": tab_count,
            "activeSessionCount": session_count,
            "hasConfigOverride": has_config_override,
        }))
    }

    pub(super) async fn project_effective_config_request(
        &self,
        payload: &Value,
    ) -> HostResult<Value> {
        let project_id = string_key(payload, "projectId")?;
        load_effective_project_config(self.runtime_store.clone(), project_id).await
    }

    pub(super) fn host_directory_roots_request(&self) -> HostResult<Value> {
        Ok(json!({ "roots": host_directory_roots() }))
    }

    pub(super) fn host_directory_list_request(&self, payload: &Value) -> HostResult<Value> {
        let path = string_key(payload, "path")?;
        serde_json::to_value(list_host_directory(&path).map_err(state_error)?).map_err(state_error)
    }

    pub(super) async fn project_branches_request(&self, payload: &Value) -> HostResult<Value> {
        let project_id = string_key(payload, "projectId")?;
        load_project_branches(self.runtime_store.clone(), project_id).await
    }

    pub(super) async fn project_clone_start_request(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let request: ProjectCloneStartRequest = parse(payload)?;
        let url = request.url.trim();
        if url.is_empty() {
            return Err(HostError::state("Clone URL cannot be empty."));
        }
        let destination = validate_clone_destination(&request.parent_path, &request.directory_name)
            .map_err(state_error)?;
        let parent_path = destination
            .parent()
            .expect("validated clone destination has a parent")
            .to_string_lossy()
            .to_string();
        let destination_path = destination.to_string_lossy().to_string();
        let now = Utc::now();
        let job = ProjectCloneJob {
            id: Uuid::new_v4().to_string(),
            source: sanitized_clone_source(url),
            parent_path,
            directory_name: request.directory_name.trim().to_string(),
            destination_path,
            project_name: request.name.filter(|name| !name.trim().is_empty()),
            status: ProjectCloneJobStatus::Queued,
            phase: ProjectCloneJobPhase::Cloning,
            progress_percent: None,
            message: Some("Waiting To Clone".to_string()),
            error: None,
            project_id: None,
            workspace_id: None,
            created_at: now,
            updated_at: now,
            finished_at: None,
        };
        self.runtime_store
            .insert_project_clone_job(job.clone())
            .await
            .map_err(state_error)?;
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.project_clone_jobs.insert(job.id.clone(), cancel_tx);
        self.cancel_shutdown_timer();
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        let raw_url = url.to_string();
        let task_job = job.clone();
        tokio::spawn(async move {
            run_clone_job(store, inbox.clone(), task_job.clone(), raw_url, cancel_rx).await;
            let _ = inbox.send(ServerCommand::ProjectCloneFinished {
                job_id: task_job.id,
            });
        });
        self.broadcast_authenticated(event("projectCloneJobsChanged", json!({ "id": job.id })));
        serde_json::to_value(job).map_err(state_error)
    }

    pub(super) async fn project_clone_list_request(&self) -> HostResult<Value> {
        serde_json::to_value(
            self.runtime_store
                .list_project_clone_jobs()
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn project_clone_cancel_request(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let id = string_key(payload, "id")?;
        let Some(cancel) = self.project_clone_jobs.remove(&id) else {
            let job = self
                .runtime_store
                .find_project_clone_job(&id)
                .await
                .map_err(state_error)?
                .ok_or_else(|| HostError::state(format!("Clone job not found: {id}")))?;
            if job.status.is_terminal() {
                return serde_json::to_value(job).map_err(state_error);
            }
            return Err(HostError::state(
                "Clone job is not running on this runtime.",
            ));
        };
        let _ = cancel.send(());
        Ok(json!({ "id": id, "cancelling": true }))
    }

    pub(super) async fn reconcile_interrupted_project_clones(&mut self) {
        let jobs = match self
            .runtime_store
            .list_interrupted_project_clone_jobs()
            .await
        {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::warn!("alera project clone recovery unavailable: {error}");
                return;
            }
        };
        for job in jobs {
            let cleanup = remove_owned_clone_destination(&job.parent_path, &job.destination_path);
            let error = match cleanup {
                Ok(()) => "Runtime Restarted During Clone".to_string(),
                Err(error) => format!("Runtime Restarted During Clone; Cleanup Failed: {error}"),
            };
            let _ = self
                .runtime_store
                .update_project_clone_job(
                    &job.id,
                    ProjectCloneJobStatus::Failed,
                    job.phase,
                    job.progress_percent,
                    Some("Clone Interrupted"),
                    Some(&error),
                    None,
                    None,
                )
                .await;
        }
    }

    pub(super) fn handle_project_clone_changed(&self, job_id: String) {
        self.broadcast_authenticated(event("projectCloneJobsChanged", json!({ "id": job_id })));
    }

    pub(super) async fn handle_project_clone_finished(&mut self, job_id: String) {
        self.project_clone_jobs.remove(&job_id);
        self.broadcast_authenticated(event("projectCloneJobsChanged", json!({ "id": job_id })));
        self.broadcast_project_state_changed();
        self.schedule_shutdown_if_idle();
    }

    fn broadcast_project_state_changed(&self) {
        self.broadcast_authenticated(event("projectsChanged", json!({})));
        self.broadcast_workspaces_changed(None);
        self.broadcast_workspace_tabs_changed(None);
    }
}

async fn run_clone_job(
    store: alera_core::runtime::RuntimeStore,
    inbox: tokio::sync::mpsc::UnboundedSender<ServerCommand>,
    job: ProjectCloneJob,
    raw_url: String,
    mut cancel: oneshot::Receiver<()>,
) {
    let _ = store
        .update_project_clone_job(
            &job.id,
            ProjectCloneJobStatus::Running,
            ProjectCloneJobPhase::Cloning,
            None,
            Some("Cloning Repository"),
            None,
            None,
            None,
        )
        .await;
    let _ = inbox.send(ServerCommand::ProjectCloneChanged {
        job_id: job.id.clone(),
    });

    let mut child = match windowless_async_command("git")
        .arg("-C")
        .arg(&job.parent_path)
        .args(["clone", "--progress", "--"])
        .arg(&raw_url)
        .arg(&job.directory_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            finish_failed_clone(&store, &job, &raw_url, &error.to_string()).await;
            return;
        }
    };
    let stderr = child.stderr.take().expect("clone stderr is piped");
    let mut lines = BufReader::new(stderr).lines();
    let progress_pattern = Regex::new(r"(?:Receiving objects|Resolving deltas):\s+(\d+)%").unwrap();
    let mut last_progress = None;
    let mut last_message = "Cloning Repository".to_string();
    loop {
        tokio::select! {
            _ = &mut cancel => {
                let _ = child.kill().await;
                let _ = remove_owned_clone_destination(&job.parent_path, &job.destination_path);
                let _ = store.update_project_clone_job(
                    &job.id,
                    ProjectCloneJobStatus::Cancelled,
                    ProjectCloneJobPhase::Cloning,
                    last_progress,
                    Some("Clone Cancelled"),
                    None,
                    None,
                    None,
                ).await;
                return;
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        last_message = sanitize_error(&line, &raw_url, &job.source);
                        last_progress = progress_pattern
                            .captures(&line)
                            .and_then(|captures| captures.get(1))
                            .and_then(|value| value.as_str().parse::<i64>().ok())
                            .or(last_progress);
                        let _ = store.update_project_clone_job(
                            &job.id,
                            ProjectCloneJobStatus::Running,
                            ProjectCloneJobPhase::Cloning,
                            last_progress,
                            Some(&last_message),
                            None,
                            None,
                            None,
                        ).await;
                        let _ = inbox.send(ServerCommand::ProjectCloneChanged { job_id: job.id.clone() });
                    }
                    Ok(None) => break,
                    Err(error) => {
                        last_message = error.to_string();
                        break;
                    }
                }
            }
        }
    }
    match child.wait().await {
        Ok(status) if status.success() => {}
        Ok(status) => {
            finish_failed_clone(
                &store,
                &job,
                &raw_url,
                &format!(
                    "Git Clone Failed With Exit Code {}: {last_message}",
                    status.code().unwrap_or(-1)
                ),
            )
            .await;
            return;
        }
        Err(error) => {
            finish_failed_clone(&store, &job, &raw_url, &error.to_string()).await;
            return;
        }
    }

    let _ = store
        .update_project_clone_job(
            &job.id,
            ProjectCloneJobStatus::Running,
            ProjectCloneJobPhase::Registering,
            Some(100),
            Some("Registering Project"),
            None,
            None,
            None,
        )
        .await;
    match register_project(&store, &job.destination_path, job.project_name.as_deref()).await {
        Ok(result) => {
            let _ = store
                .update_project_clone_job(
                    &job.id,
                    ProjectCloneJobStatus::Completed,
                    ProjectCloneJobPhase::Registering,
                    Some(100),
                    Some("Project Ready"),
                    None,
                    Some(&result.project.id),
                    Some(&result.main_workspace.id),
                )
                .await;
        }
        Err(error) => finish_failed_clone(&store, &job, &raw_url, &error.to_string()).await,
    }
}

async fn finish_failed_clone(
    store: &alera_core::runtime::RuntimeStore,
    job: &ProjectCloneJob,
    raw_url: &str,
    error: &str,
) {
    let cleanup_error = remove_owned_clone_destination(&job.parent_path, &job.destination_path)
        .err()
        .map(|error| error.to_string());
    let mut error = sanitize_error(error, raw_url, &job.source);
    if let Some(cleanup_error) = cleanup_error {
        error.push_str(&format!("; Cleanup Failed: {cleanup_error}"));
    }
    let _ = store
        .update_project_clone_job(
            &job.id,
            ProjectCloneJobStatus::Failed,
            job.phase,
            job.progress_percent,
            Some("Clone Failed"),
            Some(&error),
            None,
            None,
        )
        .await;
}

pub(super) async fn load_effective_project_config(
    runtime_store: RuntimeStore,
    project_id: String,
) -> HostResult<Value> {
    let result = effective_project_config(&runtime_store, &project_id)
        .await
        .map_err(state_error)?;
    serde_json::to_value(result).map_err(state_error)
}

pub(super) async fn load_project_branches(
    runtime_store: RuntimeStore,
    project_id: String,
) -> HostResult<Value> {
    let project = runtime_store
        .find_project(&project_id)
        .await
        .map_err(state_error)?
        .ok_or_else(|| HostError::state(format!("Project not found: {project_id}")))?;
    tokio::task::spawn_blocking(move || {
        let branches = core_git::list_branches(&project.repo_path).map_err(state_error)?;
        let local_branches = branches
            .iter()
            .filter_map(
                |branch| match core_git::branch_exists(&project.repo_path, branch) {
                    Ok(true) => Some(Ok(branch.clone())),
                    Ok(false) => None,
                    Err(error) => Some(Err(state_error(error))),
                },
            )
            .collect::<HostResult<Vec<String>>>()?;
        Ok(json!({
            "projectId": project.id,
            "branches": branches,
            "localBranches": local_branches,
        }))
    })
    .await
    .unwrap_or_else(|error| {
        Err(HostError::state(format!(
            "Deferred request failed: {error}"
        )))
    })
}

fn sanitized_clone_source(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.to_string()
}

fn sanitize_error(error: &str, raw_url: &str, safe_url: &str) -> String {
    error.replace(raw_url, safe_url)
}

fn parse<T: for<'de> Deserialize<'de>>(payload: &Value) -> HostResult<T> {
    serde_json::from_value(payload.clone()).map_err(state_error)
}

fn string_key(payload: &Value, key: &str) -> HostResult<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| HostError::state(format!("Missing or invalid {key}.")))
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[allow(dead_code)]
fn _assert_project_config_is_serializable(config: ProjectConfig) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Null)
}
