use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Notify};
use tokio::time::Instant;

/// A due occurrence may not progress when the actor is busy with a runtime
/// mutation, the definition needs approval, or the store rejects a claim.
/// Keep the maintenance/dispatch retry bounded instead of spinning on zero.
const MIN_TICK_SPACING: Duration = Duration::from_secs(1);

/// Send at most one automation tick until the actor acknowledges it. Even
/// if the database continues reporting the same overdue occurrence, neither
/// additional reads nor queued maintenance commands can accumulate while
/// the actor is handling the previous tick.
pub(super) async fn run<ReadWait, ReadFuture, SendTick>(
    wake: Arc<Notify>,
    mut read_wait: ReadWait,
    mut send_tick: SendTick,
) where
    ReadWait: FnMut() -> ReadFuture,
    ReadFuture: Future<Output = Duration>,
    SendTick: FnMut(oneshot::Sender<()>) -> bool,
{
    let mut earliest_tick = Instant::now();
    loop {
        let wait = read_wait()
            .await
            .max(earliest_tick.saturating_duration_since(Instant::now()));
        tokio::select! {
            _ = wake.notified() => continue,
            _ = tokio::time::sleep(wait) => {}
        }
        let (completed, acknowledged) = oneshot::channel();
        if !send_tick(completed) || acknowledged.await.is_err() {
            // The owning actor/channel has stopped; there is no longer
            // anyone who can safely dispatch a scheduled occurrence.
            return;
        }
        // A Notify permit raised during dispatch is retained. Recompute
        // the deadline, but never let repeated overdue results bypass the
        // floor. Ordinary future deadlines and retention sweeps are unchanged.
        earliest_tick = Instant::now() + MIN_TICK_SPACING;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    #[tokio::test]
    async fn overdue_tick_waits_for_actor_even_when_settings_wake_repeatedly() {
        let wake = Arc::new(Notify::new());
        let reads = Arc::new(AtomicUsize::new(0));
        let (tx, mut ticks) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(
            Arc::clone(&wake),
            {
                let reads = Arc::clone(&reads);
                move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Duration::ZERO)
                }
            },
            move |ack| tx.send(ack).is_ok(),
        ));
        let first = timeout(Duration::from_secs(3), ticks.recv())
            .await
            .unwrap()
            .unwrap();
        for _ in 0..100 {
            wake.notify_one();
        }
        assert!(timeout(Duration::from_millis(100), ticks.recv())
            .await
            .is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let acknowledged_at = Instant::now();
        first.send(()).unwrap();
        let next = timeout(Duration::from_secs(4), ticks.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(acknowledged_at.elapsed() >= MIN_TICK_SPACING);
        assert!(reads.load(Ordering::SeqCst) <= 3);
        // A dropped actor command terminates the ticker, not an RPC storm.
        drop(next);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(ticks.recv().await.is_none());
    }

    #[tokio::test]
    async fn settings_notification_recomputes_a_future_deadline() {
        let wake = Arc::new(Notify::new());
        let reads = Arc::new(AtomicUsize::new(0));
        let (tx, mut ticks) = mpsc::unbounded_channel();
        let (read_tx, mut read_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(
            Arc::clone(&wake),
            {
                let reads = Arc::clone(&reads);
                move || {
                    let index = reads.fetch_add(1, Ordering::SeqCst);
                    read_tx.send(index).unwrap();
                    std::future::ready(if index == 0 {
                        Duration::from_secs(60)
                    } else {
                        Duration::ZERO
                    })
                }
            },
            move |ack| tx.send(ack).is_ok(),
        ));
        assert_eq!(
            timeout(Duration::from_secs(2), read_rx.recv())
                .await
                .unwrap(),
            Some(0)
        );
        wake.notify_one();
        let tick = timeout(Duration::from_secs(2), ticks.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        drop(tick);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn closed_actor_inbox_stops_ticker() {
        let sends = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&sends);
        timeout(
            Duration::from_secs(2),
            run(
                Arc::new(Notify::new()),
                || std::future::ready(Duration::ZERO),
                move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    false
                },
            ),
        )
        .await
        .unwrap();
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }
}
