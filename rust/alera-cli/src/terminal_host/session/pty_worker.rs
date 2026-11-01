//! Cross-platform PTY process isolation with opt-in Rust Worker transport.
//!
//! The shared Host keeps tab metadata and history, while every worker owns its
//! native PTY and shell (plus a Windows Job Object when applicable).
//! The worker listens only on localhost, protects
//! its connection with a random token and buffers at most 4 MiB of output while
//! the Host is unavailable. Older/default hosts use the in-process PTY path.

use std::io::{self, Read, Write};
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows::Win32::System::Pipes::PeekNamedPipe;
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
};

use super::pty_spawn::{spawn_pty, SpawnedPty};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::TerminalHostLaunch;
use crate::terminal_host::resources::seal_shell_process;
#[cfg(unix)]
use crate::terminal_host::resources::ShellProcess;

#[path = "pty_worker_recovery.rs"]
mod recovery;

pub(crate) const WORKER_ARGUMENT: &str = "__alera-terminal-pty-worker";
const WORKER_ENABLED_ENV: &str = "ALERA_TERMINAL_WORKER_PROCESS";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 64 * 1024;
const WORKER_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchRequest {
    session_id: String,
    workspace_id: String,
    tab_id: String,
    working_directory: String,
    runtime_dir: String,
    launch: serde_json::Value,
    cols: u16,
    rows: u16,
    initial_output_stream_bytes: u64,
    max_backlog_bytes: usize,
}

pub(super) fn enabled() -> bool {
    matches!(
        std::env::var(WORKER_ENABLED_ENV).as_deref(),
        Ok("1" | "true" | "on")
    )
}

pub(crate) fn has_recoverable_worker(runtime_dir: &Path, session_id: &str, tab_id: &str) -> bool {
    enabled() && recovery::has_recoverable_worker(runtime_dir, session_id, tab_id)
}

pub(crate) fn is_invocation() -> bool {
    std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new(WORKER_ARGUMENT))
}

fn write_frame(writer: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PTY frame too large",
        ));
    }
    writer.write_all(&[kind])?;
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

fn read_frame(reader: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    read_frame_bounded(reader, MAX_FRAME_BYTES)
}

fn read_frame_bounded(
    reader: &mut impl Read,
    max_bytes: usize,
) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut kind = [0];
    if reader.read(&mut kind)? == 0 {
        return Ok(None);
    }
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME_BYTES || length > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PTY frame length exceeds limit",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(Some((kind[0], payload)))
}

