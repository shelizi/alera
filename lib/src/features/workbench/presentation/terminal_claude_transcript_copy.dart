import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/presentation/terminal_code_block.dart';
import 'package:path/path.dart' as p;

/// An opt-in, read-only fallback for Claude's virtualized terminal UI.
///
/// The screen buffer may contain only the viewport and an injected
/// "Jump to bottom" hint. In that case, prefer the complete matching source
/// from the *same workspace's* local Claude JSONL. A mismatched or inaccessible
/// transcript must never replace the visible terminal text.
Future<String?> recoverCompleteClaudeCodeFromTranscript({
  required String workspacePath,
  required String visibleCode,
  Directory? projectsRoot,
  bool viewportHintNearby = false,
}) async {
  // The TUI marker can appear beside the code, not in its source text.
  // The caller may pass a confirmed nearby marker; ordinary clipboard
  // clicks must not read history without evidence of virtualized output.
  if ((!viewportHintNearby && !visibleCode.contains('Jump to bottom')) ||
      visibleCode.split('\n').length < 12 ||
      workspacePath.isEmpty) {
    return null;
  }
  final home =
      Platform.environment['USERPROFILE'] ?? Platform.environment['HOME'];
  if (projectsRoot == null && (home == null || home.isEmpty)) return null;
  final root = projectsRoot ?? Directory(p.join(home!, '.claude', 'projects'));

  // Claude encodes every non-alphanumeric path character separately,
  // including underscores: D:\repo_name -> D--repo-name. This is one
  // folder name, never a relative path from the terminal.
  final projectName = workspacePath.replaceAll(RegExp(r'[^A-Za-z0-9-]'), '-');
  final directory = Directory(p.join(root.path, projectName));
  try {
    if (!await directory.exists()) return null;
    final sessions = <({File file, DateTime modified})>[];
    await for (final entity in directory.list(followLinks: false)) {
      if (entity is! File || !entity.path.endsWith('.jsonl')) continue;
      sessions.add((file: entity, modified: await entity.lastModified()));
    }
    sessions.sort((a, b) => b.modified.compareTo(a.modified));

    // Keep clipboard clicks responsive in large projects and avoid reading
    // histories belonging to other workspaces.
    for (final session in sessions.take(5)) {
      final transcript = await _readTranscriptTail(session.file);
      if (transcript == null) continue;
      final records = const LineSplitter().convert(transcript);
      for (var i = records.length - 1; i >= 0; i--) {
        Map<String, dynamic> record;
        try {
          final decoded = jsonDecode(records[i]);
          if (decoded is! Map<String, dynamic>) continue;
          record = decoded;
        } catch (_) {
          continue;
        }
        if (record['type'] != 'assistant' ||
            !_sameWorkspace(record['cwd'], workspacePath)) {
          continue;
        }
        final message = record['message'];
        if (message is! Map || message['content'] is! List) continue;
        for (final part in (message['content'] as List).reversed) {
          if (part is! Map ||
              part['type'] != 'text' ||
              part['text'] is! String) {
            continue;
          }
          final match = recoverCompleteClaudeSourceFromText(
            visibleCode: visibleCode,
            assistantText: part['text'] as String,
          );
          if (match != null) return match;
        }
      }
    }
  } on FileSystemException {
    // Fall back to what is visibly rendered by the terminal.
  } on FormatException {
    // A partially written Claude JSONL must not break ordinary clipboard.
  }
  return null;
}

bool _sameWorkspace(Object? cwd, String requested) {
  if (cwd is! String || cwd.isEmpty) return false;
  String normalized(String path) => p
      .normalize(path)
      .replaceAll('\\', '/')
      .toLowerCase()
      .replaceFirst(RegExp(r'/$'), '');
  return normalized(cwd) == normalized(requested);
}

Future<String?> _readTranscriptTail(File file) async {
  const maxBytes = 4 * 1024 * 1024;
  final length = await file.length();
  if (length == 0) return null;
  final start = length > maxBytes ? length - maxBytes : 0;
  final reader = await file.open(mode: FileMode.read);
  try {
    await reader.setPosition(start);
    final bytes = await reader.read(length - start);
    var decoded = utf8.decode(bytes, allowMalformed: true);
    if (start > 0) {
      final separator = decoded.indexOf('\n');
      if (separator == -1) return null;
      decoded = decoded.substring(separator + 1);
    }
    return decoded;
  } finally {
    await reader.close();
  }
}
