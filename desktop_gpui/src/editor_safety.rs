//! Headless P0 editor safety cases. These call the library the GPUI window uses.
use crate::editor::{EditorDocument, EditorDraftStore, EditorLineIndex, MAX_EDITOR_BYTES};
use crate::editor_session::{
    CloseEffect, EditorSession, OpenEffect, DIRTY_OPEN_REFUSED, DIRTY_SWITCH_REFUSED,
    RECOVERY_PLAINTEXT_NOTICE,
};
use crate::model::WorkbenchModel;
use crate::runtime_bridge::{terminal_cli_arguments, RuntimeCliBridge};
use crate::terminal::{
    list_terminal_verb, read_terminal_verb, TerminalDock, TerminalSession, HOST_SESSION_LIST_SCOPE,
};
use std::fs;
use std::path::{Path, PathBuf};
use unicode_segmentation::UnicodeSegmentation;

fn session(_root: &Path) -> (tempfile::TempDir, EditorSession) {
    let state = tempfile::tempdir().unwrap();
    let editor = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    (state, editor)
}

fn write_file(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

fn grapheme_boundary(text: &str, offset: usize) -> bool {
    offset == 0
        || offset == text.len()
        || text
            .grapheme_indices(true)
            .any(|(index, _)| index == offset)
}

#[test]
fn load_handoff_fixtures_index_line_1_601_and_last_without_splitting_text() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (state, mut editor) = session(root);

    let empty = write_file(root, "empty.txt", b"");
    editor.open_file(root, &empty).unwrap();
    let empty_text = editor.document().unwrap().content();
    let empty_lines = EditorLineIndex::new(empty_text);
    assert_eq!(empty_lines.line_count(), 1);
    assert_eq!(empty_lines.line_range(empty_text, 0), Some(0..0));
    assert_eq!(empty_lines.line_for_offset(empty_text, 0), Some(0));

    let mixed = "中文👩‍💻e\u{301}\n\n第二行\n";
    let mixed_path = write_file(root, "mixed.txt", mixed.as_bytes());
    editor.open_file(root, &mixed_path).unwrap();
    let mixed_doc = editor.document().unwrap().content();
    let mixed_lines = EditorLineIndex::new(mixed_doc);
    assert_eq!(mixed_lines.line_count(), 4);
    for line in 0..mixed_lines.line_count() {
        let range = mixed_lines.line_range(mixed_doc, line).unwrap();
        assert!(mixed_doc.is_char_boundary(range.start) && mixed_doc.is_char_boundary(range.end));
        assert!(
            grapheme_boundary(mixed_doc, range.start) && grapheme_boundary(mixed_doc, range.end)
        );
    }
    assert_eq!(
        &mixed_doc[mixed_lines.line_range(mixed_doc, 0).unwrap()],
        "中文👩‍💻e\u{301}"
    );
    assert_eq!(
        &mixed_doc[mixed_lines.line_range(mixed_doc, 1).unwrap()],
        ""
    );
    assert_eq!(
        mixed_lines.line_range(mixed_doc, 3),
        Some(mixed_doc.len()..mixed_doc.len())
    );

    let crlf = b"alpha\r\nbeta\r\n\r\n";
    let crlf_path = write_file(root, "crlf.txt", crlf);
    editor.open_file(root, &crlf_path).unwrap();
    assert_eq!(editor.document().unwrap().content().as_bytes(), crlf);
    let crlf_text = editor.document().unwrap().content();
    let crlf_lines = EditorLineIndex::new(crlf_text);
    assert_eq!(crlf_lines.line_count(), 4);
    assert_eq!(
        &crlf_text[crlf_lines.line_range(crlf_text, 0).unwrap()],
        "alpha\r"
    );
    assert_eq!(
        &crlf_text[crlf_lines.line_range(crlf_text, 1).unwrap()],
        "beta\r"
    );
    // A blank CRLF line still keeps its carriage return. The final newline
    // is an extra empty line, and neither boundary splits a code point.
    assert_eq!(
        &crlf_text[crlf_lines.line_range(crlf_text, 2).unwrap()],
        "\r"
    );
    assert_eq!(
        crlf_lines.line_range(crlf_text, 3),
        Some(crlf_text.len()..crlf_text.len())
    );
    assert_eq!(
        crlf_lines.line_for_offset(crlf_text, crlf_text.len()),
        Some(3)
    );

    let mut rows_601 = Vec::new();
    for index in 1..=601 {
        rows_601.push(format!("L{index:04} 中"));
    }
    let text_601 = rows_601.join("\n");
    assert!(text_601.len() < MAX_EDITOR_BYTES);
    let path_601 = write_file(root, "601.txt", text_601.as_bytes());
    editor.open_file(root, &path_601).unwrap();
    let loaded_601 = editor.document().unwrap().content();
    let index_601 = EditorLineIndex::new(loaded_601);
    assert_eq!(index_601.line_count(), 601);
    assert_eq!(
        &loaded_601[index_601.line_range(loaded_601, 0).unwrap()],
        rows_601[0]
    );
    assert_eq!(
        &loaded_601[index_601.line_range(loaded_601, 600).unwrap()],
        rows_601[600]
    );
    assert_eq!(
        index_601.line_for_offset(loaded_601, loaded_601.len()),
        Some(600)
    );
    assert!(loaded_601.is_char_boundary(index_601.line_range(loaded_601, 600).unwrap().start));

    let mut rows_5000 = Vec::new();
    for index in 1..=5000 {
        rows_5000.push(format!("{index:04} 中文"));
    }
    let text_5000 = rows_5000.join("\n");
    assert!(
        text_5000.len() < MAX_EDITOR_BYTES,
        "fixture must stay inside the editor cap"
    );
    let path_5000 = write_file(root, "5000.txt", text_5000.as_bytes());
    editor.open_file(root, &path_5000).unwrap();
    let loaded = editor.document().unwrap().content();
    let lines = EditorLineIndex::new(loaded);
    assert_eq!(lines.line_count(), 5000);
    for index in [0usize, 600, 4999] {
        let range = lines.line_range(loaded, index).unwrap();
        assert_eq!(&loaded[range.clone()], rows_5000[index]);
        assert!(loaded.is_char_boundary(range.start) && loaded.is_char_boundary(range.end));
        assert!(grapheme_boundary(loaded, range.start) && grapheme_boundary(loaded, range.end));
        assert_eq!(lines.line_for_offset(loaded, range.start), Some(index));
    }
    assert_eq!(lines.line_range(loaded, 5000), None);
    drop(state);
}

