use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;

/// One event-driven accept loop per Worker. The previous nonblocking
/// accept/sleep loop woke every 25ms even with no connection attempts.
/// Use the existing Tokio IO driver on the same dedicated thread, with
/// explicit shutdown so dropping an idle listener never needs a poll or
/// a synthetic loopback client. Client authentication/IO remains on its
/// own bounded blocking threads.
pub(super) struct RecoveryAcceptLoop {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RecoveryAcceptLoop {
    pub(super) fn start(
        listener: TcpListener,
        mut on_client: impl FnMut(TcpStream, SocketAddr) + Send + 'static,
    ) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        let (shutdown, mut stopped) = tokio::sync::oneshot::channel();
        let thread = thread::Builder::new()
            .name("alera-pty-worker-reconnect".to_string())
            .spawn(move || {
                runtime.block_on(async {
                    loop {
                        tokio::select! {
                            biased;
                            _ = &mut stopped => break,
                            accepted = listener.accept() => {
                                match accepted {
                                    Ok((stream, addr)) if addr.ip().is_loopback() => {
                                        if let Ok(stream) = stream.into_std() {
                                            on_client(stream, addr);
                                        }
                                    }
                                    Ok(_) => {}
                                    Err(error) if matches!(
                                        error.kind(),
                                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                                    ) => continue,
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                });
            })?;
        Ok(Self {
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    pub(super) fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // The accept thread only hands clients to separate worker threads.
        // Joining it does not wait for authentication or client requests.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RecoveryAcceptLoop {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn event_driven_listener_accepts_and_stops_without_a_client() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let mut accept_loop = RecoveryAcceptLoop::start(listener, move |stream, peer| {
            accepted_tx.send((stream, peer)).unwrap();
        })
        .unwrap();

        // An idle listener does not synthesize work and remains ready.
        assert!(matches!(
            accepted_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        for _ in 0..4 {
            let client = TcpStream::connect(addr).unwrap();
            let (server, peer) = accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(client.local_addr().unwrap(), peer);
            assert_eq!(server.local_addr().unwrap(), addr);
        }

        // No network activity is needed to wake the acceptor on shutdown.
        accept_loop.stop();
        assert!(accept_loop.thread.is_none());
        assert!(accept_loop.shutdown.is_none());
        assert!(matches!(
            accepted_rx.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_err());
        // Shutdown is idempotent.
        accept_loop.stop();
    }

    #[test]
    fn shutdown_wins_over_queued_connections() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut accept_loop = RecoveryAcceptLoop::start(listener, move |_, _| {
            entered_tx.send(()).unwrap();
            // Only the test stalls this callback. Production immediately
            // spawns the bounded authentication worker and returns.
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        })
        .unwrap();
        let first = TcpStream::connect(addr).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let queued = TcpStream::connect(addr).unwrap();
        accept_loop.shutdown.take().unwrap().send(()).unwrap();
        release_tx.send(()).unwrap();
        accept_loop.stop();
        assert!(matches!(
            entered_rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        drop((first, queued));
    }

    #[test]
    fn stopping_listener_does_not_wait_for_authenticated_client_threads() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let accept_loop = RecoveryAcceptLoop::start(listener, move |stream, _| {
            accepted_tx.send(stream).unwrap();
        })
        .unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let server = accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(accept_loop);
        // Accept shutdown does not own or close already handed-off sockets.
        server.set_nonblocking(false).unwrap();
        use std::io::{Read, Write};
        (&client).write_all(b"ok").unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0; 2];
        (&server).read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ok");
    }
}
