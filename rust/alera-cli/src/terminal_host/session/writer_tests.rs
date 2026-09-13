use std::sync::mpsc::TrySendError;

use super::*;
use crate::terminal_host::host_error::HostError;
use crate::terminal_host::server::TERMINAL_INPUT_BACKPRESSURE_CODE;

#[test]
fn blocked_writer_applies_local_backpressure_without_blocking_sender() {
    let (input_tx, input_rx) = sync_channel(1);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let on_event: Arc<dyn Fn(PtyEvent) + Send + Sync> = Arc::new(move |event| {
        event_tx.send(event).unwrap();
    });
    spawn_writer(
        Box::new(BlockingWriter {
            started_tx,
            release_rx,
        }),
        input_rx,
        on_event,
    );
    input_tx
        .try_send(PtyWrite {
            completion: PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 10,
            },
            bytes: b"first".to_vec(),
            deferred: None,
        })
        .unwrap();
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    input_tx
        .try_send(PtyWrite {
            completion: PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 11,
            },
            bytes: b"second".to_vec(),
            deferred: None,
        })
        .unwrap();
    assert!(matches!(
        input_tx.try_send(PtyWrite {
            completion: PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 12,
            },
            bytes: b"third".to_vec(),
            deferred: None,
        }),
        Err(TrySendError::Full(_))
    ));
    release_tx.send(()).unwrap();
    assert!(matches!(
        event_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        PtyEvent::InputWritten {
            completion: PtyWriteCompletion::ClientRequest { request_id: 10, .. },
            error: None,
        }
    ));
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    release_tx.send(()).unwrap();
    assert!(matches!(
        event_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        PtyEvent::InputWritten {
            completion: PtyWriteCompletion::ClientRequest { request_id: 11, .. },
            error: None,
        }
    ));
}

#[test]
fn deferred_write_keeps_its_suffix_ahead_of_queued_input() {
    let (input_tx, input_rx) = sync_channel(2);
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
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
    );
    input_tx
        .try_send(PtyWrite {
            completion: PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 10,
            },
            bytes: b"A".to_vec(),
            deferred: Some(PtyDeferredWrite {
                delay: std::time::Duration::from_millis(10),
                bytes: b"\r".to_vec(),
            }),
        })
        .unwrap();
    input_tx
        .try_send(PtyWrite {
            completion: PtyWriteCompletion::ClientRequest {
                client_id: 2,
                request_id: 11,
            },
            bytes: b"B".to_vec(),
            deferred: None,
        })
        .unwrap();

    assert!(matches!(
        event_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        PtyEvent::InputWritten {
            completion: PtyWriteCompletion::ClientRequest { request_id: 10, .. },
            error: None,
        }
    ));
    assert!(recorded.lock().unwrap().starts_with(b"A\r"));
    assert!(matches!(
        event_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        PtyEvent::InputWritten {
            completion: PtyWriteCompletion::ClientRequest { request_id: 11, .. },
            error: None,
        }
    ));
    assert_eq!(*recorded.lock().unwrap(), b"A\rB");
}

#[test]
fn failed_writer_completes_every_queued_request_with_the_same_error() {
    let (input_tx, input_rx) = sync_channel(3);
    for request_id in 10..13 {
        input_tx
            .try_send(PtyWrite {
                completion: PtyWriteCompletion::ClientRequest {
                    client_id: 1,
                    request_id,
                },
                bytes: vec![request_id as u8],
                deferred: None,
            })
            .unwrap();
    }
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let on_event: Arc<dyn Fn(PtyEvent) + Send + Sync> = Arc::new(move |event| {
        event_tx.send(event).unwrap();
    });
    spawn_writer(Box::new(FailingWriter), input_rx, on_event);
    for request_id in 10..13 {
        assert!(matches!(
            event_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            PtyEvent::InputWritten {
                completion: PtyWriteCompletion::ClientRequest {
                    request_id: actual,
                    ..
                },
                error: Some(ref message),
            } if actual == request_id && message == "writer failed"
        ));
    }
}

#[test]
fn exited_session_rejects_input_instead_of_leaving_request_pending() {
    let mut session = test_session();
    session.running = false;
    let error = session
        .queue_write(
            PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 10,
            },
            b"input",
        )
        .expect_err("exited session should reject input");
    assert!(error.wire_message().contains("not running"));
}

#[test]
fn full_input_queue_returns_typed_backpressure_error() {
    let (input_tx, _input_rx) = sync_channel(0);
    let mut session = test_session();
    session.input_tx = Some(input_tx);

    let error = session
        .queue_write(
            PtyWriteCompletion::ClientRequest {
                client_id: 1,
                request_id: 10,
            },
            b"input",
        )
        .expect_err("a zero-capacity input queue should report backpressure");

    assert!(matches!(
        &error,
        HostError::Conflict { code, .. }
            if code == TERMINAL_INPUT_BACKPRESSURE_CODE
    ));
    let message = error.wire_message();
    assert!(message.starts_with("terminal_input_backpressure:"));
    assert!(message.contains(TERMINAL_INPUT_BACKPRESSURE_CODE));
    let response = error.wire_response(10);
    assert_eq!(
        response["errorCode"].as_str(),
        Some(TERMINAL_INPUT_BACKPRESSURE_CODE)
    );
}
