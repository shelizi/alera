import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workbench_terminal_tab_transfer_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';
import 'package:crypto/crypto.dart';
import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

final class _TerminalTransferJournalLock {
  const _TerminalTransferJournalLock(this.file, this.nonce);

  final File file;
  final String nonce;
}

/// A diagnostic snapshot, never a release permission. A surviving .lock may
/// still belong to an active App and local archives are not Host-signed.
final class TerminalTransferJournalRecoveryInspection {
  const TerminalTransferJournalRecoveryInspection({
    required this.lockPresent,
    required this.pendingPresent,
    required this.pendingReadable,
    required this.localConfirmedArchive,
    this.pendingMutationId,
    this.lockOwnerProcessId,
    this.lockCreatedAtUtc,
    this.lockMetadataValid = false,
  });

  final bool lockPresent;
  final bool pendingPresent;
  final bool pendingReadable;
  final bool localConfirmedArchive;
  final String? pendingMutationId;

  /// PID and timestamp are diagnostic hints, not proof of a live process.
  final int? lockOwnerProcessId;
  final DateTime? lockCreatedAtUtc;
  final bool lockMetadataValid;

  bool get requiresManualRecovery =>
      lockPresent || (pendingPresent && !pendingReadable);
}

/// One read-only inventory row, including journals whose Tab ID cannot be
/// decoded from a damaged pending file. A file hash/PID is not a Host proof or
/// a permission to remove any lock.
final class TerminalTransferJournalInventoryItem {
  const TerminalTransferJournalInventoryItem({
    required this.fileKey,
    required this.lockPresent,
    required this.pendingPresent,
    required this.pendingReadable,
    required this.localConfirmedArchive,
    required this.lockMetadataValid,
    required this.unknownFileName,
    this.tabId,
    this.pendingMutationId,
    this.lockOwnerProcessId,
    this.lockCreatedAtUtc,
  });

  final String fileKey;
  final String? tabId;
  final bool lockPresent;
  final bool pendingPresent;
  final bool pendingReadable;
  final bool localConfirmedArchive;
  final bool lockMetadataValid;
  final bool unknownFileName;
  final String? pendingMutationId;
  final int? lockOwnerProcessId;
  final DateTime? lockCreatedAtUtc;

  bool get requiresManualRecovery =>
      unknownFileName || lockPresent || (pendingPresent && !pendingReadable);
}

