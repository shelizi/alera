//! Bound the retained directory ordering prefix without changing traversal order.
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ffi::OsString;
use std::fs::{self, DirEntry};
use std::io;
use std::path::Path;

pub(super) struct NamedDirectoryEntry {
    pub(super) name: OsString,
    pub(super) entry: DirEntry,
    ordinal: usize,
}

// Reverse filename order, with the same tie order as reversing the former
// stable ascending sort. The heap root is the least useful retained entry.
impl Ord for NamedDirectoryEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .name
            .cmp(&self.name)
            .then_with(|| other.ordinal.cmp(&self.ordinal))
    }
}

impl PartialOrd for NamedDirectoryEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NamedDirectoryEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for NamedDirectoryEntry {}

/// Read a fresh directory listing, retaining at most the descending prefix
/// the scanner can consume. Small directories use an ordinary sort; a wide
/// directory switches to a bounded heap. Enumeration errors are skipped
/// exactly as before, and no listing survives the current request.
pub(super) fn read_descending_prefix(
    directory: &Path,
    limit: usize,
) -> io::Result<Vec<NamedDirectoryEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(directory)?
        .flatten()
        .enumerate()
        .map(|(ordinal, entry)| NamedDirectoryEntry {
            name: entry.file_name(),
            entry,
            ordinal,
        });
    let mut prefix: Vec<_> = entries.by_ref().take(limit).collect();
    if let Some(extra) = entries.next() {
        let mut heap = BinaryHeap::from(prefix);
        for candidate in std::iter::once(extra).chain(entries) {
            if let Some(mut smallest) = heap.peek_mut() {
                if candidate < *smallest {
                    *smallest = candidate;
                }
            }
        }
        prefix = heap.into_sorted_vec();
    } else {
        prefix.sort_unstable();
    }
    Ok(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descending_prefix_matches_full_sort_for_small_and_wide_directories() {
        let dir = tempfile::tempdir().unwrap();
        for i in (0..128).rev() {
            let name = format!("entry-{:03}", (i * 37) % 128);
            if i % 3 == 0 {
                fs::create_dir(dir.path().join(name)).unwrap();
            } else {
                fs::File::create(dir.path().join(name)).unwrap();
            }
        }
        let mut expected: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        expected.sort();
        expected.reverse();
        for limit in [0, 1, 7, 63, 127, 128, 200] {
            let actual: Vec<_> = read_descending_prefix(dir.path(), limit)
                .unwrap()
                .into_iter()
                .map(|entry| entry.name)
                .collect();
            assert_eq!(
                actual,
                expected[..limit.min(expected.len())],
                "limit {limit}"
            );
        }
    }
}
