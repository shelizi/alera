use super::{GitDiffLine, GitDiffLineKind, GitDiffSideBySideRow, GitDiffSideBySideRowKind};

/// UTF-16 code-unit offsets, matching Flutter TextSpan indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitIntralineRange {
    pub start: u32,
    pub end: u32,
}

fn runes(text: &str) -> Vec<(char, u32, u32)> {
    let mut offset = 0u32;
    text.chars()
        .map(|c| {
            let start = offset;
            offset += c.len_utf16() as u32;
            (c, start, offset)
        })
        .collect()
}

fn changed_ranges(items: &[(char, u32, u32)], flags: &[bool]) -> Vec<GitIntralineRange> {
    let mut ranges = Vec::new();
    let mut current = None;
    for (index, &(_, start, _)) in items.iter().enumerate() {
        if flags[index] {
            if current.is_none() {
                current = Some(start);
            }
        } else if let Some(begin) = current.take() {
            ranges.push(GitIntralineRange {
                start: begin,
                end: items[index - 1].2,
            });
        }
    }
    if let Some(begin) = current {
        ranges.push(GitIntralineRange {
            start: begin,
            end: items.last().unwrap().2,
        });
    }
    ranges
}

fn full(text: &str) -> Vec<GitIntralineRange> {
    let end = text.encode_utf16().count() as u32;
    if end == 0 {
        vec![]
    } else {
        vec![GitIntralineRange { start: 0, end }]
    }
}

fn pair(left: &str, right: &str) -> (Vec<GitIntralineRange>, Vec<GitIntralineRange>) {
    if left == right {
        return (vec![], vec![]);
    }
    let left = runes(left);
    let right = runes(right);
    if left.is_empty() || right.is_empty() {
        return (
            changed_ranges(&left, &vec![true; left.len()]),
            changed_ranges(&right, &vec![true; right.len()]),
        );
    }
    if left.len().saturating_mul(right.len()) > 65536 {
        let mut prefix = 0;
        while prefix < left.len() && prefix < right.len() && left[prefix].0 == right[prefix].0 {
            prefix += 1;
        }
        let mut l_end = left.len();
        let mut r_end = right.len();
        while l_end > prefix && r_end > prefix && left[l_end - 1].0 == right[r_end - 1].0 {
            l_end -= 1;
            r_end -= 1;
        }
        let range = |items: &[(char, u32, u32)], end: usize| {
            if end == prefix {
                vec![]
            } else {
                vec![GitIntralineRange {
                    start: items[prefix].1,
                    end: items[end - 1].2,
                }]
            }
        };
        return (range(&left, l_end), range(&right, r_end));
    }
    let width = right.len() + 1;
    let mut lcs = vec![0u32; (left.len() + 1) * width];
    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            lcs[i * width + j] = if left[i].0 == right[j].0 {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut lc = vec![true; left.len()];
    let mut rc = vec![true; right.len()];
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i].0 == right[j].0 {
            lc[i] = false;
            rc[j] = false;
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (changed_ranges(&left, &lc), changed_ranges(&right, &rc))
}

// Match replacement lines by content rather than their position in a +/- hunk.
// One inserted line must not shift every subsequent character highlight.
const MAX_LINE_ALIGNMENT_CELLS: usize = 4096;
const MAX_LINE_SIMILARITY_CELLS: usize = 4096;

// Pre-decode UTF-8 exactly once per line, not once for every candidate pairing.
struct PreparedDiffLine<'a> {
    raw: &'a str,
    chars: Vec<char>,
}

impl<'a> PreparedDiffLine<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            raw,
            chars: raw.trim().chars().collect(),
        }
    }
}

// Reuse the dynamic-programming storage for all comparisons in one hunk.
#[derive(Default)]
struct SimilarityScratch {
    previous: Vec<usize>,
    current: Vec<usize>,
}

impl SimilarityScratch {
    fn lcs(&mut self, a: &[char], b: &[char]) -> usize {
        if a.is_empty() || b.is_empty() {
            return 0;
        }
        // LCS is symmetric; use the shorter side as the DP row width.
        let (longer, shorter) = if a.len() >= b.len() { (a, b) } else { (b, a) };
        let width = shorter.len() + 1;
        self.previous.resize(width, 0);
        self.current.resize(width, 0);
        self.previous[..width].fill(0);
        for &ch in longer.iter().rev() {
            self.current[shorter.len()] = 0;
            for j in (0..shorter.len()).rev() {
                self.current[j] = if ch == shorter[j] {
                    1 + self.previous[j + 1]
                } else {
                    self.previous[j].max(self.current[j + 1])
                };
            }
            std::mem::swap(&mut self.current, &mut self.previous);
        }
        self.previous[0]
    }
}

