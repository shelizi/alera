//! A Windows-only PTY process boundary.
//!
//! The Host remains the source of truth for tabs, scrollback, and client
//! attachments. Each opted-in worker owns its own ConPTY and shell Job Object:
//! an abort or OOM in one worker cannot abort the Host or another worker.
//! This pipe transport does not yet survive a *Host* crash; recovery needs a
//! reconnectable transport and durable worker discovery before default enable.

use std::io::{self, Read, Write};
use std::os::windows::io::{AsHandle, AsRawHandle, OwnedHandle};
use std::process::{ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::PeekNamedPipe;
use windows::Win32::System::Threading::TerminateProcess;

use super::pty_spawn::{spawn_pty, SpawnedPty};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::TerminalHostLaunch;
use crate::terminal_host::resources::seal_shell_process;

pub(crate) const WORKER_ARGUMENT: &str = "__alera-terminal-pty-worker";
const WORKER_ENABLED_ENV: &str = "ALERA_TERMINAL_WORKER_PROCESS";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 64 * 1024;
const WORKER_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchRequest {
    session_id: String,
    working_directory: String,
    launch: serde_json::Value,
    cols: u16,
    rows: u16,
}

pub(super) fn enabled() -> bool {
    matches!(
        std::env::var(WORKER_ENABLED_ENV).as_deref(),
        Ok("1" | "true" | "on")
    )
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
    let mut kind = [0];
    if reader.read(&mut kind)? == 0 {
        return Ok(None);
    }
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PTY frame length exceeds limit",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(Some((kind[0], payload)))
}

/// Poll without blocking ReadFile, so a hung worker cannot stall Host forever.
fn wait_for_worker_bytes(reader: &ChildStdout, minimum: u32, deadline: Instant) -> HostResult<()> {
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

fn lock_error() -> io::Error {
    io::Error::other("PTY worker pipe lock poisoned")
}

struct WorkerConnection {
    stdin: Arc<Mutex<ChildStdin>>,
    resize_tx: SyncSender<PtySize>,
}

struct WorkerMaster {
    connection: WorkerConnection,
    stdout: Mutex<Option<ChildStdout>>,
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
            stdin: Arc::clone(&self.connection.stdin),
        }))
    }
}

struct WorkerWriter {
    stdin: Arc<Mutex<ChildStdin>>,
}

impl Write for WorkerWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(MAX_WRITE_BYTES);
        write_frame(
            &mut *self.stdin.lock().map_err(|_| lock_error())?,
            b'W',
            &bytes[..count],
        )?;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdin.lock().map_err(|_| lock_error())?.flush()
    }
}

#[derive(Debug, Clone)]
struct WorkerKiller(Arc<OwnedHandle>);

impl ChildKiller for WorkerKiller {
    fn kill(&mut self) -> io::Result<()> {
        unsafe { TerminateProcess(HANDLE(self.0.as_raw_handle()), 1) }.map_err(io::Error::other)
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(self.clone())
    }
}

#[derive(Debug)]
struct WorkerChild {
    child: std::process::Child,
    killer: WorkerKiller,
    shell_pid: Option<u32>,
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
        self.child.try_wait().map(|status| status.map(Into::into))
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().map(Into::into)
    }

    fn process_id(&self) -> Option<u32> {
        self.shell_pid
    }

    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        Some(AsRawHandle::as_raw_handle(&self.child))
    }
}

