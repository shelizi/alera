use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub(super) struct QuickOpenIndex {
    pub(super) files: Vec<Arc<QuickOpenFile>>,
    pub(super) segment_prefixes: SegmentPrefixIndex,
    pub(super) character_index: CharacterIndex,
}

impl QuickOpenIndex {
    #[cfg(test)]
    pub(super) fn new(files: Vec<QuickOpenFile>) -> Self {
        Self::from_files(files.into_iter().map(Arc::new).collect())
    }

    fn from_files(files: Vec<Arc<QuickOpenFile>>) -> Self {
        let segment_prefixes = SegmentPrefixIndex::build(&files);
        let character_index = CharacterIndex::build(&files);
        Self {
            files,
            segment_prefixes,
            character_index,
        }
    }

    // A fresh, fully validated directory scan supplies sorted descriptors.
    // Reuse the entire immutable ranking index if that snapshot did not
    // change. Otherwise merge-join by path and retain unchanged file data.
    pub(super) fn from_paths(paths: Vec<QuickOpenPath>, previous: Option<&Arc<Self>>) -> Arc<Self> {
        if let Some(previous) = previous {
            if paths.len() == previous.files.len()
                && paths
                    .iter()
                    .zip(&previous.files)
                    .all(|(path, file)| path.matches(file))
            {
                return Arc::clone(previous);
            }
        }
        let mut prior = previous
            .into_iter()
            .flat_map(|index| index.files.iter())
            .peekable();
        let files = paths
            .into_iter()
            .map(|path| {
                while prior
                    .peek()
                    .is_some_and(|file| path.compare(file) == Ordering::Greater)
                {
                    prior.next();
                }
                if prior
                    .peek()
                    .is_some_and(|file| path.compare(file) == Ordering::Equal)
                {
                    let old = prior.next().expect("matching previous entry");
                    if path.matches(old) {
                        return Arc::clone(old);
                    }
                }
                Arc::new(QuickOpenFile::from_path(path))
            })
            .collect();
        Arc::new(Self::from_files(files))
    }
}

#[derive(Debug)]
pub(super) struct CharacterIndex {
    entries: Vec<(u16, Vec<CharacterMatch>)>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CharacterMatch {
    pub(super) file_index: usize,
    count: u8,
}

impl CharacterIndex {
    fn build(files: &[Arc<QuickOpenFile>]) -> Self {
        let mut entries = HashMap::<u16, Vec<CharacterMatch>>::new();
        for (file_index, file) in files.iter().enumerate() {
            for &(code_unit, count) in &file.normalized_character_counts {
                entries
                    .entry(code_unit)
                    .or_default()
                    .push(CharacterMatch { file_index, count });
            }
        }
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_unstable_by_key(|(code_unit, _)| *code_unit);
        for (_, matches) in &mut entries {
            matches.sort_unstable_by(|left, right| {
                right
                    .count
                    .cmp(&left.count)
                    .then_with(|| left.file_index.cmp(&right.file_index))
            });
        }
        Self { entries }
    }

