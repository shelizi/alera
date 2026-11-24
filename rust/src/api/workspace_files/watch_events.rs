//! Common interruptible filesystem-watch receiver. Idle workers block on
//! the same channel that carries events and shutdown, without a periodic
//! timer. An atomic stop flag lets shutdown take precedence over queued work.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(90);
const WATCH_MAX_BATCH: Duration = Duration::from_millis(350);

enum WatchMessage {
    Event(notify::Result<notify::Event>),
    Stop,
}

pub(super) struct WatchEventSender {
    tx: mpsc::Sender<WatchMessage>,
}

impl WatchEventSender {
    pub(super) fn send(&self, event: notify::Result<notify::Event>) -> bool {
        self.tx.send(WatchMessage::Event(event)).is_ok()
    }
}

impl Drop for WatchEventSender {
    fn drop(&mut self) {
        // The stopper owns a second sender. Explicitly notify the worker
        // when the actual event producer disappears.
        let _ = self.tx.send(WatchMessage::Stop);
    }
}

pub(super) struct WatchStopper {
    tx: mpsc::Sender<WatchMessage>,
    stopping: Arc<AtomicBool>,
}

impl WatchStopper {
    pub(super) fn request_stop(&self) {
        if !self.stopping.swap(true, Ordering::AcqRel) {
            let _ = self.tx.send(WatchMessage::Stop);
        }
    }
}

impl Drop for WatchStopper {
    fn drop(&mut self) {
        self.request_stop();
    }
}

pub(super) struct WatchEventInbox {
    rx: mpsc::Receiver<WatchMessage>,
    stopping: Arc<AtomicBool>,
}

pub(super) fn watch_event_channel() -> (WatchEventSender, WatchStopper, WatchEventInbox) {
    let (tx, rx) = mpsc::channel();
    let stopping = Arc::new(AtomicBool::new(false));
    (
        WatchEventSender { tx: tx.clone() },
        WatchStopper {
            tx,
            stopping: stopping.clone(),
        },
        WatchEventInbox { rx, stopping },
    )
}

enum ReceiveOutcome {
    Event(notify::Result<notify::Event>),
    Timeout,
    Stop,
}

impl WatchEventInbox {
    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    fn receive(&self, timeout: Option<Duration>) -> ReceiveOutcome {
        if self.is_stopping() {
            return ReceiveOutcome::Stop;
        }
        let message = match timeout {
            Some(timeout) => self.rx.recv_timeout(timeout),
            None => self
                .rx
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        if self.is_stopping() {
            return ReceiveOutcome::Stop;
        }
        match message {
            Ok(WatchMessage::Event(event)) => ReceiveOutcome::Event(event),
            Err(mpsc::RecvTimeoutError::Timeout) => ReceiveOutcome::Timeout,
            Ok(WatchMessage::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.stopping.store(true, Ordering::Release);
                ReceiveOutcome::Stop
            }
        }
    }

    /// Wait indefinitely for the first event, then collect for the existing
    /// quiet/max batch windows. Returns false on shutdown/disconnection:
    /// callers must discard any partially collected batch.
    pub(super) fn collect_batch(
        &self,
        mut on_event: impl FnMut(notify::Result<notify::Event>),
    ) -> bool {
        let ReceiveOutcome::Event(first) = self.receive(None) else {
            return false;
        };
        let deadline = Instant::now() + WATCH_MAX_BATCH;
        on_event(first);
        loop {
            if self.is_stopping() {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return !self.is_stopping();
            }
            match self.receive(Some(remaining.min(WATCH_DEBOUNCE))) {
                ReceiveOutcome::Event(event) => on_event(event),
                ReceiveOutcome::Timeout => return !self.is_stopping(),
                ReceiveOutcome::Stop => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn event() -> notify::Result<notify::Event> {
        Ok(notify::Event::new(notify::EventKind::Any))
    }

    #[test]
    fn idle_inbox_waits_for_first_event_and_then_coalesces() {
        let (tx, stop, inbox) = watch_event_channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut count = 0;
            let complete = inbox.collect_batch(|_| count += 1);
            done_tx.send((complete, count)).unwrap();
        });
        // Longer than the old idle poll. No completed empty batch.
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        for _ in 0..4 {
            assert!(tx.send(event()));
        }
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (true, 4)
        );
        stop.request_stop();
        worker.join().unwrap();
    }

    #[test]
    fn stop_interrupts_idle_receiver_even_with_live_producer() {
        let (_tx, stop, inbox) = watch_event_channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            entered_tx.send(()).unwrap();
            done_tx
                .send(inbox.collect_batch(|_| panic!("unexpected event")))
                .unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        stop.request_stop();
        assert!(!done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
    }

    #[test]
    fn stop_wins_over_backlog_and_discards_partial_batch() {
        let (tx, stop, inbox) = watch_event_channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        assert!(tx.send(event()));
        let worker = thread::spawn(move || {
            let mut count = 0;
            let complete = inbox.collect_batch(|_| {
                count += 1;
                if count == 1 {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }
            });
            done_tx.send((complete, count)).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..8000 {
            assert!(tx.send(event()));
        }
        stop.request_stop();
        release_tx.send(()).unwrap();
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (false, 1)
        );
        worker.join().unwrap();
    }

    #[test]
    fn dropped_producer_or_stopper_wakes_blocked_receiver() {
        for drop_producer in [true, false] {
            let (tx, stop, inbox) = watch_event_channel();
            let (done_tx, done_rx) = mpsc::channel();
            let worker = thread::spawn(move || {
                done_tx
                    .send(inbox.collect_batch(|_| panic!("unexpected event")))
                    .unwrap();
            });
            if drop_producer {
                drop(tx);
                assert!(!done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
                drop(stop);
            } else {
                drop(stop);
                assert!(!done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
                drop(tx);
            }
            worker.join().unwrap();
        }
    }
}
