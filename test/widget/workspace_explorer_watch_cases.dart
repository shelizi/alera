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

  testWidgets(
    'collapsed deep file changes reindex search without tree reload',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ];
      await _pumpExplorer(tester, service);
      final baseline = service.listChildrenCalls.length;

      service.emitWatchBatch(
        const <String>[],
        changedRelativePaths: const <String>['src/deep/new.dart'],
      );
      await tester.pumpAndSettle();

      expect(service.quickOpenRefreshRequests, isNotEmpty);
      expect(service.listChildrenCalls, hasLength(baseline));
    },
  );

  test(
    'directory no-op comparator includes all visible metadata and order',
    () {
      final previous = [_file('src/a.dart'), _file('src/b.dart')];
      expect(
        workspaceExplorerDirectoryChildrenUnchanged(previous, [
          _file('src/a.dart'),
          _file('src/b.dart'),
        ]),
        isTrue,
      );
      expect(
        workspaceExplorerDirectoryChildrenUnchanged(previous, [
          _file('src/b.dart'),
          _file('src/a.dart'),
        ]),
        isFalse,
      );
      expect(
        workspaceExplorerDirectoryChildrenUnchanged(previous, [
          _file(
            'src/a.dart',
            gitStatus: native.WorkspaceFileGitStatus.modified,
          ),
          _file('src/b.dart'),
        ]),
        isFalse,
      );
      expect(
        workspaceExplorerDirectoryChildrenUnchanged(previous, [
          _file('src/a.dart', isHidden: true),
          _file('src/b.dart'),
        ]),
        isFalse,
      );
      expect(
        workspaceExplorerDirectoryChildrenUnchanged(null, const []),
        isFalse,
      );
    },
  );

  testWidgets('watch changed directory entries still projects new nodes', (
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
    final projected = service.nativeProjectionUpdates;

    service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
      _file('src/main.dart'),
      _file('src/extra.dart'),
    ];
    service.emitWatchBatch(<String>['src']);
    await tester.pumpAndSettle();

    expect(service.nativeProjectionUpdates, projected + 1);
    expect(service.projectionCalls, 0);
    expect(find.text('extra.dart'), findsOneWidget);
  });

  testWidgets(
    'native Explorer delta recovers from stale handle by full fallback',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/original.dart'),
        ];
      await _pumpExplorer(tester, service);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(service.nativeProjectionStarts, 1);
      final previousNativeUpdates = service.nativeProjectionUpdates;
      final previousFallbacks = service.projectionCalls;
      service.failNextNativeProjection = true;
      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/replacement.dart'),
      ];
      service.emitWatchBatch(<String>['src']);
      await tester.pumpAndSettle();
      expect(service.nativeProjectionUpdates, previousNativeUpdates + 1);
      expect(service.projectionCalls, previousFallbacks + 1);
      expect(find.text('replacement.dart'), findsOneWidget);
      expect(find.text('original.dart'), findsNothing);
    },
  );

  testWidgets('native Explorer delta retains sibling expanded subtrees', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('a', hasChildrenHint: true),
        _directory('b', hasChildrenHint: true),
      ]
      ..childrenByDirectory['a'] = <native.WorkspaceFileEntry>[
        _file('a/old.dart'),
      ]
      ..childrenByDirectory['b'] = <native.WorkspaceFileEntry>[
        _file('b/stable.dart'),
      ];
    await _pumpExplorer(tester, service);
    await tester.tap(find.text('a'));
    await tester.tap(find.text('b'));
    await tester.pumpAndSettle();
    expect(service.nativeProjectionStarts, 1);
    service.childrenByDirectory['a'] = <native.WorkspaceFileEntry>[
      _file('a/new.dart'),
    ];
    service.emitWatchBatch(<String>['a']);
    await tester.pumpAndSettle();
    expect(find.text('new.dart'), findsOneWidget);
    expect(find.text('stable.dart'), findsOneWidget);
    expect(find.text('old.dart'), findsNothing);
    expect(service.projectionCalls, 0);
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
    final projectionsAfterExpand = service.nativeProjectionUpdates;

    // Native listing returns new instances even when all fields are equal.
    service.childrenByDirectory['src'] = [_file('src/main.dart')];
    service.emitWatchBatch(<String>['src']);
    await tester.pumpAndSettle();

    expect(service.watchedPathUpdates, hasLength(updatesAfterExpand));
    expect(
      service.nativeProjectionUpdates,
      projectionsAfterExpand,
      reason:
          'No-op file events should not resend all directory snapshots to Rust',
    );
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
