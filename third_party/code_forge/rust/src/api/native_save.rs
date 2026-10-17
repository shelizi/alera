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
}
