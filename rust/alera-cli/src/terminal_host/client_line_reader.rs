use std::io::{Error, ErrorKind, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Reject oversized local protocol frames before they build an unbounded
/// String. This is a per-frame safety limit, not mailbox admission.
pub(super) const MAX_LOCAL_REQUEST_LINE_BYTES: usize = 16 * 1024 * 1024;

const PENDING_READ_BYTE_UNIT: usize = 4096;

/// Shared quota held from the first byte read until the completed line has
/// been admitted to the actor. It includes partial lines and decoded lines
/// waiting for the independent actor admission semaphore.
///
/// Never await the quota while retaining bytes of an incomplete line: two
/// large partial requests could otherwise consume all permits and deadlock
/// each other. Instead, reject the connection that cannot reserve its next
/// read chunk, releasing its permits when the socket loop exits.
pub(super) struct PendingLocalLineBudget {
    quota: Arc<Semaphore>,
    reservations: Vec<OwnedSemaphorePermit>,
    reserved_units: usize,
}

impl PendingLocalLineBudget {
    pub(super) fn new(quota: Arc<Semaphore>) -> Self {
        Self {
            quota,
            reservations: Vec::new(),
            reserved_units: 0,
        }
    }

    fn reserve_for_length(&mut self, bytes: usize) -> Result<()> {
        let units = bytes.div_ceil(PENDING_READ_BYTE_UNIT);
        if units <= self.reserved_units {
            return Ok(());
        }
        let new_units = units - self.reserved_units;
        let permit = self
            .quota
            .clone()
            .try_acquire_many_owned(new_units as u32)
            .map_err(|_| {
                Error::new(
                    ErrorKind::OutOfMemory,
                    "localhost pending request read budget exhausted",
                )
            })?;
        self.reserved_units = units;
        self.reservations.push(permit);
        Ok(())
    }

    pub(super) fn release_completed_line(&mut self) {
        self.reservations.clear();
        self.reserved_units = 0;
    }
}

/// `pending` belongs to the connection loop, NOT this future. The reader is
/// used inside tokio::select!, and dropping a partially read future must not
/// discard already consumed bytes when a control frame wins that select.
pub(super) async fn next_client_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
    pending: &mut Vec<u8>,
) -> Result<Option<String>> {
    next_client_line_inner(reader, max_bytes, pending, None).await
}

pub(super) async fn next_client_line_budgeted<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
    pending: &mut Vec<u8>,
    budget: &mut PendingLocalLineBudget,
) -> Result<Option<String>> {
    next_client_line_inner(reader, max_bytes, pending, Some(budget)).await
}

async fn next_client_line_inner<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
    pending: &mut Vec<u8>,
    mut budget: Option<&mut PendingLocalLineBudget>,
) -> Result<Option<String>> {
    loop {
        let (consumed, terminated) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                if pending.is_empty() {
                    return Ok(None);
                }
                return finish_line(pending).map(Some);
            }
            let consumed = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |position| position + 1);
            // Check BEFORE extending to prevent an attacker from forcing an
            // unbounded String/Vec allocation on the loopback transport.
            if consumed > max_bytes.saturating_sub(pending.len()) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "client request line exceeds limit",
                ));
            }
            // Reserve quota before copying socket bytes into a connection's
            // owned buffer. This stays held across tokio::select! cancellation
            // because the connection loop owns `budget`, not this future.
            if let Some(budget) = budget.as_deref_mut() {
                budget.reserve_for_length(pending.len() + consumed)?;
            }
            let terminated = available[consumed - 1] == b'\n';
            pending.extend_from_slice(&available[..consumed]);
            (consumed, terminated)
        };
        reader.consume(consumed);
        if terminated {
            return finish_line(pending).map(Some);
        }
    }
}

fn finish_line(pending: &mut Vec<u8>) -> Result<String> {
    let mut bytes = std::mem::take(pending);
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "invalid UTF-8 client request"))
}

#[cfg(test)]
mod client_line_reader_tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn exact_limit_and_crlf_are_preserved_as_lines() {
        let mut reader = BufReader::with_capacity(2, &b"abcd\r\nnext\n"[..]);
        let mut pending = Vec::new();
        assert_eq!(
            next_client_line(&mut reader, 6, &mut pending)
                .await
                .unwrap()
                .as_deref(),
            Some("abcd")
        );
        assert_eq!(
            next_client_line(&mut reader, 6, &mut pending)
                .await
                .unwrap()
                .as_deref(),
            Some("next")
        );
        assert_eq!(
            next_client_line(&mut reader, 6, &mut pending)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn oversized_line_is_rejected_without_draining_the_remaining_input() {
        let data = b"abcdefghijklmno\n";
        let mut reader = BufReader::with_capacity(3, &data[..]);
        let mut pending = Vec::new();
        let err = next_client_line(&mut reader, 5, &mut pending)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(pending.len() <= 5);
        // The connection closes after this error. Do not consume the whole
        // hostile frame or allocate memory proportional to its length.
        let left = reader.buffer().len();
        assert!(left <= 3);
    }

    #[tokio::test]
    async fn utf8_split_across_read_chunks_and_no_final_newline_work() {
        let text = "你好";
        let mut reader = BufReader::with_capacity(1, text.as_bytes());
        let mut pending = Vec::new();
        assert_eq!(
            next_client_line(&mut reader, 7, &mut pending)
                .await
                .unwrap()
                .as_deref(),
            Some(text)
        );
        assert_eq!(
            next_client_line(&mut reader, 7, &mut pending)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn invalid_utf8_fails_before_forwarding_to_actor() {
        let mut reader = BufReader::with_capacity(2, &b"hello\xff\n"[..]);
        let mut pending = Vec::new();
        let err = next_client_line(&mut reader, 10, &mut pending)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn cancelled_read_preserves_partial_bytes_across_control_frame_wakes() {
        use tokio::io::AsyncWriteExt as _;
        let (mut producer, input) = tokio::io::duplex(16);
        let mut reader = BufReader::with_capacity(2, input);
        let mut pending = Vec::new();
        producer.write_all(b"partial ").await.unwrap();
        // A control frame can win select! after bytes have been consumed.
        let cancelled = tokio::time::timeout(
            std::time::Duration::from_millis(25),
            next_client_line(&mut reader, 32, &mut pending),
        )
        .await;
        assert!(cancelled.is_err());
        assert_eq!(pending, b"partial ");
        producer.write_all(b"line\n").await.unwrap();
        assert_eq!(
            next_client_line(&mut reader, 32, &mut pending)
                .await
                .unwrap()
                .as_deref(),
            Some("partial line"),
        );
    }

    #[test]
    fn upper_bound_is_finite_and_supports_2mib_transfer_chunks() {
        assert!(MAX_LOCAL_REQUEST_LINE_BYTES >= 2 * 1024 * 1024 + 1024);
        assert!(MAX_LOCAL_REQUEST_LINE_BYTES <= 16 * 1024 * 1024);
    }
}