pub(super) fn spawn_worker_pty(
    launch: TerminalHostLaunch,
    session_id: String,
    working_directory: &str,
    cols: u16,
    rows: u16,
) -> HostResult<SpawnedPty> {
    let exe = std::env::current_exe()
        .map_err(|error| HostError::state(format!("PTY worker executable missing: {error}")))?;
    let mut command = alera_core::child_process::windowless_command(exe);
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

    // Child stays in this option until all fallible startup steps complete.
    // Failed handshakes can then always terminate and reap their own worker.
    let result = (|| {
        let child = child_slot.as_mut().expect("PTY worker child is owned");
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
            session_id,
            working_directory: working_directory.to_string(),
            launch: launch.to_json(),
            cols,
            rows,
        };
        let payload = serde_json::to_vec(&request)
            .map_err(|error| HostError::state(format!("invalid PTY launch: {error}")))?;
        write_frame(&mut stdin, b'I', &payload)
            .map_err(|error| HostError::state(format!("PTY worker handshake failed: {error}")))?;
        let mut code = [0u8; 1];
        wait_for_worker_bytes(&stdout, 1, deadline)?;
        stdout
            .read_exact(&mut code)
            .map_err(|error| HostError::state(format!("PTY worker readiness failed: {error}")))?;
        if code[0] == b'E' {
            let mut size = [0u8; 4];
            wait_for_worker_bytes(&stdout, 4, deadline)?;
            stdout
                .read_exact(&mut size)
                .map_err(|error| HostError::state(format!("PTY worker error frame: {error}")))?;
            let size = u32::from_le_bytes(size) as usize;
            if size > 4096 {
                return Err(HostError::state("PTY worker rejected oversized error"));
            }
            let mut detail = vec![0; size];
            wait_for_worker_bytes(&stdout, size as u32, deadline)?;
            stdout
                .read_exact(&mut detail)
                .map_err(|error| HostError::state(format!("PTY worker error body: {error}")))?;
            return Err(HostError::state(format!(
                "PTY worker startup failed: {}",
                String::from_utf8_lossy(&detail)
            )));
        }
        if code[0] != b'R' {
            return Err(HostError::state("PTY worker returned invalid ready marker"));
        }
        let mut pid_bytes = [0u8; 4];
        wait_for_worker_bytes(&stdout, 4, deadline)?;
        stdout
            .read_exact(&mut pid_bytes)
            .map_err(|error| HostError::state(format!("PTY worker ready payload: {error}")))?;
        let pid = u32::from_le_bytes(pid_bytes);
        let cloned = Arc::new(
            child
                .as_handle()
                .try_clone_to_owned()
                .map_err(|error| HostError::state(format!("PTY worker handle: {error}")))?,
        );
        let (resize_tx, resize_rx) = sync_channel::<PtySize>(8);
        let input = Arc::new(Mutex::new(stdin));
        let resize_input = Arc::clone(&input);
        std::thread::Builder::new()
            .name("alera-pty-worker-resize".to_string())
            .spawn(move || {
                while let Ok(size) = resize_rx.recv() {
                    let payload = [size.cols.to_le_bytes(), size.rows.to_le_bytes()].concat();
                    let Ok(mut pipe) = resize_input.lock() else {
                        break;
                    };
                    if write_frame(&mut *pipe, b'R', &payload).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| HostError::state(format!("PTY resize thread: {error}")))?;
        let master: Box<dyn MasterPty + Send> = Box::new(WorkerMaster {
            connection: WorkerConnection {
                stdin: input,
                resize_tx,
            },
            stdout: Mutex::new(Some(stdout)),
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
        let shell = (pid != 0).then(|| seal_shell_process(pid)).flatten();
        let worker = WorkerChild {
            child: child_slot.take().expect("PTY worker child still owned"),
            killer: WorkerKiller(Arc::clone(&cloned)),
            shell_pid: (pid != 0).then_some(pid),
        };
        let killer = worker.clone_killer();
        Ok(SpawnedPty {
            child: Box::new(worker),
            master,
            reader,
            writer,
            killer,
            shell,
            process_job: None,
            worker_process: Some(cloned),
        })
    })();
    if result.is_err() {
        if let Some(mut child) = child_slot.take() {
            // Worker owns its shell Job; killing it tears down its descendants.
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
        request.session_id,
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
        process_job,
        worker_process: _,
    } = spawned;
    let master = Arc::new(Mutex::new(Some(master)));
    let writer = Arc::new(Mutex::new(Some(writer)));
    let shell_pid = shell.map(|process| process.pid).unwrap_or(0);
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
            let mut output = io::stdout().lock();
            let mut bytes = [0u8; MAX_WRITE_BYTES];
            loop {
                let count = match reader.read(&mut bytes) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => count,
                };
                if output.write_all(&bytes[..count]).is_err() {
                    return;
                }
                if output.flush().is_err() {
                    return;
                }
            }
        })?;

    let master_for_input = Arc::clone(&master);
    let writer_for_input = Arc::clone(&writer);
    std::thread::Builder::new()
        .name("alera-pty-worker-input".to_string())
        .spawn(move || {
            while let Ok(Some((kind, payload))) = read_frame(&mut input) {
                match kind {
                    b'W' => {
                        let Ok(mut guard) = writer_for_input.lock() else {
                            break;
                        };
                        let Some(pty) = guard.as_mut() else { break };
                        if pty.write_all(&payload).and_then(|_| pty.flush()).is_err() {
                            break;
                        }
                    }
                    b'R' if payload.len() == 4 => {
                        let cols = u16::from_le_bytes([payload[0], payload[1]]);
                        let rows = u16::from_le_bytes([payload[2], payload[3]]);
                        let Ok(guard) = master_for_input.lock() else {
                            break;
                        };
                        let Some(pty) = guard.as_ref() else { break };
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
        })?;

    let code = child
        .wait()
        .map(|status| status.exit_code() as i32)
        .unwrap_or(-1);
    drop(process_job);
    if let Ok(mut guard) = writer.lock() {
        guard.take();
    }
    if let Ok(mut guard) = master.lock() {
        guard.take();
    }
    // ConPTY can hold the reader open until process exit; joining it here
    // deadlocks a shell's normal shutdown while the input pipe remains open.
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
    }
}