/// Durable, fail-closed record of a Terminal transfer *before* Host commit.
/// Files are per Tab so unrelated workspaces do not overwrite one another.
/// Malformed records are surfaced instead of silently authorizing a new move.
final class FileWorkbenchTerminalTransferJournal
    implements WorkbenchTerminalTransferJournal {
  const FileWorkbenchTerminalTransferJournal({
    Future<Directory> Function()? supportDirectory,
  }) : _supportDirectory = supportDirectory ?? aleraApplicationSupportDirectory;

  static const int _version = 1;
  static const int _lockVersion = 1;
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
  Future<_TerminalTransferJournalLock> _acquireTabLock(String tabId) async {
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
    final nonce = const Uuid().v4();
    // If this flush fails, leave the existing exclusive sentinel in place.
    // An empty/partial lock is unknown, not evidence that it is abandoned.
    await lock.writeAsString(
      jsonEncode(<String, Object?>{
        'version': _lockVersion,
        'tabIdHash': sha256.convert(utf8.encode(tabId)).toString(),
        'nonce': nonce,
        'processId': pid,
        'createdAtUtc': DateTime.now().toUtc().toIso8601String(),
      }),
      flush: true,
    );
    return _TerminalTransferJournalLock(lock, nonce);
  }

  /// Refuse to delete a lock whose identity changed under this process.
  /// Not a durable lease: a privileged external writer could still race the
  /// check, so an interrupted lock remains a manual-recovery condition.
  Future<void> _releaseTabLock(_TerminalTransferJournalLock owned) async {
    final Object? decoded;
    try {
      decoded = jsonDecode(await owned.file.readAsString());
    } on Object {
      throw StateError(
        'Terminal transfer journal lock changed or is unreadable. '
        'Preserving it for manual recovery.',
      );
    }
    if (decoded is! Map ||
        decoded['version'] != _lockVersion ||
        decoded['nonce'] != owned.nonce ||
        decoded['processId'] != pid) {
      throw StateError(
        'Terminal transfer journal lock owner changed; refusing deletion.',
      );
    }
    await owned.file.delete();
  }

  Future<({bool valid, int? ownerPid, DateTime? created})> _inspectLock(
    File lock,
    String tabId,
  ) =>
      _inspectLockWithHash(lock, sha256.convert(utf8.encode(tabId)).toString());

  Future<({bool valid, int? ownerPid, DateTime? created})> _inspectLockWithHash(
    File lock,
    String expectedHash,
  ) async {
    try {
      final data = jsonDecode(await lock.readAsString());
      if (data is! Map ||
          data['version'] != _lockVersion ||
          data['tabIdHash'] != expectedHash ||
          data['nonce'] is! String ||
          (data['nonce'] as String).isEmpty ||
          data['processId'] is! int ||
          data['createdAtUtc'] is! String) {
        return (valid: false, ownerPid: null, created: null);
      }
      final when = DateTime.tryParse(data['createdAtUtc'] as String);
      if (when == null) {
        return (valid: false, ownerPid: null, created: null);
      }
      return (valid: true, ownerPid: data['processId'] as int, created: when);
    } on Object {
      // A crash may leave a partial sentinel; never infer owner liveness.
      return (valid: false, ownerPid: null, created: null);
    }
  }

  /// Enumerate pending / interrupted transactions without invoking the
  /// fail-closed loadPending() that intentionally stops at the first lock.
  /// Each item is a *non-atomic diagnostic observation*; concurrent Apps can
  /// change files immediately after a read. This method NEVER unlocks,
  /// deletes, replays or declares an original Host mutation resolved.
  Future<List<TerminalTransferJournalInventoryItem>>
  inspectRecoveryInventory() async {
    final folder = await _folder();
    if (!await folder.exists()) {
      return const <TerminalTransferJournalInventoryItem>[];
    }
    final fileTypes = <String, Set<String>>{};
    final unexpected = <String, Set<String>>{};
    final validDigest = RegExp(r'^[a-f0-9]{64}$');
    await for (final entity in folder.list(followLinks: false)) {
      final name = p.basename(entity.path);
      final suffix = p.extension(name);
      if (suffix != '.json' && suffix != '.lock') {
        // loadPending() now blocks every unexpected directory entry. The
        // read-only inventory must also *show* that blocker (including a
        // no-extension file) so the operator can investigate the exact name.
        unexpected
            .putIfAbsent('unexpected:$name', () => <String>{})
            .add(suffix);
        continue;
      }
      if (entity is! File) {
        // Symlinks/directories with journal-shaped names are not legitimate
        // pending records. Flag them without following their targets.
        unexpected.putIfAbsent('non-file:$name', () => <String>{}).add(suffix);
        continue;
      }
      final key = p.basenameWithoutExtension(name);
      (validDigest.hasMatch(key) ? fileTypes : unexpected)
          .putIfAbsent(key, () => <String>{})
          .add(suffix);
    }
    final result = <TerminalTransferJournalInventoryItem>[];
    final hashes = fileTypes.keys.toList()..sort();
    for (final hash in hashes) {
      final types = fileTypes[hash]!;
      final pendingPresent = types.contains('.json');
      final lockPresent = types.contains('.lock');
      final pendingFile = File(p.join(folder.path, '$hash.json'));
      final lockFile = File(p.join(folder.path, '$hash.lock'));
      final lockEvidence = lockPresent
          ? await _inspectLockWithHash(lockFile, hash)
          : (valid: false, ownerPid: null, created: null);
      WorkbenchTerminalTabTransferPlan? plan;
      if (pendingPresent) {
        try {
          final decoded = _decode(
            await pendingFile.readAsString(),
            pendingFile.path,
          );
          if (sha256.convert(utf8.encode(decoded.tabId)).toString() == hash) {
            plan = decoded;
          }
        } on Object {
          // Corrupt payload remains visible by hash; never skip or repair it.
        }
      }
      var localConfirmedArchive = false;
      if (plan != null) {
        final archive = await _resolutionFor(plan);
        if (await archive.exists()) {
          try {
            localConfirmedArchive = _samePlan(
              await _readResolution(archive),
              plan,
            );
          } on Object {
            // A corrupt archive is not proof of successful reconciliation.
          }
        }
      }
      result.add(
        TerminalTransferJournalInventoryItem(
          fileKey: hash,
          tabId: plan?.tabId,
          pendingMutationId: plan?.clientMutationId,
          pendingPresent: pendingPresent,
          pendingReadable: plan != null,
          lockPresent: lockPresent,
          localConfirmedArchive: localConfirmedArchive,
          lockMetadataValid: lockEvidence.valid,
          lockOwnerProcessId: lockEvidence.ownerPid,
          lockCreatedAtUtc: lockEvidence.created,
          unknownFileName: false,
        ),
      );
    }
    final unknownKeys = unexpected.keys.toList()..sort();
    for (final key in unknownKeys) {
      final types = unexpected[key]!;
      result.add(
        TerminalTransferJournalInventoryItem(
          fileKey: key,
          tabId: null,
          pendingPresent: types.contains('.json'),
          pendingReadable: false,
          lockPresent: types.contains('.lock'),
          localConfirmedArchive: false,
          lockMetadataValid: false,
          unknownFileName: true,
        ),
      );
    }
    return result;
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

  /// Read-only inspection of an interrupted lock and matching local audit.
  /// Never remove a lock, retry a mutation or silently discard the pending
  /// Journal just because its payload or archive happens to look complete.
  Future<TerminalTransferJournalRecoveryInspection> inspectRecovery(
    String tabId,
  ) async {
    final lock = await _lockFor(tabId);
    final pending = await _fileFor(tabId);
    final lockPresent = await lock.exists();
    final lockEvidence = lockPresent
        ? await _inspectLock(lock, tabId)
        : (valid: false, ownerPid: null, created: null);
    if (!await pending.exists()) {
      return TerminalTransferJournalRecoveryInspection(
        lockPresent: lockPresent,
        pendingPresent: false,
        pendingReadable: false,
        localConfirmedArchive: false,
        lockOwnerProcessId: lockEvidence.ownerPid,
        lockCreatedAtUtc: lockEvidence.created,
        lockMetadataValid: lockEvidence.valid,
      );
    }
    final WorkbenchTerminalTabTransferPlan plan;
    try {
      plan = _decode(await pending.readAsString(), pending.path);
      if (plan.tabId != tabId) {
        throw StateError('Pending Terminal journal Tab identity disagrees.');
      }
    } on Object {
      return TerminalTransferJournalRecoveryInspection(
        lockPresent: lockPresent,
        pendingPresent: true,
        pendingReadable: false,
        localConfirmedArchive: false,
        lockOwnerProcessId: lockEvidence.ownerPid,
        lockCreatedAtUtc: lockEvidence.created,
        lockMetadataValid: lockEvidence.valid,
      );
    }
    var localConfirmedArchive = false;
    final archive = await _resolutionFor(plan);
    if (await archive.exists()) {
      try {
        localConfirmedArchive = _samePlan(await _readResolution(archive), plan);
      } on Object {
        // Corrupt archives provide no evidence that the lock may be removed.
      }
    }
    return TerminalTransferJournalRecoveryInspection(
      lockPresent: lockPresent,
      pendingPresent: true,
      pendingReadable: true,
      localConfirmedArchive: localConfirmedArchive,
      pendingMutationId: plan.clientMutationId,
      lockOwnerProcessId: lockEvidence.ownerPid,
      lockCreatedAtUtc: lockEvidence.created,
      lockMetadataValid: lockEvidence.valid,
    );
  }

  @override
  Future<Map<String, WorkbenchTerminalTabTransferPlan>> loadPending() async {
    final folder = await _folder();
    if (!await folder.exists())
      return <String, WorkbenchTerminalTabTransferPlan>{};
    final pending = <String, WorkbenchTerminalTabTransferPlan>{};
    final validDigest = RegExp(r'^[a-f0-9]{64}$');
    await for (final entity in folder.list(followLinks: false)) {
      final name = p.basename(entity.path);
      final extension = p.extension(name);
      if (extension == '.lock') {
        // A stale lock can be a broken symlink or a directory, not only a
        // Dart File. Ignoring it would falsely advertise an empty journal.
        throw StateError(
          'Terminal transfer journal has an active or interrupted '
          'cross-process lock: ${entity.path}. Verify it before moving.',
        );
      }
      if (extension != '.json' ||
          entity is! File ||
          !validDigest.hasMatch(p.basenameWithoutExtension(name))) {
        // The directory is exclusively for pending transfers. Unexpected
        // entries (including symlinks and corrupted names) are unresolved
        // evidence; they may not be silently ignored during recovery.
        throw StateError(
          'Terminal transfer journal contains an unexpected pending '
          'entry: ${entity.path}. Inspect it without deleting it.',
        );
      }
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
      await _releaseTabLock(lock);
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
      await _releaseTabLock(lock);
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