#[test]
fn oversize_nul_and_non_utf8_are_rejected_without_rewriting_bytes() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (_state, mut editor) = session(root);

    let oversize = vec![b'a'; MAX_EDITOR_BYTES + 1];
    let oversize_path = write_file(root, "big.bin", &oversize);
    assert!(editor.open_file(root, &oversize_path).is_err());
    assert_eq!(fs::read(&oversize_path).unwrap(), oversize);
    assert!(editor.document().is_none());

    let nul = b"hello\0world".to_vec();
    let nul_path = write_file(root, "nul.txt", &nul);
    assert!(editor.open_file(root, &nul_path).is_err());
    assert_eq!(fs::read(&nul_path).unwrap(), nul);

    let binary = vec![0xFFu8, 0xFE, b'A'];
    let binary_path = write_file(root, "binary.txt", &binary);
    assert!(editor.open_file(root, &binary_path).is_err());
    assert_eq!(fs::read(&binary_path).unwrap(), binary);
    assert!(editor.target_path().is_none());
}

#[test]
fn copied_bytes_stay_on_grapheme_boundaries_for_reverse_and_cross_line_selections() {
    let workspace = tempfile::tempdir().unwrap();
    let mut document = EditorDocument::scratch(workspace.path()).unwrap();
    document.replace(None, "ab\n中文👩‍💻\ne\u{301}z").unwrap();
    let end = "ab\n中文👩‍💻".len();
    document.set_pointer_caret(end).unwrap();
    document.extend_pointer_selection(1).unwrap();
    assert!(document.selection_reversed());
    let copied = document.copied_text();
    assert_eq!(copied, "b\n中文👩‍💻");
    assert!(grapheme_boundary(
        document.content(),
        document.selection().start
    ));
    assert!(grapheme_boundary(
        document.content(),
        document.selection().end
    ));
    assert!(copied.contains("👩‍💻"));

    document.move_document_end(false);
    document.move_document_start(true);
    assert!(document.selection_reversed());
    assert_eq!(document.copied_text(), document.content());
    assert!(document.copied_text().contains("e\u{301}"));
}

