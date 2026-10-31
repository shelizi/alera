//! Reconnectable, loopback-only control plane for an isolated Windows PTY.
//!
//! The worker owns the ConPTY and a capped output tail. A Host crash closes its
//! TCP socket but not the worker or the shell's Job Object. A replacement Host
//! may authenticate with the retained session token and request a tail replay.
//! This is only used when ALERA_TERMINAL_WORKER_PROCESS is explicitly enabled.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use alera_core::runtime::{create_private_runtime_file, prepare_private_runtime_directory};
use portable_pty::{MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

use super::{read_frame, write_frame, MAX_WRITE_BYTES};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::resources::seal_shell_process;

const SCHEMA_VERSION: u32 = 1;
const RECOVERY_BACKLOG_MAX: usize = 4 * 1024 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerRecord {
    schema_version: u32,
    session_id: String,
    workspace_id: String,
    tab_id: String,
    working_directory: String,
    worker_pid: u32,
    worker_start_time: u64,
    shell_pid: u32,
    port: u16,
    token: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthRequest {
    session_id: String,
    token: String,
    resume_cursor: u64,
}

fn manifest_path(runtime_dir: &Path, session_id: &str) -> PathBuf {
    let name = hex::encode(Sha256::digest(session_id.as_bytes()));
    runtime_dir.join("pty-workers").join(format!("{name}.json"))
}

fn read_record(path: &Path) -> HostResult<Option<WorkerRecord>> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(HostError::state(format!(
                "PTY worker registry unavailable: {error}"
            )))
        }
    };
    if data.len() > 8192 {
        return Err(HostError::state("PTY worker registry exceeds size limit"));
    }
    let record: WorkerRecord = serde_json::from_slice(&data)
        .map_err(|error| HostError::state(format!("PTY worker registry is invalid: {error}")))?;
    if record.schema_version != SCHEMA_VERSION
        || record.token.len() != 36
        || record.port == 0
        || record.worker_pid == 0
        || record.worker_start_time == 0
    {
        return Err(HostError::state(
            "PTY worker registry has unsupported identity",
        ));
    }
    Ok(Some(record))
}

fn worker_is_alive(record: &WorkerRecord) -> bool {
    seal_shell_process(record.worker_pid)
        .is_some_and(|worker| worker.start_time == record.worker_start_time)
}

/// An existing worker must never be silently replaced because its host cannot
/// contact it; creating a second shell under the same session id is unsafe.
pub(super) fn has_recoverable_worker(runtime_dir: &Path, session_id: &str, tab_id: &str) -> bool {
    read_record(&manifest_path(runtime_dir, session_id))
        .ok()
        .flatten()
        .is_some_and(|record| {
            record.session_id == session_id && record.tab_id == tab_id && worker_is_alive(&record)
        })
}

pub(super) struct WorkerAttachment {
    pub(super) worker_pid: u32,
    pub(super) stream: TcpStream,
    pub(super) process: Arc<OwnedHandle>,
    pub(super) shell_pid: u32,
    pub(super) replay_start_cursor: u64,
    pub(super) working_directory: String,
}

/// None means there is no live worker and a new shell may be spawned. If a
/// worker still exists, every protocol/authorization failure is a hard error.
pub(super) fn attach_existing(
    runtime_dir: &Path,
    session_id: &str,
    tab_id: &str,
    resume_cursor: u64,
) -> HostResult<Option<WorkerAttachment>> {
    let Some(record) = read_record(&manifest_path(runtime_dir, session_id))? else {
        return Ok(None);
    };
    if record.session_id != session_id || record.tab_id != tab_id {
        return Err(HostError::state(
            "PTY worker registry identity does not match the requested terminal",
        ));
    }
    if !worker_is_alive(&record) {
        // A stale manifest is not a reason to attach to a recycled PID.
        // A fresh worker will replace it only after this check.
        return Ok(None);
    }
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), record.port);
    let mut stream = TcpStream::connect_timeout(&address, CONNECTION_TIMEOUT).map_err(|error| {
        HostError::state(format!("live PTY worker cannot be reconnected: {error}"))
    })?;
    stream
        .set_nodelay(true)
        .map_err(|error| HostError::state(error.to_string()))?;
    stream
        .set_read_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|error| HostError::state(error.to_string()))?;
    stream
        .set_write_timeout(Some(CONNECTION_TIMEOUT))
        .map_err(|error| HostError::state(error.to_string()))?;
    let request = serde_json::to_vec(&AuthRequest {
        session_id: session_id.to_string(),
        token: record.token.clone(),
        resume_cursor,
    })
    .map_err(|error| HostError::state(error.to_string()))?;
    write_frame(&mut stream, b'A', &request).map_err(|error| {
        HostError::state(format!("PTY worker reconnection handshake failed: {error}"))
    })?;
    let mut header = [0u8; 13];
    stream
        .read_exact(&mut header)
        .map_err(|error| HostError::state(format!("PTY worker reconnection rejected: {error}")))?;
    if header[0] != b'R' {
        return Err(HostError::state(
            "PTY worker reconnection returned invalid response",
        ));
    }
    let shell_pid = u32::from_le_bytes(header[1..5].try_into().unwrap());
    let replay_start_cursor = u64::from_le_bytes(header[5..13].try_into().unwrap());
    if shell_pid != record.shell_pid || replay_start_cursor < resume_cursor {
        return Err(HostError::state(
            "PTY worker returned inconsistent stream or shell identity",
        ));
    }
    // Open and seal the process after authenticating; a recycled PID or
    // replaced executable cannot satisfy both this check and the stored time.
    if !worker_is_alive(&record) {
        return Err(HostError::state(
            "PTY worker changed identity during reconnect",
        ));
    }
    let handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            false,
            record.worker_pid,
        )
    }
    .map_err(|error| HostError::state(format!("PTY worker handle unavailable: {error}")))?;
    let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    stream
        .set_read_timeout(None)
        .map_err(|error| HostError::state(error.to_string()))?;
    Ok(Some(WorkerAttachment {
        worker_pid: record.worker_pid,
        stream,
        process: Arc::new(handle),
        shell_pid,
        replay_start_cursor,
        working_directory: record.working_directory,
    }))
}

