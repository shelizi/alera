//! Request-local ancestor decoration shared by desktop Git Explorers.
//! Status types, precedence, normalization and path boundaries belong to callers.
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

/// Merge a leaf-to-root chain by maximum priority. Borrow existing keys;
/// allocate an owned key only when it is missing.
///
/// The map must start empty and be populated exclusively with complete,
/// canonical ancestor chains (using the same root boundary and priority).
/// An equal key must always have the same remaining ancestor chain. Once an
/// existing key is at least as strong, all its ancestors already have at
/// least that priority, so the rest of this chain can be skipped.
/// Do not use this with lossy or ambiguous normalization.
pub fn merge_ranked_ancestors<'a, K, Q, V>(
    statuses: &mut HashMap<K, V>,
    ancestors: impl IntoIterator<Item = &'a Q>,
    value: V,
    priority: impl Fn(V) -> u8,
) where
    K: Borrow<Q> + Eq + Hash,
    Q: ToOwned<Owned = K> + Eq + Hash + ?Sized + 'a,
    V: Copy,
{
    let rank = priority(value);
    for key in ancestors {
        if let Some(existing) = statuses.get_mut(key) {
            if priority(*existing) >= rank {
                break;
            }
            *existing = value;
        } else {
            statuses.insert(key.to_owned(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    #[test]
    fn borrowed_ancestors_stop_after_a_dominant_parent_and_propagate_upgrades() {
        let mut statuses = HashMap::<PathBuf, u8>::new();
        let visits = Cell::new(0);
        let mut merge = |path: &str, rank| {
            merge_ranked_ancestors(
                &mut statuses,
                Path::new(path)
                    .ancestors()
                    .take_while(|path| !path.as_os_str().is_empty())
                    .inspect(|_| visits.set(visits.get() + 1)),
                rank,
                |value| value,
            );
        };
        merge("src/features/deep/one.rs", 1);
        assert_eq!(visits.get(), 4);
        merge("src/features/deep/two.rs", 1);
        assert_eq!(visits.get(), 6);
        merge("src/features/deep/three.rs", 3);
        assert_eq!(visits.get(), 10);
        merge("src/features/deep/four.rs", 2);
        assert_eq!(visits.get(), 12);
        merge("src/features/deep/three.rs", 2);
        assert_eq!(visits.get(), 13);
        assert_eq!(statuses[Path::new("src")], 3);
        assert_eq!(statuses[Path::new("src/features")], 3);
        assert_eq!(statuses[Path::new("src/features/deep")], 3);
        assert_eq!(statuses[Path::new("src/features/deep/one.rs")], 1);
        assert_eq!(statuses[Path::new("src/features/deep/four.rs")], 2);
        assert_eq!(statuses[Path::new("src/features/deep/three.rs")], 3);
        assert!(!statuses.contains_key(Path::new("")));
    }

    #[test]
    fn shared_string_aggregation_matches_unconditional_merge_in_both_orders() {
        let input: Vec<_> = (0..2000)
            .map(|index| {
                (
                    format!("packages/group-{}/src/deep/file-{index}.rs", index % 7),
                    (index % 6 + 1) as u8,
                )
            })
            .collect();
        for reversed in [false, true] {
            let mut actual = HashMap::<String, u8>::new();
            let mut expected = HashMap::<String, u8>::new();
            let mut actual_visits = 0;
            let mut old_visits = 0;
            let order: Vec<_> = if reversed {
                input.iter().rev().collect()
            } else {
                input.iter().collect()
            };
            for (path, rank) in order {
                let chain = || {
                    std::iter::successors(Some(path.as_str()), |path| {
                        path.rsplit_once('/').map(|(parent, _)| parent)
                    })
                };
                merge_ranked_ancestors(
                    &mut actual,
                    chain().inspect(|_| actual_visits += 1),
                    *rank,
                    |value| value,
                );
                for parent in chain() {
                    old_visits += 1;
                    expected
                        .entry(parent.to_owned())
                        .and_modify(|value| *value = (*value).max(*rank))
                        .or_insert(*rank);
                }
            }
            assert_eq!(actual, expected);
            assert!(actual_visits < old_visits / 2);
        }
    }
}
