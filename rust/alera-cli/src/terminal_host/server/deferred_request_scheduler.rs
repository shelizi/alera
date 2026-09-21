use std::future::Future;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::terminal_host::host_error::{HostError, HostResult};

use super::deferred_admission::{DeferredAdmission, DeferredRequestClass};
use super::ServerCommand;

#[derive(Clone)]
pub(super) struct DeferredRequestScheduler {
    admission: Arc<DeferredAdmission>,
    inbox: UnboundedSender<ServerCommand>,
}

impl DeferredRequestScheduler {
    pub(super) fn new(
        admission: Arc<DeferredAdmission>,
        inbox: UnboundedSender<ServerCommand>,
    ) -> Self {
        Self { admission, inbox }
    }

    pub(super) fn schedule<F>(
        &self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        task: F,
    ) -> HostResult<()>
    where
        F: Future<Output = HostResult<Value>> + Send + 'static,
    {
        self.schedule_typed(
            client_id,
            request_id,
            request_type,
            task,
            |client_id, request_id, result| ServerCommand::DeferredRequestFinished {
                client_id,
                request_id,
                result,
            },
        )
    }

    pub(super) fn schedule_typed<T, F, C>(
        &self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        task: F,
        completion: C,
    ) -> HostResult<()>
    where
        T: Send + 'static,
        F: Future<Output = HostResult<T>> + Send + 'static,
        C: FnOnce(u64, i64, HostResult<T>) -> ServerCommand + Send + 'static,
    {
        let inbox = self.inbox.clone();
        self.admission.schedule_with_request_id(
            DeferredRequestClass::Bulk,
            request_type,
            Some(client_id),
            Some(request_id),
            async move {
                let result = task.await;
                let _ = inbox.send(completion(client_id, request_id, result));
            },
        )
    }

    pub(super) fn schedule_blocking<F>(
        &self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        task: F,
    ) -> HostResult<()>
    where
        F: FnOnce() -> HostResult<Value> + Send + 'static,
    {
        self.schedule(client_id, request_id, request_type, async move {
            tokio::task::spawn_blocking(task)
                .await
                .unwrap_or_else(|error| {
                    Err(HostError::state(format!(
                        "Deferred request failed: {error}"
                    )))
                })
        })
    }
}