#[test]
fn ime_cancel_leaves_undo_unchanged_and_restart_omits_preedit() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let path = write_file(root, "ime.txt", b"base");
    let (state, mut editor) = session(root);
    assert!(matches!(
        editor.open_file(root, &path).unwrap(),
        OpenEffect::Opened { recovered: false }
    ));
    let before = editor.document().unwrap().history_depth();
    editor
        .document_mut()
        .unwrap()
        .replace_and_mark(None, "ㄓ", Some(1..1))
        .unwrap();
    editor
        .document_mut()
        .unwrap()
        .replace_and_mark(None, "中", Some(1..1))
        .unwrap();
    editor.persist_draft().unwrap();
    let mut restarted = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    restarted.open_file(root, &path).unwrap();
    assert_eq!(restarted.document().unwrap().content(), "base");
    assert!(restarted.document().unwrap().marked().is_none());
    assert!(!restarted.document().unwrap().is_dirty());

    editor.document_mut().unwrap().replace(None, "").unwrap();
    assert_eq!(editor.document().unwrap().content(), "base");
    assert_eq!(editor.document().unwrap().history_depth(), before);

    editor
        .document_mut()
        .unwrap()
        .replace_and_mark(None, "ㄅ", Some(1..1))
        .unwrap();
    editor
        .document_mut()
        .unwrap()
        .replace_and_mark(None, "不", Some(1..1))
        .unwrap();
    editor.document_mut().unwrap().unmark();
    assert_eq!(editor.document().unwrap().history_depth().0, before.0 + 1);
    editor.persist_draft().unwrap();
    let mut committed = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    let effect = committed.open_file(root, &path).unwrap();
    assert!(matches!(effect, OpenEffect::Opened { recovered: true }));
    assert_eq!(committed.document().unwrap().content(), "base不");
    assert!(committed.document().unwrap().marked().is_none());
    assert!(effect.status().contains(RECOVERY_PLAINTEXT_NOTICE));
    assert!(effect.status().contains("plaintext"));
    assert!(effect.status().contains("not secure storage"));
    assert!(!effect.status().to_lowercase().contains("encrypted"));
}

