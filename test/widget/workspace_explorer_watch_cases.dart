part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerWatchTests() {
  testWidgets('watch batch storm coalesces into bounded refreshes', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('a', hasChildrenHint: true),
        _directory('b', hasChildrenHint: true),
        _directory('c', hasChildrenHint: true),
        _directory('d', hasChildrenHint: true),
      ]
      ..childrenByDirectory['a'] = <native.WorkspaceFileEntry>[
        _file('a/one.dart'),
      ]
      ..childrenByDirectory['b'] = <native.WorkspaceFileEntry>[
        _file('b/two.dart'),
      ]
      ..childrenByDirectory['c'] = <native.WorkspaceFileEntry>[
        _file('c/three.dart'),
      ]
      ..childrenByDirectory['d'] = <native.WorkspaceFileEntry>[
        _file('d/four.dart'),
      ];
    final gitBackend = _BlockingSnapshotGitBackend();

    await _pumpExplorer(tester, service, gitBackend: gitBackend);
    for (final path in <String>['a', 'b', 'c', 'd']) {
      await tester.tap(find.text(path));
    }
    await tester.pumpAndSettle();

    int statusCalls() => gitBackend.calls
        .where((call) => call.method == 'explorerStatusSnapshot')
        .length;
    final baseline = statusCalls();
    final baselineListCalls = service.listChildrenCalls.length;

    // Block the in-flight refresh so every subsequent batch lands in the
    // pending set; the storm must resolve in at most one extra merged pass
    // instead of one pass per batch.
    final gate = Completer<void>();
    gitBackend.snapshotGates.add(gate);
    service.emitWatchBatch(<String>['a']);
    service.emitWatchBatch(<String>['b']);
    service.emitWatchBatch(<String>['a', 'c']);
    service.emitWatchBatch(<String>['d']);
    service.emitWatchBatch(<String>['b']);
    await tester.pump();
    expect(statusCalls(), baseline + 1);

    gate.complete();
    await tester.pumpAndSettle();

    expect(statusCalls() - baseline, lessThanOrEqualTo(2));
    expect(
      service.listChildrenCalls.length - baselineListCalls,
      lessThanOrEqualTo(5),
    );
  });

  testWidgets('watched directory updates are skipped when nothing changed', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/main.dart'),
      ];

    await _pumpExplorer(tester, service);
    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    final updatesAfterExpand = service.watchedPathUpdates.length;

    service.emitWatchBatch(<String>['src']);
    await tester.pumpAndSettle();

    expect(service.watchedPathUpdates, hasLength(updatesAfterExpand));
  });
}

class _BlockingSnapshotGitBackend extends FakeGitBackend {
  final List<Completer<void>> snapshotGates = <Completer<void>>[];

  @override
  Future<GitExplorerStatusSnapshot> explorerStatusSnapshot(String path) async {
    calls.add(
      GitBackendCall('explorerStatusSnapshot', <String, Object?>{'path': path}),
    );
    if (snapshotGates.isNotEmpty) {
      await snapshotGates.removeAt(0).future;
    }
    return gitExplorerStatusSnapshot;
  }
}