struct OutputTail {
    end: u64,
    limit: usize,
    tail: VecDeque<u8>,
    socket: Option<TcpStream>,
    generation: u64,
}

impl OutputTail {
    fn push(&mut self, bytes: &[u8]) {
        self.end = self.end.saturating_add(bytes.len() as u64);
        if bytes.len() >= self.limit {
            self.tail.clear();
            self.tail
                .extend(bytes[bytes.len() - self.limit..].iter().copied());
        } else {
            self.tail.extend(bytes.iter().copied());
            while self.tail.len() > self.limit {
                self.tail.pop_front();
            }
        }
        if let Some(socket) = self.socket.as_mut() {
            if socket.write_all(bytes).is_err() {
                self.socket.take();
            }
        }
    }

    fn replay_from(&self, cursor: u64) -> (u64, Vec<u8>) {
        let begin = self.end.saturating_sub(self.tail.len() as u64);
        let start = cursor.max(begin);
        let count = (start - begin) as usize;
        (start, self.tail.iter().skip(count).copied().collect())
    }
}

#[derive(Clone)]
pub(super) struct RecoveryOutput(Arc<Mutex<OutputTail>>);

impl RecoveryOutput {
    pub(super) fn forward(&self, bytes: &[u8]) {
        if let Ok(mut tail) = self.0.lock() {
            tail.push(bytes);
        }
    }
}

pub(super) struct WorkerRecoveryServer {
    tail: Arc<Mutex<OutputTail>>,
    stop: Arc<AtomicBool>,
    registry: PathBuf,
    worker_pid: u32,
}

