use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Semaphore;

use super::{
    connection_loop, ClientFrame, ClientHandle, ClientKind, ServerCommand,
    CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};

/// Each local socket can retain one not-yet-admitted request of up to 16 MiB,
/// so request inbox quotas alone cannot protect against connection flooding.
/// This caps those socket loops independently from the 64-command / 32 MiB
/// admitted request budget. Excess localhost clients fail closed immediately.
pub(super) const LOCAL_CLIENT_CONNECTION_LIMIT: usize = 32;

/// Accept local clients and hand each one to its own connection loop.
pub(super) fn spawn_accept_loop(
    listener: TcpListener,
    inbox: UnboundedSender<ServerCommand>,
    next_client_id: Arc<AtomicU64>,
    ingress_admission: Arc<crate::terminal_host::client::LocalClientIngressAdmission>,
) {
    spawn_accept_loop_with_limit(
        listener,
        inbox,
        next_client_id,
        ingress_admission,
        Arc::new(Semaphore::new(LOCAL_CLIENT_CONNECTION_LIMIT)),
    );
}

fn spawn_accept_loop_with_limit(
    listener: TcpListener,
    inbox: UnboundedSender<ServerCommand>,
    next_client_id: Arc<AtomicU64>,
    ingress_admission: Arc<crate::terminal_host::client::LocalClientIngressAdmission>,
    connection_slots: Arc<Semaphore>,
) {
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(connection_permit) = connection_slots.clone().try_acquire_owned() else {
                // Do not enqueue a ClientConnected record or allocate per-client
                // output queues when the local socket budget is exhausted.
                drop(stream);
                continue;
            };
            let _ = stream.set_nodelay(true);
            let id = next_client_id.fetch_add(1, Ordering::Relaxed);
            let (control_out_tx, control_out_rx) = mpsc::unbounded_channel::<ClientFrame>();
            let (terminal_out_tx, terminal_out_rx) =
                mpsc::channel::<ClientFrame>(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
            // Register the client before its lines can arrive: the
            // ClientConnected command is enqueued before the connection loop
            // (and thus any ClientLine) starts.
            if inbox
                .send(ServerCommand::ClientConnected {
                    id,
                    handle: ClientHandle::new(control_out_tx, terminal_out_tx),
                    kind: ClientKind::Local,
                })
                .is_err()
            {
                break;
            }
            let client_inbox = inbox.clone();
            let client_admission = ingress_admission.clone();
            tokio::spawn(async move {
                // The permit follows the socket writer's entire lifecycle,
                // including time spent blocked on request admission.
                let _connection_permit = connection_permit;
                connection_loop(
                    stream,
                    id,
                    client_inbox,
                    control_out_rx,
                    terminal_out_rx,
                    client_admission,
                )
                .await;
            });
        }
    });
}

pub(super) fn display_socket_address(host: &str, port: u16) -> String {
    if host.parse::<Ipv6Addr>().is_ok() {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn saturated_local_connections_are_rejected_without_leaking_slots_or_blocking_new_clients(
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (inbox, mut messages) = mpsc::unbounded_channel();
        let next_id = Arc::new(AtomicU64::new(1));
        let ingress =
            Arc::new(crate::terminal_host::client::LocalClientIngressAdmission::new(2, 4096));
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        spawn_accept_loop_with_limit(listener, inbox, next_id, ingress, slots.clone());

        let first_client = tokio::net::TcpStream::connect(address).await.unwrap();
        let first = timeout(Duration::from_secs(2), messages.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            first,
            ServerCommand::ClientConnected { id: 1, .. }
        ));
        assert_eq!(slots.available_permits(), 0);

        let mut rejected = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut buf = [0u8; 1];
        let n = timeout(
            Duration::from_secs(2),
            tokio::io::AsyncReadExt::read(&mut rejected, &mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(n, 0, "excess local connections must be closed, not queued");
        assert!(
            messages.try_recv().is_err(),
            "rejected sockets must not register clients"
        );

        drop(first);
        drop(first_client);
        timeout(Duration::from_secs(2), async {
            while slots.available_permits() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let _third_client = tokio::net::TcpStream::connect(address).await.unwrap();
        let third = timeout(Duration::from_secs(2), messages.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            third,
            ServerCommand::ClientConnected { id: 2, .. }
        ));
    }
}
