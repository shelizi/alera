use super::{GitDiffLine, GitDiffLineKind};

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
            let count = (addition_start - deletion_start).min(index - addition_start);
            for offset in 0..count {
                let l = deletion_start + offset;
                let r = addition_start + offset;
                let (lr, rr) = pair(
                    lines[l].text.get(1..).unwrap_or(""),
                    lines[r].text.get(1..).unwrap_or(""),
                );
                result[l] = lr;
                result[r] = rr;
            }
            for i in deletion_start + count..addition_start {
                result[i] = full(lines[i].text.get(1..).unwrap_or(""));
            }
            for i in addition_start + count..index {
                result[i] = full(lines[i].text.get(1..).unwrap_or(""));
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
}
