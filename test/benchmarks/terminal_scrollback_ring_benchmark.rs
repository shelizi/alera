//! Run with rustc -O from a prepared checkout. Compares the previous
//! chunked scrollback algorithm with the production byte ring. Excludes PTY,
//! base64, IPC, SQLite and emulator work.
#[allow(dead_code)]
#[path = "../../rust/alera-cli/src/terminal_host/buffer.rs"]
mod current;

use std::collections::VecDeque;
use std::hint::black_box;
use std::time::Instant;

// Reference implementation of the pre-ring allocation and replay paths.
struct ChunkedScrollback {
    chunks: VecDeque<Vec<u8>>,
    len: usize,
    cap: usize,
}

impl ChunkedScrollback {
    fn new(cap: usize, initial: &[u8]) -> Self {
        let mut buffer = Self {
            chunks: VecDeque::new(),
            len: 0,
            cap,
        };
        buffer.append(initial);
        buffer
    }

    fn append(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if data.len() >= self.cap {
            self.chunks.clear();
            let tail = data[data.len() - self.cap..].to_vec();
            self.len = tail.len();
            self.chunks.push_back(tail);
            return;
        }
        self.chunks.push_back(data.to_vec());
        self.len += data.len();
        let mut excess = self.len.saturating_sub(self.cap);
        while excess > 0 {
            let first = self.chunks.pop_front().unwrap();
            if first.len() <= excess {
                self.len -= first.len();
                excess -= first.len();
            } else {
                self.chunks.push_front(first[excess..].to_vec());
                self.len -= excess;
                excess = 0;
            }
        }
    }

    fn slice_from(&self, start: usize) -> Vec<u8> {
        if start >= self.len {
            return Vec::new();
        }
        let mut output = Vec::with_capacity(self.len - start);
        let mut skipped = 0;
        for chunk in &self.chunks {
            if skipped + chunk.len() <= start {
                skipped += chunk.len();
                continue;
            }
            output.extend_from_slice(&chunk[start.saturating_sub(skipped)..]);
            skipped += chunk.len();
        }
        output
    }

    fn tail(&self, limit: usize) -> Vec<u8> {
        if self.len <= limit {
            return self.slice_from(0);
        }
        let bytes = self.slice_from(self.len - limit);
        let start = match bytes.iter().take(4096).position(|b| *b == b'\n') {
            Some(newline) => newline + 1,
            None => bytes
                .iter()
                .position(|b| b & 0b1100_0000 != 0b1000_0000)
                .unwrap_or(bytes.len()),
        };
        bytes[start..].to_vec()
    }
}

fn compare(
    label: &str,
    calls: usize,
    mut old: impl FnMut() -> usize,
    mut new: impl FnMut() -> usize,
) {
    assert_eq!(old(), new(), "initial checksum");
    let measure = |f: &mut dyn FnMut() -> usize| {
        let watch = Instant::now();
        let mut checksum = 0usize;
        for _ in 0..calls {
            checksum = checksum.wrapping_add(black_box(f()));
        }
        (watch.elapsed().as_secs_f64() * 1000.0, checksum)
    };
    for _ in 0..2 {
        assert_eq!(measure(&mut old).1, measure(&mut new).1);
    }
    let mut before = Vec::new();
    let mut after = Vec::new();
    for i in 0..7 {
        let (a, b) = if i % 2 == 0 {
            (measure(&mut old), measure(&mut new))
        } else {
            let b = measure(&mut new);
            (measure(&mut old), b)
        };
        assert_eq!(a.1, b.1);
        before.push(a.0);
        after.push(b.0);
    }
    before.sort_by(f64::total_cmp);
    after.sort_by(f64::total_cmp);
    println!(
        "{label} calls={calls} old_ms={:.4} new_ms={:.4}",
        before[3], after[3]
    );
}

fn checksum(bytes: &[u8]) -> usize {
    black_box(bytes);
    bytes
        .len()
        .wrapping_add(bytes.first().copied().unwrap_or_default() as usize)
        .wrapping_add(bytes.last().copied().unwrap_or_default() as usize)
}

fn main() {
    let cap = 1024 * 1024;
    let seed = vec![b'x'; cap];
    let mut old = ChunkedScrollback::new(cap, &seed);
    let mut new = current::ScrollbackBuffer::new(cap, &seed);
    for _ in 0..4096 {
        old.append(b"a");
        new.append(b"a");
    }
    assert_eq!(old.slice_from(0), new.to_bytes());
    compare(
        "seeded_1mib_4096_tiny_appends",
        1,
        || {
            let mut b = ChunkedScrollback::new(cap, &seed);
            for _ in 0..4096 {
                b.append(black_box(b"a"));
            }
            checksum(&b.slice_from(0))
        },
        || {
            let mut b = current::ScrollbackBuffer::new(cap, &seed);
            for _ in 0..4096 {
                b.append(black_box(b"a"));
            }
            checksum(&b.to_bytes())
        },
    );

    let cap = 8 * 1024 * 1024;
    let data: Vec<u8> = (0..128)
        .map(|i| if i == 120 { b'\n' } else { (i + 33) as u8 })
        .collect();
    let mut old = ChunkedScrollback::new(cap, &[]);
    let mut new = current::ScrollbackBuffer::new(cap, &[]);
    for _ in 0..65_636 {
        old.append(&data);
        new.append(&data);
    }
    assert_eq!(old.slice_from(0), new.to_bytes());
    assert_eq!(old.slice_from(cap - 4096), new.slice_from(cap - 4096));
    assert_eq!(old.tail(4 * 1024 * 1024), new.tail(4 * 1024 * 1024));
    compare(
        "resume_4k_from_8mib",
        2000,
        || checksum(&old.slice_from(black_box(cap - 4096))),
        || checksum(&new.slice_from(black_box(cap - 4096))),
    );
    compare(
        "aligned_4mib_restore",
        32,
        || checksum(&old.tail(black_box(4 * 1024 * 1024))),
        || checksum(&new.tail(black_box(4 * 1024 * 1024))),
    );
    compare(
        "steady_128_byte_pty_appends",
        100_000,
        || {
            old.append(black_box(&data));
            old.len
        },
        || {
            new.append(black_box(&data));
            new.len()
        },
    );
    assert_eq!(old.slice_from(0), new.to_bytes());

    let mut ring = VecDeque::from(seed);
    for _ in 0..500 {
        current::append_deque_tail(&mut ring, &data, 1024 * 1024);
    }
    let legacy_copy = |limit: usize| {
        ring.iter()
            .skip(ring.len().saturating_sub(limit))
            .copied()
            .collect::<Vec<u8>>()
    };
    assert_eq!(legacy_copy(65536), current::copy_deque_tail(&ring, 65536));
    compare(
        "cached_64k_tail",
        4096,
        || checksum(&legacy_copy(black_box(65536))),
        || checksum(&current::copy_deque_tail(&ring, black_box(65536))),
    );
}