    pub(super) fn candidates(&self, query: &[(u16, u8)]) -> Option<&[CharacterMatch]> {
        let mut selected = None;
        let mut selected_len = usize::MAX;
        for &(code_unit, required_count) in query {
            let entry_index = self
                .entries
                .binary_search_by_key(&code_unit, |(unit, _)| *unit)
                .ok()?;
            let matches = &self.entries[entry_index].1;
            let eligible_len = matches.partition_point(|matching| matching.count >= required_count);
            if eligible_len == 0 {
                return None;
            }
            if eligible_len < selected_len {
                selected = Some(&matches[..eligible_len]);
                selected_len = eligible_len;
            }
        }
        selected
    }
}

#[derive(Debug)]
pub(super) struct SegmentPrefixIndex {
    nodes: Vec<SegmentPrefixNode>,
}

#[derive(Debug)]
pub(super) struct SegmentPrefixNode {
    children: Vec<(u16, usize)>,
    pub(super) prefix_matches: Vec<SegmentMatch>,
    pub(super) exact_matches: Vec<SegmentMatch>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentMatch {
    pub(super) file_index: usize,
    pub(super) segment_index: usize,
}

impl SegmentPrefixIndex {
    fn build(files: &[Arc<QuickOpenFile>]) -> Self {
        let mut index = Self {
            nodes: vec![SegmentPrefixNode {
                children: Vec::new(),
                prefix_matches: Vec::new(),
                exact_matches: Vec::new(),
            }],
        };
        for (file_index, file) in files.iter().enumerate() {
            for (segment_index, &(start, end)) in file.normalized_segment_ranges.iter().enumerate()
            {
                let segment = &file.normalized_path[start..end];
                let mut node_index = 0;
                for code_unit in segment.encode_utf16() {
                    let child = index.nodes[node_index]
                        .children
                        .iter()
                        .find(|(child_unit, _)| *child_unit == code_unit)
                        .map(|(_, child_index)| *child_index);
                    node_index = match child {
                        Some(child_index) => child_index,
                        None => {
                            let child_index = index.nodes.len();
                            index.nodes.push(SegmentPrefixNode {
                                children: Vec::new(),
                                prefix_matches: Vec::new(),
                                exact_matches: Vec::new(),
                            });
                            index.nodes[node_index]
                                .children
                                .push((code_unit, child_index));
                            child_index
                        }
                    };
                    append_first_file_match(
                        &mut index.nodes[node_index].prefix_matches,
                        file_index,
                        segment_index,
                    );
                }
                append_first_file_match(
                    &mut index.nodes[node_index].exact_matches,
                    file_index,
                    segment_index,
                );
            }
        }
        for node in &mut index.nodes {
            sort_matches(&mut node.prefix_matches);
            sort_matches(&mut node.exact_matches);
        }
        index
    }

    pub(super) fn lookup(&self, query: &[u16]) -> Option<&SegmentPrefixNode> {
        let mut node_index = 0;
        for &code_unit in query {
            node_index = self.nodes[node_index]
                .children
                .iter()
                .find(|(child_unit, _)| *child_unit == code_unit)
                .map(|(_, child_index)| *child_index)?;
        }
        Some(&self.nodes[node_index])
    }
}

fn append_first_file_match(
    matches: &mut Vec<SegmentMatch>,
    file_index: usize,
    segment_index: usize,
) {
    // Traversal is already ascending by file, then segment. The first
    // posting per file is exactly the one the old sort/dedup retained.
    if matches
        .last()
        .is_none_or(|last| last.file_index != file_index)
    {
        matches.push(SegmentMatch {
            file_index,
            segment_index,
        });
    }
}

fn sort_matches(matches: &mut [SegmentMatch]) {
    matches.sort_unstable_by_key(|matching| (matching.segment_index, matching.file_index));
}

#[derive(Debug)]
pub(super) struct QuickOpenPath {
    pub(super) relative_path: String,
    pub(super) is_gitignored: bool,
    pub(super) is_directory: bool,
    pub(super) normalized_path: String,
}

impl QuickOpenPath {
    pub(super) fn new(relative_path: String, is_gitignored: bool, is_directory: bool) -> Self {
        let normalized_path = relative_path.to_lowercase();
        Self {
            relative_path,
            is_gitignored,
            is_directory,
            normalized_path,
        }
    }

    fn compare(&self, file: &QuickOpenFile) -> Ordering {
        self.normalized_path
            .cmp(&file.normalized_path)
            .then_with(|| self.relative_path.cmp(&file.relative_path))
    }

    fn matches(&self, file: &QuickOpenFile) -> bool {
        self.compare(file) == Ordering::Equal
            && self.is_gitignored == file.is_gitignored
            && self.is_directory == file.is_directory
    }
}

#[derive(Debug)]
pub(super) struct QuickOpenFile {
    pub(super) relative_path: String,
    pub(super) is_gitignored: bool,
    pub(super) is_directory: bool,
    pub(super) normalized_path: String,
    pub(super) normalized_segment_ranges: Vec<(usize, usize)>,
    pub(super) normalized_code_units: Vec<u16>,
    pub(super) normalized_character_counts: Vec<(u16, u8)>,
}

impl QuickOpenFile {
    #[cfg(test)]
    pub(super) fn new(relative_path: String) -> Self {
        Self::with_kind(relative_path, false, false)
    }

