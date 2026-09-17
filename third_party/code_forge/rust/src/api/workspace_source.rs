use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use chardetng::EncodingDetector;
use encoding_rs::{BIG5, EUC_JP, EUC_KR, GBK, SHIFT_JIS, WINDOWS_1252};

const MAX_TEXT_FILE_BYTES: u64 = 10 * 1024 * 1024;
const PROTECTED_NAMES: [&str; 3] = [".git", ".hg", ".svn"];
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
const UTF16_LE_BOM: &[u8] = b"\xff\xfe";
const UTF16_BE_BOM: &[u8] = b"\xfe\xff";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeWorkspaceTextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Big5,
    Gbk,
    ShiftJis,
    EucJp,
    EucKr,
    Windows1252,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSourceInfo {
    pub encoding: NativeWorkspaceTextEncoding,
    pub content_token: String,
    pub modified_millis: i64,
    pub size: u64,
    pub raw_chars: usize,
    pub display_chars: usize,
}

pub(crate) struct DecodedWorkspaceSource {
    pub display_content: String,
    pub info: WorkspaceSourceInfo,
}

pub(crate) fn open_workspace_source(
    workspace_path: &str,
    relative_path: &str,
    tab_size: i32,
    requested_encoding: Option<NativeWorkspaceTextEncoding>,
) -> Result<DecodedWorkspaceSource, String> {
    reject_protected(relative_path)?;
    let root = fs::canonicalize(workspace_path)
        .map_err(|error| format!("workspace path {workspace_path}: {error}"))?;
    let path = resolve_existing(&root, relative_path)?;
    let metadata = fs::metadata(&path).map_err(|error| format!("{relative_path}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("not a file: {relative_path}"));
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(format!(
            "unsupported text file size {} bytes (limit {MAX_TEXT_FILE_BYTES})",
            metadata.len()
        ));
    }
    let bytes = fs::read(&path).map_err(|error| format!("{relative_path}: {error}"))?;
    let encoding = requested_encoding.unwrap_or(detect_encoding(&bytes)?);
    let raw_content = decode_with_encoding(&bytes, encoding)?;
    let raw_chars = raw_content.chars().count();
    let display_content = expand_tabs(&raw_content, tab_size);
    let display_chars = display_content.chars().count();
    let modified_millis = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default();
    Ok(DecodedWorkspaceSource {
        display_content,
        info: WorkspaceSourceInfo {
            encoding,
            content_token: format!("{}:{modified_millis}", metadata.len()),
            modified_millis,
            size: metadata.len(),
            raw_chars,
            display_chars,
        },
    })
}

fn resolve_existing(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let path = root.join(relative_components(relative_path)?);
    let canonical = fs::canonicalize(&path).map_err(|error| format!("{relative_path}: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(format!("path escapes workspace: {relative_path}"));
    }
    Ok(canonical)
}

fn relative_components(relative_path: &str) -> Result<PathBuf, String> {
    if relative_path.trim().is_empty() {
        return Ok(PathBuf::new());
    }
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(format!(
            "absolute workspace path is not allowed: {relative_path}"
        ));
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(format!("invalid workspace path: {relative_path}")),
        }
    }
    Ok(out)
}

fn reject_protected(relative_path: &str) -> Result<(), String> {
    if Path::new(relative_path).components().any(|component| {
        matches!(component, Component::Normal(part) if part.to_str().is_some_and(|part| PROTECTED_NAMES.contains(&part)))
    }) {
        return Err(format!("protected workspace path: {relative_path}"));
    }
    Ok(())
}

fn detect_encoding(bytes: &[u8]) -> Result<NativeWorkspaceTextEncoding, String> {
    if bytes.starts_with(UTF8_BOM) {
        return Ok(NativeWorkspaceTextEncoding::Utf8Bom);
    }
    if bytes.starts_with(UTF16_LE_BOM) {
        return Ok(NativeWorkspaceTextEncoding::Utf16Le);
    }
    if bytes.starts_with(UTF16_BE_BOM) {
        return Ok(NativeWorkspaceTextEncoding::Utf16Be);
    }
    if bytes.contains(&0) {
        return Err("binary-looking text content".to_string());
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Ok(NativeWorkspaceTextEncoding::Utf8);
    }
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    encoding_from_encoding_rs(detector.guess(None, true))
        .ok_or_else(|| "unsupported detected text encoding".to_string())
}

