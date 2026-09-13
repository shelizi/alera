use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde_json::{json, Value};

use super::deferred_admission_metrics::AdmissionMetrics;
use crate::terminal_host::host_error::{HostError, HostResult};

mod delayed;
#[cfg(test)]
mod request_id_test_support;

// Bounds active read-side work without making the actor mailbox wait for capacity.
pub(super) const DEFERRED_REQUEST_CONCURRENCY: usize = 8;
const DEFERRED_REQUEST_CAPACITY: usize = 64;
const DEFERRED_DISPATCH_RESERVE: usize = 8;
pub(super) const DEFERRED_REQUEST_BACKPRESSURE_CODE: &str = "deferred_request_backpressure";

const DEFERRED_REQUEST_BUSY_MESSAGE: &str = "The runtime host is busy. Retry the request.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DeferredRequestClass {
    DispatchCritical,
    Maintenance,
    Bulk,
}

impl DeferredRequestClass {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::DispatchCritical => "dispatchCritical",
            Self::Maintenance => "maintenance",
            Self::Bulk => "bulk",
        }
    }
}

const CLASSES: [DeferredRequestClass; 3] = [
    DeferredRequestClass::DispatchCritical,
    DeferredRequestClass::Maintenance,
    DeferredRequestClass::Bulk,
];

type BoxedTask = Pin<Box<dyn Future<Output = ()> + Send>>;

struct QueuedJob {
    request_id: Option<i64>,
    request_type: String,
    class: DeferredRequestClass,
    client_id: Option<u64>,
    queued_at: Instant,
    task: BoxedTask,
}

struct ActiveJob {
    request_id: Option<i64>,
    request_type: String,
    class: DeferredRequestClass,
    client_id: Option<u64>,
    queue_wait_ms: u64,
    disconnected: bool,
}

struct PendingSpawn {
    job_id: u64,
    task: BoxedTask,
}

#[derive(Default)]
struct AdmissionState {
    active: BTreeMap<u64, ActiveJob>,
    delayed: BTreeMap<u64, delayed::DelayedJob>,
    queued: [VecDeque<QueuedJob>; 3],
    next_job_id: u64,
    next_timer_id: u64,
    metrics: AdmissionMetrics,
    #[cfg(test)]
    gated: bool,
    #[cfg(test)]
    test_permits: usize,
}

impl AdmissionState {
    fn pending_count(&self) -> usize {
        self.queued.iter().map(VecDeque::len).sum()
    }

    fn occupied_count(&self) -> usize {
        self.active.len() + self.pending_count() + self.delayed.len()
    }

    fn noncritical_count(&self) -> usize {
        self.active
            .values()
            .filter(|job| job.class != DeferredRequestClass::DispatchCritical)
            .count()
            + self.queued[DeferredRequestClass::Maintenance as usize].len()
            + self.queued[DeferredRequestClass::Bulk as usize].len()
            + self
                .delayed
                .values()
                .filter(|job| job.class != DeferredRequestClass::DispatchCritical)
                .count()
    }
}

pub(super) struct DeferredAdmission {
    inner: Arc<AdmissionInner>,
}

struct AdmissionInner {
    state: Mutex<AdmissionState>,
    active_limit: usize,
    total_limit: usize,
    dispatch_reserve: usize,
}

struct ActiveJobGuard {
    inner: Arc<AdmissionInner>,
    job_id: u64,
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        self.inner.finish_job(self.job_id);
    }
}

impl Default for DeferredAdmission {
    fn default() -> Self {
        Self::with_limits(
            DEFERRED_REQUEST_CONCURRENCY,
            DEFERRED_REQUEST_CAPACITY,
            DEFERRED_DISPATCH_RESERVE,
        )
    }
}