fn line_similarity_score(
    left: &PreparedDiffLine<'_>,
    right: &PreparedDiffLine<'_>,
    scratch: &mut SimilarityScratch,
) -> i32 {
    if left.raw == right.raw {
        return 2000; // Preserve exact matches even in otherwise ambiguous hunks.
    }
    let left = left.chars.as_slice();
    let right = right.chars.as_slice();
    if left.is_empty() || right.is_empty() {
        return 0;
    }
    let mut prefix = 0;
    while prefix < left.len() && prefix < right.len() && left[prefix] == right[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < left.len() - prefix
        && suffix < right.len() - prefix
        && left[left.len() - suffix - 1] == right[right.len() - suffix - 1]
    {
        suffix += 1;
    }
    let anchored = prefix + suffix;
    // An optimal LCS can always include the common prefix and suffix.
    // Skip their DP cells, preserving the original similarity score exactly.
    let lcs = if left.len().saturating_mul(right.len()) <= MAX_LINE_SIMILARITY_CELLS {
        anchored
            + scratch.lcs(
                &left[prefix..left.len() - suffix],
                &right[prefix..right.len() - suffix],
            )
    } else {
        // Preserve the existing bounded fallback for long lines.
        anchored
    };

    let score = (200 * (7 * lcs + 3 * anchored) / (left.len() + right.len())) as i32;
    if score >= 480 {
        score
    } else {
        0
    }
}

fn align_large_change_block(left: &[&str], right: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    use std::collections::{HashMap, VecDeque};

    // Never guess replacement pairs for oversized hunks. Keep exact common
    // lines as anchors and display all remaining lines as true +/- changes.
    let mut positions: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (index, &text) in right.iter().enumerate() {
        positions.entry(text).or_default().push_back(index);
    }
    let mut result = Vec::with_capacity(left.len() + right.len());
    let mut next_right = 0;
    for (left_index, &text) in left.iter().enumerate() {
        let match_right = positions.get_mut(text).and_then(|candidates| {
            while candidates.front().is_some_and(|&index| index < next_right) {
                candidates.pop_front();
            }
            candidates.pop_front()
        });
        if let Some(right_index) = match_right {
            for index in next_right..right_index {
                result.push((None, Some(index)));
            }
            result.push((Some(left_index), Some(right_index)));
            next_right = right_index + 1;
        } else {
            result.push((Some(left_index), None));
        }
    }
    for index in next_right..right.len() {
        result.push((None, Some(index)));
    }
    result
}

pub(super) fn align_changed_lines(
    left: &[&str],
    right: &[&str],
) -> Vec<(Option<usize>, Option<usize>)> {
    // A one-sided hunk needs neither Unicode decoding nor similarity DP.
    if left.is_empty() {
        return (0..right.len()).map(|index| (None, Some(index))).collect();
    }
    if right.is_empty() {
        return (0..left.len()).map(|index| (Some(index), None)).collect();
    }
    if left.len().saturating_mul(right.len()) > MAX_LINE_ALIGNMENT_CELLS {
        return align_large_change_block(left, right);
    }
    let height = left.len();
    let width = right.len();
    let stride = width + 1;
    let prepared_left: Vec<_> = left
        .iter()
        .map(|&text| PreparedDiffLine::new(text))
        .collect();
    let prepared_right: Vec<_> = right
        .iter()
        .map(|&text| PreparedDiffLine::new(text))
        .collect();
    let mut scratch = SimilarityScratch::default();
    let mut scores = vec![0i32; height * width];
    let mut best = vec![0i32; (height + 1) * stride];
    for i in (0..height).rev() {
        for j in (0..width).rev() {
            let score = line_similarity_score(&prepared_left[i], &prepared_right[j], &mut scratch);
            scores[i * width + j] = score;
            let pair = if score > 0 {
                score + best[(i + 1) * stride + j + 1]
            } else {
                0
            };
            best[i * stride + j] = pair
                .max(best[(i + 1) * stride + j])
                .max(best[i * stride + j + 1]);
        }
    }

    let mut result = Vec::with_capacity(height + width);
    let (mut i, mut j) = (0, 0);
    while i < height || j < width {
        if i == height {
            result.push((None, Some(j)));
            j += 1;
            continue;
        }
        if j == width {
            result.push((Some(i), None));
            i += 1;
            continue;
        }
        let score = scores[i * width + j];
        let skip_left = best[(i + 1) * stride + j];
        let skip_right = best[i * stride + j + 1];
        if score > 0 && score + best[(i + 1) * stride + j + 1] >= skip_left.max(skip_right) {
            result.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if skip_left >= skip_right {
            result.push((Some(i), None));
            i += 1;
        } else {
            result.push((None, Some(j)));
            j += 1;
        }
    }
    result
}

/// Project character changes from a side-by-side row plan that has already
/// aligned +/- lines. This avoids repeating the expensive line similarity DP
/// that side_by_side_projection performed for the same GitDiffFile.
///
/// This is an internal Rust shortcut; the public project() API remains
/// available for callers that do not already have a row projection.
pub(super) fn project_from_side_by_side(
    lines: &[GitDiffLine],
    rows: &[GitDiffSideBySideRow],
) -> Vec<Vec<GitIntralineRange>> {
    let mut result = vec![vec![]; lines.len()];
    for row in rows {
        if row.kind != GitDiffSideBySideRowKind::Pair {
            continue;
        }
        // Context rows project the same line to both sides. Only changed
        // sides contribute character ranges; never highlight context lines.
        let deletion = row
            .left_line_index
            .map(|index| index as usize)
            .filter(|&index| {
                lines
                    .get(index)
                    .is_some_and(|line| line.kind == GitDiffLineKind::Deletion)
            });
        let addition = row
            .right_line_index
            .map(|index| index as usize)
            .filter(|&index| {
                lines
                    .get(index)
                    .is_some_and(|line| line.kind == GitDiffLineKind::Addition)
            });
        match (deletion, addition) {
            (Some(left), Some(right)) => {
                let (old, new) = pair(
                    lines[left].text.get(1..).unwrap_or(""),
                    lines[right].text.get(1..).unwrap_or(""),
                );
                result[left] = old;
                result[right] = new;
            }
            (Some(left), None) => {
                result[left] = full(lines[left].text.get(1..).unwrap_or(""));
            }
            (None, Some(right)) => {
                result[right] = full(lines[right].text.get(1..).unwrap_or(""));
            }
            (None, None) => {}
        }
    }
    result
}

pub fn project(lines: &[GitDiffLine]) -> Vec<Vec<GitIntralineRange>> {
    let mut result = vec![vec![]; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == GitDiffLineKind::Deletion {
            let deletion_start = index;
            while index < lines.len() && lines[index].kind == GitDiffLineKind::Deletion {
                index += 1;
            }
            let addition_start = index;
            while index < lines.len() && lines[index].kind == GitDiffLineKind::Addition {
                index += 1;
            }
            let deletions: Vec<_> = lines[deletion_start..addition_start]
                .iter()
                .map(|line| line.text.get(1..).unwrap_or(""))
                .collect();
            let additions: Vec<_> = lines[addition_start..index]
                .iter()
                .map(|line| line.text.get(1..).unwrap_or(""))
                .collect();
            for (left_index, right_index) in align_changed_lines(&deletions, &additions) {
                match (left_index, right_index) {
                    (Some(l), Some(r)) => {
                        let (lr, rr) = pair(deletions[l], additions[r]);
                        result[deletion_start + l] = lr;
                        result[addition_start + r] = rr;
                    }
                    (Some(l), None) => {
                        result[deletion_start + l] = full(deletions[l]);
                    }
                    (None, Some(r)) => {
                        result[addition_start + r] = full(additions[r]);
                    }
                    (None, None) => unreachable!(),
                }
            }
        } else {
            if lines[index].kind == GitDiffLineKind::Addition {
                result[index] = full(lines[index].text.get(1..).unwrap_or(""));
            }
            index += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_offsets_and_lcs_parity() {
        let (l, r) = pair("a😀b", "a中b");
        assert_eq!(l, vec![GitIntralineRange { start: 1, end: 3 }]);
        assert_eq!(r, vec![GitIntralineRange { start: 1, end: 2 }]);
    }
    #[test]
    fn unchanged_prefix_suffix_and_unpaired() {
        let make = |text: &str, kind| GitDiffLine {
            text: text.to_owned(),
            kind,
        };
        let lines = vec![
            make("-same", GitDiffLineKind::Deletion),
            make("-missing", GitDiffLineKind::Deletion),
            make("+same", GitDiffLineKind::Addition),
        ];
        let r = project(&lines);
        assert!(r[0].is_empty());
        assert_eq!(r[1], vec![GitIntralineRange { start: 0, end: 7 }]);
        assert!(r[2].is_empty());
    }
    #[test]
    fn large_line_fallback() {
        let l = format!("p{}z", "a".repeat(270));
        let r = format!("p{}z", "b".repeat(270));
        assert_eq!(
            pair(&l, &r).0,
            vec![GitIntralineRange { start: 1, end: 271 }]
        );
    }
    #[test]
    fn inserted_line_does_not_shift_intraline_pairs() {
        let make = |text: &str, kind| GitDiffLine {
            text: text.to_owned(),
            kind,
        };
        let lines = vec![
            make("-alpha = 1;", GitDiffLineKind::Deletion),
            make("-beta = 2;", GitDiffLineKind::Deletion),
            make("+log();", GitDiffLineKind::Addition),
            make("+alpha = 10;", GitDiffLineKind::Addition),
            make("+beta = 2;", GitDiffLineKind::Addition),
        ];
        let ranges = project(&lines);
        assert!(ranges[0].is_empty(), "unchanged old characters highlighted");
        assert!(ranges[1].is_empty(), "identical old line highlighted");
        assert_eq!(ranges[2], full("log();"));
        assert_eq!(ranges[3], vec![GitIntralineRange { start: 9, end: 10 }]);
        assert!(ranges[4].is_empty(), "identical new line highlighted");
    }

    // Guard the optimization against silently changing replacement pairing.
    // The reference deliberately retains the previous per-pair allocations.
    fn original_similarity_score(left: &str, right: &str) -> i32 {
        if left == right {
            return 2000;
        }
        let left: Vec<char> = left.trim().chars().collect();
        let right: Vec<char> = right.trim().chars().collect();
        if left.is_empty() || right.is_empty() {
            return 0;
        }
        let mut prefix = 0;
        while prefix < left.len() && prefix < right.len() && left[prefix] == right[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < left.len() - prefix
            && suffix < right.len() - prefix
            && left[left.len() - suffix - 1] == right[right.len() - suffix - 1]
        {
            suffix += 1;
        }
        let anchored = prefix + suffix;
        let lcs = if left.len().saturating_mul(right.len()) <= MAX_LINE_SIMILARITY_CELLS {
            let width = right.len() + 1;
            let mut below = vec![0usize; width];
            let mut row = vec![0usize; width];
            for i in (0..left.len()).rev() {
                row[right.len()] = 0;
                for j in (0..right.len()).rev() {
                    row[j] = if left[i] == right[j] {
                        1 + below[j + 1]
                    } else {
                        below[j].max(row[j + 1])
                    };
                }
                std::mem::swap(&mut row, &mut below);
            }
            below[0]
        } else {
            anchored
        };
        let score = (200 * (7 * lcs + 3 * anchored) / (left.len() + right.len())) as i32;
        if score >= 480 {
            score
        } else {
            0
        }
    }

    #[test]
    fn one_sided_hunks_have_no_false_character_pairings() {
        let added = ["new", "😀 Unicode", "long line"];
        assert_eq!(
            align_changed_lines(&[], &added),
            vec![(None, Some(0)), (None, Some(1)), (None, Some(2))]
        );
        assert_eq!(
            align_changed_lines(&added, &[]),
            vec![(Some(0), None), (Some(1), None), (Some(2), None)]
        );
        assert!(align_changed_lines(&[], &[]).is_empty());
    }

    #[test]
    fn optimized_line_similarity_matches_previous_algorithm() {
        let mut samples: Vec<String> = [
            "",
            " ",
            "a",
            "aaa",
            "alpha = 1;",
            "alpha = 10;",
            "beta = 2;",
            "log();",
            "  alpha = 1;  ",
            "\talpha = 1;",
            "😊中文測試",
            "😊中文改動",
            "e\u{301}",
            "é",
            "a😀b",
            "a中b",
            "prefix_same_middle_changed_suffix",
            "prefix_same_middle_saved_suffix",
            "long unchanged part",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let alphabet = ['a', 'b', 'c', '(', ')', ' ', '😃', '中', '\u{0301}'];
        for length in [2usize, 11, 43, 67, 270] {
            for salt in 0..5 {
                let mut candidate = String::new();
                for n in 0..length {
                    candidate.push(alphabet[(n * 7 + salt * 3 + n / 5) % alphabet.len()]);
                }
                samples.push(candidate);
            }
        }
        let prepared: Vec<_> = samples.iter().map(|s| PreparedDiffLine::new(s)).collect();
        let mut scratch = SimilarityScratch::default();
        for (i, old) in samples.iter().enumerate() {
            for (j, new) in samples.iter().enumerate() {
                assert_eq!(
                    line_similarity_score(&prepared[i], &prepared[j], &mut scratch),
                    original_similarity_score(old, new),
                    "score changed for candidate pair {i} / {j}: {old:?} -> {new:?}"
                );
            }
        }
    }

    // Manual CPU profile of the pre-existing two-pass native projection path.
    // Run with: cargo test --manifest-path rust/Cargo.toml --lib
    //   profile_diff_projection_reuse -- --ignored --nocapture
    #[test]
    #[ignore = "manual Rust Git Diff projection benchmark"]
    fn profile_diff_projection_reuse() {
        use std::hint::black_box;
        use std::time::Instant;

        fn fixture(count: usize, width: usize) -> Vec<GitDiffLine> {
            let mut lines = vec![GitDiffLine {
                text: format!("@@ -1,{count} +1,{count} @@"),
                kind: GitDiffLineKind::Hunk,
            }];
            let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let mut contents = Vec::with_capacity(count);
            for i in 0..count {
                let mut value = String::with_capacity(width + 24);
                for j in 0..width {
                    value.push(alphabet[(i * 13 + j * 7 + j / 4) % alphabet.len()] as char);
                }
                contents.push(value);
            }
            for (i, text) in contents.iter().enumerate() {
                lines.push(GitDiffLine {
                    text: format!("-item_{i:03} {text} done"),
                    kind: GitDiffLineKind::Deletion,
                });
            }
            for (i, text) in contents.iter().enumerate() {
                let mut changed = text.as_bytes().to_vec();
                changed[width / 2] = b'Z';
                lines.push(GitDiffLine {
                    text: format!("+item_{i:03} {} done", String::from_utf8(changed).unwrap()),
                    kind: GitDiffLineKind::Addition,
                });
            }
            lines
        }

        for (count, width, iterations) in [(10, 28, 200), (36, 48, 24), (64, 95, 24)] {
            let lines = fixture(count, width);
            let rows = super::super::git_diff_impl::side_by_side_projection(&lines);
            assert_eq!(project(&lines), project_from_side_by_side(&lines, &rows));
            for shared in [false, true] {
                let mut samples = Vec::new();
                for _ in 0..7 {
                    let began = Instant::now();
                    for _ in 0..iterations {
                        let rows =
                            super::super::git_diff_impl::side_by_side_projection(black_box(&lines));
                        let ranges = if shared {
                            project_from_side_by_side(black_box(&lines), &rows)
                        } else {
                            project(black_box(&lines))
                        };
                        black_box((rows, ranges));
                    }
                    samples.push(began.elapsed().as_secs_f64() * 1_000_000.0 / iterations as f64);
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "{}_{count}x{count}: median_us={:.2}",
                    if shared {
                        "shared_one_pass"
                    } else {
                        "original_two_pass"
                    },
                    samples[3]
                );
            }
        }
    }

    #[test]
    fn shared_row_projection_preserves_intraline_ranges() {
        let line = |kind, text: &str| GitDiffLine {
            kind,
            text: text.to_owned(),
        };
        let cases = vec![
            vec![],
            vec![line(GitDiffLineKind::Addition, "+hello")],
            vec![line(GitDiffLineKind::Deletion, "-goodbye")],
            vec![
                line(GitDiffLineKind::Hunk, "@@ -1,2 +1,3 @@"),
                line(GitDiffLineKind::Context, " unchanged"),
                line(GitDiffLineKind::Deletion, "-alpha = 1;"),
                line(GitDiffLineKind::Deletion, "-beta = 2;"),
                line(GitDiffLineKind::Addition, "+log();"),
                line(GitDiffLineKind::Addition, "+alpha = 10;"),
                line(GitDiffLineKind::Addition, "+beta = 2;"),
                line(GitDiffLineKind::Context, " shared footer"),
            ],
            vec![
                line(GitDiffLineKind::Header, "diff --git a/test b/test"),
                line(GitDiffLineKind::Hunk, "@@ -1,1 +1,2 @@"),
                line(GitDiffLineKind::Addition, "+追加😃"),
                line(GitDiffLineKind::Deletion, "-原本😀"),
                line(GitDiffLineKind::Addition, "+原本😁"),
                line(GitDiffLineKind::Hunk, "@@ -20,2 +21,1 @@"),
                line(GitDiffLineKind::Deletion, "-e\u{301}"),
                line(GitDiffLineKind::Deletion, "-中文測試"),
                line(GitDiffLineKind::Addition, "+é"),
            ],
            vec![
                line(GitDiffLineKind::Deletion, "-"),
                line(GitDiffLineKind::Addition, "+"),
                line(GitDiffLineKind::Deletion, "-   "),
                line(GitDiffLineKind::Addition, "+    "),
            ],
        ];
        for (case, lines) in cases.iter().enumerate() {
            let rows = super::super::git_diff_impl::side_by_side_projection(lines);
            assert_eq!(
                project(lines),
                project_from_side_by_side(lines, &rows),
                "changed character ranges for case {case}"
            );
        }

        // Exercise alignment ties, unmatched +/- rows, and Unicode indexing.
        let samples = [
            "alpha = 1;",
            "alpha = 10;",
            "beta = 2;",
            "shared",
            "x😀y",
            "x😃y",
            "中文測試",
            "",
            "  a()",
            "  b()",
        ];
        for old_len in 0..=6 {
            for new_len in 0..=6 {
                let mut lines = vec![line(GitDiffLineKind::Hunk, "@@ -1,6 +1,6 @@")];
                for index in 0..old_len {
                    lines.push(line(
                        GitDiffLineKind::Deletion,
                        &format!("-{}", samples[(index * 3 + old_len) % samples.len()]),
                    ));
                }
                for index in 0..new_len {
                    lines.push(line(
                        GitDiffLineKind::Addition,
                        &format!("+{}", samples[(index * 5 + new_len) % samples.len()]),
                    ));
                }
                let rows = super::super::git_diff_impl::side_by_side_projection(&lines);
                assert_eq!(
                    project(&lines),
                    project_from_side_by_side(&lines, &rows),
                    "replacement counts {old_len}/{new_len}"
                );
            }
        }

        let mut large_hunk = vec![line(GitDiffLineKind::Hunk, "@@ -1,70 +1,70 @@")];
        for index in 0..70 {
            large_hunk.push(line(
                GitDiffLineKind::Deletion,
                &format!("-id_{index}: {}z", "a".repeat(280)),
            ));
        }
        for index in 0..70 {
            large_hunk.push(line(
                GitDiffLineKind::Addition,
                &format!("+id_{index}: {}z", "b".repeat(280)),
            ));
        }
        let rows = super::super::git_diff_impl::side_by_side_projection(&large_hunk);
        assert_eq!(
            project(&large_hunk),
            project_from_side_by_side(&large_hunk, &rows),
            "oversized alignment and character-range fallback"
        );
    }

    #[test]
    fn side_by_side_alignment_matches_intraline_pairs() {
        let make = |text: &str, kind| GitDiffLine {
            text: text.to_owned(),
            kind,
        };
        let lines = vec![
            make("@@ -1,2 +1,3 @@", GitDiffLineKind::Hunk),
            make("-alpha = 1;", GitDiffLineKind::Deletion),
            make("-beta = 2;", GitDiffLineKind::Deletion),
            make("+log();", GitDiffLineKind::Addition),
            make("+alpha = 10;", GitDiffLineKind::Addition),
            make("+beta = 2;", GitDiffLineKind::Addition),
        ];
        let rows = super::super::git_diff_impl::side_by_side_projection(&lines);
        let pairs: Vec<_> = rows
            .iter()
            .filter(|row| row.kind == super::super::GitDiffSideBySideRowKind::Pair)
            .map(|row| (row.left_line_index, row.right_line_index))
            .collect();
        assert_eq!(
            pairs,
            vec![(None, Some(3)), (Some(1), Some(4)), (Some(2), Some(5))]
        );
    }
}
