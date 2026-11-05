use super::*;

#[test]
fn worker_is_enabled_by_default_with_explicit_rollback() {
    assert!(worker_mode_enabled(None));
    assert!(worker_mode_enabled(Some("1")));
    assert!(worker_mode_enabled(Some(" ON ")));
    assert!(worker_mode_enabled(Some("true")));
    assert!(!worker_mode_enabled(Some("0")));
    assert!(!worker_mode_enabled(Some("off")));
    assert!(!worker_mode_enabled(Some("false")));
    assert!(!worker_mode_enabled(Some("invalid")));
}

#[test]
fn worker_frame_round_trips_binary_and_rejects_oversized_length() {
    let mut wire = Vec::new();
    write_frame(&mut wire, b'W', &[0, 255, b'\n', 0]).unwrap();
    assert_eq!(
        read_frame(&mut wire.as_slice()).unwrap(),
        Some((b'W', vec![0, 255, b'\n', 0]))
    );
    let mut oversize = vec![b'W'];
    oversize.extend_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
    assert_eq!(
        read_frame(&mut oversize.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut too_large_for_auth = vec![b'A'];
    too_large_for_auth.extend_from_slice(&(4097_u32).to_le_bytes());
    assert_eq!(
        read_frame_bounded(&mut too_large_for_auth.as_slice(), 4096)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}
