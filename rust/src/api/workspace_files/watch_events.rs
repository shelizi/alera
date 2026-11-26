//! Interruptible, bounded filesystem-watch mailbox. A Source Control consumer
//! only needs an event count; Explorer keeps detail until its finite budget is
//! exhausted. Overflow replaces detail with an authoritative rescan request.
//! No periodic polling or blocking on consumer progress.
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(90);
const WATCH_MAX_BATCH: Duration = Duration::from_millis(350);
const MAX_QUEUED_EVENTS: usize = 512;
const MAX_QUEUED_PATHS: usize = 2048;

type FsEvent = notify::Result<notify::Event>;

pub(super) enum WatchDelivery {
    Event(FsEvent),
    Reconcile(u32),
}

impl WatchDelivery {
    pub(super) fn event_count(&self) -> u32 {
        match self {
            Self::Event(_) => 1,
            Self::Reconcile(count) => *count,
        }
    }
}

#[derive(Default)]
struct MailboxState {
    events: VecDeque<FsEvent>,
    path_count: usize,
    collapsed_count: u32,
    stopped: bool,
}

struct WatchMailbox {
    // Signal-only mode never stores the original events or their paths.
    signal_only: bool,
    state: Mutex<MailboxState>,
    ready: Condvar,
}

fn path_count(event: &FsEvent) -> usize {
    match event {
        Ok(event) => event.paths.len(),
        Err(error) => error.paths.len(),
    }
}

impl WatchMailbox {
    fn lock(&self) -> MutexGuard<'_, MailboxState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn stop(&self) {
        let mut state = self.lock();
        state.stopped = true;
        state.events.clear();
        state.path_count = 0;
        state.collapsed_count = 0;
        drop(state);
        self.ready.notify_all();
    }
}

pub(super) struct WatchEventSender {
    mailbox: Arc<WatchMailbox>,
}

impl WatchEventSender {
    pub(super) fn send(&self, event: FsEvent) -> bool {
        let mut state = self.mailbox.lock();
        if state.stopped {
            return false;
        }
        let was_empty = state.events.is_empty() && state.collapsed_count == 0;
        if self.mailbox.signal_only || state.collapsed_count != 0 {
            state.collapsed_count = state.collapsed_count.saturating_add(1);
        } else {
            let paths = path_count(&event);
            if state.events.len() >= MAX_QUEUED_EVENTS
                || state.path_count.saturating_add(paths) > MAX_QUEUED_PATHS
            {
                // Every queued event plus this one is covered by the single
                // rescan. Keep its diagnostic count without keeping paths.
                state.collapsed_count = (state.events.len() as u32).saturating_add(1);
                state.events.clear();
                state.path_count = 0;
            } else {
                state.path_count += paths;
                state.events.push_back(event);
            }
        }
        drop(state);
        if was_empty {
            self.mailbox.ready.notify_one();
        }
        true
    }
}

impl Drop for WatchEventSender {
    fn drop(&mut self) {
        // Producer disappearance wakes an idle consumer even while the
        // stopper is still alive.
        self.mailbox.stop();
    }
}

pub(super) struct WatchStopper {
    mailbox: Arc<WatchMailbox>,
}

impl WatchStopper {
    pub(super) fn request_stop(&self) {
        self.mailbox.stop();
    }
}

impl Drop for WatchStopper {
    fn drop(&mut self) {
        self.request_stop();
    }
}

pub(super) struct WatchEventInbox {
    mailbox: Arc<WatchMailbox>,
}

impl Drop for WatchEventInbox {
    fn drop(&mut self) {
        self.mailbox.stop();
    }
}

fn watch_mailbox(signal_only: bool) -> (WatchEventSender, WatchStopper, WatchEventInbox) {
    let mailbox = Arc::new(WatchMailbox {
        signal_only,
        state: Mutex::new(MailboxState::default()),
        ready: Condvar::new(),
    });
    (
        WatchEventSender {
            mailbox: mailbox.clone(),
        },
        WatchStopper {
            mailbox: mailbox.clone(),
        },
        WatchEventInbox { mailbox },
    )
}

pub(super) fn watch_event_channel() -> (WatchEventSender, WatchStopper, WatchEventInbox) {
    watch_mailbox(false)
}

pub(super) fn watch_signal_channel() -> (WatchEventSender, WatchStopper, WatchEventInbox) {
    watch_mailbox(true)
}

enum ReceiveOutcome {
    Event(WatchDelivery),
    Timeout,
    Stop,
}

impl WatchEventInbox {
    pub(super) fn is_stopping(&self) -> bool {
        self.mailbox.lock().stopped
    }

