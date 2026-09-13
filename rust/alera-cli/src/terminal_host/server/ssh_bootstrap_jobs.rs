use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use alera_core::runtime::{
    SshAuthKind, SshBootstrapStatus, SshTarget, SshTargetBootstrapStateUpdate,
};
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::ssh_bootstrap::{
    cancel_ssh_bootstrap, mark_ssh_bootstrap_installing, new_bootstrap_job_id, run_ssh_bootstrap,
    SshTargetBootstrapJob, SshTargetBootstrapProgress, SshTargetBootstrapRequest,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;

use super::{deferred_admission, ServerActor, ServerCommand, SshBootstrapJobState};

impl ServerActor {
    pub(super) async fn start_ssh_bootstrap_job(
        &mut self,
        client_id: u64,
        request: SshTargetBootstrapRequest,
    ) -> HostResult<Value> {
        if let Some(existing) = self.ssh_bootstrap_jobs.get(&request.target_id) {
            return Ok(json!(SshTargetBootstrapJob {
                job_id: existing.job_id.clone(),
                target_id: existing.target_id.clone(),
                status: existing.status,
            }));
        }
        let target = self
            .runtime_store
            .find_ssh_target(&request.target_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| {
                HostError::state(format!("ssh target not found: {}", request.target_id))
            })?;
        if matches!(target.auth_kind, SshAuthKind::Password) {
            return Err(HostError::state(
                "password SSH targets are not supported for bootstrap; configure SSH agent or key authentication.",
            ));
        }
        let job_id = new_bootstrap_job_id();
        mark_ssh_bootstrap_installing(&self.runtime_store, &target.id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        self.broadcast_authenticated(event(
            "sshTargetBootstrapProgress",
            json!(SshTargetBootstrapProgress {
                job_id: job_id.clone(),
                target_id: target.id.clone(),
                status: SshBootstrapStatus::Installing,
                stage: "auth".to_string(),
                message: "Checking SSH Authentication".to_string(),
                error: None,
            }),
        ));
        self.broadcast_authenticated(event("sshTargetsChanged", json!({})));
        let target_id = request.target_id.clone();
        let store = self.runtime_store.clone();
        let cache_dir = self.runtime_dir.join("runtime-artifacts");
        let inbox = self.inbox.clone();
        let task_job_id = job_id.clone();
        let task_target_id = target_id.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_notify = Arc::new(Notify::new());
        let task_cancel = Arc::clone(&cancel);
        let task_cancel_notify = Arc::clone(&cancel_notify);
        let progress_cancel = Arc::clone(&cancel);
        let progress_inbox = inbox.clone();
        let job = SshBootstrapJobState {
            job_id: job_id.clone(),
            target_id: target_id.clone(),
            status: SshBootstrapStatus::Installing,
            cancel,
            cancel_notify,
        };
        self.ssh_bootstrap_jobs.insert(target_id.clone(), job);
        if let Err(error) = self.deferred_admission.schedule(
            deferred_admission::DeferredRequestClass::Bulk,
            "sshTarget.bootstrap",
            Some(client_id),
            async move {
                if task_cancel.load(Ordering::Acquire) {
                    return;
                }
                // Dropping this future is the cancellation path owned by DeferredAdmission;
                // the SSH/SFTP child processes use kill_on_drop.
                let result = tokio::select! {
                    result = run_ssh_bootstrap(
                        store,
                        cache_dir,
                        request,
                        task_job_id.clone(),
                        move |progress| {
                            if !progress_cancel.load(Ordering::Acquire) {
                                let _ = progress_inbox.send(ServerCommand::SshBootstrapProgress { progress });
                            }
                        },
                    ) => result,
                    _ = super::wait_for_ssh_bootstrap_cancellation(
                        Arc::clone(&task_cancel),
                        Arc::clone(&task_cancel_notify),
                    ) => return,
                };
                if task_cancel.load(Ordering::Acquire) {
                    return;
                }
                let status = if result.is_ok() {
                    SshBootstrapStatus::Installed
                } else {
                    SshBootstrapStatus::Failed
                };
                if task_cancel.load(Ordering::Acquire) {
                    return;
                }
                let _ = inbox.send(ServerCommand::SshBootstrapFinished {
                    target_id: task_target_id,
                    job_id: task_job_id,
                    status,
                });
            },
        ) {
            self.ssh_bootstrap_jobs.remove(&target_id);
            let error_message = error.wire_message();
            let _ = self
                .runtime_store
                .update_ssh_target_bootstrap_state(
                    &target_id,
                    SshTargetBootstrapStateUpdate {
                        status: SshBootstrapStatus::Failed,
                        install_dir: None,
                        runtime_version: None,
                        runtime_platform: None,
                        runtime_arch: None,
                        last_error: Some(&error_message),
                    },
                )
                .await;
            self.broadcast_authenticated(event(
                "sshTargetBootstrapProgress",
                json!(SshTargetBootstrapProgress {
                    job_id: job_id.clone(),
                    target_id: target_id.clone(),
                    status: SshBootstrapStatus::Failed,
                    stage: "failed".to_string(),
                    message: "Remote Runtime Install Failed".to_string(),
                    error: Some(error_message),
                }),
            ));
            self.broadcast_authenticated(event("sshTargetsChanged", json!({})));
            self.schedule_shutdown_if_idle();
            return Err(error);
        }
        self.cancel_shutdown_timer();
        Ok(json!(SshTargetBootstrapJob {
            job_id,
            target_id,
            status: SshBootstrapStatus::Installing,
        }))
    }

    pub(super) async fn cancel_ssh_bootstrap_job(&mut self, target_id: &str) -> HostResult<Value> {
        if let Some(target) = self
            .cancel_active_ssh_bootstrap_job(target_id, "Remote Runtime Install Cancelled")
            .await?
        {
            self.schedule_shutdown_if_idle();
            return Ok(json!(target));
        }
        if let Some(target) = self
            .runtime_store
            .find_ssh_target(target_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
        {
            if target.bootstrap_status == SshBootstrapStatus::Installing {
                let target = self
                    .mark_ssh_bootstrap_cancelled(
                        target_id,
                        new_bootstrap_job_id(),
                        "Stale Remote Runtime Install Cancelled",
                    )
                    .await?;
                self.schedule_shutdown_if_idle();
                return Ok(json!(target));
            }
        }
        Err(HostError::state(format!(
            "No active bootstrap job for SSH target: {target_id}"
        )))
    }

    pub(super) async fn cancel_ssh_bootstrap_job_before_remove(
        &mut self,
        target_id: &str,
    ) -> HostResult<()> {
        self.cancel_active_ssh_bootstrap_job(target_id, "Remote Runtime Install Cancelled")
            .await?;
        self.schedule_shutdown_if_idle();
        Ok(())
    }

    async fn cancel_active_ssh_bootstrap_job(
        &mut self,
        target_id: &str,
        message: &str,
    ) -> HostResult<Option<SshTarget>> {
        let Some(job) = self.ssh_bootstrap_jobs.remove(target_id) else {
            return Ok(None);
        };
        job.cancel.store(true, Ordering::Release);
        job.cancel_notify.notify_one();
        self.mark_ssh_bootstrap_cancelled(target_id, job.job_id, message)
            .await
            .map(Some)
    }

    async fn mark_ssh_bootstrap_cancelled(
        &mut self,
        target_id: &str,
        job_id: String,
        message: &str,
    ) -> HostResult<SshTarget> {
        let target = cancel_ssh_bootstrap(&self.runtime_store, target_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let progress = SshTargetBootstrapProgress {
            job_id,
            target_id: target_id.to_string(),
            status: SshBootstrapStatus::Cancelled,
            stage: "cancelled".to_string(),
            message: message.to_string(),
            error: None,
        };
        self.broadcast_authenticated(event("sshTargetBootstrapProgress", json!(progress)));
        self.broadcast_authenticated(event("sshTargetsChanged", json!({})));
        Ok(target)
    }

    pub(super) fn list_ssh_bootstrap_jobs(&self) -> Value {
        let jobs = self
            .ssh_bootstrap_jobs
            .values()
            .map(|job| {
                json!(SshTargetBootstrapJob {
                    job_id: job.job_id.clone(),
                    target_id: job.target_id.clone(),
                    status: job.status,
                })
            })
            .collect::<Vec<_>>();
        json!(jobs)
    }

    pub(super) fn handle_ssh_bootstrap_progress(&mut self, progress: SshTargetBootstrapProgress) {
        let Some(job) = self.ssh_bootstrap_jobs.get_mut(&progress.target_id) else {
            return;
        };
        if job.job_id != progress.job_id {
            return;
        }
        job.status = progress.status;
        self.broadcast_authenticated(event("sshTargetBootstrapProgress", json!(progress)));
        self.broadcast_authenticated(event("sshTargetsChanged", json!({})));
    }

    pub(super) fn handle_ssh_bootstrap_finished(
        &mut self,
        target_id: String,
        job_id: String,
        status: SshBootstrapStatus,
    ) {
        if self
            .ssh_bootstrap_jobs
            .get(&target_id)
            .is_some_and(|job| job.job_id == job_id)
        {
            self.ssh_bootstrap_jobs.remove(&target_id);
            self.broadcast_authenticated(event(
                "sshTargetBootstrapProgress",
                json!(SshTargetBootstrapProgress {
                    job_id,
                    target_id,
                    status,
                    stage: status.as_str().to_string(),
                    message: match status {
                        SshBootstrapStatus::Installed => "Remote Runtime Installed",
                        SshBootstrapStatus::Failed => "Remote Runtime Install Failed",
                        SshBootstrapStatus::Cancelled => "Remote Runtime Install Cancelled",
                        _ => "Remote Runtime Bootstrap Updated",
                    }
                    .to_string(),
                    error: None,
                }),
            ));
            self.broadcast_authenticated(event("sshTargetsChanged", json!({})));
            self.schedule_shutdown_if_idle();
        }
    }
}
