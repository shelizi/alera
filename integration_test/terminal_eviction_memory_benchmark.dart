/// Native Windows P2/P3 terminal eviction memory and reveal benchmark.
///
/// Run each mode in a fresh Flutter test process so allocator retention from one
/// mode cannot contaminate the other:
///
///   flutter test integration_test/terminal_eviction_memory_benchmark.dart -d windows \
///     --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=soft
///
///   flutter test integration_test/terminal_eviction_memory_benchmark.dart -d windows \
///     --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=hard
///
/// Optional:
///
///   --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS=4
///   --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_ROWS=1000
///   --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_REVEAL_RUNS=5
///   --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SETTLE_MS=500
///
/// The benchmark reports absolute RSS at baseline, after identical parser/UI
/// hydration, and after UI eviction. Hard mode additionally closes eligible
/// parser-worker isolates and retains only the compact structured checkpoint.
/// It also measures five reveal cycles from the evicted state.
library;

import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const _modeName = String.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_MODE',
  defaultValue: 'hard',
);
const _sessionCount = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS',
  defaultValue: 4,
);
const _snapshotRows = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_ROWS',
  defaultValue: 1000,
);
const _revealRuns = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_REVEAL_RUNS',
  defaultValue: 5,
);
const _postEvictionSettleMs = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_SETTLE_MS',
  defaultValue: 500,
);
const _marker = 'P4-SNAPSHOT-END';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('T8e P4 native eviction profile ($_modeName)', (tester) async {
    expect(_modeName, anyOf('soft', 'hard'));
    expect(_sessionCount, greaterThan(1));
    expect(_snapshotRows, greaterThan(0));
    expect(_revealRuns, greaterThan(0));
    expect(_postEvictionSettleMs, greaterThanOrEqualTo(0));
    final hardEviction = _modeName == 'hard';

    final runtime = XtermTerminalRuntime(parserWorkerEnabled: true);
    var disposed = false;
    addTearDown(() {
      if (!disposed) {
        runtime.dispose();
      }
    });

    final warmup = runtime.sessionFor(
      workspace: _workspace(),
      tab: _tab('warmup'),
    );
    final warmupVisibility = acquireTerminalVisibilityForTesting(warmup);
    await _writeOutputInChunks(
      warmup,
      _buildSnapshotText(rows: 128, marker: 'P4-WARMUP-END'),
    );
    warmupVisibility.dispose();
    runtime.closeTab(warmup.tabId);
    await _settle(tester, const Duration(milliseconds: 200));

    final snapshotText = _buildSnapshotText(
      rows: _snapshotRows,
      marker: _marker,
    );
    final snapshotBytes = utf8.encode(snapshotText).length;
    expect(snapshotBytes, _snapshotRows * 256);

    final rssBaseline = ProcessInfo.currentRss;
    final sessions = <TerminalSessionHandle>[];
    for (var index = 0; index < _sessionCount; index++) {
      final session = runtime.sessionFor(
        workspace: _workspace(),
        tab: _tab('session-$index'),
      );
      setTerminalParserWorkerHardEvictionEnabledForTesting(
        session,
        hardEviction,
      );
      final visibility = acquireTerminalVisibilityForTesting(session);
      await _writeOutputInChunks(session, snapshotText);
      expect(terminalBufferTextForTesting(session), contains(_marker));
      visibility.dispose();
      sessions.add(session);
    }

    await _settle(tester, const Duration(milliseconds: 250));
    final rssHydrated = ProcessInfo.currentRss;

    for (final session in sessions) {
      evictTerminalSessionForTesting(runtime, session.tabId);
    }
    for (final session in sessions) {
      await waitForTerminalParserApplyForTesting(session);
      expect(terminalUiBufferEvictedForTesting(session), isTrue);
    }

    final hardEvictedCount = sessions
        .where(terminalParserWorkerHardEvictedForTesting)
        .length;
    expect(hardEvictedCount, hardEviction ? _sessionCount : 0);
    await _settle(tester, Duration(milliseconds: _postEvictionSettleMs));
    final rssEvicted = ProcessInfo.currentRss;

    final revealMs = <double>[];
    final revealSession = sessions.first;
    for (var run = 0; run < _revealRuns; run++) {
      if (!terminalUiBufferEvictedForTesting(revealSession)) {
        evictTerminalSessionForTesting(runtime, revealSession.tabId);
        await waitForTerminalParserApplyForTesting(revealSession);
      }
      expect(terminalUiBufferEvictedForTesting(revealSession), isTrue);
      expect(
        terminalParserWorkerHardEvictedForTesting(revealSession),
        hardEviction,
      );

      final watch = Stopwatch()..start();
      final visibility = acquireTerminalVisibilityForTesting(revealSession);
      await waitForTerminalParserApplyForTesting(revealSession);
      await tester.pump();
      watch.stop();
      revealMs.add(
        watch.elapsedMicroseconds / Duration.microsecondsPerMillisecond,
      );

      expect(terminalUiBufferEvictedForTesting(revealSession), isFalse);
      expect(terminalBufferTextForTesting(revealSession), contains(_marker));
      visibility.dispose();

      if (run + 1 < _revealRuns) {
        evictTerminalSessionForTesting(runtime, revealSession.tabId);
        await waitForTerminalParserApplyForTesting(revealSession);
        await _settle(tester, const Duration(milliseconds: 25));
      }
    }

    final hydratedDelta = rssHydrated - rssBaseline;
    final evictedDelta = rssEvicted - rssBaseline;
    final reclaimed = rssHydrated - rssEvicted;
    final report = <String, Object?>{
      'version': 1,
      'mode': _modeName,
      'sessions': _sessionCount,
      'snapshot_rows_per_session': _snapshotRows,
      'snapshot_bytes_per_session': snapshotBytes,
      'rss_baseline_bytes': rssBaseline,
      'rss_hydrated_bytes': rssHydrated,
      'rss_evicted_bytes': rssEvicted,
      'rss_hydrated_delta_bytes': hydratedDelta,
      'rss_evicted_delta_bytes': evictedDelta,
      'rss_reclaimed_after_eviction_bytes': reclaimed,
      'post_eviction_settle_ms': _postEvictionSettleMs,
      'rss_reclaimed_fraction_of_hydrated_delta': hydratedDelta > 0
          ? reclaimed / hydratedDelta
          : null,
      'hard_evicted_sessions': hardEvictedCount,
      'reveal_ms_samples': revealMs,
      'reveal_median_ms': _median(revealMs),
      'reveal_p95_ms': _percentile(revealMs, 0.95),
    };

    // ignore: avoid_print
    print('T8E_P4_PROFILE ${jsonEncode(report)}');

    await tester.pumpWidget(const SizedBox());
    runtime.dispose();
    disposed = true;
    await tester.pump();
  });
}