impl DeferredAdmission {
    pub(super) fn with_limits(
        active_limit: usize,
        total_limit: usize,
        dispatch_reserve: usize,
    ) -> Self {
        Self {
            inner: Arc::new(AdmissionInner {
                state: Mutex::new(AdmissionState::default()),
                active_limit,
                total_limit,
                dispatch_reserve,
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn paused_with_limits(
        active_limit: usize,
        total_limit: usize,
        dispatch_reserve: usize,
    ) -> Self {
        let admission = Self::with_limits(active_limit, total_limit, dispatch_reserve);
        {
            let mut state = admission.inner.lock_state();
            state.gated = true;
        }
        admission
    }

    #[cfg(test)]
    pub(super) fn add_test_permits(&self, permits: usize) {
        let mut to_start = Vec::new();
        {
            let mut state = self.inner.lock_state();
            state.test_permits = state.test_permits.saturating_add(permits);
            self.inner.drain_locked(&mut state, &mut to_start);
        }
        self.inner.spawn_all(to_start);
    }

    pub(super) fn schedule<F>(
        self: &Arc<Self>,
        class: DeferredRequestClass,
        request_type: impl Into<String>,
        client_id: Option<u64>,
        task: F,
    ) -> HostResult<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.schedule_with_request_id(class, request_type, client_id, None, task)
    }

    pub(super) fn schedule_with_request_id<F>(
        self: &Arc<Self>,
        class: DeferredRequestClass,
        request_type: impl Into<String>,
        client_id: Option<u64>,
        request_id: Option<i64>,
        task: F,
    ) -> HostResult<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let request_type = request_type.into();
        let job = QueuedJob {
            request_id,
            request_type,
            class,
            client_id,
            queued_at: Instant::now(),
            task: Box::pin(task),
        };
        let mut to_start = Vec::new();
        let result = {
            let mut state = self.inner.lock_state();
            let active = state.active.len();
            let pending = state.pending_count();
            let over_total = state.occupied_count() >= self.inner.total_limit;
            let over_reserve = class != DeferredRequestClass::DispatchCritical
                && state.noncritical_count() >= self.inner.noncritical_limit();
            if over_total || over_reserve {
                state.metrics.rejected(&job.request_type, class);
                tracing::debug!(
                    request_id = ?job.request_id,
                    client_id = ?job.client_id,
                    request_type = %job.request_type,
                    request_class = class.as_str(),
                    active,
                    pending,
                    queue_wait_ms = 0_u64,
                    "deferred request rejected"
                );
                Err(HostError::conflict(
                    DEFERRED_REQUEST_BACKPRESSURE_CODE,
                    DEFERRED_REQUEST_BUSY_MESSAGE,
                    json!({
                        "requestType": job.request_type,
                        "requestClass": class.as_str(),
                        "active": active,
                        "pending": pending,
                        "capacity": self.inner.total_limit,
                    }),
                ))
            } else if self.inner.can_start(&state) {
                self.inner
                    .admit_locked(&mut state, job, false, &mut to_start);
                Ok(())
            } else {
                state.metrics.queued(&job.request_type, class);
                tracing::debug!(
                    request_id = ?job.request_id,
                    client_id = ?job.client_id,
                    request_type = %job.request_type,
                    request_class = class.as_str(),
                    active,
                    pending = pending.saturating_add(1),
                    queue_wait_ms = 0_u64,
                    "deferred request enqueued"
                );
                state.queued[class as usize].push_back(job);
                Ok(())
            }
        };
        self.inner.spawn_all(to_start);
        result
    }

    pub(super) fn disconnect_client(&self, client_id: u64) {
        let mut dropped = Vec::new();
        {
            let mut state = self.inner.lock_state();
            let mut affected: Vec<(String, DeferredRequestClass, bool, Option<i64>)> = Vec::new();
            for class in CLASSES {
                let queue = &mut state.queued[class as usize];
                let mut kept = VecDeque::with_capacity(queue.len());
                while let Some(job) = queue.pop_front() {
                    if job.client_id == Some(client_id) {
                        affected.push((job.request_type.clone(), class, true, job.request_id));
                        dropped.push(job);
                    } else {
                        kept.push_back(job);
                    }
                }
                *queue = kept;
            }
            for job in state.active.values_mut() {
                if job.client_id == Some(client_id) && !job.disconnected {
                    job.disconnected = true;
                    affected.push((job.request_type.clone(), job.class, false, job.request_id));
                }
            }
            for job in state.delayed.values_mut() {
                if job.client_id == Some(client_id) && !job.disconnected {
                    job.disconnected = true;
                    tracing::debug!(
                        request_id = ?job.request_id,
                        client_id = ?job.client_id,
                        request_type = %job.request_type,
                        request_class = job.class.as_str(),
                        queue_wait_ms = 0_u64,
                        "deferred timer owner disconnected; timer remains armed"
                    );
                }
            }
            let request_ids = affected
                .iter()
                .map(|(_, _, _, request_id)| *request_id)
                .collect::<Vec<_>>();
            for (request_type, class, was_pending, _) in affected {
                state
                    .metrics
                    .disconnected(&request_type, class, was_pending);
            }
            tracing::debug!(
                client_id,
                request_ids = ?request_ids,
                dropped = dropped.len(),
                "deferred client disconnect sweep"
            );
        }
        drop(dropped);
    }

    pub(super) fn snapshot(&self) -> Value {
        let state = self.inner.lock_state();
        state.metrics.snapshot(
            state.active.len(),
            state.pending_count(),
            self.inner.active_limit,
            self.inner.total_limit,
        )
    }
}

impl AdmissionInner {
    fn lock_state(&self) -> MutexGuard<'_, AdmissionState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn noncritical_limit(&self) -> usize {
        self.total_limit.saturating_sub(self.dispatch_reserve)
    }

    fn can_start(&self, state: &AdmissionState) -> bool {
        if state.active.len() >= self.active_limit {
            return false;
        }
        #[cfg(test)]
        if state.gated && state.test_permits == 0 {
            return false;
        }
        true
    }

    fn admit_locked(
        &self,
        state: &mut AdmissionState,
        job: QueuedJob,
        was_queued: bool,
        to_start: &mut Vec<PendingSpawn>,
    ) {
        let queue_wait_ms = if was_queued {
            queue_wait_ms(job.queued_at)
        } else {
            0
        };
        let job_id = state.next_job_id;
        state.next_job_id += 1;
        #[cfg(test)]
        if state.gated {
            state.test_permits = state.test_permits.saturating_sub(1);
        }
        state
            .metrics
            .started(&job.request_type, job.class, was_queued, queue_wait_ms);
        state.active.insert(
            job_id,
            ActiveJob {
                request_id: job.request_id,
                request_type: job.request_type.clone(),
                class: job.class,
                client_id: job.client_id,
                queue_wait_ms,
                disconnected: false,
            },
        );
        tracing::debug!(
            request_id = ?job.request_id,
            client_id = ?job.client_id,
            request_type = %job.request_type,
            request_class = job.class.as_str(),
            active = state.active.len(),
            pending = state.pending_count(),
            queued = was_queued,
            queue_wait_ms,
            "deferred request started"
        );
        to_start.push(PendingSpawn {
            job_id,
            task: job.task,
        });
    }

    fn drain_locked(&self, state: &mut AdmissionState, to_start: &mut Vec<PendingSpawn>) {
        while self.can_start(state) {
            let next = CLASSES.iter().find_map(|class| {
                state.queued[*class as usize]
                    .pop_front()
                    .map(|job| (*class, job))
            });
            let Some((_, job)) = next else {
                break;
            };
            self.admit_locked(state, job, true, to_start);
        }
    }

    fn spawn_all(self: &Arc<Self>, to_start: Vec<PendingSpawn>) {
        for pending in to_start {
            let inner = Arc::clone(self);
            tokio::spawn(async move {
                let _guard = ActiveJobGuard {
                    inner,
                    job_id: pending.job_id,
                };
                pending.task.await;
            });
        }
    }

    fn finish_job(self: &Arc<Self>, job_id: u64) {
        let mut to_start = Vec::new();
        {
            let mut state = self.lock_state();
            if let Some(job) = state.active.remove(&job_id) {
                state.metrics.completed(&job.request_type, job.class);
                if job.disconnected {
                    state.metrics.stale_completion();
                    tracing::debug!(
                        request_id = ?job.request_id,
                        client_id = ?job.client_id,
                        request_type = %job.request_type,
                        request_class = job.class.as_str(),
                        stale = true,
                        "stale-completion"
                    );
                }
                #[cfg(test)]
                if state.gated {
                    state.test_permits = state.test_permits.saturating_add(1);
                }
                tracing::debug!(
                    request_id = ?job.request_id,
                    client_id = ?job.client_id,
                    request_type = %job.request_type,
                    request_class = job.class.as_str(),
                    active = state.active.len(),
                    pending = state.pending_count(),
                    queue_wait_ms = job.queue_wait_ms,
                    "deferred request finished"
                );
            }
            self.drain_locked(&mut state, &mut to_start);
        }
        self.spawn_all(to_start);
    }
}

fn queue_wait_ms(queued_at: Instant) -> u64 {
    u64::try_from(queued_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}
