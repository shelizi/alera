//! Bounded best-match collection shared by the desktop search engines.
//! The caller supplies its own ranking and deterministic tie-breaking via Ord.
use std::collections::BinaryHeap;

/// Retains the smallest (best) K values according to the caller's Ord.
/// No more than K candidates are retained; equal keys keep the existing
/// candidate once full. Include a stable tie-breaker in Ord when needed.
pub struct TopMatches<T: Ord> {
    limit: usize,
    heap: BinaryHeap<T>,
}

impl<T: Ord> TopMatches<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::new(),
        }
    }

    pub fn worst_if_full(&self) -> Option<&T> {
        if self.heap.len() == self.limit {
            self.heap.peek()
        } else {
            None
        }
    }

    pub fn push(&mut self, candidate: T) {
        if self.limit == 0 {
            return;
        }
        if self.heap.len() < self.limit {
            self.heap.push(candidate);
        } else if self.heap.peek().is_some_and(|worst| candidate < *worst) {
            // Replace the worst match in-place and restore the heap once.
            *self.heap.peek_mut().expect("nonempty full heap") = candidate;
        }
    }

    pub fn into_sorted_vec(self) -> Vec<T> {
        self.heap.into_sorted_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Reverse;

    #[test]
    fn bounded_matches_equal_full_sort_for_dense_ties_and_limits() {
        let candidates: Vec<_> = (0..10_000)
            .rev()
            .map(|index| (Reverse(index % 17), index % 5, index))
            .collect();
        for limit in [0, 1, 17, 60, 200, 10_001] {
            let mut best = TopMatches::new(limit);
            let mut expected = candidates.clone();
            expected.sort();
            expected.truncate(limit);
            for candidate in &candidates {
                best.push(*candidate);
                assert!(best.heap.len() <= limit);
            }
            assert_eq!(best.into_sorted_vec(), expected);
        }
    }

    #[test]
    fn full_heap_exposes_worst_score_and_rejects_worse_matches() {
        let mut best = TopMatches::new(2);
        best.push(5);
        assert_eq!(best.worst_if_full(), None);
        best.push(2);
        assert_eq!(best.worst_if_full(), Some(&5));
        best.push(9);
        assert_eq!(best.worst_if_full(), Some(&5));
        best.push(1);
        assert_eq!(best.worst_if_full(), Some(&2));
        assert_eq!(best.into_sorted_vec(), vec![1, 2]);
    }
}