#[test]
fn save_refuses_external_readonly_missing_renamed_and_escaped_paths() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (state, mut editor) = session(root);
    let path = write_file(root, "note.txt", b"before\r\n");
    editor.open_file(root, &path).unwrap();
    editor.document_mut().unwrap().move_document_start(false);
    editor.document_mut().unwrap().replace(None, "X").unwrap();
    editor.persist_draft().unwrap();

    fs::write(&path, b"external\r\n").unwrap();
    let blocked = editor.save();
    assert!(blocked.blocked());
    assert!(blocked.status().starts_with("Save blocked:"));
    assert!(blocked.status().contains(RECOVERY_PLAINTEXT_NOTICE));
    assert_eq!(fs::read(&path).unwrap(), b"external\r\n");
    assert_eq!(editor.document().unwrap().content(), "Xbefore\r\n");
    let mut restored = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    restored.open_file(root, &path).unwrap();
    assert_eq!(restored.document().unwrap().content(), "Xbefore\r\n");

    fs::write(&path, b"before\r\n").unwrap();
    let mut readonly = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    readonly.open_file(root, &path).unwrap();
    readonly.document_mut().unwrap().replace(None, "!").unwrap();
    readonly.persist_draft().unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let blocked = readonly.save();
    assert!(blocked.blocked());
    assert_eq!(fs::read(&path).unwrap(), b"before\r\n");
    assert!(readonly.document().unwrap().content().ends_with('!'));
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();

    let mut missing = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    missing.open_file(root, &path).unwrap();
    missing.document_mut().unwrap().replace(None, "?").unwrap();
    missing.persist_draft().unwrap();
    fs::remove_file(&path).unwrap();
    assert!(missing.save().blocked());
    assert!(!path.exists());
    assert!(missing.document().unwrap().content().ends_with('?'));

    fs::write(&path, b"before\r\n").unwrap();
    let mut renamed = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    renamed.open_file(root, &path).unwrap();
    renamed.document_mut().unwrap().replace(None, "Q").unwrap();
    let moved = root.join("renamed.txt");
    fs::rename(&path, &moved).unwrap();
    assert!(renamed.save().blocked());
    assert_eq!(fs::read(&moved).unwrap(), b"before\r\n");
    assert!(!path.exists());

    fs::write(&path, b"before\r\n").unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, b"do-not-touch").unwrap();
    let mut escaped = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    escaped.open_file(root, &path).unwrap();
    escaped.document_mut().unwrap().replace(None, "Z").unwrap();
    fs::remove_file(&path).unwrap();
    symlink_file(&secret, &path).expect("symlink creation");
    assert!(escaped.save().blocked());
    assert_eq!(fs::read(&secret).unwrap(), b"do-not-touch");
    assert_eq!(
        escaped.document().unwrap().content().chars().last(),
        Some('Z')
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn failed_replace_keeps_original_bytes_buffer_and_draft() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (state, mut editor) = session(root);
    let path = write_file(root, "replace.txt", b"original\r\n");
    editor.open_file(root, &path).unwrap();
    editor
        .document_mut()
        .unwrap()
        .replace(None, " edited")
        .unwrap();
    editor.persist_draft().unwrap();
    let lock = lock_destination(&path);
    let blocked = editor.save();
    drop(lock);
    assert!(
        blocked.blocked(),
        "replace must fail while the destination is locked"
    );
    assert!(blocked.status().contains("Save blocked:"));
    assert_eq!(fs::read(&path).unwrap(), b"original\r\n");
    assert!(editor.document().unwrap().content().ends_with(" edited"));
    assert!(editor.document().unwrap().is_dirty());
    let leftovers: Vec<_> = fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".alera-gpui-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "failed replace must remove its temp file"
    );
    let mut restarted = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    restarted.open_file(root, &path).unwrap();
    assert!(restarted.document().unwrap().content().ends_with(" edited"));
    assert_eq!(fs::read(&path).unwrap(), b"original\r\n");
}

#[test]
fn crlf_and_final_newline_are_not_rewritten_on_save() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (_state, mut editor) = session(root);
    let path = write_file(root, "endings.txt", b"alpha\r\nbeta\r\n");
    editor.open_file(root, &path).unwrap();
    editor.document_mut().unwrap().move_document_start(false);
    editor.document_mut().unwrap().replace(None, "X").unwrap();
    assert!(editor.save().wrote_file());
    assert_eq!(fs::read(&path).unwrap(), b"Xalpha\r\nbeta\r\n");
    assert!(!editor.document().unwrap().is_dirty());
}