    fn receive(&self, timeout: Option<Duration>) -> ReceiveOutcome {
        let deadline = timeout.map(|duration| Instant::now() + duration);
        let mut state = self.mailbox.lock();
        loop {
            // Shutdown wins even with an overflowing mailbox.
            if state.stopped {
                return ReceiveOutcome::Stop;
            }
            if state.collapsed_count != 0 {
                return ReceiveOutcome::Event(WatchDelivery::Reconcile(std::mem::take(
                    &mut state.collapsed_count,
                )));
            }
            if let Some(event) = state.events.pop_front() {
                state.path_count -= path_count(&event);
                return ReceiveOutcome::Event(WatchDelivery::Event(event));
            }
            state = if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return ReceiveOutcome::Timeout;
                }
                self.mailbox
                    .ready
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|poison| poison.into_inner())
                    .0
            } else {
                self.mailbox
                    .ready
                    .wait(state)
                    .unwrap_or_else(|poison| poison.into_inner())
            };
        }
    }

    /// Wait indefinitely for the first delivery, then collect for the
    /// existing quiet/max batch windows. A collapsed delivery represents
    /// all notifications it replaced. Never call consumer code under the
    /// mailbox lock. Return false on shutdown and discard the partial batch.
    pub(super) fn collect_batch(&self, mut on_event: impl FnMut(WatchDelivery)) -> bool {
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
    use std::{sync::mpsc, thread};

    fn event() -> notify::Result<notify::Event> {
        Ok(notify::Event::new(notify::EventKind::Any))
    }

    #[test]
    fn signal_only_mailbox_retains_one_count_for_a_large_burst() {
        let (tx, _stop, inbox) = watch_signal_channel();
        for i in 0..100_000 {
            assert!(tx
                .send(Ok(notify::Event::new(notify::EventKind::Any)
                    .add_path(format!("src/{i}.rs").into()))));
        }
        {
            let state = inbox.mailbox.lock();
            assert_eq!(state.collapsed_count, 100_000);
            assert_eq!(state.events.len(), 0);
            assert_eq!(state.events.capacity(), 0);
            assert_eq!(state.path_count, 0);
        }
        assert!(matches!(
            inbox.receive(None),
            ReceiveOutcome::Event(WatchDelivery::Reconcile(100_000))
        ));
        assert!(tx.send(event()));
        assert!(matches!(
            inbox.receive(None),
            ReceiveOutcome::Event(WatchDelivery::Reconcile(1))
        ));
        println!("Signal-only burst: 100000 notifications, zero retained events/paths");
    }

    #[test]
    fn detailed_mailbox_bounds_queue_and_recovers_after_overflow() {
        let (tx, _stop, inbox) = watch_event_channel();
        let make_event = |i: usize| {
            Ok(notify::Event::new(notify::EventKind::Any).add_path(format!("src/{i}.rs").into()))
        };
        for i in 0..MAX_QUEUED_EVENTS {
            assert!(tx.send(make_event(i)));
        }
        {
            let state = inbox.mailbox.lock();
            assert_eq!(state.events.len(), MAX_QUEUED_EVENTS);
            assert_eq!(state.path_count, MAX_QUEUED_EVENTS);
        }
        for i in MAX_QUEUED_EVENTS..100_000 {
            assert!(tx.send(make_event(i)));
        }
        {
            let state = inbox.mailbox.lock();
            assert_eq!(state.events.len(), 0);
            assert_eq!(state.path_count, 0);
            assert_eq!(state.collapsed_count, 100_000);
        }
        assert!(matches!(
            inbox.receive(None),
            ReceiveOutcome::Event(WatchDelivery::Reconcile(100_000))
        ));
        for i in 0..4 {
            assert!(tx.send(make_event(i)));
        }
        for i in 0..4 {
            let ReceiveOutcome::Event(WatchDelivery::Event(Ok(event))) = inbox.receive(None) else {
                panic!("expected detailed FIFO event");
            };
            assert_eq!(
                event.paths,
                [std::path::PathBuf::from(format!("src/{i}.rs"))]
            );
        }
        assert_eq!(inbox.mailbox.lock().path_count, 0);
        println!("Detailed burst: 100000 notifications, peak queued 512 events, one reconciliation, FIFO resumed");
    }

    #[test]
    fn detailed_mailbox_also_bounds_paths_across_multi_path_events() {
        let (tx, _stop, inbox) = watch_event_channel();
        let multi = || {
            let mut event = notify::Event::new(notify::EventKind::Any);
            for i in 0..(MAX_QUEUED_PATHS / 2 + 1) {
                event.paths.push(format!("src/{i}.rs").into());
            }
            Ok(event)
        };
        assert!(tx.send(multi()));
        assert!(tx.send(multi()));
        assert_eq!(inbox.mailbox.lock().path_count, 0);
        assert!(matches!(
            inbox.receive(None),
            ReceiveOutcome::Event(WatchDelivery::Reconcile(2))
        ));
    }

    #[test]
    fn dropping_inbox_closes_producer_without_waiting() {
        let (tx, _stop, inbox) = watch_event_channel();
        assert!(tx.send(event()));
        drop(inbox);
        assert!(!tx.send(event()));
    }

    #[test]
    fn idle_inbox_waits_for_first_event_and_then_coalesces() {
        let (tx, stop, inbox) = watch_event_channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut count = 0;
            let complete = inbox.collect_batch(|delivery| count += delivery.event_count());
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