Future<void> _settle(WidgetTester tester, Duration delay) async {
  await tester.runAsync(() => Future<void>.delayed(delay));
  await tester.pump();
}

Future<void> _writeOutputInChunks(
  TerminalSessionHandle session,
  String text,
) async {
  const chunkChars = 8 * 1024;
  for (var start = 0; start < text.length; start += chunkChars) {
    final next = start + chunkChars;
    final end = next < text.length ? next : text.length;
    writeTerminalOutputForTesting(session, text.substring(start, end));
    await waitForTerminalParserApplyForTesting(session);
  }
}

String _buildSnapshotText({required int rows, required String marker}) {
  const styledToken = '\x1b[32mOK\x1b[0m';
  final body = List<String>.filled(21, styledToken).join();
  return List<String>.generate(rows, (index) {
    final suffix = index == rows - 1
        ? marker
        : 'ROW-${index.toString().padLeft(5, '0')}';
    return '$body${suffix.padRight(23, '.')}\r\n';
  }).join();
}

double _median(Iterable<double> values) {
  final sorted = values.toList()..sort();
  final middle = sorted.length ~/ 2;
  if (sorted.length.isOdd) {
    return sorted[middle];
  }
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

double _percentile(Iterable<double> values, double percentile) {
  final sorted = values.toList()..sort();
  final index = (sorted.length * percentile).ceil() - 1;
  return sorted[index.clamp(0, sorted.length - 1)];
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 9, 18);
  return Workspace(
    id: 'workspace-t8e-p4-profile',
    projectId: 'project-t8e-p4-profile',
    name: 'T8e P4 Profile',
    branch: 'main',
    path: '/repo/t8e-p4-profile',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab(String suffix) {
  final now = DateTime.utc(2026, 9, 18);
  return WorkspaceTabRecord(
    id: 'tab-t8e-p4-$suffix',
    workspaceId: 'workspace-t8e-p4-profile',
    title: 'T8e P4 $suffix',
    createdAt: now,
    updatedAt: now,
    payload: const <String, Object?>{},
  );
}
