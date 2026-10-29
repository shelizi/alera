use std::collections::VecDeque;

/// Small bounded lookahead for terminal-host control commands. Does not own or
/// replace the canonical actor inbox, its senders, or any PTY/session state.
pub(crate) struct PriorityCommandQueue<T> {
    buffered: VecDeque<T>,
}

impl<T> Default for PriorityCommandQueue<T> {
    fn default() -> Self {
        Self {
            buffered: VecDeque::new(),
        }
    }
}

impl<T> PriorityCommandQueue<T> {
    pub(crate) const MAX_LOOKAHEAD: usize = 64;

    pub(crate) fn is_empty(&self) -> bool {
        self.buffered.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.buffered.len()
    }

    pub(crate) fn push(&mut self, command: T) {
        self.buffered.push_back(command);
    }

    pub(crate) fn next(&mut self, urgent: impl Fn(&T) -> bool) -> Option<T> {
        // Only explicitly marked control commands may leap ahead. All other
        // commands (including connect/disconnect and PTY events) remain FIFO.
        let index = self.buffered.iter().position(urgent).unwrap_or(0);
        self.buffered.remove(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_preempts_a_backlog_without_reordering_other_commands() {
        let mut queue = PriorityCommandQueue::default();
        for id in 0..20 {
            queue.push(id);
        }
        queue.push(-1);
        for id in 20..40 {
            queue.push(id);
        }
        assert_eq!(queue.next(|id| *id < 0), Some(-1));
        for id in 0..40 {
            assert_eq!(queue.next(|value| *value < 0), Some(id));
        }
        assert_eq!(queue.next(|id| *id < 0), None);
    }

    #[test]
    fn multiple_control_commands_keep_their_arrival_order() {
        let mut queue = PriorityCommandQueue::default();
        queue.push(20);
        queue.push(-1);
        queue.push(21);
        queue.push(-2);
        assert_eq!(queue.next(|id| *id < 0), Some(-1));
        assert_eq!(queue.next(|id| *id < 0), Some(-2));
        assert_eq!(queue.next(|id| *id < 0), Some(20));
        assert_eq!(queue.next(|id| *id < 0), Some(21));
    }

    #[test]
    fn ordinary_mailbox_preserves_fifo_when_no_control_is_pending() {
        let mut queue = PriorityCommandQueue::default();
        queue.push(7);
        queue.push(8);
        assert_eq!(queue.next(|id| *id < 0), Some(7));
        assert_eq!(queue.next(|id| *id < 0), Some(8));
        assert!(queue.is_empty());
        assert_eq!(PriorityCommandQueue::<i32>::MAX_LOOKAHEAD, 64);
    }
}
