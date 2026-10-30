import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workbench_terminal_tab_transfer_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';
import 'package:crypto/crypto.dart';
import 'package:path/path.dart' as p;

/// Durable, fail-closed record of a Terminal transfer *before* Host commit.
/// Files are per Tab so unrelated workspaces do not overwrite one another.
/// Malformed records are surfaced instead of silently authorizing a new move.
final class FileWorkbenchTerminalTransferJournal
    implements WorkbenchTerminalTransferJournal {
  const FileWorkbenchTerminalTransferJournal({
    Future<Directory> Function()? supportDirectory,
  }) : _supportDirectory = supportDirectory ?? aleraApplicationSupportDirectory;

  static const int _version = 1;
  final Future<Directory> Function() _supportDirectory;

  Future<Directory> _folder() async {
    final support = await _supportDirectory();
    return Directory(p.join(support.path, 'terminal_transfer_pending_v1'));
  }

  Future<File> _fileFor(String tabId) async {
    final name = sha256.convert(utf8.encode(tabId)).toString();
    return File(p.join((await _folder()).path, '$name.json'));
  }

  @override
  Future<Map<String, WorkbenchTerminalTabTransferPlan>> loadPending() async {
    final folder = await _folder();
    if (!await folder.exists())
      return <String, WorkbenchTerminalTabTransferPlan>{};
    final pending = <String, WorkbenchTerminalTabTransferPlan>{};
    await for (final entity in folder.list(followLinks: false)) {
      if (entity is! File || !entity.path.endsWith('.json')) continue;
      final plan = _decode(await entity.readAsString(), entity.path);
      final expected = await _fileFor(plan.tabId);
      if (p.normalize(entity.path) != p.normalize(expected.path)) {
        throw StateError(
          'Terminal transfer journal path and Tab identity disagree.',
        );
      }
      if (pending.containsKey(plan.tabId)) {
        throw StateError('Duplicate pending Terminal transfer journal.');
      }
      pending[plan.tabId] = plan;
    }
    return pending;
  }

  @override
  Future<void> remember(WorkbenchTerminalTabTransferPlan plan) async {
    final file = await _fileFor(plan.tabId);
    await file.parent.create(recursive: true);
    if (await file.exists()) {
      throw StateError(
        'A previous Terminal move is pending. Check its Host status before moving again.',
      );
    }
    final temp = File(
      '${file.path}.${DateTime.now().microsecondsSinceEpoch}.tmp',
    );
    try {
      await temp.writeAsString(
        jsonEncode(<String, Object?>{
          'version': _version,
          ...plan.toRuntimePayload(),
        }),
        flush: true,
      );
      if (await file.exists()) {
        throw StateError('Another Terminal transfer recorded this tab.');
      }
      await temp.rename(file.path);
    } finally {
      if (await temp.exists()) await temp.delete();
    }
  }

  @override
  Future<void> forget(WorkbenchTerminalTabTransferPlan plan) async {
    final file = await _fileFor(plan.tabId);
    if (!await file.exists()) return;
    final saved = _decode(await file.readAsString(), file.path);
    if (saved.clientMutationId != plan.clientMutationId ||
        saved.sessionId != plan.sessionId ||
        saved.sourceWorkspaceId != plan.sourceWorkspaceId ||
        saved.targetWorkspaceId != plan.targetWorkspaceId) {
      throw StateError(
        'Refusing to remove a different Terminal transfer journal.',
      );
    }
    await file.delete();
  }

  WorkbenchTerminalTabTransferPlan _decode(String raw, String source) {
    try {
      final data = jsonDecode(raw);
      if (data is! Map || data['version'] != _version) {
        throw const FormatException('Unsupported journal format.');
      }
      String text(String key) {
        final value = data[key];
        if (value is! String || value.trim().isEmpty) {
          throw FormatException('Missing transfer field $key.');
        }
        return value;
      }

      WorkbenchLayout? readLayout(String key, {bool optional = false}) {
        final value = data[key];
        if (value == null && optional) return null;
        if (value is! Map) throw FormatException('Invalid layout $key.');
        return WorkbenchLayout.fromJson(Map<String, Object?>.from(value));
      }

      final from = text('sourceWorkspaceId');
      final to = text('targetWorkspaceId');
      if (from == to) throw const FormatException('Identical workspaces.');
      final sourceLayout = readLayout('sourceLayout')!;
      final targetLayout = readLayout('targetLayout')!;
      if (sourceLayout.workspaceId != from || targetLayout.workspaceId != to) {
        throw const FormatException('Layout workspace mismatch.');
      }
      return WorkbenchTerminalTabTransferPlan(
        tabId: text('tabId'),
        sourceWorkspaceId: from,
        targetWorkspaceId: to,
        sessionId: text('sessionId'),
        clientMutationId: text('clientMutationId'),
        expectedSourceLayout: readLayout(
          'expectedSourceLayout',
          optional: true,
        ),
        expectedTargetLayout: readLayout(
          'expectedTargetLayout',
          optional: true,
        ),
        sourceLayout: sourceLayout,
        targetLayout: targetLayout,
      );
    } on Object catch (error) {
      throw StateError('Invalid Terminal transfer journal at $source: $error');
    }
  }
}