#[test]
fn save_and_discard_clear_draft_but_failed_save_and_cancel_do_not() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let state_dir = tempfile::tempdir().unwrap();
    let store = EditorDraftStore::new(state_dir.path());
    let path = write_file(root, "draft.txt", b"disk");
    let mut editor = EditorSession::new(Some(store.clone()));
    editor.open_file(root, &path).unwrap();
    editor
        .document_mut()
        .unwrap()
        .replace(None, "\nlocal")
        .unwrap();
    editor.persist_draft().unwrap();
    fs::write(&path, b"other").unwrap();
    assert!(editor.save().blocked());
    assert_eq!(fs::read(&path).unwrap(), b"other");
    let mut kept = EditorSession::new(Some(store.clone()));
    assert!(matches!(
        kept.open_file(root, &path).unwrap(),
        OpenEffect::Opened { recovered: true }
    ));
    assert_eq!(kept.document().unwrap().content(), "disk\nlocal");

    fs::write(&path, b"disk").unwrap();
    let mut saving = EditorSession::new(Some(store.clone()));
    assert!(matches!(
        saving.open_file(root, &path).unwrap(),
        OpenEffect::Opened { recovered: true }
    ));
    assert!(saving.save().wrote_file());
    assert_eq!(fs::read(&path).unwrap(), b"disk\nlocal");
    let mut cleared = EditorSession::new(Some(store.clone()));
    assert!(matches!(
        cleared.open_file(root, &path).unwrap(),
        OpenEffect::Opened { recovered: false }
    ));
    assert_eq!(cleared.document().unwrap().content(), "disk\nlocal");

    let canonical = cleared.target_path().unwrap().to_path_buf();
    cleared
        .document_mut()
        .unwrap()
        .replace(None, "\nagain")
        .unwrap();
    cleared.persist_draft().unwrap();
    assert!(matches!(cleared.close(true), CloseEffect::Armed));
    assert_eq!(fs::read(&path).unwrap(), b"disk\nlocal");
    cleared.cancel();
    assert!(!cleared.discard_armed());
    assert_eq!(cleared.target_path(), Some(canonical.as_path()));
    assert!(matches!(cleared.close(false), CloseEffect::RefusedUnsaved));
    assert_eq!(fs::read(&path).unwrap(), b"disk\nlocal");
    assert!(matches!(cleared.close(true), CloseEffect::Armed));
    assert!(cleared.close(true).closed());
    assert_eq!(fs::read(&path).unwrap(), b"disk\nlocal");
    let mut discarded = EditorSession::new(Some(store));
    assert!(matches!(
        discarded.open_file(root, &path).unwrap(),
        OpenEffect::Opened { recovered: false }
    ));
    assert_eq!(discarded.document().unwrap().content(), "disk\nlocal");
}

#[test]
fn two_workspace_roots_do_not_restore_each_others_draft() {
    let state = tempfile::tempdir().unwrap();
    let store = EditorDraftStore::new(state.path());
    let workspace_a = tempfile::tempdir().unwrap();
    let workspace_b = tempfile::tempdir().unwrap();
    let path_a = write_file(workspace_a.path(), "src/same.txt", b"alpha");
    let path_b = write_file(workspace_b.path(), "src/same.txt", b"beta");
    let mut first = EditorSession::new(Some(store.clone()));
    first.open_file(workspace_a.path(), &path_a).unwrap();
    first
        .document_mut()
        .unwrap()
        .replace(None, "\nfrom-a")
        .unwrap();
    first.persist_draft().unwrap();

    let mut second = EditorSession::new(Some(store.clone()));
    let effect = second.open_file(workspace_b.path(), &path_b).unwrap();
    assert!(matches!(effect, OpenEffect::Opened { recovered: false }));
    assert_eq!(second.document().unwrap().content(), "beta");
    assert_eq!(
        second.workspace_root().unwrap(),
        dunce::canonicalize(workspace_b.path()).unwrap()
    );
    assert_ne!(first.workspace_root(), second.workspace_root());

    let mut restarted = EditorSession::new(Some(store));
    restarted.open_file(workspace_a.path(), &path_a).unwrap();
    assert_eq!(restarted.document().unwrap().content(), "alpha\nfrom-a");
    assert_eq!(fs::read(&path_b).unwrap(), b"beta");
}

