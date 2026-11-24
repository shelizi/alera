use std::collections::VecDeque;

/// How far into a trimmed tail to look for a line break before giving up and
/// cutting at a code point boundary instead.
const LINE_ALIGN_SCAN_BYTES: usize = 4 * 1024;

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

/// Copy the logical range of a byte ring using at most two bulk copies.
/// This does not rotate or traverse the discarded prefix.
fn copy_deque_from(buffer: &VecDeque<u8>, start: usize) -> Vec<u8> {
    if start >= buffer.len() {
        return Vec::new();
    }
    let (first, second) = buffer.as_slices();
    let mut output = Vec::with_capacity(buffer.len() - start);
    if start < first.len() {
        output.extend_from_slice(&first[start..]);
        output.extend_from_slice(second);
    } else {
        output.extend_from_slice(&second[start - first.len()..]);
    }
    output
}

pub(super) fn copy_deque_tail(buffer: &VecDeque<u8>, max_bytes: usize) -> Vec<u8> {
    copy_deque_from(buffer, buffer.len().saturating_sub(max_bytes))
}

pub(super) fn retain_deque_tail(buffer: &mut VecDeque<u8>, max_bytes: usize) {
    if max_bytes == 0 {
        *buffer = VecDeque::new();
        return;
    }
    if buffer.len() > max_bytes {
        buffer.drain(..buffer.len() - max_bytes);
    }
    // An explicit cap reduction should release an unnecessarily large ring.
    if buffer.capacity() > max_bytes {
        buffer.shrink_to(max_bytes);
    }
}

pub(super) fn append_deque_tail(buffer: &mut VecDeque<u8>, data: &[u8], max_bytes: usize) {
    if max_bytes == 0 {
        *buffer = VecDeque::new();
        return;
    }
    let data = &data[data.len().saturating_sub(max_bytes)..];
    let keep_before = max_bytes - data.len();
    if buffer.len() > keep_before {
        buffer.drain(..buffer.len() - keep_before);
    }
    // Grow only with bytes actually retained, and never request more than
    // the configured cap. Reuse the allocation as the oldest bytes expire.
    let required = buffer.len() + data.len();
    if required > buffer.capacity() {
        let target = buffer
            .capacity()
            .saturating_mul(2)
            .max(required)
            .min(max_bytes);
        buffer.reserve_exact(target - buffer.len());
    } else if buffer.capacity() > max_bytes.saturating_mul(2) {
        // A caller can lower the cap on append without a separate trim.
        buffer.shrink_to(max_bytes);
    }
    buffer.extend(data);
}

/// Bounded scrollback buffer retaining at most the configured number of bytes.
/// A byte ring avoids per-PTY-read allocations and repeated partial-chunk
/// copies while dropping the oldest bytes. Reads never make it contiguous.
pub struct ScrollbackBuffer {
    bytes: VecDeque<u8>,
    max_bytes: usize,
}

impl ScrollbackBuffer {
    pub fn new(max_bytes: usize, initial: &[u8]) -> Self {
        let mut buffer = Self {
            bytes: VecDeque::new(),
            max_bytes,
        };
        buffer.append(initial);
        buffer
    }

    pub fn set_max_bytes(&mut self, value: usize) {
        self.max_bytes = value;
        retain_deque_tail(&mut self.bytes, value);
    }

