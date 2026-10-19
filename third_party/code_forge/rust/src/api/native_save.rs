//! Native snapshot byte encoding; the disk writer remains behind a safety gate.
use super::workspace_source::NativeWorkspaceTextEncoding as E;
use encoding_rs::{BIG5, EUC_JP, EUC_KR, GBK, SHIFT_JIS, WINDOWS_1252};
use ropey::Rope;

pub(crate) fn snapshot_bytes(
    rope: &Rope,
    original: &str,
    expanded: &str,
    encoding: E,
) -> Result<Vec<u8>, String> {
    // With no tab expansion, the saved text is exactly the native Rope.
    // Copy its chunks straight to the destination bytes, avoiding a full
    // Rope -> String allocation and the line-segmentation/copying pass.
    if original == expanded {
        match encoding {
            E::Utf8 => return Ok(rope_utf8_bytes(rope, false)),
            E::Utf8Bom => return Ok(rope_utf8_bytes(rope, true)),
            E::Utf16Le | E::Utf16Be => return Ok(rope_utf16_bytes(rope, encoding == E::Utf16Le)),
            // encoding_rs currently needs a continuous string; it still
            // benefits from skipping preserve_tabs when nothing was expanded.
            _ => return encode_snapshot_text(rope.to_string(), encoding),
        }
    }
    let raw = preserve_tabs(&rope.to_string(), original, expanded);
    encode_snapshot_text(raw, encoding)
}

fn rope_utf8_bytes(rope: &Rope, bom: bool) -> Vec<u8> {
    let mut output = Vec::with_capacity(rope.len_bytes() + if bom { 3 } else { 0 });
    if bom {
        output.extend_from_slice(b"\xef\xbb\xbf");
    }
    for chunk in rope.chunks() {
        output.extend_from_slice(chunk.as_bytes());
    }
    output
}

