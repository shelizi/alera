//! Bounded, read-only comparison of native Agent transcripts.
//! Never retain a cross-pass metadata cache: account files may be edited,
//! replaced or deleted by providers while Alera is running.

use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::Path;

const COMPARE_BLOCK: usize = 64 * 1024;

pub(super) struct PrefixInspection {
    /// A verified complete source version, if requested and still stable.
    /// A tail/readback failure must not discard the already verified prefix.
    pub(super) snapshot: io::Result<Option<Vec<u8>>>,
}

fn same_metadata(before: &Metadata, after: &Metadata) -> bool {
    before.file_type().is_file()
        && after.file_type().is_file()
        && before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
}

/// Only read the shorter file's extent from both streams. Early differences
/// stop at one block; a non-newline-terminated shorter file is not a complete
/// mirrored prefix. Capture validated source bytes only for quiet archives.
fn compare_prefix(
    shorter: &mut impl Read,
    longer: &mut impl Read,
    mut remaining: u64,
    mut capture: Option<&mut Vec<u8>>,
) -> io::Result<bool> {
    if remaining == 0 {
        return Ok(false);
    }
    let mut short = [0u8; COMPARE_BLOCK];
    let mut long = [0u8; COMPARE_BLOCK];
    let mut final_byte = 0;
    while remaining > 0 {
        let n = remaining.min(COMPARE_BLOCK as u64) as usize;
        shorter.read_exact(&mut short[..n])?;
        longer.read_exact(&mut long[..n])?;
        if short[..n] != long[..n] {
            return Ok(false);
        }
        final_byte = long[n - 1];
        if let Some(bytes) = capture.as_mut() {
            bytes.extend_from_slice(&long[..n]);
        }
        remaining -= n as u64;
    }
    Ok(final_byte == b'\n')
}