    pub fn append(&mut self, data: &[u8]) {
        if !data.is_empty() {
            append_deque_tail(&mut self.bytes, data, self.max_bytes);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        copy_deque_from(&self.bytes, 0)
    }

    /// The buffer from an offset, without visiting or copying earlier bytes.
    pub fn slice_from(&self, start: usize) -> Vec<u8> {
        copy_deque_from(&self.bytes, start)
    }

    /// The last bytes, started at a line boundary when one is close by.
    ///
    /// Determine the alignment directly on the ring before allocating the
    /// replay buffer, so even a multi-megabyte snapshot is copied only once.
    pub fn tail(&self, max_bytes: usize) -> Vec<u8> {
        if self.bytes.len() <= max_bytes {
            return self.to_bytes();
        }
        let start = self.bytes.len() - max_bytes;
        let mut bytes = self.bytes.range(start..);
        let skip = match bytes
            .clone()
            .take(LINE_ALIGN_SCAN_BYTES)
            .position(|byte| *byte == b'\n')
        {
            Some(newline) => newline + 1,
            None => bytes
                .position(|byte| !is_utf8_continuation(*byte))
                .unwrap_or(max_bytes),
        };
        self.slice_from(start + skip)
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_under_limit() {
        let mut buffer = ScrollbackBuffer::new(100, &[]);
        buffer.append(b"hello ");
        buffer.append(b"world");
        assert_eq!(buffer.to_bytes(), b"hello world");
        assert_eq!(buffer.len(), 11);
    }

    #[test]
    fn drops_oldest_chunks_over_limit() {
        let mut buffer = ScrollbackBuffer::new(5, &[]);
        buffer.append(b"abc");
        buffer.append(b"de");
        buffer.append(b"fg");
        // Keeps the most recent 5 bytes ("cdefg").
        assert_eq!(buffer.to_bytes(), b"cdefg");
        assert_eq!(buffer.len(), 5);
    }

    #[test]
    fn trims_partial_leading_chunk() {
        let mut buffer = ScrollbackBuffer::new(4, &[]);
        buffer.append(b"abcd");
        buffer.append(b"ef");
        // "abcd" + "ef" -> trim 2 from the front of "abcd" -> "cdef".
        assert_eq!(buffer.to_bytes(), b"cdef");
        assert_eq!(buffer.len(), 4);
    }

    #[test]
    fn single_chunk_larger_than_cap_keeps_tail() {
        let mut buffer = ScrollbackBuffer::new(3, &[]);
        buffer.append(b"abcdefg");
        assert_eq!(buffer.to_bytes(), b"efg");
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn shrinking_max_bytes_trims_immediately() {
        let mut buffer = ScrollbackBuffer::new(10, &[]);
        buffer.append(b"abcdefgh");
        buffer.set_max_bytes(3);
        assert_eq!(buffer.to_bytes(), b"fgh");
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn seeds_from_initial_buffer() {
        let buffer = ScrollbackBuffer::new(4, b"abcdef");
        assert_eq!(buffer.to_bytes(), b"cdef");
    }

    #[test]
    fn tail_cuts_at_a_line_break_when_one_is_in_reach() {
        let mut buffer = ScrollbackBuffer::new(100, &[]);
        buffer.append(b"first line\nsecond line\nthird line");
        assert_eq!(buffer.tail(1000), b"first line\nsecond line\nthird line");
        assert_eq!(buffer.tail(20), b"third line");
    }

    #[test]
    fn tail_without_a_line_break_stops_at_a_code_point_boundary() {
        let mut buffer = ScrollbackBuffer::new(100, &[]);
        buffer.append("aéb".as_bytes());
        // Four bytes: a, then the two of é, then b. A two-byte tail would start
        // on é's continuation byte, so é is dropped whole rather than mangled.
        assert_eq!(buffer.tail(2), b"b");
        assert_eq!(buffer.tail(3), "éb".as_bytes());
    }

    #[test]
    fn slices_from_an_offset_across_chunks() {
        let mut buffer = ScrollbackBuffer::new(100, &[]);
        buffer.append(b"abc");
        buffer.append(b"de");
        buffer.append(b"fgh");
        assert_eq!(buffer.slice_from(0), b"abcdefgh");
        assert_eq!(buffer.slice_from(3), b"defgh");
        assert_eq!(buffer.slice_from(4), b"efgh");
        assert_eq!(buffer.slice_from(8), b"");
        assert_eq!(buffer.slice_from(99), b"");
    }

    fn reference_tail(bytes: &[u8], limit: usize) -> Vec<u8> {
        if bytes.len() <= limit {
            return bytes.to_vec();
        }
        let tail = &bytes[bytes.len() - limit..];
        let skip = match tail
            .iter()
            .take(LINE_ALIGN_SCAN_BYTES)
            .position(|b| *b == b'\n')
        {
            Some(newline) => newline + 1,
            None => tail
                .iter()
                .position(|byte| !is_utf8_continuation(*byte))
                .unwrap_or(tail.len()),
        };
        tail[skip..].to_vec()
    }

    #[test]
    fn ring_read_ranges_match_reference_through_wrap_resize_and_binary_output() {
        let mut buffer = ScrollbackBuffer::new(257, &[]);
        let mut expected = Vec::new();
        let mut cap = 257;
        let mut rng = 0x12345678_u32;
        for step in 0..2400 {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            if step % 37 == 0 {
                cap = (rng as usize) % 800;
                buffer.set_max_bytes(cap);
            } else {
                let len = (rng as usize) % 91;
                let data: Vec<u8> = (0..len)
                    .map(|i| ((rng as usize + i * 29) % 256) as u8)
                    .collect();
                expected.extend_from_slice(&data);
                buffer.append(&data);
            }
            expected.drain(..expected.len().saturating_sub(cap));
            assert_eq!(buffer.len(), expected.len());
            assert_eq!(buffer.is_empty(), expected.is_empty());
            assert_eq!(buffer.to_bytes(), expected);
            for start in [0, 1, expected.len() / 2, expected.len(), usize::MAX] {
                assert_eq!(
                    buffer.slice_from(start),
                    expected.get(start..).unwrap_or(&[])
                );
            }
            for limit in [0, 1, 3, 19, 160, cap, usize::MAX] {
                assert_eq!(buffer.tail(limit), reference_tail(&expected, limit));
            }
        }
    }

    #[test]
    fn large_ring_reuses_its_allocation_during_tiny_steady_state_appends() {
        let cap = 1024 * 1024;
        let initial = vec![b'x'; cap];
        let mut buffer = ScrollbackBuffer::new(cap, &initial);
        let capacity = buffer.bytes.capacity();
        for _ in 0..8192 {
            buffer.append(b"a");
            assert_eq!(buffer.bytes.capacity(), capacity);
            assert_eq!(buffer.len(), cap);
        }
        assert_eq!(buffer.tail(8192), vec![b'a'; 8192]);
        assert_eq!(
            buffer.slice_from(cap - 8193),
            [b"x".as_slice(), &vec![b'a'; 8192]].concat()
        );
        assert!(
            !buffer.bytes.as_slices().1.is_empty(),
            "the test must exercise wraparound"
        );
        buffer.set_max_bytes(16);
        assert_eq!(buffer.to_bytes(), vec![b'a'; 16]);
        assert!(buffer.bytes.capacity() < capacity);
        buffer.set_max_bytes(0);
        assert_eq!(buffer.bytes.capacity(), 0);
        buffer.append(b"ignored");
        assert!(buffer.is_empty());
    }

    #[test]
    fn aligned_tail_scans_across_wrap_and_preserves_utf8_and_scan_limit() {
        let mut buffer = ScrollbackBuffer::new(16, b"0123456789abcdef");
        buffer.append("界\n😀end".as_bytes());
        let bytes = buffer.to_bytes();
        assert!(!buffer.bytes.as_slices().1.is_empty());
        for limit in 0..=bytes.len() + 1 {
            assert_eq!(buffer.tail(limit), reference_tail(&bytes, limit));
        }

        let mut input = vec![0x80; LINE_ALIGN_SCAN_BYTES + 8];
        input.push(b'\n');
        input.extend_from_slice(b"end");
        let mut buffer = ScrollbackBuffer::new(input.len() + 1, b"x");
        buffer.append(&input);
        assert_eq!(buffer.tail(input.len()), b"\nend");
        input[LINE_ALIGN_SCAN_BYTES - 1] = b'\n';
        let mut buffer = ScrollbackBuffer::new(input.len() + 1, b"x");
        buffer.append(&input);
        assert_eq!(buffer.tail(input.len()), &input[LINE_ALIGN_SCAN_BYTES..]);
    }

    #[test]
    fn deque_copy_skips_wrapped_prefix_without_mutation() {
        let mut ring = VecDeque::from(vec![0x80, 0xff, b'a', b'b', b'c', b'd', b'e', b'f']);
        let capacity = ring.capacity();
        append_deque_tail(&mut ring, b"123", 8);
        assert!(!ring.as_slices().1.is_empty());
        let before = ring.clone();
        let expected: Vec<u8> = ring.iter().copied().collect();
        for limit in 0..=10 {
            assert_eq!(
                copy_deque_tail(&ring, limit),
                expected[expected.len().saturating_sub(limit)..]
            );
            assert_eq!(ring, before);
            assert_eq!(ring.capacity(), capacity);
        }
        append_deque_tail(&mut ring, b"oversized", 3);
        assert_eq!(copy_deque_tail(&ring, 10), b"zed");
    }
}