fn rope_utf16_bytes(rope: &Rope, little_endian: bool) -> Vec<u8> {
    let mut output = Vec::with_capacity(2 + rope.len_chars().saturating_mul(2));
    output.extend_from_slice(if little_endian {
        &[0xff, 0xfe]
    } else {
        &[0xfe, 0xff]
    });
    // Rope chunks always end at UTF-8 character boundaries, even for
    // multibyte characters and surrogate pairs in the output.
    for chunk in rope.chunks() {
        for unit in chunk.encode_utf16() {
            output.extend_from_slice(&if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
    }
    output
}

fn encode_snapshot_text(raw: String, encoding: E) -> Result<Vec<u8>, String> {
    Ok(match encoding {
        E::Utf8 => raw.into_bytes(),
        E::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), raw.as_bytes()].concat(),
        E::Utf16Le | E::Utf16Be => {
            let little = encoding == E::Utf16Le;
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in raw.encode_utf16() {
                bytes.extend_from_slice(&if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            bytes
        }
        _ => {
            let codec = match encoding {
                E::Big5 => BIG5,
                E::Gbk => GBK,
                E::ShiftJis => SHIFT_JIS,
                E::EucJp => EUC_JP,
                E::EucKr => EUC_KR,
                E::Windows1252 => WINDOWS_1252,
                _ => unreachable!(),
            };
            let (bytes, _, errors) = codec.encode(&raw);
            if errors {
                return Err(format!("unrepresentable as {}", codec.name()));
            }
            bytes.into_owned()
        }
    })
}
fn segments(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let (mut result, mut start, mut index) = (vec![], 0, 0);
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 1;
            }
            result.push(&text[start..=index]);
            start = index + 1;
        } else if bytes[index] == b'\n' {
            result.push(&text[start..=index]);
            start = index + 1;
        }
        index += 1;
    }
    if start < bytes.len() {
        result.push(&text[start..]);
    }
    result
}
fn preserve_tabs(current: &str, raw: &str, display: &str) -> String {
    if current == display {
        return raw.to_owned();
    }
    let (a, b, c) = (segments(raw), segments(display), segments(current));
    if a.len() != b.len() {
        return current.to_owned();
    }
    let mut prefix = 0;
    while prefix < b.len() && prefix < c.len() && b[prefix] == c[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < b.len().saturating_sub(prefix)
        && suffix < c.len().saturating_sub(prefix)
        && b[b.len() - suffix - 1] == c[c.len() - suffix - 1]
    {
        suffix += 1;
    }
    let mut out = String::with_capacity(current.len());
    for (i, item) in c.iter().enumerate() {
        if i < prefix {
            out.push_str(a[i]);
        } else if i >= c.len() - suffix {
            out.push_str(a[a.len() - (c.len() - i)]);
        } else {
            out.push_str(item);
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    // Reference implementation retained for byte parity and A/B CPU profiling.
    // It mirrors the old Rope -> String -> line-segments -> encoder path.
    fn original_snapshot_bytes(
        rope: &Rope,
        original: &str,
        expanded: &str,
        encoding: E,
    ) -> Result<Vec<u8>, String> {
        let text = rope.to_string();
        let raw = preserve_tabs(&text, original, expanded);
        Ok(match encoding {
            E::Utf8 => raw.into_bytes(),
            E::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), raw.as_bytes()].concat(),
            E::Utf16Le | E::Utf16Be => {
                let little = encoding == E::Utf16Le;
                let mut bytes = if little {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xfe, 0xff]
                };
                for unit in raw.encode_utf16() {
                    bytes.extend_from_slice(&if little {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
                bytes
            }
            _ => {
                let codec = match encoding {
                    E::Big5 => BIG5,
                    E::Gbk => GBK,
                    E::ShiftJis => SHIFT_JIS,
                    E::EucJp => EUC_JP,
                    E::EucKr => EUC_KR,
                    E::Windows1252 => WINDOWS_1252,
                    _ => unreachable!(),
                };
                let (bytes, _, errors) = codec.encode(&raw);
                if errors {
                    return Err(format!("unrepresentable as {}", codec.name()));
                }
                bytes.into_owned()
            }
        })
    }

    #[test]
    fn rope_snapshot_matches_reference_across_encodings_and_lines() {
        let cases = [
            (
                "fn main() {\r\n println!(\"ok\");\r\n}\r\n",
                "fn main() {\r\n println!(\"edit\");\r\n}\r\n",
            ),
            ("α 😀 漢字\r\n", "α 😃 漢字\r\n"),
            ("one\n\ntwo\n", "one\n\nnew two\n"),
            ("a\rb\r", "a\rc\r"),
            ("", "new"),
            ("original", ""),
            (
                "\tline one\r\n\tline two\r\n",
                "    line one\r\n    altered\r\n",
            ),
            ("abc\tdef\n", "abc def\n"),
        ];
        let encodings = [
            E::Utf8,
            E::Utf8Bom,
            E::Utf16Le,
            E::Utf16Be,
            E::Big5,
            E::Gbk,
            E::ShiftJis,
            E::EucJp,
            E::EucKr,
            E::Windows1252,
        ];
        for encoding in encodings {
            for (raw, current) in cases {
                let displayed = if raw.contains('\t') {
                    super::super::workspace_source::expand_tabs(raw, 4)
                } else {
                    raw.to_owned()
                };
                let rope = Rope::from_str(current);
                assert_eq!(
                    snapshot_bytes(&rope, raw, &displayed, encoding),
                    original_snapshot_bytes(&rope, raw, &displayed, encoding),
                    "{encoding:?} changed {raw:?} -> {current:?}"
                );
            }
        }

        // Include thousands of Rope chunks, split Unicode scalars, CRLF, and
        // supplementary-plane chars: streaming must never split code points.
        let original = "a😀中\r\n".repeat(40_000);
        let changed = format!("{original}new 😃 line\n");
        let rope = Rope::from_str(&changed);
        for encoding in [E::Utf8, E::Utf8Bom, E::Utf16Le, E::Utf16Be] {
            assert_eq!(
                snapshot_bytes(&rope, &original, &original, encoding).unwrap(),
                original_snapshot_bytes(&rope, &original, &original, encoding).unwrap(),
                "{encoding:?} multi-chunk Unicode input"
            );
        }
    }

    #[test]
    #[ignore = "manual Rust-only baseline-vs-streaming snapshot encoding CPU benchmark"]
    fn profile_snapshot_encoding_mib() {
        use std::{hint::black_box, time::Instant};
        for mib in [1usize, 4, 10] {
            let old = "a\n".repeat(mib * 1024 * 1024 / 2);
            let changed = "b\n".repeat(mib * 1024 * 1024 / 2);
            let rope = Rope::from_str(&changed);
            for encoding in [E::Utf8, E::Utf16Le] {
                let previous = original_snapshot_bytes(&rope, &old, &old, encoding).unwrap();
                assert_eq!(
                    snapshot_bytes(&rope, &old, &old, encoding).unwrap(),
                    previous,
                    "byte parity required before benchmarking"
                );
                let iterations = if mib == 1 {
                    18
                } else if mib == 4 {
                    7
                } else {
                    3
                };
                let mut originals = Vec::new();
                let mut optimized = Vec::new();
                for round in 0..8 {
                    let mut measure = |legacy: bool| {
                        let started = Instant::now();
                        for _ in 0..iterations {
                            let result = if legacy {
                                original_snapshot_bytes(
                                    black_box(&rope),
                                    black_box(&old),
                                    black_box(&old),
                                    black_box(encoding),
                                )
                                .unwrap()
                            } else {
                                snapshot_bytes(
                                    black_box(&rope),
                                    black_box(&old),
                                    black_box(&old),
                                    black_box(encoding),
                                )
                                .unwrap()
                            };
                            black_box(result);
                        }
                        if round > 0 {
                            let us =
                                started.elapsed().as_secs_f64() * 1_000_000.0 / iterations as f64;
                            (if legacy {
                                &mut originals
                            } else {
                                &mut optimized
                            })
                            .push(us);
                        }
                    };
                    if round % 2 == 0 {
                        measure(true);
                        measure(false);
                    } else {
                        measure(false);
                        measure(true);
                    }
                }
                originals.sort_by(f64::total_cmp);
                optimized.sort_by(f64::total_cmp);
                eprintln!(
                    "ROPE_SNAPSHOT_ENCODE mib={mib} encoding={encoding:?} old_median_us={:.1} new_median_us={:.1} samples=7",
                    originals[3], optimized[3]
                );
            }
        }
    }

    #[test]
    fn unchanged_tabs() {
        let r = Rope::from_str("    alpha\n    beta\n");
        assert_eq!(
            snapshot_bytes(&r, "\talpha\n\tbeta\n", "    alpha\n    beta\n", E::Utf8).unwrap(),
            b"\talpha\n\tbeta\n"
        );
    }
    #[test]
    fn changed_middle() {
        let r = Rope::from_str("    a\n    edited\n    c\n");
        assert_eq!(
            snapshot_bytes(&r, "\ta\n\tb\n\tc\n", "    a\n    b\n    c\n", E::Utf8).unwrap(),
            b"\ta\n    edited\n\tc\n"
        );
    }
    #[test]
    fn unsupported_big5() {
        assert!(snapshot_bytes(&Rope::from_str("🦀"), "abc", "abc", E::Big5).is_err());
    }
    #[test]
    fn utf16_bom() {
        assert!(snapshot_bytes(&Rope::from_str("中"), "a", "a", E::Utf16Le)
            .unwrap()
            .starts_with(b"\xff\xfe"));
    }

    #[test]
    fn supported_legacy_encodings_round_trip_without_replacement() {
        for (encoding, text, codec) in [
            (E::Big5, "繁體中文", BIG5),
            (E::Gbk, "简体中文", GBK),
            (E::ShiftJis, "日本語", SHIFT_JIS),
        ] {
            let raw = snapshot_bytes(&Rope::from_str(text), "before", "before", encoding).unwrap();
            let (decoded, errors) = codec.decode_without_bom_handling(&raw);
            assert!(!errors);
            assert_eq!(decoded, text);
        }
    }
}
