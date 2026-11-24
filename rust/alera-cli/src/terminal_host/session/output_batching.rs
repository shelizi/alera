use super::{DurableOutputBatch, OutputBatch, Session};

/// Coalescing PTY output into batches, on two independent tracks.
///
/// The delivery batch is what active clients receive and the durable batch is
/// what reaches the history store. They drain on their own generations, so a
/// slow writer cannot hold up delivery and a paused client cannot hold up
/// persistence. If nobody is consuming live output, the ring is the source
/// for a later attach/resume and no delivery batch is necessary.
impl Session {
    /// Seed either empty batch with the reader's own allocation instead of
    /// allocating and copying into both. Prefer the longer-lived durable
    /// batch, but preserve each batch's independent flush boundary.
    pub(super) fn append_output_batches(&mut self, data: Vec<u8>) {
        let len = data.len();
        if len == 0 {
            return;
        }
        // Avoid retaining an oversized reader allocation for a tiny payload.
        let can_move = data.capacity() <= len.saturating_mul(2);
        // Attach/resume flush existing live output before changing the
        // recipient set, then replay the missing bytes from the ring.
        // Backpressured and explicitly paused clients therefore do not need
        // a second copy of bytes that cannot be sent to them.
        let has_live_recipient = self.clients.len() > self.output_paused_clients.len()
            || self
                .clients
                .iter()
                .any(|id| !self.output_paused_clients.contains(id));
        if !has_live_recipient {
            // Defensively discard any older delivery batch after the last
            // recipient disappears, invalidating its pending generation.
            drop(self.flush_output_batch());
            if can_move
                && self.durable_output_batch.is_empty()
                && self.durable_output_batch.capacity() < len
            {
                self.durable_output_batch = data;
            } else {
                self.durable_output_batch.extend_from_slice(&data);
            }
            return;
        }
        if can_move
            && self.durable_output_batch.is_empty()
            && self.durable_output_batch.capacity() < len
        {
            self.output_batch.extend_from_slice(&data);
            self.durable_output_batch = data;
        } else if can_move && self.output_batch.is_empty() && self.output_batch.capacity() < len {
            self.durable_output_batch.extend_from_slice(&data);
            self.output_batch = data;
        } else {
            self.output_batch.extend_from_slice(&data);
            self.durable_output_batch.extend_from_slice(&data);
        }
    }

    pub fn recent_output_since(&self, cursor: Option<u64>, max_bytes: usize) -> Vec<u8> {
        let (base, end) = self.output_stream_range();
        let available = end.saturating_sub(cursor.unwrap_or(base).max(base));
        self.buffer.tail(available.min(max_bytes as u64) as usize)
    }

    pub fn output_batch_len(&self) -> usize {
        self.output_batch.len()
    }

    pub fn output_stream_range(&self) -> (u64, u64) {
        (
            self.output_stream_bytes
                .saturating_sub(self.buffer.len() as u64),
            self.output_stream_bytes,
        )
    }

    pub fn output_batch_due(&self, generation: u64) -> bool {
        self.output_batch_armed && self.output_batch_gen == generation
    }

    pub fn flush_output_batch(&mut self) -> Option<OutputBatch> {
        if self.output_batch.is_empty() {
            self.output_batch_armed = false;
            return None;
        }
        self.output_batch_gen = self.output_batch_gen.wrapping_add(1);
        self.output_batch_armed = false;
        let data = std::mem::take(&mut self.output_batch);
        Some(OutputBatch { data })
    }

    pub fn durable_output_batch_len(&self) -> usize {
        self.durable_output_batch.len()
    }

    pub fn durable_output_batch_due(&self, generation: u64) -> bool {
        self.durable_output_batch_armed && self.durable_output_batch_gen == generation
    }

    pub fn flush_durable_output_batch(&mut self) -> Option<DurableOutputBatch> {
        if self.durable_output_batch.is_empty() {
            self.durable_output_batch_armed = false;
            return None;
        }
        self.durable_output_batch_gen = self.durable_output_batch_gen.wrapping_add(1);
        self.durable_output_batch_armed = false;
        let data = std::mem::take(&mut self.durable_output_batch);
        let sequence = self.durable_output_batch_sequence;
        self.durable_output_batch_sequence = self.durable_output_batch_sequence.wrapping_add(1);
        Some(DurableOutputBatch {
            data,
            sequence,
            end_cursor: self.output_stream_bytes,
        })
    }
}