#[test]
fn dirty_open_cancel_discard_and_workspace_switch_keep_the_save_target() {
    let workspace = tempfile::tempdir().unwrap();
    let other_workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let (state, mut editor) = session(root);
    let first = write_file(root, "first.txt", b"one");
    let second = write_file(root, "second.txt", b"two");
    let foreign = write_file(other_workspace.path(), "first.txt", b"foreign");
    editor.open_file(root, &first).unwrap();
    let canonical_first = editor.target_path().unwrap().to_path_buf();
    editor
        .document_mut()
        .unwrap()
        .replace(None, "\nedit")
        .unwrap();
    editor.persist_draft().unwrap();

    let refused = editor.open_file(root, &second).unwrap();
    assert_eq!(refused, OpenEffect::RefusedDirty);
    assert_eq!(refused.status(), DIRTY_OPEN_REFUSED);
    assert_eq!(editor.target_path(), Some(canonical_first.as_path()));
    assert_eq!(fs::read(&second).unwrap(), b"two");

    assert_eq!(editor.can_switch_workspace(), Err(DIRTY_SWITCH_REFUSED));
    assert_eq!(editor.target_path(), Some(canonical_first.as_path()));
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
    editor.cancel();
    assert!(!editor.discard_armed());
    assert_eq!(editor.target_path(), Some(canonical_first.as_path()));
    assert!(matches!(editor.close(false), CloseEffect::RefusedUnsaved));
    assert_eq!(fs::read(&first).unwrap(), b"one");

    assert!(editor.save().wrote_file());
    assert_eq!(fs::read(&canonical_first).unwrap(), b"one\nedit");
    assert_eq!(fs::read(&second).unwrap(), b"two");
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
    assert!(editor.can_switch_workspace().is_ok());

    editor
        .document_mut()
        .unwrap()
        .replace(None, "\nmore")
        .unwrap();
    editor.persist_draft().unwrap();
    assert!(matches!(editor.close(true), CloseEffect::Armed));
    assert_eq!(fs::read(&canonical_first).unwrap(), b"one\nedit");
    assert!(editor.close(true).closed());
    assert_eq!(fs::read(&canonical_first).unwrap(), b"one\nedit");
    let mut restarted = EditorSession::new(Some(EditorDraftStore::new(state.path())));
    assert!(matches!(
        restarted.open_file(root, &canonical_first).unwrap(),
        OpenEffect::Opened { recovered: false }
    ));
    assert_eq!(restarted.document().unwrap().content(), "one\nedit");

    let opened = restarted.open_file(root, &second).unwrap();
    assert!(matches!(opened, OpenEffect::Opened { recovered: false }));
    assert_eq!(
        restarted.target_path().unwrap(),
        dunce::canonicalize(&second).unwrap()
    );
    restarted
        .document_mut()
        .unwrap()
        .replace(None, "!")
        .unwrap();
    assert!(restarted.save().wrote_file());
    assert_eq!(fs::read(&second).unwrap(), b"two!");
    assert_eq!(fs::read(&canonical_first).unwrap(), b"one\nedit");
}