impl WorkerRecoveryServer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start(
        runtime_dir: &Path,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
        working_directory: &str,
        shell_pid: u32,
        initial_cursor: u64,
        max_backlog_bytes: usize,
        writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
        master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    ) -> anyhow::Result<Self> {
        if session_id.is_empty() || tab_id.is_empty() || shell_pid == 0 {
            anyhow::bail!("PTY worker recovery identity is incomplete");
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let worker_pid = std::process::id();
        let worker_start_time = seal_shell_process(worker_pid)
            .ok_or_else(|| anyhow::anyhow!("cannot seal PTY worker process identity"))?
            .start_time;
        let registry = manifest_path(runtime_dir, session_id);
        let parent = registry
            .parent()
            .expect("registry path includes workers directory");
        prepare_private_runtime_directory(parent)?;
        let record = WorkerRecord {
            schema_version: SCHEMA_VERSION,
            session_id: session_id.to_string(),
            workspace_id: workspace_id.to_string(),
            tab_id: tab_id.to_string(),
            working_directory: working_directory.to_string(),
            worker_pid,
            worker_start_time,
            shell_pid,
            port: listener.local_addr()?.port(),
            token: uuid::Uuid::new_v4().to_string(),
        };
        // No shell environment or API credentials ever enter the manifest.
        let body = serde_json::to_vec(&record)?;
        let tmp = registry.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let write_result = (|| -> io::Result<()> {
            let mut file = create_private_runtime_file(&tmp)?;
            file.write_all(&body)?;
            file.sync_all()?;
            std::fs::rename(&tmp, &registry)
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        write_result?;

        let tail = Arc::new(Mutex::new(OutputTail {
            end: initial_cursor,
            limit: max_backlog_bytes.clamp(64 * 1024, RECOVERY_BACKLOG_MAX),
            tail: VecDeque::new(),
            socket: None,
            generation: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_tail = Arc::clone(&tail);
        let thread_stop = Arc::clone(&stop);
        let thread_record = record.clone();
        let thread_result = thread::Builder::new()
            .name("alera-pty-worker-reconnect".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, addr)) if addr.ip().is_loopback() => {
                            let stream_tail = Arc::clone(&thread_tail);
                            let stream_writer = Arc::clone(&writer);
                            let stream_master = Arc::clone(&master);
                            let current_record = thread_record.clone();
                            let _ = thread::Builder::new()
                                .name("alera-pty-worker-client".to_string())
                                .spawn(move || {
                                    serve_client(
                                        stream,
                                        current_record,
                                        stream_tail,
                                        stream_writer,
                                        stream_master,
                                    );
                                });
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
            });
        if let Err(error) = thread_result {
            let _ = std::fs::remove_file(&registry);
            return Err(error.into());
        }
        Ok(Self {
            tail,
            stop,
            registry,
            worker_pid,
        })
    }

    pub(super) fn output(&self) -> RecoveryOutput {
        RecoveryOutput(Arc::clone(&self.tail))
    }
}

impl Drop for WorkerRecoveryServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Prevent an older worker from removing a newer generation's record.
        if let Ok(Some(record)) = read_record(&self.registry) {
            if record.worker_pid == self.worker_pid {
                let _ = std::fs::remove_file(&self.registry);
            }
        }
    }
}

fn serve_client(
    mut stream: TcpStream,
    record: WorkerRecord,
    tail: Arc<Mutex<OutputTail>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
) {
    // A connection accepted from a nonblocking listener may inherit that
    // socket mode on Windows. After auth, the input reader must block for
    // subsequent terminal keystrokes rather than treating WouldBlock as EOF.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
    let Ok(Some((b'A', payload))) = read_frame(&mut stream) else {
        return;
    };
    let Ok(auth) = serde_json::from_slice::<AuthRequest>(&payload) else {
        return;
    };
    if auth.session_id != record.session_id || auth.token != record.token {
        return;
    }
    let Ok(mut state) = tail.lock() else { return };
    if auth.resume_cursor > state.end {
        return;
    }
    let (start, replay) = state.replay_from(auth.resume_cursor);
    let mut header = Vec::with_capacity(13);
    header.push(b'R');
    header.extend_from_slice(&record.shell_pid.to_le_bytes());
    header.extend_from_slice(&start.to_le_bytes());
    if stream
        .write_all(&header)
        .and_then(|_| stream.write_all(&replay))
        .is_err()
    {
        return;
    }
    // Only the most recent authenticated owner may supply input.
    if let Some(old) = state.socket.take() {
        let _ = old.shutdown(Shutdown::Both);
    }
    if let Ok(outgoing) = stream.try_clone() {
        state.socket = Some(outgoing);
    } else {
        return;
    }
    state.generation = state.generation.wrapping_add(1);
    let generation = state.generation;
    drop(state);
    let _ = stream.set_read_timeout(None);
    while let Ok(Some((kind, payload))) = read_frame(&mut stream) {
        if tail
            .lock()
            .map_or(true, |state| state.generation != generation)
        {
            break;
        }
        match kind {
            b'W' if payload.len() <= MAX_WRITE_BYTES => {
                let Ok(mut locked) = writer.lock() else { break };
                let Some(pty) = locked.as_mut() else { break };
                if pty.write_all(&payload).and_then(|_| pty.flush()).is_err() {
                    break;
                }
            }
            b'R' if payload.len() == 4 => {
                let cols = u16::from_le_bytes([payload[0], payload[1]]);
                let rows = u16::from_le_bytes([payload[2], payload[3]]);
                let Ok(locked) = master.lock() else { break };
                let Some(pty) = locked.as_ref() else { break };
                let _ = pty.resize(PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            _ => break,
        }
    }
    // A superseded Host connection must never close the new owner's socket.
    if let Ok(mut state) = tail.lock() {
        if state.generation == generation {
            state.socket.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_output_tail_is_bounded_and_replays_at_absolute_cursor() {
        let mut buffer = OutputTail {
            end: 100,
            limit: 5,
            tail: VecDeque::new(),
            socket: None,
            generation: 0,
        };
        buffer.push(b"abcdef");
        assert_eq!(buffer.end, 106);
        assert_eq!(buffer.replay_from(103), (103, b"def".to_vec()));
        assert_eq!(buffer.replay_from(100), (101, b"bcdef".to_vec()));
        buffer.push(b"XY");
        assert_eq!(buffer.replay_from(106), (106, b"XY".to_vec()));
        assert_eq!(buffer.tail.len(), 5);
    }

    #[test]
    fn manifest_paths_do_not_interpret_session_id_as_paths() {
        let root = Path::new("C:/alera");
        let path = manifest_path(root, "../C:../\\escape");
        assert_eq!(path.parent().unwrap(), root.join("pty-workers"));
        assert_eq!(path.extension().unwrap(), "json");
    }
}