    #[cfg(test)]
    pub(super) fn with_kind(
        relative_path: String,
        is_gitignored: bool,
        is_directory: bool,
    ) -> Self {
        Self::from_path(QuickOpenPath::new(
            relative_path,
            is_gitignored,
            is_directory,
        ))
    }

    fn from_path(path: QuickOpenPath) -> Self {
        let QuickOpenPath {
            relative_path,
            is_gitignored,
            is_directory,
            normalized_path,
        } = path;
        let normalized_segment_ranges = segment_ranges(&normalized_path);
        let normalized_code_units = normalized_path.encode_utf16().collect::<Vec<_>>();
        let normalized_character_counts = character_counts(&normalized_code_units);
        Self {
            relative_path,
            is_gitignored,
            is_directory,
            normalized_path,
            normalized_segment_ranges,
            normalized_code_units,
            normalized_character_counts,
        }
    }
}

fn segment_ranges(path: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut segment_start = 0;
    for (index, byte) in path.bytes().enumerate() {
        if byte == b'/' || byte == b'\\' {
            if segment_start < index {
                ranges.push((segment_start, index));
            }
            segment_start = index + 1;
        }
    }
    if segment_start < path.len() {
        ranges.push((segment_start, path.len()));
    }
    ranges
}

pub(super) fn character_counts(code_units: &[u16]) -> Vec<(u16, u8)> {
    let mut sorted = code_units.to_vec();
    sorted.sort_unstable();
    let mut counts: Vec<(u16, u8)> = Vec::new();
    for code_unit in sorted {
        match counts.last_mut() {
            Some((last_code_unit, count)) if *last_code_unit == code_unit => {
                *count = count.saturating_add(1);
            }
            _ => counts.push((code_unit, 1)),
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_posting_per_file_equals_old_sort_dedup_for_every_prefix() {
        let files = [
            "src/src/src/main.rs",
            "src/source/src/main.rs",
            "source/src/src.rs",
            "中文/中文/中文🙂.rs",
            "ab/abc/abcd/ab.rs",
            "SRC/src/src",
        ]
        .into_iter()
        .map(|path| Arc::new(QuickOpenFile::new(path.into())))
        .collect::<Vec<_>>();
        let index = SegmentPrefixIndex::build(&files);
        let mut prefixes = HashMap::<Vec<u16>, Vec<(usize, usize)>>::new();
        let mut exact = HashMap::<Vec<u16>, Vec<(usize, usize)>>::new();
        for (file_index, file) in files.iter().enumerate() {
            for (segment_index, &(start, end)) in file.normalized_segment_ranges.iter().enumerate()
            {
                let segment = file.normalized_path[start..end]
                    .encode_utf16()
                    .collect::<Vec<_>>();
                for end in 1..=segment.len() {
                    prefixes
                        .entry(segment[..end].to_vec())
                        .or_default()
                        .push((file_index, segment_index));
                }
                exact
                    .entry(segment)
                    .or_default()
                    .push((file_index, segment_index));
            }
        }
        let old = |mut postings: Vec<(usize, usize)>| {
            postings.sort_unstable();
            postings.dedup_by_key(|(file, _)| *file);
            postings.sort_unstable_by_key(|&(file, segment)| (segment, file));
            postings
        };
        for (prefix, expected) in prefixes {
            let node = index.lookup(&prefix).unwrap();
            let actual = |postings: &[SegmentMatch]| {
                postings
                    .iter()
                    .map(|m| (m.file_index, m.segment_index))
                    .collect::<Vec<_>>()
            };
            assert_eq!(actual(&node.prefix_matches), old(expected));
            assert_eq!(
                actual(&node.exact_matches),
                old(exact.remove(&prefix).unwrap_or_default())
            );
        }
        assert!(exact.is_empty());
    }
}