/// Polls the startup pipe so a wedged child cannot hang the whole Host.
#[cfg(windows)]
fn wait_for_worker_bytes(
    reader: &std::process::ChildStdout,
    minimum: u32,
    deadline: Instant,
) -> HostResult<()> {
    loop {
        let mut available = 0u32;
        unsafe {
            PeekNamedPipe(
                HANDLE(reader.as_raw_handle()),
                None,
                0,
                None,
                Some(&mut available),
                None,
            )
        }
        .map_err(|error| HostError::state(format!("PTY worker handshake pipe closed: {error}")))?;
        if available >= minimum {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(HostError::state("PTY worker startup exceeded 15 seconds"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn wait_for_worker_bytes(
    reader: &std::process::ChildStdout,
    minimum: u32,
    deadline: Instant,
) -> HostResult<()> {
    // Linux and macOS support FIONREAD on pipes. Do not block waiting for
    // a partial response from a Worker that has failed during startup.
    loop {
        let mut available: libc::c_int = 0;
        let status = unsafe { libc::ioctl(reader.as_raw_fd(), libc::FIONREAD, &mut available) };
        if status < 0 {
            return Err(HostError::state(format!(
                "PTY Worker startup pipe failed: {}",
                io::Error::last_os_error()
            )));
        }
        if available >= 0 && available as u32 >= minimum {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(HostError::state("PTY Worker startup exceeded 15 seconds"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn lock_error() -> io::Error {
    io::Error::other("PTY worker pipe lock poisoned")
}

struct WorkerConnection {
    socket: Arc<Mutex<TcpStream>>,
    resize_tx: SyncSender<PtySize>,
}

struct WorkerMaster {
    connection: WorkerConnection,
    stdout: Mutex<Option<TcpStream>>,
    size: Mutex<PtySize>,
    writer_taken: AtomicBool,
}

impl MasterPty for WorkerMaster {
    fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        self.connection
            .resize_tx
            .try_send(size)
            .map_err(|error| anyhow::anyhow!("PTY worker resize unavailable: {error}"))?;
        *self.size.lock().map_err(|_| lock_error())? = size;
        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        Ok(*self.size.lock().map_err(|_| lock_error())?)
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn Read + Send>> {
        let stream = self
            .stdout
            .lock()
            .map_err(|_| lock_error())?
            .take()
            .ok_or_else(|| anyhow::anyhow!("PTY worker reader already taken"))?;
        Ok(Box::new(stream))
    }

    fn take_writer(&self) -> anyhow::Result<Box<dyn Write + Send>> {
        if self.writer_taken.swap(true, Ordering::AcqRel) {
            anyhow::bail!("PTY worker writer already taken");
        }
        Ok(Box::new(WorkerWriter {
            socket: Arc::clone(&self.connection.socket),
        }))
    }
}

struct WorkerWriter {
    socket: Arc<Mutex<TcpStream>>,
}

impl Write for WorkerWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(MAX_WRITE_BYTES);
        write_frame(
            &mut *self.socket.lock().map_err(|_| lock_error())?,
            b'W',
            &bytes[..count],
        )?;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.socket.lock().map_err(|_| lock_error())?.flush()
    }
}

fn worker_streams(
    stream: TcpStream,
    cols: u16,
    rows: u16,
) -> HostResult<(
    Box<dyn MasterPty + Send>,
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
)> {
    let reader = stream
        .try_clone()
        .map_err(|error| HostError::state(error.to_string()))?;
    let input = Arc::new(Mutex::new(stream));
    let (resize_tx, resize_rx) = sync_channel::<PtySize>(8);
    let resize_input = Arc::clone(&input);
    std::thread::Builder::new()
        .name("alera-pty-worker-resize".to_string())
        .spawn(move || {
            while let Ok(size) = resize_rx.recv() {
                let payload = [size.cols.to_le_bytes(), size.rows.to_le_bytes()].concat();
                let Ok(mut socket) = resize_input.lock() else {
                    break;
                };
                if write_frame(&mut *socket, b'R', &payload).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| HostError::state(format!("PTY resize thread: {error}")))?;
    let master: Box<dyn MasterPty + Send> = Box::new(WorkerMaster {
        connection: WorkerConnection {
            socket: input,
            resize_tx,
        },
        stdout: Mutex::new(Some(reader)),
        size: Mutex::new(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        }),
        writer_taken: AtomicBool::new(false),
    });
    let reader = master
        .try_clone_reader()
        .map_err(|error| HostError::state(error.to_string()))?;
    let writer = master
        .take_writer()
        .map_err(|error| HostError::state(error.to_string()))?;
    Ok((master, reader, writer))
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct WorkerKiller(Arc<OwnedHandle>);

#[cfg(unix)]
#[derive(Debug, Clone)]
struct WorkerKiller(ShellProcess);

#[cfg(windows)]
impl ChildKiller for WorkerKiller {
    fn kill(&mut self) -> io::Result<()> {
        unsafe { TerminateProcess(HANDLE(self.0.as_raw_handle()), 1) }.map_err(io::Error::other)
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(self.clone())
    }
}

#[cfg(unix)]
impl ChildKiller for WorkerKiller {
    fn kill(&mut self) -> io::Result<()> {
        // Refuse to signal a recycled Worker PID.
        if seal_shell_process(self.0.pid).is_none_or(|p| p.start_time != self.0.start_time) {
            return Ok(());
        }
        let status = unsafe { libc::kill(self.0.pid as libc::pid_t, libc::SIGTERM) };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(self.clone())
    }
}

#[derive(Debug)]
enum WorkerChildHandle {
    Spawned(std::process::Child),
    #[cfg(windows)]
    Attached(Arc<OwnedHandle>),
    #[cfg(unix)]
    Attached(ShellProcess),
}

#[derive(Debug)]
struct WorkerChild {
    child: WorkerChildHandle,
    killer: WorkerKiller,
    shell_pid: Option<u32>,
}

#[cfg(windows)]
fn process_status(handle: &OwnedHandle) -> io::Result<ExitStatus> {
    let mut code = 0u32;
    unsafe { GetExitCodeProcess(HANDLE(handle.as_raw_handle()), &mut code) }
        .map_err(io::Error::other)?;
    Ok(ExitStatus::with_exit_code(code))
}

impl ChildKiller for WorkerChild {
    fn kill(&mut self) -> io::Result<()> {
        self.killer.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        self.killer.clone_killer()
    }
}

impl Child for WorkerChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match &mut self.child {
            WorkerChildHandle::Spawned(child) => child.try_wait().map(|s| s.map(Into::into)),
            #[cfg(windows)]
            WorkerChildHandle::Attached(handle) => {
                match unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 0) } {
                    WAIT_OBJECT_0 => Ok(Some(process_status(handle)?)),
                    WAIT_TIMEOUT => Ok(None),
                    _ => Err(io::Error::last_os_error()),
                }
            }
            #[cfg(unix)]
            WorkerChildHandle::Attached(worker) => Ok(
                if seal_shell_process(worker.pid).is_some_and(|p| p.start_time == worker.start_time)
                {
                    None
                } else {
                    Some(ExitStatus::with_exit_code(1))
                },
            ),
        }
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        match &mut self.child {
            WorkerChildHandle::Spawned(child) => child.wait().map(Into::into),
            #[cfg(windows)]
            WorkerChildHandle::Attached(handle) => {
                match unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), u32::MAX) } {
                    WAIT_OBJECT_0 => process_status(handle),
                    _ => Err(io::Error::last_os_error()),
                }
            }
            #[cfg(unix)]
            WorkerChildHandle::Attached(worker) => loop {
                if seal_shell_process(worker.pid).is_none_or(|p| p.start_time != worker.start_time)
                {
                    return Ok(ExitStatus::with_exit_code(1));
                }
                std::thread::sleep(Duration::from_millis(30));
            },
        }
    }

    fn process_id(&self) -> Option<u32> {
        self.shell_pid
    }

    #[cfg(windows)]
    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        Some(match &self.child {
            WorkerChildHandle::Spawned(child) => AsRawHandle::as_raw_handle(child),
            WorkerChildHandle::Attached(handle) => handle.as_raw_handle(),
        })
    }
}

fn assemble_worker(
    attachment: recovery::WorkerAttachment,
    process: WorkerChildHandle,
    cols: u16,
    rows: u16,
) -> HostResult<SpawnedPty> {
    let recovered_cursor = attachment.replay_start_cursor;
    let working_directory = attachment.working_directory;
    let shell_pid = attachment.shell_pid;
    #[cfg(windows)]
    let handle = Arc::clone(&attachment.process);
    #[cfg(unix)]
    let worker = seal_shell_process(attachment.worker_pid)
        .filter(|identity| identity.start_time == attachment.worker_start_time)
        .ok_or_else(|| HostError::state("Unix Worker identity changed during PTY attachment"))?;
    let (master, reader, writer) = worker_streams(attachment.stream, cols, rows)?;
    let child = WorkerChild {
        child: process,
        #[cfg(windows)]
        killer: WorkerKiller(Arc::clone(&handle)),
        #[cfg(unix)]
        killer: WorkerKiller(worker),
        shell_pid: Some(shell_pid),
    };
    let killer = child.clone_killer();
    Ok(SpawnedPty {
        child: Box::new(child),
        master,
        reader,
        writer,
        killer,
        shell: seal_shell_process(shell_pid),
        #[cfg(windows)]
        process_job: None,
        #[cfg(windows)]
        worker_process: Some(handle),
        replay_start_cursor: Some(recovered_cursor),
        recovered_working_directory: Some(working_directory),
    })
}

pub(super) fn try_recover_worker(
    runtime_dir: &Path,
    session_id: &str,
    tab_id: &str,
    cols: u16,
    rows: u16,
    resume_cursor: u64,
) -> HostResult<Option<SpawnedPty>> {
    let Some(attachment) =
        recovery::attach_existing(runtime_dir, session_id, tab_id, resume_cursor)?
    else {
        return Ok(None);
    };
    #[cfg(windows)]
    let process = WorkerChildHandle::Attached(Arc::clone(&attachment.process));
    #[cfg(unix)]
    let process = WorkerChildHandle::Attached(
        seal_shell_process(attachment.worker_pid)
            .filter(|p| p.start_time == attachment.worker_start_time)
            .ok_or_else(|| HostError::state("Unix Worker PID was reused"))?,
    );
    assemble_worker(attachment, process, cols, rows).map(Some)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_worker_pty(
    launch: TerminalHostLaunch,
    session_id: String,
    workspace_id: String,
    tab_id: String,
    working_directory: &str,
    runtime_dir: &Path,
    cols: u16,
    rows: u16,
    initial_output_stream_bytes: u64,
    max_backlog_bytes: usize,
) -> HostResult<SpawnedPty> {
    let exe = std::env::current_exe()
        .map_err(|error| HostError::state(format!("PTY worker executable missing: {error}")))?;
    #[cfg(windows)]
    let mut command = alera_core::child_process::windowless_command(exe.clone());
    #[cfg(unix)]
    let mut command = Command::new(exe);
    command
        .arg(WORKER_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child_slot = Some(
        command
            .spawn()
            .map_err(|error| HostError::state(format!("failed to start PTY worker: {error}")))?,
    );

    let result = (|| {
        let child = child_slot.as_mut().expect("worker child owned");
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| HostError::state("PTY worker input pipe missing"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| HostError::state("PTY worker output pipe missing"))?;
        let deadline = Instant::now() + WORKER_STARTUP_TIMEOUT;
        let request = LaunchRequest {
            session_id: session_id.clone(),
            workspace_id,
            tab_id: tab_id.clone(),
            working_directory: working_directory.to_string(),
            runtime_dir: runtime_dir.to_string_lossy().into_owned(),
            launch: launch.to_json(),
            cols,
            rows,
            initial_output_stream_bytes,
            max_backlog_bytes,
        };
        let payload = serde_json::to_vec(&request)
            .map_err(|error| HostError::state(format!("invalid PTY launch: {error}")))?;
        write_frame(&mut stdin, b'I', &payload)
            .map_err(|error| HostError::state(format!("PTY worker handshake failed: {error}")))?;
        drop(stdin);
        wait_for_worker_bytes(&stdout, 1, deadline)?;
        let mut code = [0u8; 1];
        stdout
            .read_exact(&mut code)
            .map_err(|error| HostError::state(format!("PTY worker readiness failed: {error}")))?;
        if code[0] == b'E' {
            wait_for_worker_bytes(&stdout, 4, deadline)?;
            let mut size = [0u8; 4];
            stdout
                .read_exact(&mut size)
                .map_err(|error| HostError::state(format!("PTY worker error frame: {error}")))?;
            let size = u32::from_le_bytes(size) as usize;
            if size > 4096 {
                return Err(HostError::state("PTY worker rejected oversized error"));
            }
            wait_for_worker_bytes(&stdout, size as u32, deadline)?;
            let mut detail = vec![0; size];
            stdout
                .read_exact(&mut detail)
                .map_err(|error| HostError::state(format!("PTY worker error body: {error}")))?;
            return Err(HostError::state(format!(
                "PTY worker startup failed: {}",
                String::from_utf8_lossy(&detail),
            )));
        }
        if code[0] != b'R' {
            return Err(HostError::state("PTY worker returned invalid ready marker"));
        }
        wait_for_worker_bytes(&stdout, 4, deadline)?;
        let mut pid_bytes = [0u8; 4];
        stdout
            .read_exact(&mut pid_bytes)
            .map_err(|error| HostError::state(format!("PTY worker ready payload: {error}")))?;
        let pid = u32::from_le_bytes(pid_bytes);
        drop(stdout);
        let attachment = recovery::attach_existing(
            runtime_dir,
            &session_id,
            &tab_id,
            initial_output_stream_bytes,
        )?
        .ok_or_else(|| {
            HostError::state("new PTY worker did not publish a live reconnect endpoint")
        })?;
        if attachment.shell_pid != pid
            || child_slot.as_ref().expect("worker exists").id() != attachment.worker_pid
        {
            return Err(HostError::state(
                "PTY worker startup process identity mismatch",
            ));
        }
        let process = WorkerChildHandle::Spawned(child_slot.take().expect("worker exists"));
        assemble_worker(attachment, process, cols, rows)
    })();
    if result.is_err() {
        if let Some(mut child) = child_slot.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    result
}

pub(crate) fn run() -> i32 {
    match run_inner() {
        Ok(code) => code,
        Err(error) => {
            let mut stdout = io::stdout().lock();
            let message = error.to_string();
            let bytes = message.as_bytes();
            let length = bytes.len().min(4096);
            let _ = stdout.write_all(b"E");
            let _ = stdout.write_all(&(length as u32).to_le_bytes());
            let _ = stdout.write_all(&bytes[..length]);
            let _ = stdout.flush();
            1
        }
    }
}

fn run_inner() -> anyhow::Result<i32> {
    let mut input = io::stdin();
    let Some((b'I', payload)) = read_frame(&mut input)? else {
        anyhow::bail!("missing PTY worker initialization");
    };
    let request: LaunchRequest = serde_json::from_slice(&payload)?;
    let launch = TerminalHostLaunch::from_json(&request.launch)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let spawned = spawn_pty(
        launch,
        request.session_id.clone(),
        &request.working_directory,
        request.cols,
        request.rows,
    )?;
    let SpawnedPty {
        mut child,
        master,
        reader,
        writer,
        killer: _,
        shell,
        #[cfg(windows)]
        process_job,
        #[cfg(windows)]
            worker_process: _,
        replay_start_cursor: _,
        recovered_working_directory: _,
    } = spawned;
    let master = Arc::new(Mutex::new(Some(master)));
    let writer = Arc::new(Mutex::new(Some(writer)));
    let shell_pid = shell.map(|process| process.pid).unwrap_or(0);
    let recovery = recovery::WorkerRecoveryServer::start(
        Path::new(&request.runtime_dir),
        &request.session_id,
        &request.workspace_id,
        &request.tab_id,
        &request.working_directory,
        shell_pid,
        request.initial_output_stream_bytes,
        request.max_backlog_bytes,
        Arc::clone(&writer),
        Arc::clone(&master),
    )?;
    let relay = recovery.output();
    {
        let mut output = io::stdout().lock();
        output.write_all(b"R")?;
        output.write_all(&shell_pid.to_le_bytes())?;
        output.flush()?;
    }
    let output_thread = std::thread::Builder::new()
        .name("alera-pty-worker-output".to_string())
        .spawn(move || {
            let mut reader = reader;
            let mut bytes = [0u8; MAX_WRITE_BYTES];
            loop {
                let count = match reader.read(&mut bytes) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => count,
                };
                relay.forward(&bytes[..count]);
            }
        })?;
    let code = child
        .wait()
        .map(|status| status.exit_code() as i32)
        .unwrap_or(-1);
    #[cfg(windows)]
    drop(process_job);
    if let Ok(mut guard) = writer.lock() {
        guard.take();
    }
    if let Ok(mut guard) = master.lock() {
        guard.take();
    }
    drop(recovery);
    // ConPTY may keep the PTY reader waiting until this worker process exits.
    drop(output_thread);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_frame_round_trips_binary_and_rejects_oversized_length() {
        let mut wire = Vec::new();
        write_frame(&mut wire, b'W', &[0, 255, b'\n', 0]).unwrap();
        assert_eq!(
            read_frame(&mut wire.as_slice()).unwrap(),
            Some((b'W', vec![0, 255, b'\n', 0]))
        );
        let mut oversize = vec![b'W'];
        oversize.extend_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
        assert_eq!(
            read_frame(&mut oversize.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let mut too_large_for_auth = vec![b'A'];
        too_large_for_auth.extend_from_slice(&(4097_u32).to_le_bytes());
        assert_eq!(
            read_frame_bounded(&mut too_large_for_auth.as_slice(), 4096)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
