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

  Future<File> _lockFor(String tabId) async {
    final name = sha256.convert(utf8.encode(tabId)).toString();
    return File(p.join((await _folder()).path, '$name.lock'));
  }

  Future<File> _resolutionFor(WorkbenchTerminalTabTransferPlan plan) async {
    final key = sha256.convert(
      utf8.encode(jsonEncode(<String>[plan.tabId, plan.clientMutationId])),
    );
    final support = await _supportDirectory();
    return File(
      p.join(support.path, 'terminal_transfer_resolved_v1', '$key.json'),
    );
  }

  /// The .lock sentinel serializes both remember and forget across apps
  /// sharing the same Support Directory. A process crash leaves it blocking:
  /// only a verified recovery flow may repair an abandoned journal lock.
  Future<File> _acquireTabLock(String tabId) async {
    final lock = await _lockFor(tabId);
    await lock.parent.create(recursive: true);
    try {
      await lock.create(exclusive: true);
    } on FileSystemException {
      if (await lock.exists()) {
        throw StateError(
          'Terminal transfer journal for this Tab is being modified or '
          'has a stale recovery lock. Keep it and verify the original '
          'Host operation before attempting manual repair.',
        );
      }
      rethrow;
    }
    return lock;
  }

  bool _samePlan(
    WorkbenchTerminalTabTransferPlan first,
    WorkbenchTerminalTabTransferPlan second,
  ) =>
      jsonEncode(first.toRuntimePayload()) ==
      jsonEncode(second.toRuntimePayload());

  Future<WorkbenchTerminalTabTransferPlan> _readResolution(File file) async {
    final raw = await file.readAsString();
    try {
      final data = jsonDecode(raw);
      final resolvedAtUtc = data is Map ? data['resolvedAtUtc'] : null;
      if (data is! Map ||
          data['resolution'] != 'callerConfirmedHostAndBothLayouts' ||
          resolvedAtUtc is! String ||
          DateTime.tryParse(resolvedAtUtc) == null) {
        throw const FormatException('Missing verified resolution marker.');
      }
    } on Object catch (error) {
      throw StateError(
        'Invalid Terminal transfer resolution archive at ${file.path}: $error',
      );
    }
    return _decode(raw, file.path);
  }

  @override
  Future<Map<String, WorkbenchTerminalTabTransferPlan>> loadPending() async {
    final folder = await _folder();
    if (!await folder.exists())
      return <String, WorkbenchTerminalTabTransferPlan>{};
    final pending = <String, WorkbenchTerminalTabTransferPlan>{};
    await for (final entity in folder.list(followLinks: false)) {
      if (entity is File && entity.path.endsWith('.lock')) {
        throw StateError(
          'Terminal transfer journal has an active or interrupted '
          'cross-process lock: ${entity.path}. Verify it before moving.',
        );
      }
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
    final lock = await _acquireTabLock(plan.tabId);
    try {
      final file = await _fileFor(plan.tabId);
      await file.parent.create(recursive: true);
      // A contains()/rename() check is not an inter-process mutex: two
      // desktop windows may both observe "no file", then rename their own
      // mutation journals onto the same Tab, losing one original receipt.
      // Exclusive creation is the filesystem's atomic compare-and-create.
      // A crash after creation but before the flushed payload is written
      // intentionally leaves a corrupt sentinel: loadPending() fails closed
      // rather than silently authorizing a second transfer.
      try {
        await file.create(exclusive: true);
      } on FileSystemException {
        if (await file.exists()) {
          throw StateError(
            'A Terminal transfer is already recorded for this tab. '
            'Verify the original Host receipt before moving it again.',
          );
        }
        rethrow;
      }
      await file.writeAsString(
        jsonEncode(<String, Object?>{
          'version': _version,
          ...plan.toRuntimePayload(),
        }),
        flush: true,
      );
    } finally {
      await lock.delete();
    }
  }

  @override
  Future<void> forget(WorkbenchTerminalTabTransferPlan plan) async {
    final lock = await _acquireTabLock(plan.tabId);
    try {
      final file = await _fileFor(plan.tabId);
      final resolution = await _resolutionFor(plan);
      if (await file.exists()) {
        final saved = _decode(await file.readAsString(), file.path);
        if (!_samePlan(saved, plan)) {
          throw StateError(
            'Refusing to remove a different Terminal transfer journal.',
          );
        }
        // The caller has verified the Host and reconciled both Layouts.
        // This local archive is NOT a signed Host receipt. Archive before
        // deleting pending data: an interruption leaves inspectable evidence.
        await resolution.parent.create(recursive: true);
        if (!await resolution.exists()) {
          await resolution.create(exclusive: true);
          await resolution.writeAsString(
            jsonEncode(<String, Object?>{
              'version': _version,
              ...plan.toRuntimePayload(),
              'resolution': 'callerConfirmedHostAndBothLayouts',
              'resolvedAtUtc': DateTime.now().toUtc().toIso8601String(),
            }),
            flush: true,
          );
        }
        final archived = await _readResolution(resolution);
        if (!_samePlan(archived, plan)) {
          throw StateError(
            'Terminal transfer audit and pending journal disagree. '
            'Preserving the original unresolved operation.',
          );
        }
        await file.delete();
      } else {
        if (!await resolution.exists() ||
            !_samePlan(await _readResolution(resolution), plan)) {
          throw StateError(
            'Missing Terminal transfer journal without a matching '
            'audited resolution. Refusing to release its guard.',
          );
        }
      }
    } finally {
      await lock.delete();
    }
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
