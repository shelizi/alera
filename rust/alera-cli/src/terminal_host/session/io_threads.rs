use std::io::{Read, Write};
use std::sync::mpsc::Receiver;
#[cfg(windows)]
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;

use super::{PtyEvent, PtyWrite};

const READ_CHUNK_BYTES: usize = 64 * 1024;
// Each Windows PTY must apply backpressure rather than retaining an unlimited
// number of 64 KiB output chunks in the shared Host process.
#[cfg(windows)]
const WINDOWS_PTY_EVENT_QUEUE_CAPACITY: usize = 8;

/// Read the PTY on a dedicated thread, forwarding output and the final exit code.
#[cfg(not(windows))]
pub(super) fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    on_event: Arc<dyn Fn(PtyEvent) + Send + Sync>,
    child_reaper: Arc<tokio::sync::Mutex<()>>,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("alera-pty-reader".to_string())
        .spawn(move || {
            let mut buffer = vec![0u8; READ_CHUNK_BYTES];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => on_event(PtyEvent::Output(buffer[..read].to_vec())),
                    Err(error) => {
                        on_event(PtyEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
            let code = loop {
                // A workspace shutdown retains the unreaped shell as its OS
                // session anchor, so its PID cannot be recycled mid-cleanup.
                let status = {
                    let _lease = child_reaper.blocking_lock();
                    child.try_wait()
                };
                match status {
                    Ok(Some(status)) => break status.exit_code() as i32,
                    Err(_) => break -1,
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                }
            };
            on_event(PtyEvent::Exit(code));
        })
        .map(|_| ())
}

#[cfg(windows)]
enum WindowsReaderEvent {
    Output(Vec<u8>),
    Error(String),
    ReaderClosed,
    ChildExited(i32),
}

#[cfg(windows)]
fn read_windows_pty(mut reader: Box<dyn Read + Send>, reader_tx: SyncSender<WindowsReaderEvent>) {
    let mut buffer = vec![0u8; READ_CHUNK_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                // A slow Host actor blocks this session's PTY reader only.
                // No other terminal shares this queue or is starved by it.
                if reader_tx
                    .send(WindowsReaderEvent::Output(buffer[..read].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                let _ = reader_tx.send(WindowsReaderEvent::Error(error.to_string()));
                break;
            }
        }
    }
    let _ = reader_tx.send(WindowsReaderEvent::ReaderClosed);
}

/// ConPTY keeps its output pipe open until the pseudoconsole and input writer
/// are released. Wait for the child independently, ask the server actor to
/// release those handles, then drain all remaining output before publishing
/// the final exit event.
#[cfg(windows)]
pub(super) fn spawn_reader(
    reader: Box<dyn Read + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    on_event: Arc<dyn Fn(PtyEvent) + Send + Sync>,
) -> std::io::Result<()> {
    let (event_tx, event_rx) = sync_channel(WINDOWS_PTY_EVENT_QUEUE_CAPACITY);
    let reader_tx = event_tx.clone();
    std::thread::Builder::new()
        .name("alera-pty-reader".to_string())
        .spawn(move || read_windows_pty(reader, reader_tx))?;

    std::thread::Builder::new()
        .name("alera-pty-child-waiter".to_string())
        .spawn(move || {
            let code = child
                .wait()
                .map(|status| status.exit_code() as i32)
                .unwrap_or(-1);
            let _ = event_tx.send(WindowsReaderEvent::ChildExited(code));
        })?;

    std::thread::Builder::new()
        .name("alera-pty-event-coordinator".to_string())
        .spawn(move || {
            let mut child_exit = None;
            let mut reader_closed = false;
            while let Ok(event) = event_rx.recv() {
                match event {
                    WindowsReaderEvent::Output(data) => on_event(PtyEvent::Output(data)),
                    WindowsReaderEvent::Error(message) => on_event(PtyEvent::Error(message)),
                    WindowsReaderEvent::ReaderClosed => reader_closed = true,
                    WindowsReaderEvent::ChildExited(code) => {
                        child_exit = Some(code);
                        on_event(PtyEvent::ChildExited);
                    }
                }
                if reader_closed {
                    if let Some(code) = child_exit {
                        on_event(PtyEvent::Exit(code));
                        return;
                    }
                }
            }
        })
        .map(|_| ())
}

pub(super) fn spawn_writer(
    mut writer: Box<dyn Write + Send>,
    input_rx: Receiver<PtyWrite>,
    on_event: Arc<dyn Fn(PtyEvent) + Send + Sync>,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("alera-pty-writer".to_string())
        .spawn(move || {
            let mut writer_failure: Option<String> = None;
            while let Ok(request) = input_rx.recv() {
                let pulse_cancelled = matches!(
                    &request.completion,
                    crate::terminal_host::session::PtyWriteCompletion::TerminalPulse {
                        active,
                        ..
                    } if !active.load(std::sync::atomic::Ordering::Acquire)
                );
                let error = if pulse_cancelled {
                    None
                } else {
                    match writer_failure.as_ref() {
                        Some(message) => Some(message.clone()),
                        None => {
                            let mut result = writer
                                .write_all(&request.bytes)
                                .and_then(|_| writer.flush());
                            if result.is_ok() {
                                if let Some(deferred) = request.deferred {
                                    std::thread::sleep(deferred.delay);
                                    result = writer
                                        .write_all(&deferred.bytes)
                                        .and_then(|_| writer.flush());
                                }
                            }
                            result.err().map(|error| error.to_string())
                        }
                    }
                };
                if !pulse_cancelled && writer_failure.is_none() {
                    writer_failure.clone_from(&error);
                }
                on_event(PtyEvent::InputWritten {
                    completion: request.completion,
                    error,
                });
            }
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::sync_channel;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::terminal_host::session::input_queue::PtyWrite;
    use crate::terminal_host::session::{PtyEvent, PtyWriteCompletion};

    struct RecordingWriter {
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(windows)]
    #[test]
    fn flooding_conpty_reader_backpressures_only_its_session() {
        use std::sync::atomic::AtomicUsize;
        use std::time::Instant;

        struct EndlessOutput {
            reads: Arc<AtomicUsize>,
        }

        impl Read for EndlessOutput {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.reads.fetch_add(1, Ordering::SeqCst);
                buffer.fill(b'x');
                Ok(buffer.len())
            }
        }

        let reads = Arc::new(AtomicUsize::new(0));
        let (blocked_tx, blocked_rx) = sync_channel(WINDOWS_PTY_EVENT_QUEUE_CAPACITY);
        let counter = Arc::clone(&reads);
        let blocked_reader = std::thread::spawn(move || {
            read_windows_pty(Box::new(EndlessOutput { reads: counter }), blocked_tx);
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while reads.load(Ordering::SeqCst) < WINDOWS_PTY_EVENT_QUEUE_CAPACITY + 1
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        // The last read blocks on send: the queue cannot grow while the
        // shared Host is occupied with another session.
        assert_eq!(
            reads.load(Ordering::SeqCst),
            WINDOWS_PTY_EVENT_QUEUE_CAPACITY + 1
        );
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            reads.load(Ordering::SeqCst),
            WINDOWS_PTY_EVENT_QUEUE_CAPACITY + 1
        );

        // A separate PTY reader must keep delivering while the first stalls.
        let (healthy_tx, healthy_rx) = sync_channel(WINDOWS_PTY_EVENT_QUEUE_CAPACITY);
        let healthy_reader = std::thread::spawn(move || {
            read_windows_pty(
                Box::new(std::io::Cursor::new(b"healthy".to_vec())),
                healthy_tx,
            );
        });
        assert!(matches!(
            healthy_rx.recv_timeout(Duration::from_secs(2)),
            Ok(WindowsReaderEvent::Output(data)) if data == b"healthy"
        ));
        assert!(matches!(
            healthy_rx.recv_timeout(Duration::from_secs(2)),
            Ok(WindowsReaderEvent::ReaderClosed)
        ));
        healthy_reader.join().unwrap();

        // A closed Host receiver also releases the blocked producer.
        drop(blocked_rx);
        blocked_reader.join().unwrap();
    }

    #[test]
    fn cancelled_terminal_pulse_is_skipped_without_dropping_normal_input() {
        let (input_tx, input_rx) = sync_channel(2);
        let active = Arc::new(AtomicBool::new(true));
        input_tx
            .try_send(PtyWrite {
                completion: PtyWriteCompletion::TerminalPulse {
                    session_instance_id: 7,
                    active: Arc::clone(&active),
                },
                bytes: b"pulse".to_vec(),
                deferred: None,
            })
            .unwrap();
        input_tx
            .try_send(PtyWrite {
                completion: PtyWriteCompletion::ClientRequest {
                    client_id: 1,
                    request_id: 10,
                },
                bytes: b"normal".to_vec(),
                deferred: None,
            })
            .unwrap();
        active.store(false, Ordering::Release);

        let recorded = Arc::new(Mutex::new(Vec::new()));
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let on_event: Arc<dyn Fn(PtyEvent) + Send + Sync> = Arc::new(move |event| {
            event_tx.send(event).unwrap();
        });
        spawn_writer(
            Box::new(RecordingWriter {
                bytes: Arc::clone(&recorded),
            }),
            input_rx,
            on_event,
        )
        .unwrap();

        for _ in 0..2 {
            assert!(matches!(
                event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                PtyEvent::InputWritten { error: None, .. }
            ));
        }
        assert_eq!(*recorded.lock().unwrap(), b"normal");
    }
}
