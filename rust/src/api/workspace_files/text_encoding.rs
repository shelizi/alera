use std::borrow::Cow;

use chardetng::EncodingDetector;
use encoding_rs::{BIG5, EUC_JP, EUC_KR, GBK, SHIFT_JIS, WINDOWS_1252};

use super::{
    WorkspaceDecodedText, WorkspaceFileError, WorkspaceFileErrorKind, WorkspaceTextEncoding,
};

const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
const UTF16_LE_BOM: &[u8] = b"\xff\xfe";
const UTF16_BE_BOM: &[u8] = b"\xfe\xff";

pub(super) fn decode_workspace_text_bytes_impl(
    bytes: &[u8],
    requested_encoding: Option<WorkspaceTextEncoding>,
) -> Result<WorkspaceDecodedText, WorkspaceFileError> {
    let encoding = match requested_encoding {
        Some(encoding) => encoding,
        None => detect_workspace_text_encoding(bytes)?,
    };
    let content = decode_with_encoding(bytes, encoding)?;
    Ok(WorkspaceDecodedText { content, encoding })
}

pub(super) fn encode_workspace_text_bytes_impl(
    text: &str,
    encoding: WorkspaceTextEncoding,
) -> Result<Vec<u8>, WorkspaceFileError> {
    match encoding {
        WorkspaceTextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        WorkspaceTextEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(UTF8_BOM.len() + text.len());
            bytes.extend_from_slice(UTF8_BOM);
            bytes.extend_from_slice(text.as_bytes());
            Ok(bytes)
        }
        WorkspaceTextEncoding::Utf16Le => Ok(encode_utf16(text, true)),
        WorkspaceTextEncoding::Utf16Be => Ok(encode_utf16(text, false)),
        _ => encode_legacy(text, encoding),
    }
}

fn detect_workspace_text_encoding(
    bytes: &[u8],
) -> Result<WorkspaceTextEncoding, WorkspaceFileError> {
    if bytes.starts_with(UTF8_BOM) {
        return Ok(WorkspaceTextEncoding::Utf8Bom);
    }
    if bytes.starts_with(UTF16_LE_BOM) {
        return Ok(WorkspaceTextEncoding::Utf16Le);
    }
    if bytes.starts_with(UTF16_BE_BOM) {
        return Ok(WorkspaceTextEncoding::Utf16Be);
    }
    if bytes.contains(&0) {
        return Err(unsupported_encoding("binary-looking text content"));
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Ok(WorkspaceTextEncoding::Utf8);
    }

    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let detected = detector.guess(None, true);
    encoding_from_encoding_rs(detected).ok_or_else(|| {
        unsupported_encoding(format!(
            "unsupported detected encoding: {}",
            detected.name()
        ))
    })
}

fn decode_with_encoding(
    bytes: &[u8],
    encoding: WorkspaceTextEncoding,
) -> Result<String, WorkspaceFileError> {
    match encoding {
        WorkspaceTextEncoding::Utf8 => decode_utf8(strip_prefix(bytes, UTF8_BOM)),
        WorkspaceTextEncoding::Utf8Bom => decode_utf8(strip_prefix(bytes, UTF8_BOM)),
        WorkspaceTextEncoding::Utf16Le => decode_utf16(strip_prefix(bytes, UTF16_LE_BOM), true),
        WorkspaceTextEncoding::Utf16Be => decode_utf16(strip_prefix(bytes, UTF16_BE_BOM), false),
        _ => decode_legacy(bytes, encoding),
    }
}

fn decode_utf8(bytes: &[u8]) -> Result<String, WorkspaceFileError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| unsupported_encoding("invalid UTF-8 content"))
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, WorkspaceFileError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(unsupported_encoding("invalid UTF-16 byte length"));
    }
    let units = bytes.chunks_exact(2).map(|chunk| {
        let pair = [chunk[0], chunk[1]];
        if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    });
    String::from_utf16(&units.collect::<Vec<_>>())
        .map_err(|_| unsupported_encoding("invalid UTF-16 content"))
}