fn decode_with_encoding(
    bytes: &[u8],
    encoding: NativeWorkspaceTextEncoding,
) -> Result<String, String> {
    match encoding {
        NativeWorkspaceTextEncoding::Utf8 | NativeWorkspaceTextEncoding::Utf8Bom => {
            std::str::from_utf8(strip_prefix(bytes, UTF8_BOM))
                .map(str::to_owned)
                .map_err(|_| "invalid UTF-8 content".to_string())
        }
        NativeWorkspaceTextEncoding::Utf16Le => {
            decode_utf16(strip_prefix(bytes, UTF16_LE_BOM), true)
        }
        NativeWorkspaceTextEncoding::Utf16Be => {
            decode_utf16(strip_prefix(bytes, UTF16_BE_BOM), false)
        }
        _ => decode_legacy(bytes, encoding),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("invalid UTF-16 byte length".to_string());
    }
    let units = bytes.chunks_exact(2).map(|chunk| {
        let pair = [chunk[0], chunk[1]];
        if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    });
    String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| "invalid UTF-16 content".to_string())
}

fn decode_legacy(bytes: &[u8], encoding: NativeWorkspaceTextEncoding) -> Result<String, String> {
    let codec =
        encoding_rs_for(encoding).ok_or_else(|| "unsupported legacy text encoding".to_string())?;
    let (decoded, had_errors) = codec.decode_without_bom_handling(bytes);
    if had_errors {
        return Err(format!("invalid {} content", codec.name()));
    }
    Ok(decoded.into_owned())
}

fn encoding_rs_for(
    encoding: NativeWorkspaceTextEncoding,
) -> Option<&'static encoding_rs::Encoding> {
    match encoding {
        NativeWorkspaceTextEncoding::Big5 => Some(BIG5),
        NativeWorkspaceTextEncoding::Gbk => Some(GBK),
        NativeWorkspaceTextEncoding::ShiftJis => Some(SHIFT_JIS),
        NativeWorkspaceTextEncoding::EucJp => Some(EUC_JP),
        NativeWorkspaceTextEncoding::EucKr => Some(EUC_KR),
        NativeWorkspaceTextEncoding::Windows1252 => Some(WINDOWS_1252),
        _ => None,
    }
}

fn encoding_from_encoding_rs(
    codec: &'static encoding_rs::Encoding,
) -> Option<NativeWorkspaceTextEncoding> {
    if std::ptr::eq(codec, BIG5) {
        Some(NativeWorkspaceTextEncoding::Big5)
    } else if std::ptr::eq(codec, GBK) {
        Some(NativeWorkspaceTextEncoding::Gbk)
    } else if std::ptr::eq(codec, SHIFT_JIS) {
        Some(NativeWorkspaceTextEncoding::ShiftJis)
    } else if std::ptr::eq(codec, EUC_JP) {
        Some(NativeWorkspaceTextEncoding::EucJp)
    } else if std::ptr::eq(codec, EUC_KR) {
        Some(NativeWorkspaceTextEncoding::EucKr)
    } else if std::ptr::eq(codec, WINDOWS_1252) {
        Some(NativeWorkspaceTextEncoding::Windows1252)
    } else {
        None
    }
}

fn strip_prefix<'a>(bytes: &'a [u8], prefix: &[u8]) -> &'a [u8] {
    bytes.strip_prefix(prefix).unwrap_or(bytes)
}

fn expand_tabs(text: &str, tab_size: i32) -> String {
    let effective_tab_size = tab_size.clamp(1, 8) as usize;
    let mut expanded = String::with_capacity(text.len());
    let mut column = 0usize;
    for character in text.chars() {
        if character == '\t' {
            let spaces = effective_tab_size - (column % effective_tab_size);
            expanded.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            expanded.push(character);
            if character == '\n' || character == '\r' {
                column = 0;
            } else {
                column += 1;
            }
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_escape() {
        assert!(relative_components("../outside.txt").is_err());
    }

    #[test]
    fn expands_tabs_with_workspace_editor_semantics() {
        assert_eq!(expand_tabs("a\tb\n\tc", 4), "a   b\n    c");
    }

    #[test]
    fn detects_utf8_bom() {
        assert_eq!(
            detect_encoding(b"\xef\xbb\xbfhello").unwrap(),
            NativeWorkspaceTextEncoding::Utf8Bom
        );
    }
}
