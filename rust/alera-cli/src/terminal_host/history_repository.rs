use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::Result;
use bytes::Bytes;
use tokio::sync::{Notify, Semaphore};

use super::buffer::{append_deque_tail as append_tail, copy_deque_tail, retain_deque_tail};
pub use super::history_store::TerminalHostCheckpoint;
use super::history_store::TerminalHostHistoryStore;

const HISTORY_FLUSH_CONCURRENCY: usize = 2;
const HISTORY_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct TerminalHostHistoryRepository {
    inner: Arc<HistoryRepositoryInner>,
}

struct HistoryRepositoryInner {
    store: TerminalHostHistoryStore,
    sessions: Mutex<HashMap<String, SessionPersistenceState>>,
    flush_slots: Semaphore,
    idle_changed: Notify,
}

#[derive(Default)]
struct SessionPersistenceState {
    checkpoint: Option<TerminalHostCheckpoint>,
    checkpoint_revision: Option<u64>,
    outputs: BTreeMap<i64, PendingOutput>,
    pending_bytes: usize,
    memory_tail: VecDeque<u8>,
    // Only the suffix covered by a full live snapshot or contiguous PTY
    // batches is eligible for checkpoint reuse. A cold SQLite read cannot
    // establish this proof: its checkpoint cursor may lag the output rows.
    verified_tail: Option<VerifiedTail>,
    max_bytes: usize,
    delete_revision: Option<u64>,
    trim_request: Option<TrimRequest>,
    last_output_revision: u64,
    next_revision: u64,
    worker_running: bool,
    last_error: Option<String>,
}

#[derive(Clone, Copy)]
struct VerifiedTail {
    end: u64,
    len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrimRequest {
    revision: u64,
    max_bytes: usize,
    output_revision: u64,
}

#[derive(Clone)]
struct PendingOutput {
    revision: u64,
    data: Bytes,
    // Capacity of the backing byte allocation, not just the visible slice.
    // A snapshot may share it, so changing this view must not mutate it.
    backing_bytes: usize,
}

impl PendingOutput {
    fn from_vec(revision: u64, data: Vec<u8>) -> Self {
        let len = data.len();
        let capacity = data.capacity();
        let (data, backing_bytes) = if capacity > len.saturating_mul(2) {
            // Producers can deliver a short Vec with a large spare capacity.
            // Do not retain that excess for the lifetime of the queue.
            (Bytes::copy_from_slice(&data), len)
        } else {
            (Bytes::from(data), capacity)
        };
        Self {
            revision,
            data,
            backing_bytes,
        }
    }

    #[cfg(test)]
    fn from_static(revision: u64, data: &'static [u8]) -> Self {
        Self {
            revision,
            data: Bytes::from_static(data),
            backing_bytes: data.len(),
        }
    }

    /// Reuse a shared suffix while at least half of its backing remains.
    /// Compact geometrically after that point. Pending backing allocations
    /// are bounded by twice the visible pending bytes, while trimming one
    /// byte at a time no longer copies the entire surviving row every time.
    fn trim_prefix(&mut self, bytes: usize, revision: u64) {
        debug_assert!(bytes > 0 && bytes < self.data.len());
        let remaining = self.data.len() - bytes;
        if self.backing_bytes > remaining.saturating_mul(2) {
            self.data = Bytes::copy_from_slice(&self.data[bytes..]);
            self.backing_bytes = remaining;
        } else {
            self.data = self.data.slice(bytes..);
        }
        self.revision = revision;
    }
}

#[derive(Clone)]
enum FlushSnapshot {
    Delete {
        revision: u64,
    },
    Persist {
        checkpoint: Option<(u64, Box<TerminalHostCheckpoint>)>,
        outputs: Vec<(i64, u64, Bytes)>,
        trim_request: Option<TrimRequest>,
    },
}

impl SessionPersistenceState {
    fn cached_checkpoint(&self, max_bytes: usize) -> Option<Option<TerminalHostCheckpoint>> {
        if self.delete_revision.is_some() {
            return Some(None);
        }
        let mut checkpoint = self.checkpoint.as_ref()?.clone();
        checkpoint.buffer = copy_deque_tail(&self.memory_tail, max_bytes);
        Some(Some(checkpoint))
    }

    fn can_reuse_tail(&self, end: u64, len: usize) -> bool {
        self.delete_revision.is_none()
            && (len == 0
                || self.verified_tail.is_some_and(|tail| {
                    tail.end == end && tail.len >= len && self.memory_tail.len() >= len
                }))
    }

    fn verify_full_tail(&mut self, end: u64) {
        let len = self.memory_tail.len();
        self.verified_tail =
            (end != u64::MAX && len as u64 <= end).then_some(VerifiedTail { end, len });
    }

    fn verify_appended_tail(&mut self, end: Option<u64>, appended: usize) {
        self.verified_tail = end.filter(|end| *end != u64::MAX).and_then(|end| {
            let start = end.checked_sub(appended as u64)?;
            let previous = self
                .verified_tail
                .filter(|tail| tail.end == start)
                .map_or(0, |tail| tail.len);
            Some(VerifiedTail {
                end,
                len: previous
                    .saturating_add(appended)
                    .min(self.memory_tail.len()),
            })
        });
    }

    fn record_checkpoint(&mut self, checkpoint: TerminalHostCheckpoint, max_bytes: usize) {
        debug_assert!(checkpoint.buffer.is_empty());
        self.max_bytes = max_bytes;
        self.delete_revision = None;
        self.checkpoint = Some(checkpoint);
        self.checkpoint_revision = Some(self.next_revision());
        self.trim_pending_outputs();
        self.request_trim();
    }

    fn next_revision(&mut self) -> u64 {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.next_revision
    }

    fn request_trim(&mut self) {
        if self.trim_request.is_some_and(|request| {
            request.max_bytes == self.max_bytes
                && request.output_revision == self.last_output_revision
        }) {
            // An identical trim is already queued or running. Keep its
            // revision so that its acknowledgement can clear this request.
            return;
        }
        self.trim_request = Some(TrimRequest {
            revision: self.next_revision(),
            max_bytes: self.max_bytes,
            output_revision: self.last_output_revision,
        });
    }

    fn has_pending(&self) -> bool {
        self.delete_revision.is_some()
            || self.checkpoint_revision.is_some()
            || !self.outputs.is_empty()
            || self.trim_request.is_some()
    }

    fn snapshot(&self) -> Option<FlushSnapshot> {
        if let Some(revision) = self.delete_revision {
            return Some(FlushSnapshot::Delete { revision });
        }
        if !self.has_pending() {
            return None;
        }
        Some(FlushSnapshot::Persist {
            checkpoint: self.checkpoint_revision.and_then(|revision| {
                self.checkpoint
                    .as_ref()
                    .cloned()
                    .map(|checkpoint| (revision, Box::new(checkpoint)))
            }),
            outputs: self
                .outputs
                .iter()
                .map(|(sequence, output)| (*sequence, output.revision, output.data.clone()))
                .collect(),
            trim_request: self.trim_request,
        })
    }

    fn insert_pending_output(&mut self, sequence: i64, output: PendingOutput) {
        let len = output.data.len();
        self.last_output_revision = output.revision;
        if let Some(previous) = self.outputs.insert(sequence, output) {
            self.pending_bytes -= previous.data.len();
        }
        self.pending_bytes += len;
    }