fn encode_utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let bom = if little_endian {
        UTF16_LE_BOM
    } else {
        UTF16_BE_BOM
    };
    let mut bytes = Vec::with_capacity(bom.len() + text.len() * 2);
    bytes.extend_from_slice(bom);
    for unit in text.encode_utf16() {
        let encoded = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

fn decode_legacy(
    bytes: &[u8],
    encoding: WorkspaceTextEncoding,
) -> Result<String, WorkspaceFileError> {
    let codec = encoding_rs_for(encoding)
        .ok_or_else(|| unsupported_encoding("unsupported legacy text encoding"))?;
    let (decoded, had_errors) = codec.decode_without_bom_handling(bytes);
    if had_errors {
        return Err(unsupported_encoding(format!(
            "invalid {} content",
            codec.name()
        )));
    }
    Ok(decoded.into_owned())
}

fn encode_legacy(
    text: &str,
    encoding: WorkspaceTextEncoding,
) -> Result<Vec<u8>, WorkspaceFileError> {
    let codec = encoding_rs_for(encoding)
        .ok_or_else(|| unsupported_encoding("unsupported legacy text encoding"))?;
    let (encoded, _, had_errors) = codec.encode(text);
    if had_errors {
        return Err(unsupported_encoding(format!(
            "text cannot be represented as {}",
            codec.name()
        )));
    }
    Ok(match encoded {
        Cow::Borrowed(bytes) => bytes.to_vec(),
        Cow::Owned(bytes) => bytes,
    })
}

fn encoding_rs_for(encoding: WorkspaceTextEncoding) -> Option<&'static encoding_rs::Encoding> {
    match encoding {
        WorkspaceTextEncoding::Big5 => Some(BIG5),
        WorkspaceTextEncoding::Gbk => Some(GBK),
        WorkspaceTextEncoding::ShiftJis => Some(SHIFT_JIS),
        WorkspaceTextEncoding::EucJp => Some(EUC_JP),
        WorkspaceTextEncoding::EucKr => Some(EUC_KR),
        WorkspaceTextEncoding::Windows1252 => Some(WINDOWS_1252),
        WorkspaceTextEncoding::Utf8
        | WorkspaceTextEncoding::Utf8Bom
        | WorkspaceTextEncoding::Utf16Le
        | WorkspaceTextEncoding::Utf16Be => None,
    }
}

fn encoding_from_encoding_rs(
    codec: &'static encoding_rs::Encoding,
) -> Option<WorkspaceTextEncoding> {
    if std::ptr::eq(codec, BIG5) {
        Some(WorkspaceTextEncoding::Big5)
    } else if std::ptr::eq(codec, GBK) {
        Some(WorkspaceTextEncoding::Gbk)
    } else if std::ptr::eq(codec, SHIFT_JIS) {
        Some(WorkspaceTextEncoding::ShiftJis)
    } else if std::ptr::eq(codec, EUC_JP) {
        Some(WorkspaceTextEncoding::EucJp)
    } else if std::ptr::eq(codec, EUC_KR) {
        Some(WorkspaceTextEncoding::EucKr)
    } else if std::ptr::eq(codec, WINDOWS_1252) {
        Some(WorkspaceTextEncoding::Windows1252)
    } else {
        None
    }
}

fn strip_prefix<'a>(bytes: &'a [u8], prefix: &[u8]) -> &'a [u8] {
    bytes.strip_prefix(prefix).unwrap_or(bytes)
}

fn unsupported_encoding(context: impl Into<String>) -> WorkspaceFileError {
    WorkspaceFileError::new(WorkspaceFileErrorKind::Unsupported, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detects_utf8_bom_and_preserves_it_on_encode() {
        let bytes = b"\xef\xbb\xbfhello";

        let decoded = decode_workspace_text_bytes_impl(bytes, None).expect("decode UTF-8 BOM");

        assert_eq!(decoded.content, "hello");
        assert_eq!(decoded.encoding, WorkspaceTextEncoding::Utf8Bom);
        assert_eq!(
            encode_workspace_text_bytes_impl(&decoded.content, decoded.encoding)
                .expect("encode UTF-8 BOM"),
            bytes
        );
    }

    #[test]
    fn auto_detects_big5_text() {
        let (bytes, _, had_errors) = BIG5.encode("繁體中文測試");
        assert!(!had_errors);

        let decoded = decode_workspace_text_bytes_impl(bytes.as_ref(), None).expect("decode Big5");

        assert_eq!(decoded.content, "繁體中文測試");
        assert_eq!(decoded.encoding, WorkspaceTextEncoding::Big5);
    }

    #[test]
    fn manually_decodes_utf16le_without_bom() {
        let bytes = [0x41, 0x00, 0x42, 0x00, 0x2d, 0x4e];

        let decoded =
            decode_workspace_text_bytes_impl(&bytes, Some(WorkspaceTextEncoding::Utf16Le))
                .expect("decode UTF-16 LE");

        assert_eq!(decoded.content, "AB中");
        assert_eq!(decoded.encoding, WorkspaceTextEncoding::Utf16Le);
    }

    #[test]
    fn rejects_unrepresentable_characters_when_encoding() {
        let error = encode_workspace_text_bytes_impl("emoji 🙂", WorkspaceTextEncoding::Big5)
            .expect_err("Big5 should reject emoji");

        assert_eq!(error.kind, WorkspaceFileErrorKind::Unsupported);
    }

    #[test]
    fn auto_rejects_nul_heavy_binary_content() {
        let error = decode_workspace_text_bytes_impl(b"a\0b\0c", None)
            .expect_err("binary-looking content should be rejected");

        assert_eq!(error.kind, WorkspaceFileErrorKind::Unsupported);
    }
}
