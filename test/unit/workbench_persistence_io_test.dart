import 'package:alera/src/features/workbench/application/workspace_activity_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/infra/drift_workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/infra/drift_workspace_activity_repository.dart';
import 'package:alera/src/features/workbench/infra/runtime_workspace_activity_repository.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:drift/native.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final older = DateTime.utc(2026, 11, 24, 10);
  final newer = DateTime.utc(2026, 11, 24, 11);

  test(
    'identical activity snapshots cause no writes or Host upserts',
    () async {
      final local = _LocalActivity({'same': newer});
      final host = _ActivityHost({'same': newer});
      var accesses = 0;
      final repository = RuntimeWorkspaceActivityRepository(
        client: host,
        legacyRepository: local,
        beforeAccess: () async {
          accesses++;
        },
      );
      expect(await repository.loadAll(), {'same': newer});
      await repository.upsertAll({});
      expect(accesses, 1);
      expect(local.writes, isEmpty);
      expect(host.writes, isEmpty);
    },
  );

  test(
    'activity synchronization writes only newer or missing entries',
    () async {
      final local = _LocalActivity({
        'same': newer,
        'local-newer': newer,
        'remote-newer': older,
        'local-only': older,
      });
      final host = _ActivityHost({
        'same': newer,
        'local-newer': older,
        'remote-newer': newer,
        'remote-only': older,
      });
      final repository = RuntimeWorkspaceActivityRepository(
        client: host,
        legacyRepository: local,
      );
      final merged = {
        'same': newer,
        'local-newer': newer,
        'remote-newer': newer,
        'local-only': older,
        'remote-only': older,
      };
      expect(await repository.loadAll(), merged);
      expect(local.writes, [
        {'remote-newer': newer, 'remote-only': older},
      ]);
      expect(host.writes, [
        {
          'local-newer': newer.toIso8601String(),
          'local-only': older.toIso8601String(),
        },
      ]);
      expect(await repository.loadAll(), merged);
      expect(local.writes, hasLength(1));
      expect(host.writes, hasLength(1));
    },
  );

  test(
    'Host write failure preserves local fallback and retries next load',
    () async {
      final local = _LocalActivity({'local': newer});
      final host = _ActivityHost({'remote': older})..failWrites = true;
      final repository = RuntimeWorkspaceActivityRepository(
        client: host,
        legacyRepository: local,
      );
      expect(await repository.loadAll(), {'local': newer});
      expect(local.writes, isEmpty);
      host.failWrites = false;
      expect(await repository.loadAll(), {'local': newer, 'remote': older});
      expect(local.data, {'local': newer, 'remote': older});
      expect(host.data, {'local': newer, 'remote': older});
    },
  );

  test('SQLite activity skips unchanged and stale timestamps', () async {
    final db = AleraDatabase(executor: NativeDatabase.memory());
    addTearDown(db.close);
    final repository = DriftWorkspaceActivityRepository(db);
    await repository.upsertAll({'workspace': newer});
    final before = await _changes(db);
    await repository.upsertAll({'workspace': newer});
    await repository.upsertAll({'workspace': older});
    // Drift stores timestamps at second precision.
    await repository.upsertAll({
      'workspace': newer.add(const Duration(milliseconds: 123)),
    });
    expect(await _changes(db), before);
    expect(await repository.loadAll(), {'workspace': newer});
    final next = newer.add(const Duration(seconds: 1));
    await repository.upsertAll({'workspace': next, 'new': older});
    expect(await _changes(db), before + 2);
    expect(await repository.loadAll(), {'workspace': next, 'new': older});
  });

  test('SQLite view preferences skip identical serialized values', () async {
    final db = AleraDatabase(executor: NativeDatabase.memory());
    addTearDown(db.close);
    final repository = DriftWorkbenchViewPrefsRepository(db);
    await repository.save(WorkbenchViewPrefs.defaults);
    final before = await _changes(db);
    await repository.save(WorkbenchViewPrefs.defaults);
    expect(await _changes(db), before);
    final changed = WorkbenchViewPrefs.defaults.copyWith(sidebarWidth: 350);
    await repository.save(changed);
    expect(await _changes(db), before + 1);
    expect(await repository.load(), changed);
  });
}

Future<int> _changes(AleraDatabase db) async =>
    (await db.customSelect('SELECT total_changes() AS n').getSingle())
        .read<int>('n');

final class _LocalActivity implements WorkspaceActivityRepository {
  _LocalActivity(Map<String, DateTime> entries) : data = {...entries};

  final Map<String, DateTime> data;
  final writes = <Map<String, DateTime>>[];

  @override
  Future<Map<String, DateTime>> loadAll() async => {...data};

  @override
  Future<void> upsertAll(Map<String, DateTime> entries) async {
    writes.add({...entries});
    data.addAll(entries);
  }

  @override
  Future<void> remove(String workspaceId) async {
    data.remove(workspaceId);
  }
}

final class _ActivityHost implements RuntimeHostClient {
  _ActivityHost(Map<String, DateTime> entries) : data = {...entries};

  final Map<String, DateTime> data;
  final writes = <Map<String, Object?>>[];
  bool failWrites = false;

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const {},
    Duration? timeout,
  ]) async {
    if (type == 'workspaceActivity.list') {
      return {
        for (final entry in data.entries)
          entry.key: entry.value.toIso8601String(),
      };
    }
    if (type == 'workspaceActivity.upsertAll') {
      if (failWrites) throw StateError('Host unavailable');
      writes.add({...payload});
      for (final entry in payload.entries) {
        data[entry.key] = DateTime.parse(entry.value as String);
      }
      return null;
    }
    throw StateError('unexpected request: $type');
  }
}
