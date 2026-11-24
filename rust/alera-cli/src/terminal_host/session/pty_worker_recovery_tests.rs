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

#[test]
fn recovery_server_reconnects_replays_and_removes_idle_listener() {
    let sandbox = tempfile::tempdir().unwrap();
    let worker = WorkerRecoveryServer::start(
        sandbox.path(),
        "session",
        "workspace",
        "tab",
        sandbox.path().to_str().unwrap(),
        std::process::id(),
        100,
        64 * 1024,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
    )
    .unwrap();
    let manifest = manifest_path(sandbox.path(), "session");
    let record = read_record(&manifest).unwrap().unwrap();
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), record.port);
    worker.output().forward(b"initial");

    let attach = |cursor: u64| {
        let mut client = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write_frame(
            &mut client,
            b'A',
            &serde_json::to_vec(&AuthRequest {
                session_id: record.session_id.clone(),
                token: record.token.clone(),
                resume_cursor: cursor,
            })
            .unwrap(),
        )
        .unwrap();
        let mut response = [0; 13];
        client.read_exact(&mut response).unwrap();
        assert_eq!(response[0], b'R');
        assert_eq!(
            u64::from_le_bytes(response[5..].try_into().unwrap()),
            cursor
        );
        client
    };

    let mut first = attach(100);
    let mut replay = [0; 7];
    first.read_exact(&mut replay).unwrap();
    assert_eq!(&replay, b"initial");
    write_frame(&mut first, b'H', &[]).unwrap();
    worker.wait_for_initial_host_attachment();
    assert!(worker.initial_host_attached.load(Ordering::Acquire));

    first.shutdown(Shutdown::Both).unwrap();
    worker.output().forward(b"missed");
    let mut second = attach(107);
    let mut replay = [0; 6];
    second.read_exact(&mut replay).unwrap();
    assert_eq!(&replay, b"missed");
    second.shutdown(Shutdown::Both).unwrap();
    drop(second);
    drop(first);

    drop(worker);
    assert!(read_record(&manifest).unwrap().is_none());
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_err());
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

#[test]
fn worker_registry_temp_file_keeps_atomic_rename_in_the_same_directory_without_max_path_bloat() {
    // The original registry file stays recognizable by its hashed session ID,
    // but its temporary path must not append another full UUID to that hash.
    let runtime = Path::new("C:/").join("nested".repeat(25));
    let registry = manifest_path(&runtime, "original-native-session");
    let old_temp = registry.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let new_temp = temporary_manifest_path(&registry);
    let second_temp = temporary_manifest_path(&registry);

    assert!(old_temp.to_string_lossy().len() >= 260);
    assert!(new_temp.to_string_lossy().len() < 260);
    assert!(new_temp.to_string_lossy().len() + 30 < old_temp.to_string_lossy().len());
    assert_eq!(
        new_temp.parent(),
        registry.parent(),
        "atomic rename must remain on one volume"
    );
    assert_eq!(new_temp.extension().unwrap(), "tmp");
    assert_ne!(
        new_temp, second_temp,
        "independent retries must never share a temporary file"
    );
}
#[cfg(windows)]
#[test]
fn deep_windows_worker_registry_temp_can_be_privately_published() {
    let sandbox = tempfile::tempdir().unwrap();
    // One valid directory component; the final manifest fits normal Windows
    // paths while the old <hash>.<uuid>.tmp would cross MAX_PATH.
    let runtime = sandbox.path().join("deep".repeat(31));
    let registry = manifest_path(&runtime, "isolated-worker-session");
    let old_temp = registry.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    assert!(old_temp.to_string_lossy().len() >= 260);

    let directory = registry.parent().unwrap();
    prepare_private_runtime_directory(directory).unwrap();
    acl::protect_path(directory, true).unwrap();

    let temporary = temporary_manifest_path(&registry);
    let mut file = create_private_runtime_file(&temporary).unwrap();
    file.write_all(b"synthetic worker registry").unwrap();
    file.sync_all().unwrap();
    drop(file);
    acl::protect_path(&temporary, false).unwrap();
    std::fs::rename(&temporary, &registry).unwrap();
    acl::protect_path(&registry, false).unwrap();
    assert_eq!(
        std::fs::read(registry).unwrap(),
        b"synthetic worker registry"
    );
}
