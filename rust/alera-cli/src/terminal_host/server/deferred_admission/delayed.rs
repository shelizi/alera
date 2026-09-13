//! Bounded delayed timers: a reservation is checked synchronously at submit
//! time, but no active deferred-work slot is held while the timer sleeps.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::terminal_host::host_error::{HostError, HostResult};

use super::{
    AdmissionInner, DeferredAdmission, DeferredRequestClass,
    DEFERRED_REQUEST_BACKPRESSURE_CODE, DEFERRED_REQUEST_BUSY_MESSAGE,
};


pub(super) struct DelayedJob {
    pub(super) request_type: String,
    pub(super) class: DeferredRequestClass,
    pub(super) client_id: Option<u64>,
    pub(super) disconnected: bool,
}

struct DelayedTimerGuard {
    inner: Arc<AdmissionInner>,
    timer_id: u64,
}

impl Drop for DelayedTimerGuard {
    fn drop(&mut self) {
        self.inner.finish_delayed(self.timer_id);
    }
}

impl DeferredAdmission {
    /// Arms a bounded timer without holding an active deferred-work slot while
    /// it sleeps. The timer reservation is checked synchronously at submit
    /// time, but it is intentionally absent from admission metrics. Once the
    /// delay elapses, the reservation is released before the task runs; the
    /// task must therefore only perform cheap post-wake delivery and must not
    /// re-enter admission.
    ///
    /// Timers retain their client id for disconnect bookkeeping. A disconnected
    /// client marks an armed timer but does not cancel it, because the timer
    /// only sends a cheap mailbox command and its handler re-validates state.
    pub(in crate::terminal_host::server) fn schedule_delayed<F>(
        self: &Arc<Self>,
        delay: Duration,
        class: DeferredRequestClass,
        request_type: impl Into<String>,
        client_id: Option<u64>,
        task: F,
    ) -> HostResult<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let request_type = request_type.into();
        let timer_id = {
            let mut state = self.inner.lock_state();
            let active = state.active.len();
            let pending = state.pending_count();
            let delayed = state.delayed.len();
            let over_total = state.occupied_count() >= self.inner.total_limit;
            let over_reserve = class != DeferredRequestClass::DispatchCritical
                && state.noncritical_count() >= self.inner.noncritical_limit();
            if over_total || over_reserve {
                tracing::debug!(
                    request_type = %request_type,
                    request_class = class.as_str(),
                    active,
                    pending,
                    delayed,
                    "deferred timer rejected"
                );
                return Err(HostError::conflict(
                    DEFERRED_REQUEST_BACKPRESSURE_CODE,
                    DEFERRED_REQUEST_BUSY_MESSAGE,
                    json!({
                        "requestType": request_type,
                        "requestClass": class.as_str(),
                        "active": active,
                        "pending": pending,
                        "capacity": self.inner.total_limit,
                    }),
                ));
            }
            let timer_id = state.next_timer_id;
            state.next_timer_id += 1;
            state.delayed.insert(
                timer_id,
                DelayedJob {
                    request_type: request_type.clone(),
                    class,
                    client_id,
                    disconnected: false,
                },
            );
            tracing::debug!(
                request_type = %request_type,
                request_class = class.as_str(),
                active,
                pending,
                delayed = state.delayed.len(),
                "deferred timer armed"
            );
            timer_id
        };

        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let guard = DelayedTimerGuard { inner, timer_id };
            tokio::time::sleep(delay).await;
            drop(guard);
            task.await;
        });
        Ok(())
    }
}

impl AdmissionInner {
    fn finish_delayed(&self, timer_id: u64) {
        let mut state = self.lock_state();
        if let Some(job) = state.delayed.remove(&timer_id) {
            tracing::debug!(
                request_type = %job.request_type,
                request_class = job.class.as_str(),
                disconnected = job.disconnected,
                delayed = state.delayed.len(),
                "deferred timer released"
            );
        }
    }
}