    fn remove_pending_output(&mut self, sequence: i64) {
        if let Some(output) = self.outputs.remove(&sequence) {
            self.pending_bytes -= output.data.len();
        }
    }

    fn trim_pending_outputs(&mut self) {
        while self.pending_bytes > self.max_bytes {
            let Some(sequence) = self.outputs.keys().next().copied() else {
                break;
            };
            let excess = self.pending_bytes - self.max_bytes;
            let len = self
                .outputs
                .get(&sequence)
                .map_or(0, |output| output.data.len());
            if len <= excess {
                self.remove_pending_output(sequence);
                continue;
            }

            let revision = self.next_revision();
            if let Some(output) = self.outputs.get_mut(&sequence) {
                output.trim_prefix(excess, revision);
                self.pending_bytes -= excess;
            }
        }
    }
}

impl TerminalHostHistoryRepository {
    pub async fn open(runtime_dir: &std::path::Path) -> Result<Self> {
        let store = TerminalHostHistoryStore::open(runtime_dir).await?;
        Ok(Self::from_store(store))
    }

    fn from_store(store: TerminalHostHistoryStore) -> Self {
        Self {
            inner: Arc::new(HistoryRepositoryInner {
                store,
                sessions: Mutex::new(HashMap::new()),
                flush_slots: Semaphore::new(HISTORY_FLUSH_CONCURRENCY),
                idle_changed: Notify::new(),
            }),
        }
    }

    pub fn queue_checkpoint(&self, mut checkpoint: TerminalHostCheckpoint, max_bytes: usize) {
        retain_tail(&mut checkpoint.buffer, max_bytes);
        let session_id = checkpoint.session_id.clone();
        let memory_tail = std::mem::take(&mut checkpoint.buffer);
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            state.memory_tail = memory_tail.into();
            state.verify_full_tail(checkpoint.output_stream_bytes);
            state.record_checkpoint(checkpoint, max_bytes);
        }
        self.ensure_worker(session_id);
    }

    /// The actor has flushed all durable output through checkpoint's cursor.
    /// Reuse an exactly covered suffix without capturing the live ring. If
    /// the write-behind worker already released its cache, a batch is missing
    /// or the cursor cannot prove coverage, seed the original full snapshot.
    /// Do not keep acknowledged caches alive merely to make this path hit.
    pub(super) fn queue_checkpoint_from_live(
        &self,
        checkpoint: TerminalHostCheckpoint,
        live_tail_len: usize,
        max_bytes: usize,
        capture: impl FnOnce() -> Vec<u8>,
    ) {
        debug_assert!(checkpoint.buffer.is_empty());
        let session_id = checkpoint.session_id.clone();
        let len = live_tail_len.min(max_bytes);
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            if state.can_reuse_tail(checkpoint.output_stream_bytes, len) {
                if state.memory_tail.len() > len {
                    state.memory_tail.drain(..state.memory_tail.len() - len);
                }
                retain_deque_tail(&mut state.memory_tail, max_bytes);
                state.verify_full_tail(checkpoint.output_stream_bytes);
                state.record_checkpoint(checkpoint, max_bytes);
                drop(sessions);
                self.ensure_worker(session_id);
                return;
            }
        }