/// Inspect an unequal-length pair using the caller's already-read metadata.
/// For an archive, capture the longer file exactly once: verified prefix plus
/// remaining tail from the same handle. Bound reads even if the file grows,
/// and revalidate both native paths before returning any archive bytes.
pub(super) fn inspect_prefix(
    shorter: &Path,
    short_meta: &Metadata,
    longer: &Path,
    long_meta: &Metadata,
    max_bytes: u64,
    capture: bool,
) -> io::Result<Option<PrefixInspection>> {
    if !short_meta.file_type().is_file()
        || !long_meta.file_type().is_file()
        || short_meta.len() == 0
        || short_meta.len() >= long_meta.len()
        || long_meta.len() > max_bytes
    {
        return Ok(None);
    }

    let mut short_file = File::open(shorter)?;
    let mut long_file = File::open(longer)?;
    if !same_metadata(short_meta, &short_file.metadata()?)
        || !same_metadata(long_meta, &long_file.metadata()?)
    {
        return Ok(None);
    }

    let mut bytes = Vec::new();
    if !compare_prefix(
        &mut short_file,
        &mut long_file,
        short_meta.len(),
        capture.then_some(&mut bytes),
    )? {
        return Ok(None);
    }

    // Without a requested archive, never read the unneeded longer suffix.
    if !capture {
        return Ok(Some(PrefixInspection { snapshot: Ok(None) }));
    }

    let snapshot = (|| -> io::Result<Option<Vec<u8>>> {
        let tail = long_meta.len() - short_meta.len();
        // One extra byte detects growth, without allocating an unbounded
        // buffer if a running Agent appends while this pass is in progress.
        (&mut long_file).take(tail + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != long_meta.len()
            || bytes.last() != Some(&b'\n')
            || !same_metadata(long_meta, &long_file.metadata()?)
            || !same_metadata(short_meta, &short_file.metadata()?)
            || !same_metadata(long_meta, &fs::symlink_metadata(longer)?)
            || !same_metadata(short_meta, &fs::symlink_metadata(shorter)?)
        {
            return Ok(None);
        }
        Ok(Some(bytes))
    })();

    Ok(Some(PrefixInspection { snapshot }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fixture() -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("alera-sync-io-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn real_files_capture_exact_version_and_refuse_changed_metadata() {
        let root = fixture();
        let short = root.join("short.jsonl");
        let long = root.join("long.jsonl");
        let prefix = b"{\"turn\":1}\n";
        let full = b"{\"turn\":1}\n{\"turn\":2}\n";
        fs::write(&short, prefix).unwrap();
        fs::write(&long, full).unwrap();
        let short_meta = fs::symlink_metadata(&short).unwrap();
        let long_meta = fs::symlink_metadata(&long).unwrap();
        let inspect = |capture, max_bytes| {
            inspect_prefix(&short, &short_meta, &long, &long_meta, max_bytes, capture).unwrap()
        };
        assert!(inspect(false, 1024).unwrap().snapshot.unwrap().is_none());
        assert_eq!(
            inspect(true, 1024).unwrap().snapshot.unwrap().unwrap(),
            full
        );
        assert!(inspect(true, 10).is_none());
        fs::write(&long, b"{\"turn\":1}\n{\"turn\":3}\n{\"turn\":4}\n").unwrap();
        assert!(inspect(true, 1024).is_none());
        assert_eq!(fs::read(&short).unwrap(), prefix);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_longer_file_has_a_matching_prefix_but_no_archive() {
        let root = fixture();
        let short = root.join("short.jsonl");
        let long = root.join("long.jsonl");
        fs::write(&short, b"{}\n").unwrap();
        fs::write(&long, b"{}\n{").unwrap();
        let result = inspect_prefix(
            &short,
            &fs::symlink_metadata(&short).unwrap(),
            &long,
            &fs::symlink_metadata(&long).unwrap(),
            1024,
            true,
        )
        .unwrap()
        .unwrap();
        assert!(result.snapshot.unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compares_only_shorter_extent_and_captures_exact_prefix() {
        let short = b"complete\n";
        let mut long = short.to_vec();
        long.resize(16 * 1024 * 1024, b'x');
        let mut a = Cursor::new(short);
        let mut b = Cursor::new(long);
        let mut captured = Vec::new();
        assert!(compare_prefix(&mut a, &mut b, short.len() as u64, Some(&mut captured)).unwrap());
        assert_eq!(a.position(), short.len() as u64);
        assert_eq!(b.position(), short.len() as u64);
        assert_eq!(captured, short);
    }

    #[test]
    fn mismatch_stops_at_first_block_without_reading_tail() {
        let mut a = Cursor::new(vec![b'a'; 4 * COMPARE_BLOCK]);
        let mut b = Cursor::new(vec![b'b'; 8 * COMPARE_BLOCK]);
        let mut captured = Vec::new();
        assert!(!compare_prefix(
            &mut a,
            &mut b,
            (4 * COMPARE_BLOCK) as u64,
            Some(&mut captured)
        )
        .unwrap());
        assert_eq!(a.position(), COMPARE_BLOCK as u64);
        assert_eq!(b.position(), COMPARE_BLOCK as u64);
        assert!(captured.is_empty());
    }

    #[test]
    fn checks_complete_line_at_exact_block_boundary_and_rejects_short_read() {
        let mut line = vec![b'x'; COMPARE_BLOCK * 2];
        *line.last_mut().unwrap() = b'\n';
        let mut a = Cursor::new(&line);
        let mut b = Cursor::new(&line);
        assert!(compare_prefix(&mut a, &mut b, line.len() as u64, None).unwrap());
        let truncated = &line[..line.len() - 1];
        let mut a = Cursor::new(truncated);
        let mut b = Cursor::new(&line);
        assert!(!compare_prefix(&mut a, &mut b, truncated.len() as u64, None).unwrap());
        let mut a = Cursor::new(truncated);
        let mut b = Cursor::new(&line);
        assert_eq!(
            compare_prefix(&mut a, &mut b, line.len() as u64, None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