#[test]
fn read_only_preview_does_not_write_project_bytes() {
    let workspace = tempfile::tempdir().unwrap();
    let file = workspace.path().join("long.txt");
    let body = (0..700)
        .map(|index| format!("row {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&file, &body).unwrap();
    let mut model = WorkbenchModel::new(workspace.path()).unwrap();
    assert!(model.open_path(&file));
    assert!(model.content().contains("preview limited to 600"));
    assert_eq!(fs::read_to_string(&file).unwrap(), body);
    model.reload_active();
    model.activate_tab(0);
    model.refresh_directory();
    assert_eq!(fs::read_to_string(&file).unwrap(), body);
}

#[test]
fn terminal_list_and_read_do_not_write_files_or_accept_stale_replies() {
    let project = tempfile::tempdir().unwrap();
    let kept = project.path().join("keep.txt");
    fs::write(&kept, b"project-bytes").unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let bridge = RuntimeCliBridge::new(runtime.path().join("missing-alera.exe"), runtime.path());
    assert!(bridge.list_terminals().is_err());
    assert!(bridge.read_terminal("term-1", Some(4)).is_err());
    assert_eq!(fs::read(&kept).unwrap(), b"project-bytes");
    assert!(!runtime.path().join("host.json").exists());

    let list_args = terminal_cli_arguments(runtime.path(), &list_terminal_verb());
    assert!(list_args.iter().any(|arg| arg == "--no-start"));
    assert!(list_args.iter().any(|arg| arg == "list"));
    assert!(!list_args.iter().any(|arg| arg == "write" || arg == "input"));
    let read_args = terminal_cli_arguments(
        runtime.path(),
        &read_terminal_verb("term-1", Some(4)).unwrap(),
    );
    assert!(read_args.iter().any(|arg| arg == "read"));
    assert!(read_args.iter().any(|arg| arg == "--no-start"));
    assert!(!read_args
        .iter()
        .any(|arg| arg == "write" || arg == "input" || arg == "resize"));

    let mut dock = TerminalDock::for_workspace(4);
    dock.apply_session_list(
        4,
        vec![TerminalSession {
            handle: "term-1".into(),
            workspace_id: "workspace-a".into(),
            tab_id: "tab-old".into(),
            running: true,
            agent_type: Some("codex".into()),
        }],
    );
    assert!(dock.open("term-1"));
    assert!(dock.status.contains("all workspaces"));
    assert!(HOST_SESSION_LIST_SCOPE.contains("all workspaces"));
    assert!(!HOST_SESSION_LIST_SCOPE
        .to_lowercase()
        .contains("current workspace only"));
    let stale_list = dock.apply_session_list(
        3,
        vec![TerminalSession {
            handle: "term-9".into(),
            workspace_id: "workspace-b".into(),
            tab_id: "tab-other".into(),
            running: true,
            agent_type: None,
        }],
    );
    assert!(!stale_list);
    assert_eq!(dock.sessions.len(), 1);
    assert_eq!(dock.sessions[0].handle, "term-1");

    let ticket = dock.active_ticket().unwrap();
    let mut chunk = crate::terminal::TerminalChunk {
        handle: "term-1".into(),
        cursor: 0,
        next_cursor: 5,
        truncated: false,
        running: true,
        text: "stale".into(),
    };
    assert!(!dock.apply_workspace_read(3, &ticket, chunk.clone()));
    assert!(dock.active().unwrap().output.is_empty());
    chunk.text = "fresh".into();
    assert!(dock.apply_workspace_read(4, &ticket, chunk));
    assert_eq!(dock.active().unwrap().output, "fresh");

    let mut replacement = dock.sessions[0].clone();
    replacement.tab_id = "tab-new".into();
    assert!(dock.apply_session_list(4, vec![replacement]));
    assert!(dock.active_ticket().is_none());
    assert!(!dock.apply_workspace_read(
        4,
        &ticket,
        crate::terminal::TerminalChunk {
            handle: "term-1".into(),
            cursor: 5,
            next_cursor: 8,
            truncated: false,
            running: true,
            text: "nope".into(),
        }
    ));
    assert_eq!(dock.active().unwrap().output, "fresh");
}

#[cfg(windows)]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}

#[cfg(unix)]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

/// Holds a real OS restriction that makes `EditorDocument::save` fail while
/// replacing the destination. Drop releases it so the temp directory can be
/// removed.
struct DestinationLock {
    #[cfg(windows)]
    file: fs::File,
    #[cfg(unix)]
    parent: PathBuf,
    #[cfg(unix)]
    mode: u32,
}

impl Drop for DestinationLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // Closing the handle releases the sharing restriction.
            let _ = &self.file;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&self.parent)
                .map(|metadata| metadata.permissions())
                .unwrap_or_else(|_| fs::Permissions::from_mode(self.mode));
            permissions.set_mode(self.mode);
            let _ = fs::set_permissions(&self.parent, permissions);
        }
    }
}

fn lock_destination(path: &Path) -> DestinationLock {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Share read and write, but not delete, so replacing the file fails.
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002)
            .open(path)
            .unwrap();
        DestinationLock { file }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().expect("destination parent");
        let mode = fs::metadata(parent).unwrap().permissions().mode();
        let mut locked = fs::metadata(parent).unwrap().permissions();
        // `rename(2)` needs write permission on the parent. Clearing those
        // bits also stops the replacement temp file from being created.
        // Root bypasses this, and the probe below refuses a false pass.
        locked.set_mode(mode & !0o222);
        fs::set_permissions(parent, locked).unwrap();
        let lock = DestinationLock {
            parent: parent.to_path_buf(),
            mode,
        };
        let probe = parent.join(".replace-lock-probe");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
        {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(&probe);
                panic!("directory mode did not block the replacement file");
            }
            Err(error) => assert!(
                error.kind() == std::io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(13)
                    || error.raw_os_error() == Some(30),
                "unexpected lock failure: {error}"
            ),
        }
        lock
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        compile_error!("destination replace lock is implemented for Windows and Unix");
    }
}
