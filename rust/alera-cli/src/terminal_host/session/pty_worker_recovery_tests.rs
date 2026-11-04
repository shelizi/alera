use super::*;

#[test]
fn expired_worker_identity_is_replaceable_but_a_live_worker_is_not() {
    let this = seal_shell_process(std::process::id()).expect("test process is alive");
    let mut record = WorkerRecord {
        schema_version: SCHEMA_VERSION,
        session_id: "s".into(),
        workspace_id: "w".into(),
        tab_id: "t".into(),
        working_directory: String::new(),
        worker_pid: this.pid,
        worker_start_time: this.start_time,
        shell_pid: this.pid,
        port: 1234,
        token: uuid::Uuid::new_v4().to_string(),
    };
    assert!(
        !wait_for_worker_shutdown_for(&record, Duration::from_millis(1)),
        "the still-living sealed process must not be replaced"
    );
    record.worker_start_time += 1;
    assert!(
        wait_for_worker_shutdown_for(&record, Duration::from_millis(1)),
        "a recycled/dead identity may be replaced"
    );
}

#[test]
fn initial_worker_attachment_requires_authenticated_host_confirmation() {
    let attached = Arc::new(AtomicBool::new(false));
    let record = WorkerRecord {
        schema_version: SCHEMA_VERSION,
        session_id: "s".into(),
        workspace_id: "w".into(),
        tab_id: "t".into(),
        working_directory: String::new(),
        worker_pid: std::process::id(),
        worker_start_time: 1,
        shell_pid: 1234,
        port: 1234,
        token: uuid::Uuid::new_v4().to_string(),
    };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let attached_for_server = Arc::clone(&attached);
    let record_for_server = record.clone();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        serve_client(
            stream,
            record_for_server,
            Arc::new(Mutex::new(OutputTail {
                end: 0,
                limit: 1024,
                tail: VecDeque::new(),
                socket: None,
                generation: 0,
            })),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            attached_for_server,
        );
    });
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write_frame(
        &mut client,
        b'A',
        &serde_json::to_vec(&AuthRequest {
            session_id: record.session_id,
            token: record.token,
            resume_cursor: 0,
        })
        .unwrap(),
    )
    .unwrap();
    let mut response = [0; 13];
    client.read_exact(&mut response).unwrap();
    assert_eq!(response[0], b'R');
    assert!(
        !attached.load(Ordering::Acquire),
        "replaying the authentication greeting alone must not release the worker"
    );
    write_frame(&mut client, b'H', &[]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !attached.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        attached.load(Ordering::Acquire),
        "the authenticated Host confirmation must release worker startup"
    );
    client.shutdown(Shutdown::Both).unwrap();
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn unix_reconnect_manifest_requires_private_owner_and_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let registry = dir.path().join("workers");
    prepare_private_runtime_directory(&registry).unwrap();
    verify_unix_registry_path(&registry, true).unwrap();

    let manifest = registry.join("session.json");
    create_private_runtime_file(&manifest).unwrap();
    verify_unix_registry_path(&manifest, false).unwrap();

    std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        verify_unix_registry_path(&manifest, false)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );

    std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = registry.join("malicious.json");
    symlink(&manifest, &link).unwrap();
    assert_eq!(
        verify_unix_registry_path(&link, false).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn reconnect_client_threads_are_bounded_and_slots_released() {
    let slots = Arc::new(ClientSlots::default());
    let permits: Vec<ClientPermit> = (0..MAX_CLIENT_CONNECTIONS)
        .map(|_| slots.try_acquire().expect("available slot"))
        .collect();
    assert!(
        slots.try_acquire().is_none(),
        "extra client must not spawn a thread"
    );
    drop(permits);
    assert_eq!(slots.0.load(Ordering::Acquire), 0);
    assert!(
        slots.try_acquire().is_some(),
        "disconnected clients release slots"
    );
}

#[test]
fn reconnect_output_tail_is_bounded_and_replays_at_absolute_cursor() {
    let mut buffer = OutputTail {
        end: 100,
        limit: 5,
        tail: VecDeque::new(),
        socket: None,
        generation: 0,
    };
    buffer.push(b"abcdef");
    assert_eq!(buffer.end, 106);
    assert_eq!(buffer.replay_from(103), (103, b"def".to_vec()));
    assert_eq!(buffer.replay_from(100), (101, b"bcdef".to_vec()));
    buffer.push(b"XY");
    assert_eq!(buffer.replay_from(106), (106, b"XY".to_vec()));
    assert_eq!(buffer.tail.len(), 5);
}

#[test]
fn manifest_paths_do_not_interpret_session_id_as_paths() {
    let root = Path::new("C:/alera");
    let path = manifest_path(root, "../C:../\\escape");
    assert_eq!(path.parent().unwrap(), root.join("pty-workers"));
    assert_eq!(path.extension().unwrap(), "json");
}