        // Capture outside the repository lock, as before. The Host actor is
        // the sole producer for this session; workers can only acknowledge or
        // evict pending state while the fallback snapshot is materialized.
        let mut checkpoint = checkpoint;
        checkpoint.buffer = capture();
        self.queue_checkpoint(checkpoint, max_bytes);
    }

    #[cfg(test)]
    pub fn queue_output(&self, session_id: String, sequence: i64, data: Vec<u8>, max_bytes: usize) {
        self.queue_output_inner(session_id, sequence, data, max_bytes, None);
    }

    pub(super) fn queue_output_at_cursor(
        &self,
        session_id: String,
        sequence: i64,
        data: Vec<u8>,
        max_bytes: usize,
        end: u64,
    ) {
        self.queue_output_inner(session_id, sequence, data, max_bytes, Some(end));
    }

    fn queue_output_inner(
        &self,
        session_id: String,
        sequence: i64,
        data: Vec<u8>,
        max_bytes: usize,
        end: Option<u64>,
    ) {
        if data.is_empty() {
            return;
        }
        let mut queued = false;
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            if state.delete_revision.is_none() {
                state.max_bytes = max_bytes;
                append_tail(&mut state.memory_tail, &data, max_bytes);
                state.verify_appended_tail(end, data.len());
                let revision = state.next_revision();
                state.insert_pending_output(sequence, PendingOutput::from_vec(revision, data));
                state.trim_pending_outputs();
                // If configuration changed while a trim was in flight,
                // its old cap must not acknowledge the new cap.
                if state
                    .trim_request
                    .is_some_and(|request| request.max_bytes != max_bytes)
                {
                    state.request_trim();
                }
                queued = true;
            }
        }
        if queued {
            self.ensure_worker(session_id);
        }
    }

    pub fn queue_trim(&self, session_id: impl Into<String>, max_bytes: usize) {
        let session_id = session_id.into();
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            if state.delete_revision.is_some() {
                return;
            }
            state.max_bytes = max_bytes;
            retain_deque_tail(&mut state.memory_tail, max_bytes);
            if let Some(tail) = state.verified_tail.as_mut() {
                tail.len = tail.len.min(state.memory_tail.len());
            }
            if let Some(checkpoint) = state.checkpoint.as_mut() {
                retain_tail(&mut checkpoint.buffer, max_bytes);
            }
            state.trim_pending_outputs();
            state.request_trim();
        }
        self.ensure_worker(session_id);
    }

    pub fn queue_delete(&self, session_id: impl Into<String>) {
        let session_id = session_id.into();
        {
            let mut sessions = self.lock_sessions();
            let state = sessions.entry(session_id.clone()).or_default();
            let revision = state.next_revision();
            state.delete_revision = Some(revision);
            state.checkpoint = None;
            state.checkpoint_revision = None;
            state.outputs.clear();
            state.pending_bytes = 0;
            state.trim_request = None;
            state.memory_tail = VecDeque::new();
            state.verified_tail = None;
            state.last_error = None;
        }
        self.ensure_worker(session_id);
    }

    pub async fn read(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<Option<TerminalHostCheckpoint>> {
        if let Some(cached) = self.cached_checkpoint(session_id, max_bytes) {
            return Ok(cached);
        }
        if self.is_deleted(session_id) {
            return Ok(None);
        }

        let persisted = self
            .inner
            .store
            .read_with_maintenance(session_id, max_bytes)
            .await?;
        let Some((persisted, maintenance_required)) = persisted else {
            return Ok(None);
        };
        Ok(self.finish_cold_read(session_id, max_bytes, persisted, maintenance_required))
    }

    fn finish_cold_read(
        &self,
        session_id: &str,
        max_bytes: usize,
        mut persisted: TerminalHostCheckpoint,
        maintenance_required: bool,
    ) -> Option<TerminalHostCheckpoint> {
        debug_assert!(persisted.buffer.len() <= max_bytes);
        {
            let sessions = self.lock_sessions();
            if let Some(state) = sessions.get(session_id) {
                if let Some(cached) = state.cached_checkpoint(max_bytes) {
                    return cached;
                }
            } else if !maintenance_required {
                // All rows fit and no write-behind state needs reconciling.
                // Return the owned tail without creating a temporary cache
                // or scheduling a worker that would only confirm a no-op.
                return Some(persisted);
            }
        }

        // Only maintenance or existing pending state needs a memory cache.
        // Allocate and copy the potentially large tail outside the global
        // sessions lock, then recheck for a newer checkpoint or deletion.
        let buffer = std::mem::take(&mut persisted.buffer);
        let metadata = persisted.clone();
        let memory_tail = VecDeque::from(buffer.clone());
        persisted.buffer = buffer;

        let retired_tail = {
            let mut sessions = self.lock_sessions();
            if !maintenance_required && !sessions.contains_key(session_id) {
                return Some(persisted);
            }
            let state = sessions.entry(session_id.to_string()).or_default();
            if let Some(cached) = state.cached_checkpoint(max_bytes) {
                return cached;
            }
            state.max_bytes = max_bytes;
            state.checkpoint = Some(metadata);
            state.verified_tail = None;
            let retired = std::mem::replace(&mut state.memory_tail, memory_tail);
            state.trim_pending_outputs();
            // Publish the cache and trim request atomically. Do not queue
            // another trim after releasing the lock, which could overwrite
            // a concurrent producer's newer retention configuration.
            state.request_trim();
            retired
        };
        drop(retired_tail);

        // Physical compaction must not delay the restore.
        self.ensure_worker(session_id.to_string());
        Some(persisted)
    }

    pub async fn next_output_sequence(&self, session_id: &str) -> Result<i64> {
        let pending_next = {
            let sessions = self.lock_sessions();
            sessions
                .get(session_id)
                .and_then(|state| state.outputs.keys().next_back().copied())
                .map(|sequence| sequence.saturating_add(1))
                .unwrap_or(0)
        };
        Ok(self
            .inner
            .store
            .next_output_sequence(session_id)
            .await?
            .max(pending_next))
    }

    fn cached_checkpoint(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Option<Option<TerminalHostCheckpoint>> {
        self.lock_sessions()
            .get(session_id)?
            .cached_checkpoint(max_bytes)
    }

    fn is_deleted(&self, session_id: &str) -> bool {
        self.lock_sessions()
            .get(session_id)
            .is_some_and(|state| state.delete_revision.is_some())
    }

    fn ensure_worker(&self, session_id: String) {
        let should_spawn = {
            let mut sessions = self.lock_sessions();
            let Some(state) = sessions.get_mut(&session_id) else {
                return;
            };
            if state.worker_running {
                false
            } else {
                state.worker_running = true;
                true
            }
        };
        if !should_spawn {
            return;
        }

        let repository = self.clone();
        tokio::spawn(async move {
            repository.flush_worker(session_id).await;
        });
    }

    async fn flush_worker(&self, session_id: String) {
        loop {
            // Release an idle session immediately, without waiting for a
            // slot held by another session. Waiting workers must not retain
            // snapshots of output that can be superseded or trimmed.
            if !self.has_pending_or_stop(&session_id) {
                return;
            }
            let permit = match self.inner.flush_slots.acquire().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let Some(snapshot) = self.next_snapshot_or_stop(&session_id) else {
                return;
            };

            match self.persist_snapshot(&session_id, &snapshot).await {
                Ok(()) => {
                    let keep_running = self.ack_snapshot(&session_id, &snapshot);
                    drop(snapshot);
                    drop(permit);
                    if !keep_running {
                        return;
                    }
                }
                Err(error) => {
                    // Keep only the current write-behind state during
                    // backoff, not the failed attempt's older payloads.
                    drop(snapshot);
                    drop(permit);
                    let message = error.to_string();
                    {
                        let mut sessions = self.lock_sessions();
                        if let Some(state) = sessions.get_mut(&session_id) {
                            state.last_error = Some(message.clone());
                        }
                    }
                    tracing::warn!(
                        session_id = %session_id,
                        "terminal history persistence failed; keeping data in memory for retry: {message}"
                    );
                    tokio::time::sleep(HISTORY_RETRY_DELAY).await;
                }
            }
        }
    }

    pub async fn flush_pending(&self, timeout: Duration) -> bool {
        tokio::time::timeout(
            timeout,
            self.wait_for_idle(|sessions| {
                sessions
                    .values()
                    .all(|state| !state.worker_running && !state.has_pending())
            }),
        )
        .await
        .is_ok()
    }

    /// Wait only on a retirement event or the caller's timeout. Register
    /// before inspecting the predicate: Notify::notify_waiters reaches a
    /// Notified future created before the event even if not yet polled.
    /// This closes the race between the final durable ack and waiting.
    async fn wait_for_idle(
        &self,
        mut is_idle: impl FnMut(&HashMap<String, SessionPersistenceState>) -> bool,
    ) {
        loop {
            let changed = self.inner.idle_changed.notified();
            let idle = {
                let sessions = self.lock_sessions();
                is_idle(&sessions)
            };
            if idle {
                return;
            }
            changed.await;
        }
    }

    /// Remove a completed worker while still holding the state lock, then
    /// drop the possibly large buffers and wake waiters outside that lock.
    /// Unchanged acknowledgements and producer output never wake waiters.
    fn retire_session(
        &self,
        mut sessions: MutexGuard<'_, HashMap<String, SessionPersistenceState>>,
        session_id: &str,
    ) {
        let retired = sessions.remove(session_id);
        drop(sessions);
        if let Some(retired) = retired {
            drop(retired);
            self.inner.idle_changed.notify_waiters();
        }
    }

    fn has_pending_or_stop(&self, session_id: &str) -> bool {
        let sessions = self.lock_sessions();
        if sessions
            .get(session_id)
            .is_some_and(SessionPersistenceState::has_pending)
        {
            return true;
        }
        self.retire_session(sessions, session_id);
        false
    }

    fn next_snapshot_or_stop(&self, session_id: &str) -> Option<FlushSnapshot> {
        let sessions = self.lock_sessions();
        let snapshot = sessions.get(session_id)?.snapshot();
        if snapshot.is_none() {
            // A producer can cancel pending work while admission is blocked.
            self.retire_session(sessions, session_id);
        }
        snapshot
    }

    async fn persist_snapshot(&self, session_id: &str, snapshot: &FlushSnapshot) -> Result<()> {
        match snapshot {
            FlushSnapshot::Delete { .. } => self.inner.store.delete(session_id).await,
            FlushSnapshot::Persist {
                checkpoint,
                outputs,
                trim_request,
            } => {
                let output_rows = outputs
                    .iter()
                    .map(|(sequence, _, data)| (*sequence, data.as_ref()))
                    .collect::<Vec<_>>();
                // Output-only batches stay free of retention scans. At the
                // maintenance cadence, append and trim share one transaction.
                self.inner
                    .store
                    .persist_session_with_trim(
                        session_id,
                        checkpoint
                            .as_ref()
                            .map(|(_, checkpoint)| checkpoint.as_ref()),
                        &output_rows,
                        trim_request.map(|request| request.max_bytes),
                    )
                    .await
            }
        }
    }

    /// Return false when this worker's state is gone. A replacement
    /// session can then start its own worker without racing the old loop.
    fn ack_snapshot(&self, session_id: &str, snapshot: &FlushSnapshot) -> bool {
        let mut sessions = self.lock_sessions();
        let Some(state) = sessions.get_mut(session_id) else {
            return false;
        };
        match snapshot {
            FlushSnapshot::Delete { revision } => {
                if state.delete_revision == Some(*revision) {
                    self.retire_session(sessions, session_id);
                    return false;
                }
            }
            FlushSnapshot::Persist {
                checkpoint,
                outputs,
                trim_request,
            } => {
                if let Some((revision, _)) = checkpoint {
                    if state.checkpoint_revision == Some(*revision) {
                        state.checkpoint_revision = None;
                    }
                }
                for (sequence, revision, _) in outputs {
                    if state
                        .outputs
                        .get(sequence)
                        .is_some_and(|output| output.revision == *revision)
                    {
                        state.remove_pending_output(*sequence);
                    }
                }
                if trim_request.is_some() && state.trim_request == *trim_request {
                    state.trim_request = None;
                }
                state.last_error = None;
            }
        }
        if !state.has_pending() {
            // The last durable acknowledgement can retire this worker
            // immediately. No extra lock acquisition on the next loop and
            // no producer can create a second worker until we remove it.
            self.retire_session(sessions, session_id);
            return false;
        }
        true
    }

    fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, SessionPersistenceState>> {
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    #[cfg(test)]
    async fn wait_until_idle(&self, session_id: &str) {
        tokio::time::timeout(
            Duration::from_secs(2),
            self.wait_for_idle(|sessions| {
                sessions
                    .get(session_id)
                    .is_none_or(|state| !state.worker_running && !state.has_pending())
            }),
        )
        .await
        .expect("history repository should become idle");
    }
}

fn retain_tail(buffer: &mut Vec<u8>, max_bytes: usize) {
    if buffer.len() <= max_bytes {
        return;
    }
    let start = buffer.len() - max_bytes;
    buffer.drain(..start);
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn checkpoint(session_id: &str, buffer: &[u8]) -> TerminalHostCheckpoint {
        TerminalHostCheckpoint {
            session_id: session_id.to_string(),
            workspace_id: "ws".to_string(),
            tab_id: "tab".to_string(),
            working_directory: "/tmp/project".to_string(),
            running: true,
            exit_code: None,
            ended_at: None,
            output_stream_bytes: buffer.len() as u64,
            updated_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            buffer: buffer.to_vec(),
        }
    }

    #[tokio::test]
    async fn waiting_workers_do_not_hold_or_persist_superseded_output_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        let slots = repository
            .inner
            .flush_slots
            .acquire_many(HISTORY_FLUSH_CONCURRENCY as u32)
            .await
            .unwrap();

        // Each waiting session initially has a large payload. None of them
        // may create a second owner before a flush slot is available.
        let cap = 512 * 1024;
        for index in 0..8 {
            let id = format!("s{index}");
            repository.queue_checkpoint(checkpoint(&id, b""), cap);
            repository.queue_output(id, 0, vec![b'x'; cap], cap);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        {
            let sessions = repository.lock_sessions();
            assert_eq!(sessions.len(), 8);
            for state in sessions.values() {
                assert!(state.worker_running);
                assert!(
                    state.outputs[&0].data.is_unique(),
                    "a waiting worker must not pin its old payload"
                );
            }
        }

        // Install a trigger that rejects any superseded large write. The
        // first transaction after admission must see the newly bounded tail.
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(
                dir.path()
                    .join(super::super::history_store::HISTORY_DATABASE_FILE_NAME),
            ),
        )
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_stale_output BEFORE INSERT ON outputChunks
                     WHEN length(NEW.data) > 4 BEGIN
                     SELECT RAISE(ABORT, 'superseded snapshot'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..8 {
            let id = format!("s{index}");
            repository.queue_output(id.clone(), 1, b"tail".to_vec(), 4);
            repository.queue_trim(id, 4);
        }
        drop(slots);
        assert!(repository.flush_pending(Duration::from_secs(5)).await);
        assert!(repository.lock_sessions().is_empty());
        for index in 0..8 {
            let persisted = repository
                .inner
                .store
                .read(&format!("s{index}"), cap)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(persisted.buffer, b"tail");
        }
        pool.close().await;
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn retry_backoff_releases_failed_snapshot_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.inner.store.close_for_test().await;
        repository.queue_checkpoint(checkpoint("s1", b""), 64 * 1024);
        repository.queue_output("s1".into(), 0, vec![b'x'; 64 * 1024], 64 * 1024);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                {
                    let sessions = repository.lock_sessions();
                    let state = &sessions["s1"];
                    if state.last_error.is_some() {
                        assert!(
                            state.outputs[&0].data.is_unique(),
                            "the failed flush must not retain a payload during backoff"
                        );
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        repository.queue_output("s1".into(), 1, b"new".to_vec(), 3);
        let sessions = repository.lock_sessions();
        assert_eq!(sessions["s1"].pending_bytes, 3);
        assert_eq!(sessions["s1"].outputs[&1].data.as_ref(), b"new");
    }

    #[tokio::test]
    async fn repeated_trim_coalesces_but_new_output_and_cap_require_fresh_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        let mut state = SessionPersistenceState {
            max_bytes: 128,
            worker_running: true,
            ..Default::default()
        };
        state.request_trim();
        let initial_request = state.trim_request.unwrap();
        let initial_snapshot = state.snapshot().unwrap();
        for _ in 0..1000 {
            state.request_trim();
            assert_eq!(state.trim_request, Some(initial_request));
        }
        let revision = state.next_revision();
        state.insert_pending_output(0, PendingOutput::from_static(revision, b"later"));
        state.request_trim();
        let output_request = state.trim_request.unwrap();
        assert_ne!(initial_request.revision, output_request.revision);
        let output_snapshot = state.snapshot().unwrap();
        state.max_bytes = 4;
        state.trim_pending_outputs();
        state.request_trim();
        let smaller_request = state.trim_request.unwrap();
        assert_eq!(smaller_request.max_bytes, 4);
        assert_ne!(smaller_request.revision, output_request.revision);
        let latest_snapshot = state.snapshot().unwrap();
        repository.lock_sessions().insert("s1".into(), state);

        assert!(repository.ack_snapshot("s1", &initial_snapshot));
        assert_eq!(
            repository.lock_sessions()["s1"].trim_request,
            Some(smaller_request)
        );
        assert!(repository.ack_snapshot("s1", &output_snapshot));
        assert_eq!(
            repository.lock_sessions()["s1"].trim_request,
            Some(smaller_request)
        );
        assert!(!repository.ack_snapshot("s1", &latest_snapshot));
        assert!(repository.lock_sessions().is_empty());
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn completed_delete_stops_old_worker_but_superseded_delete_keeps_worker() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        let mut state = SessionPersistenceState {
            worker_running: true,
            ..Default::default()
        };
        state.delete_revision = Some(state.next_revision());
        let obsolete = state.snapshot().unwrap();
        state.record_checkpoint(checkpoint("s1", b""), 64);
        let current = state.snapshot().unwrap();
        repository.lock_sessions().insert("s1".into(), state);
        assert!(repository.ack_snapshot("s1", &obsolete));
        // A delete arriving while the replacement checkpoint is in flight
        // must keep the worker alive until that delete is durable.
        repository.queue_delete("s1");
        let final_delete = repository.lock_sessions()["s1"].snapshot().unwrap();
        assert!(repository.ack_snapshot("s1", &current));
        assert!(!repository.ack_snapshot("s1", &final_delete));
        assert!(repository.lock_sessions().is_empty());
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn checkpoint_reuses_proven_contiguous_tail_during_write_backlog() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        // No worker can acknowledge or evict state while the actor queues
        // these checkpoints. Cover the high-output / slow-disk path directly.
        let cap = 2 * 1024 * 1024;
        let initial = vec![b'x'; cap];
        repository.queue_checkpoint(checkpoint("s1", &initial), cap);
        let mut expected = VecDeque::from(initial);
        let initial_capacity = repository.lock_sessions()["s1"].memory_tail.capacity();
        let mut cursor = cap as u64;
        let mut captures = 0;
        for sequence in 0..256 {
            let data = vec![(sequence % 256) as u8; 1024];
            cursor += data.len() as u64;
            append_tail(&mut expected, &data, cap);
            repository.queue_output_at_cursor("s1".into(), sequence, data, cap, cursor);
            let mut metadata = checkpoint("s1", b"");
            metadata.output_stream_bytes = cursor;
            metadata.working_directory = format!("/tmp/project-{sequence}");
            repository.queue_checkpoint_from_live(metadata, cap, cap, || {
                captures += 1;
                expected.iter().copied().collect()
            });
        }
        assert_eq!(
            captures, 0,
            "256 complete checkpoints must not capture the live ring"
        );
        {
            let sessions = repository.lock_sessions();
            let state = &sessions["s1"];
            assert_eq!(state.memory_tail, expected);
            assert_eq!(state.memory_tail.capacity(), initial_capacity);
            assert!(state.checkpoint.as_ref().unwrap().buffer.is_empty());
            assert_eq!(
                state.checkpoint.as_ref().unwrap().output_stream_bytes,
                cursor
            );
            assert_eq!(
                state.checkpoint.as_ref().unwrap().working_directory,
                "/tmp/project-255"
            );
            assert_eq!(state.verified_tail.unwrap().len, cap);
        }
        let read = repository.read("s1", 4096).await.unwrap().unwrap();
        assert_eq!(read.buffer, copy_deque_tail(&expected, 4096));
        assert_eq!(read.output_stream_bytes, cursor);
        assert!(repository.flush_pending(Duration::from_secs(5)).await);
        assert!(!repository.lock_sessions().contains_key("s1"));
        let persisted = repository
            .inner
            .store
            .read("s1", cap)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.output_stream_bytes, cursor);
        assert_eq!(persisted.buffer, copy_deque_tail(&expected, 256 * 1024));
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn checkpoint_falls_back_after_eviction_and_incomplete_or_discontinuous_tail() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"abcdef"), 64);
        repository.wait_until_idle("s1").await;
        assert!(!repository.lock_sessions().contains_key("s1"));

        let mut captures = 0;
        let mut capture = |cursor: u64, expected: &[u8], max_bytes: usize| {
            let mut metadata = checkpoint("s1", b"");
            metadata.output_stream_bytes = cursor;
            repository.queue_checkpoint_from_live(metadata, expected.len(), max_bytes, || {
                captures += 1;
                expected.to_vec()
            });
        };

        // A worker acknowledged the older checkpoint; a new pending batch
        // alone is not proof of the whole live tail.
        repository.queue_output_at_cursor("s1".into(), 0, b"gh".to_vec(), 64, 8);
        capture(8, b"abcdefgh", 64);
        // Now there is a fully verified seed.
        repository.queue_output_at_cursor("s1".into(), 1, b"ij".to_vec(), 64, 10);
        capture(10, b"abcdefghij", 64);
        // Equal byte counts are insufficient when the output cursor has a gap.
        repository.queue_output_at_cursor("s1".into(), 2, b"kl".to_vec(), 64, 14);
        capture(14, b"abcdefghij??kl", 64);
        // Even after a gap, a known contiguous suffix can cover a reduced cap.
        repository.queue_trim("s1", 4);
        repository.queue_output_at_cursor("s1".into(), 3, b"mn".to_vec(), 4, 16);
        capture(16, b"klmn", 4);
        // Increasing the cap does not make discarded data authoritative.
        capture(16, b"??klmn", 64);
        // Saturating cursors cannot establish a unique position.
        repository.queue_output_at_cursor("s1".into(), 4, b"z".to_vec(), 64, u64::MAX);
        capture(u64::MAX, b"??klmnz", 64);
        assert_eq!(captures, 4);

        let read = repository.read("s1", 64).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"??klmnz");
        assert_eq!(read.output_stream_bytes, u64::MAX);
        repository.queue_delete("s1");
        assert!(repository.read("s1", 64).await.unwrap().is_none());
        assert!(repository.flush_pending(Duration::from_secs(5)).await);
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn checkpoint_reuses_new_pending_suffix_only_when_it_covers_entire_live_tail() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"old"), 4);
        repository.wait_until_idle("s1").await;
        repository.queue_output_at_cursor("s1".into(), 0, b"abcdefgh".to_vec(), 4, 11);
        let mut metadata = checkpoint("s1", b"");
        metadata.output_stream_bytes = 11;
        repository.queue_checkpoint_from_live(metadata, 4, 4, || {
            panic!("new batch already covers all four retained bytes")
        });
        assert_eq!(
            repository.read("s1", 4).await.unwrap().unwrap().buffer,
            b"efgh"
        );
        repository.wait_until_idle("s1").await;
        assert!(!repository.lock_sessions().contains_key("s1"));
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn checkpoint_metadata_reuses_complete_tail_when_sqlite_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.inner.store.close_for_test().await;
        repository.queue_checkpoint(checkpoint("s1", b"hello"), 64);
        repository.queue_output_at_cursor("s1".into(), 0, b" world".to_vec(), 64, 11);
        let mut metadata = checkpoint("s1", b"");
        metadata.output_stream_bytes = 11;
        repository.queue_checkpoint_from_live(metadata, 11, 64, || {
            panic!("unavailable SQLite must retain the complete in-memory tail")
        });
        let read = repository.read("s1", 64).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"hello world");
        assert_eq!(read.output_stream_bytes, 11);
    }

    #[tokio::test]
    async fn memory_cache_is_authoritative_before_sqlite_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b"abc"), 1024);
        repository.queue_output("s1".to_string(), 0, b"def".to_vec(), 1024);

        let read = repository.read("s1", 1024).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"abcdef");
    }

    #[tokio::test]
    async fn memory_cache_keeps_only_the_configured_tail() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b"abcd"), 5);
        repository.queue_output("s1".to_string(), 0, b"efgh".to_vec(), 5);

        let read = repository.read("s1", 5).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"defgh");
    }

    #[tokio::test]
    async fn idle_waiters_do_not_poll_and_wake_on_retirement() {
        use std::cell::Cell;
        use std::future::Future;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::task::{Context, Wake, Waker};

        #[derive(Default)]
        struct WakeCounter(AtomicUsize);

        impl Wake for WakeCounter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }

            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        // Model two workers with a completed durable operation ready to
        // acknowledge. Neither may be retired until its matching revision.
        let snapshot = |id: &str| {
            let mut state = SessionPersistenceState {
                worker_running: true,
                ..Default::default()
            };
            state.record_checkpoint(checkpoint(id, b""), 64);
            let snapshot = state.snapshot().unwrap();
            repository.lock_sessions().insert(id.into(), state);
            snapshot
        };
        let first = snapshot("s1");
        let second = snapshot("s2");
        let scans = Cell::new(0);
        let all_wakes = Arc::new(WakeCounter::default());
        let first_wakes = Arc::new(WakeCounter::default());
        let all_waker = Waker::from(all_wakes.clone());
        let first_waker = Waker::from(first_wakes.clone());
        let mut all = Box::pin(repository.wait_for_idle(|sessions| {
            scans.set(scans.get() + 1);
            sessions.is_empty()
        }));
        let mut one = Box::pin(repository.wait_for_idle(|sessions| !sessions.contains_key("s1")));

        assert!(all
            .as_mut()
            .poll(&mut Context::from_waker(&all_waker))
            .is_pending());
        assert!(one
            .as_mut()
            .poll(&mut Context::from_waker(&first_waker))
            .is_pending());
        // Across multiple old 10 ms polling periods, no idle timer wakes
        // either future and no extra state-map scan is performed.
        tokio::time::sleep(Duration::from_millis(55)).await;
        assert_eq!(all_wakes.0.load(Ordering::Relaxed), 0);
        assert_eq!(first_wakes.0.load(Ordering::Relaxed), 0);
        assert_eq!(scans.get(), 1);

        // A notification between future creation and its first poll must
        // not be lost. The same retirement wakes every registered waiter.
        let early = repository.inner.idle_changed.notified();
        assert!(!repository.ack_snapshot("s1", &first));
        tokio::time::timeout(Duration::from_secs(1), early)
            .await
            .unwrap();
        assert_eq!(all_wakes.0.load(Ordering::Relaxed), 1);
        assert_eq!(first_wakes.0.load(Ordering::Relaxed), 1);
        assert!(one
            .as_mut()
            .poll(&mut Context::from_waker(&first_waker))
            .is_ready());
        assert!(all
            .as_mut()
            .poll(&mut Context::from_waker(&all_waker))
            .is_pending());
        assert_eq!(scans.get(), 2);

        assert!(!repository.ack_snapshot("s2", &second));
        assert_eq!(all_wakes.0.load(Ordering::Relaxed), 2);
        assert!(all
            .as_mut()
            .poll(&mut Context::from_waker(&all_waker))
            .is_ready());
        assert_eq!(scans.get(), 3);
        assert!(repository.lock_sessions().is_empty());
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn flush_waiters_timeout_or_complete_only_after_durable_ack() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        assert!(repository.flush_pending(Duration::ZERO).await);
        let slots = repository
            .inner
            .flush_slots
            .acquire_many(HISTORY_FLUSH_CONCURRENCY as u32)
            .await
            .unwrap();
        for session in ["s1", "s2"] {
            repository.queue_checkpoint(checkpoint(session, b""), 64);
            repository.queue_output(session.into(), 0, b"durable".to_vec(), 64);
        }

        let first = {
            let repository = repository.clone();
            tokio::spawn(async move { repository.flush_pending(Duration::from_secs(5)).await })
        };
        let second = {
            let repository = repository.clone();
            tokio::spawn(async move { repository.flush_pending(Duration::from_secs(5)).await })
        };
        // Timing out or cancelling one waiter must not consume the shared
        // completion event or report durability before writers are admitted.
        assert!(!repository.flush_pending(Duration::from_millis(25)).await);
        assert!(!first.is_finished());
        assert!(!second.is_finished());
        let cancelled = {
            let repository = repository.clone();
            tokio::spawn(async move { repository.flush_pending(Duration::from_secs(5)).await })
        };
        tokio::task::yield_now().await;
        cancelled.abort();
        assert!(cancelled.await.unwrap_err().is_cancelled());

        drop(slots);
        let (first, second) = tokio::join!(first, second);
        assert!(first.unwrap());
        assert!(second.unwrap());
        assert!(repository.lock_sessions().is_empty());
        for session in ["s1", "s2"] {
            let persisted = repository
                .inner
                .store
                .read(session, 64)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(persisted.buffer, b"durable");
        }
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn idle_notification_covers_cancelled_pending_output_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        // Exercise both non-ack retirement paths. These can be reached
        // when a cap change removes the last queued output before admission.
        for before_admission in [true, false] {
            repository.lock_sessions().insert(
                "s1".into(),
                SessionPersistenceState {
                    worker_running: true,
                    ..Default::default()
                },
            );
            let changed = repository.inner.idle_changed.notified();
            if before_admission {
                assert!(!repository.has_pending_or_stop("s1"));
            } else {
                assert!(repository.next_snapshot_or_stop("s1").is_none());
            }
            tokio::time::timeout(Duration::from_secs(1), changed)
                .await
                .unwrap();
        }

        repository.queue_checkpoint(checkpoint("s1", b""), 64);
        repository.queue_output("s1".into(), 0, b"old".to_vec(), 64);
        assert!(repository.flush_pending(Duration::from_secs(2)).await);
        let changed = repository.inner.idle_changed.notified();
        repository.queue_delete("s1");
        tokio::time::timeout(Duration::from_secs(2), changed)
            .await
            .unwrap();
        assert!(repository.flush_pending(Duration::ZERO).await);
        assert!(repository
            .inner
            .store
            .read("s1", 64)
            .await
            .unwrap()
            .is_none());
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn background_flush_persists_buffered_output() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b""), 1024);
        repository.queue_output("s1".to_string(), 0, b"hello".to_vec(), 1024);
        repository.wait_until_idle("s1").await;

        let persisted = repository
            .inner
            .store
            .read("s1", 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"hello");
    }

    #[tokio::test]
    async fn flush_pending_waits_for_durable_ack_and_releases_memory() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        repository.queue_checkpoint(checkpoint("s1", b""), 1024);
        repository.queue_output("s1".to_string(), 0, b"hello".to_vec(), 1024);

        assert!(repository.flush_pending(Duration::from_secs(2)).await);
        assert!(!repository.lock_sessions().contains_key("s1"));
        let persisted = repository
            .inner
            .store
            .read("s1", 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"hello");
    }

    #[tokio::test]
    async fn parallel_clean_cold_reads_do_not_create_cache_or_workers() {
        let dir = tempfile::tempdir().unwrap();
        let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
        let cap = 512 * 1024;
        let payload = vec![0x9b; cap / 2];
        let ids: Vec<_> = (0..8).map(|index| format!("cold-{index}")).collect();
        for id in &ids {
            store.upsert(checkpoint(id, b"")).await.unwrap();
            store.append_output(id, 0, &payload).await.unwrap();
        }
        let repository = TerminalHostHistoryRepository::from_store(store);
        // Stop background workers from retiring so a spurious cache or
        // trim request cannot disappear before we inspect it.
        let slots = repository
            .inner
            .flush_slots
            .acquire_many(HISTORY_FLUSH_CONCURRENCY as u32)
            .await
            .unwrap();
        let reads =
            futures_util::future::join_all(ids.iter().map(|id| repository.read(id, cap))).await;
        for read in reads {
            assert_eq!(read.unwrap().unwrap().buffer, payload);
        }
        assert!(repository.lock_sessions().is_empty());
        assert!(repository.flush_pending(Duration::from_millis(50)).await);
        drop(slots);
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn bounded_cold_reads_keep_cache_and_trim_until_durable_ack() {
        let dir = tempfile::tempdir().unwrap();
        let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
        store.upsert(checkpoint("s1", b"")).await.unwrap();
        store.append_output("s1", 0, b"abcdef").await.unwrap();
        let repository = TerminalHostHistoryRepository::from_store(store);
        let slots = repository
            .inner
            .flush_slots
            .acquire_many(HISTORY_FLUSH_CONCURRENCY as u32)
            .await
            .unwrap();

        assert_eq!(
            repository.read("s1", 3).await.unwrap().unwrap().buffer,
            b"def"
        );
        {
            let sessions = repository.lock_sessions();
            let state = &sessions["s1"];
            assert!(state.worker_running);
            assert_eq!(state.trim_request.unwrap().max_bytes, 3);
            assert_eq!(copy_deque_tail(&state.memory_tail, 100), b"def");
            assert!(state.verified_tail.is_none());
        }
        // A larger read must not resurrect the already logically trimmed prefix.
        assert_eq!(
            repository.read("s1", 100).await.unwrap().unwrap().buffer,
            b"def"
        );
        assert!(!repository.flush_pending(Duration::from_millis(20)).await);
        drop(slots);
        assert!(repository.flush_pending(Duration::from_secs(2)).await);
        assert!(repository.lock_sessions().is_empty());
        assert_eq!(
            repository
                .inner
                .store
                .read("s1", 100)
                .await
                .unwrap()
                .unwrap()
                .buffer,
            b"def"
        );
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn cold_read_completion_prefers_newer_pending_checkpoint_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        let slots = repository
            .inner
            .flush_slots
            .acquire_many(HISTORY_FLUSH_CONCURRENCY as u32)
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"new"), 100);
        for maintenance_required in [false, true] {
            let read = repository
                .finish_cold_read("s1", 3, checkpoint("s1", b"old"), maintenance_required)
                .unwrap();
            assert_eq!(read.buffer, b"new");
            assert_eq!(repository.lock_sessions()["s1"].max_bytes, 100);
        }
        repository.queue_delete("s1");
        for maintenance_required in [false, true] {
            assert!(repository
                .finish_cold_read("s1", 3, checkpoint("s1", b"old"), maintenance_required)
                .is_none());
        }
        drop(slots);
        assert!(repository.flush_pending(Duration::from_secs(2)).await);
        repository.inner.store.close_for_test().await;
    }

    #[tokio::test]
    async fn cold_read_schedules_physical_trim_without_blocking_the_read_contract() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
            store.upsert(checkpoint("s1", b"")).await.unwrap();
            store.append_output("s1", 0, b"abc").await.unwrap();
            store.append_output("s1", 1, b"de").await.unwrap();
            store.append_output("s1", 2, b"fg").await.unwrap();
        }
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();

        let read = repository.read("s1", 5).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"cdefg");

        repository.wait_until_idle("s1").await;
        let persisted = repository
            .inner
            .store
            .read("s1", 100)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.buffer, b"cdefg");
    }

    #[tokio::test]
    async fn sqlite_failure_keeps_dirty_history_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.inner.store.close_for_test().await;

        repository.queue_checkpoint(checkpoint("s1", b"abc"), 1024);
        repository.queue_output("s1".to_string(), 0, b"def".to_vec(), 1024);

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let failed = repository
                    .lock_sessions()
                    .get("s1")
                    .is_some_and(|state| state.last_error.is_some());
                if failed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("closed SQLite pool should fail the background flush");

        let read = repository.read("s1", 1024).await.unwrap().unwrap();
        assert_eq!(read.buffer, b"abcdef");
        let sessions = repository.lock_sessions();
        let state = sessions.get("s1").unwrap();
        assert!(state.checkpoint_revision.is_some());
        assert!(!state.outputs.is_empty());
        assert!(state.worker_running);
    }

    #[tokio::test]
    async fn delete_hides_cached_history_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"hello"), 1024);

        repository.queue_delete("s1");

        assert!(repository.read("s1", 1024).await.unwrap().is_none());
    }

    #[test]
    fn pending_output_cap_keeps_latest_bytes_without_acknowledging_old_snapshot() {
        let mut state = SessionPersistenceState {
            max_bytes: 5,
            ..Default::default()
        };
        let first_revision = state.next_revision();
        state.insert_pending_output(0, PendingOutput::from_static(first_revision, b"abcd"));
        let second_revision = state.next_revision();
        state.insert_pending_output(1, PendingOutput::from_static(second_revision, b"efgh"));

        let snapshot = state.snapshot().unwrap();
        state.trim_pending_outputs();

        assert_eq!(state.outputs[&0].data.as_ref(), b"d");
        assert_ne!(state.outputs[&0].revision, first_revision);
        assert_eq!(state.outputs[&1].data.as_ref(), b"efgh");
        assert_eq!(state.pending_bytes, 5);
        let FlushSnapshot::Persist { outputs, .. } = snapshot else {
            panic!("expected persist snapshot");
        };
        assert_eq!(
            outputs[0].2.as_ref(),
            b"abcd",
            "in-flight snapshot remains intact"
        );
        assert_eq!(
            outputs[1].2.as_ptr(),
            state.outputs[&1].data.as_ptr(),
            "unchanged output shares allocation"
        );
        state.insert_pending_output(1, PendingOutput::from_static(second_revision, b"xyz"));
        assert_eq!(state.pending_bytes, 4);
        state.remove_pending_output(0);
        state.remove_pending_output(1);
        assert_eq!(state.pending_bytes, 0);
    }

    #[test]
    fn pending_trim_amortizes_copies_and_bounds_backing_capacity() {
        let size = 1024 * 1024;
        let source: Vec<u8> = (0..size).map(|index| index as u8).collect();
        let mut state = SessionPersistenceState {
            max_bytes: size,
            ..Default::default()
        };
        let revision = state.next_revision();
        state.insert_pending_output(0, PendingOutput::from_vec(revision, source.clone()));
        let snapshot = state.snapshot().unwrap();

        // Repeated one-byte shrinks used to copy nearly 8 GiB across
        // these 8192 calls. The new path retains the same backing.
        let original_ptr = state.outputs[&0].data.as_ptr();
        for discarded in 1..=8192 {
            state.max_bytes = size - discarded;
            state.trim_pending_outputs();
            let output = &state.outputs[&0];
            assert_eq!(output.data.as_ptr(), original_ptr.wrapping_add(discarded));
            assert_eq!(output.backing_bytes, size);
            assert_eq!(output.data.len(), size - discarded);
            assert_eq!(state.pending_bytes, size - discarded);
        }
        assert_eq!(state.outputs[&0].data.as_ref(), &source[8192..]);
        // At exactly half, the existing backing still meets the bound.
        state.max_bytes = size / 2;
        state.trim_pending_outputs();
        assert_eq!(
            state.outputs[&0].data.as_ptr(),
            original_ptr.wrapping_add(size / 2)
        );
        // Crossing the threshold compacts the live queue. The snapshot
        // retains its original bytes independently until it is acknowledged.
        state.max_bytes -= 1;
        state.trim_pending_outputs();
        assert_eq!(state.outputs[&0].backing_bytes, state.max_bytes);
        assert_ne!(
            state.outputs[&0].data.as_ptr(),
            original_ptr.wrapping_add(size - state.max_bytes)
        );
        assert_eq!(
            state.outputs[&0].data.as_ref(),
            &source[size - state.max_bytes..]
        );
        assert_ne!(state.outputs[&0].revision, revision);
        let FlushSnapshot::Persist { outputs, .. } = snapshot else {
            panic!("expected persist snapshot");
        };
        assert_eq!(outputs[0].2.as_ref(), source);
        state.max_bytes = 0;
        state.trim_pending_outputs();
        assert!(state.outputs.is_empty());
        assert_eq!(state.pending_bytes, 0);
    }

    #[test]
    fn pending_output_compacts_excessive_vec_capacity_but_reuses_normal_allocations() {
        let mut oversized = Vec::with_capacity(1024 * 1024);
        oversized.extend_from_slice(b"abc");
        let output = PendingOutput::from_vec(1, oversized);
        assert_eq!(output.data.as_ref(), b"abc");
        assert_eq!(output.backing_bytes, 3);

        let mut normal = Vec::with_capacity(128);
        normal.resize(100, 0xff);
        let ptr = normal.as_ptr();
        let capacity = normal.capacity();
        let mut output = PendingOutput::from_vec(2, normal);
        assert_eq!(output.data.as_ptr(), ptr);
        assert_eq!(output.backing_bytes, capacity);
        output.trim_prefix(36, 3);
        assert_eq!(output.data.as_ptr(), ptr.wrapping_add(36));
        assert_eq!(output.backing_bytes, 128);
        let snapshot = output.data.clone();
        output.trim_prefix(1, 4);
        assert_eq!(output.backing_bytes, 63);
        assert_eq!(output.data.len(), 63);
        assert_ne!(output.data.as_ptr(), ptr.wrapping_add(37));
        assert_eq!(snapshot.as_ref(), &[0xff; 64]);
        assert_eq!(output.revision, 4);
    }

    #[test]
    fn pending_output_randomized_trims_and_replacements_match_reference() {
        let mut state = SessionPersistenceState {
            max_bytes: 512,
            ..Default::default()
        };
        let mut expected: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
        let mut random = 0x51db_fa93_62a2_441du64;
        let mut next = || {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            random >> 32
        };
        for index in 0..2400 {
            if next() % 3 != 0 {
                let sequence = (next() % 48) as i64;
                let len = (next() % 400 + 1) as usize;
                let mut bytes = Vec::with_capacity(len * (1 + (next() % 8) as usize));
                bytes.extend((0..len).map(|byte| (byte + index) as u8));
                expected.insert(sequence, bytes.clone());
                let revision = state.next_revision();
                state.insert_pending_output(sequence, PendingOutput::from_vec(revision, bytes));
            }
            state.max_bytes = (next() % 1024) as usize;
            state.trim_pending_outputs();

            let mut excess = expected
                .values()
                .map(Vec::len)
                .sum::<usize>()
                .saturating_sub(state.max_bytes);
            while excess > 0 {
                let sequence = *expected.keys().next().unwrap();
                let len = expected[&sequence].len();
                if len <= excess {
                    expected.remove(&sequence);
                    excess -= len;
                } else {
                    expected.get_mut(&sequence).unwrap().drain(..excess);
                    excess = 0;
                }
            }
            assert_eq!(state.outputs.len(), expected.len(), "iteration {index}");
            let mut total = 0;
            let mut backing = 0;
            for (sequence, output) in &state.outputs {
                assert_eq!(
                    output.data.as_ref(),
                    expected[sequence],
                    "iteration {index}"
                );
                total += output.data.len();
                backing += output.backing_bytes;
                assert!(output.backing_bytes <= output.data.len() * 2);
            }
            assert_eq!(state.pending_bytes, total);
            assert!(total <= state.max_bytes);
            assert!(backing <= total * 2);
        }
    }

    /// Compare the original eager copy against the production queue trim
    /// on the same byte-by-byte workload; count only copied payload bytes.
    #[test]
    #[ignore = "manual pending output trim microbenchmark"]
    fn benchmark_pending_output_trim() {
        use std::time::Instant;
        const SIZE: usize = 1024 * 1024;
        const STEPS: usize = 4096;
        let seed: Vec<u8> = (0..SIZE).map(|index| index as u8).collect();
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut copied_before = 0u64;
        let mut copied_after = 0u64;
        for round in 0..9 {
            for optimized in [round % 2 != 0, round % 2 == 0] {
                let mut state = SessionPersistenceState {
                    max_bytes: SIZE,
                    ..Default::default()
                };
                state.insert_pending_output(0, PendingOutput::from_vec(1, seed.clone()));
                // A durable flush may already hold a shared snapshot.
                let snapshot = state.snapshot().unwrap();
                let mut copied = 0u64;
                let start = Instant::now();
                for _ in 0..STEPS {
                    state.max_bytes -= 1;
                    if optimized {
                        let backing = state.outputs[&0].backing_bytes;
                        state.trim_pending_outputs();
                        let output = &state.outputs[&0];
                        if output.backing_bytes != backing {
                            copied += output.data.len() as u64;
                        }
                    } else {
                        let output = state.outputs.get_mut(&0).unwrap();
                        output.data = Bytes::copy_from_slice(&output.data[1..]);
                        copied += output.data.len() as u64;
                        output.backing_bytes = output.data.len();
                        state.pending_bytes -= 1;
                    }
                }
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(state.outputs[&0].data.as_ref(), &seed[STEPS..]);
                let FlushSnapshot::Persist { outputs, .. } = snapshot else {
                    panic!("expected snapshot");
                };
                assert_eq!(outputs[0].2.as_ref(), seed);
                if round >= 2 {
                    if optimized {
                        after.push(ms);
                        copied_after = copied;
                    } else {
                        before.push(ms);
                        copied_before = copied;
                    }
                }
            }
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        eprintln!("pending_trim_bench steps={STEPS} old_copied_bytes={copied_before} new_copied_bytes={copied_after} old_ms={:.4} new_ms={:.4}",before[3],after[3]);
        assert_eq!(copied_after, 0);
    }

    #[test]
    fn ring_tail_retains_order_across_wrap_and_smaller_read_limits() {
        let mut ring: VecDeque<u8> = VecDeque::from(b"abcd".to_vec());
        for byte in b"efghijkl" {
            append_tail(&mut ring, &[*byte], 4);
        }
        assert_eq!(copy_deque_tail(&ring, 4), b"ijkl");
        assert_eq!(copy_deque_tail(&ring, 2), b"kl");
        assert!(copy_deque_tail(&ring, 0).is_empty());
        retain_deque_tail(&mut ring, 3);
        assert_eq!(copy_deque_tail(&ring, 9), b"jkl");
        append_tail(&mut ring, b"oversized", 4);
        assert_eq!(copy_deque_tail(&ring, 4), b"ized");
        append_tail(&mut ring, b"x", 0);
        assert!(ring.is_empty());
    }

    #[tokio::test]
    async fn cached_history_read_respects_requested_tail_size() {
        let dir = tempfile::tempdir().unwrap();
        let repository = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
        repository.queue_checkpoint(checkpoint("s1", b"abcdef"), 1024);
        repository.queue_output("s1".into(), 0, b"ghij".to_vec(), 1024);
        assert_eq!(
            repository.read("s1", 3).await.unwrap().unwrap().buffer,
            b"hij"
        );
        assert!(repository
            .read("s1", 0)
            .await
            .unwrap()
            .unwrap()
            .buffer
            .is_empty());
        assert_eq!(
            repository.read("s1", 1024).await.unwrap().unwrap().buffer,
            b"abcdefghij"
        );
    }
}
