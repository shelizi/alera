part of 'workspace_explorer_test.dart';

/// Widget tests must advance fake time through the watch quiet-period.
/// pumpAndSettle alone can return before a trailing debounce timer fires.
Future<void> _pumpWatchedRefresh(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 130));
  await tester.pumpAndSettle();
}

void _registerWorkspaceExplorerWatchTests() {
  testWidgets('rapid workspace switching keeps only the latest Explorer', (
    tester,
  ) async {
    final first = _workspace();
    final second = _workspace(
      id: 'workspace-2',
      path: '/repo/other',
    ).copyWith(projectId: 'project-2');
    final service = _FakeWorkspaceFileService()
      ..childrenByWorkspacePath[first.path] =
          <String, List<native.WorkspaceFileEntry>>{
            '': <native.WorkspaceFileEntry>[_file('first.dart')],
          }
      ..childrenByWorkspacePath[second.path] =
          <String, List<native.WorkspaceFileEntry>>{
            '': <native.WorkspaceFileEntry>[_file('second.dart')],
          };
    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 320,
              height: 480,
              child: _SwitchingWorkspaceExplorerHarness(
                first: first,
                second: second,
                onOpenFile: (_) {},
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('first.dart'), findsOneWidget);

    // Simulate A -> B -> A while B's watcher teardown is still outstanding.
    await tester.tap(find.text('Switch workspace'));
    await tester.pump();
    await tester.tap(find.text('Switch workspace'));
    await tester.pump(const Duration(milliseconds: 1000));
    await tester.pumpAndSettle();

    expect(find.text('first.dart'), findsOneWidget);
    expect(find.text('second.dart'), findsNothing);
    expect(find.byType(CircularProgressIndicator), findsNothing);
  });

  testWidgets(
    'closed native watcher reconnects and discovers newly created files',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _file('before.dart'),
        ];
      await _pumpExplorer(tester, service);
      expect(service.explorerWatcherStarts, 1);
      expect(service.hasExplorerWatchSubscriber, isTrue);
      expect(find.text('before.dart'), findsOneWidget);

      service.childrenByDirectory[''] = [
        _file('before.dart'),
        _file('created.dart'),
      ];
      await service.closeExplorerWatchStream();
      await tester.pump();
      expect(service.quickOpenRefreshRequests, isNotEmpty);
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 650)),
      );
      for (
        var attempt = 0;
        attempt < 8 && service.explorerWatcherStarts < 2;
        attempt++
      ) {
        await tester.pump(const Duration(milliseconds: 150));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 30)),
        );
      }
      await tester.pumpAndSettle();
      expect(service.explorerWatcherStops, greaterThanOrEqualTo(1));
      expect(service.explorerWatcherStarts, 2);
      expect(find.text('created.dart'), findsOneWidget);

      service.childrenByDirectory[''] = [
        _file('before.dart'),
        _file('created.dart'),
        _file('after.dart'),
      ];
      service.emitWatchBatch([''], changedRelativePaths: ['after.dart']);
      await _pumpWatchedRefresh(tester);
      expect(find.text('after.dart'), findsOneWidget);
    },
  );
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
    expect(statusCalls(), baseline);
    await tester.pump(const Duration(milliseconds: 130));
    expect(statusCalls(), baseline + 1);

    gate.complete();
    await tester.pumpAndSettle();

    expect(statusCalls() - baseline, lessThanOrEqualTo(2));
    expect(
      service.listChildrenCalls.length - baselineListCalls,
      lessThanOrEqualTo(5),
    );
  });

  testWidgets('Quick Open coalesces collapsed-folder notification storms', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ];
    await _pumpExplorer(tester, service);
    final indexRequestsBefore = service.quickOpenRefreshRequests.length;
    final directoryReadsBefore = service.listChildrenCalls.length;
    for (var index = 0; index < 100; index++) {
      service.emitWatchBatch(
        const <String>[],
        changedRelativePaths: const <String>['src/deep/example.dart'],
      );
    }
    await _pumpWatchedRefresh(tester);

    expect(
      service.quickOpenRefreshRequests.length - indexRequestsBefore,
      1,
      reason: 'A file storm should not request 100 separate index rebuilds',
    );
    expect(service.listChildrenCalls.length, directoryReadsBefore);
  });

  testWidgets('Quick Open refreshes during continuous watch notifications', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ];
    await _pumpExplorer(tester, service);
    final before = service.quickOpenRefreshRequests.length;
    for (var index = 0; index < 30; index++) {
      service.emitWatchBatch(
        const <String>[],
        changedRelativePaths: const <String>['src/deep/changed.dart'],
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 40));
    }

    expect(
      service.quickOpenRefreshRequests.length - before,
      inInclusiveRange(1, 3),
      reason: 'The 750-ms maximum wait must prevent index starvation',
    );
    await _pumpWatchedRefresh(tester);
    expect(
      service.quickOpenRefreshRequests.length - before,
      inInclusiveRange(2, 4),
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
      await _pumpWatchedRefresh(tester);

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

  testWidgets(
    'native metadata-only Delta refreshes row status and keeps bindings',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/a.dart'),
          _file('src/b.dart'),
        ];
      await _pumpExplorer(tester, service);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      final initial = service.nativeProjectionUpdates;
      final originalTree = tester
          .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
          .controller
          .data;
      expect(find.text('M'), findsNothing);

      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/a.dart', gitStatus: native.WorkspaceFileGitStatus.modified),
        _file('src/b.dart'),
      ];
      service.emitWatchBatch(<String>['src']);
      await _pumpWatchedRefresh(tester);
      expect(service.nativeProjectionUpdates, initial + 1);
      expect(service.projectionCalls, 0);
      expect(find.text('M'), findsOneWidget);
      expect(
        tester
            .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
            .controller
            .data,
        same(originalTree),
        reason: 'Git metadata changes must not rebuild the entire TreeData',
      );
      expect(find.text('a.dart'), findsOneWidget);
      expect(find.text('b.dart'), findsOneWidget);

      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/a.dart'),
        _file('src/b.dart'),
      ];
      service.emitWatchBatch(<String>['src']);
      await _pumpWatchedRefresh(tester);
      expect(find.text('M'), findsNothing);
      expect(
        tester
            .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
            .controller
            .data,
        same(originalTree),
      );
      expect(find.text('b.dart'), findsOneWidget);
      expect(service.projectionCalls, 0);
    },
  );

  testWidgets('scroll position stays fixed after metadata-only refresh', (
    tester,
  ) async {
    final initial = <native.WorkspaceFileEntry>[
      for (var i = 0; i < 180; i++) _file('row_$i.dart'),
    ];
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = initial;
    await _pumpExplorer(tester, service);
    final view = find.byType(tree.DirectoryTreeView);
    final treeBefore = tester
        .widget<tree.DirectoryTreeView>(view)
        .controller
        .data;
    final scrollable = find
        .descendant(of: view, matching: find.byType(Scrollable))
        .first;
    await tester.drag(scrollable, const Offset(0, -640));
    await tester.pumpAndSettle();
    final state = tester.state<ScrollableState>(scrollable);
    final offset = state.position.pixels;
    expect(offset, greaterThan(0));
    service.childrenByDirectory[''] = [
      for (var i = 0; i < 180; i++)
        _file(
          'row_$i.dart',
          gitStatus: i == 30 ? native.WorkspaceFileGitStatus.modified : null,
        ),
    ];
    service.emitWatchBatch(<String>['']);
    await _pumpWatchedRefresh(tester);
    expect(
      tester.widget<tree.DirectoryTreeView>(view).controller.data,
      same(treeBefore),
    );
    expect(state.position.pixels, closeTo(offset, 0.5));
    expect(service.projectionCalls, 0);
  });
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
    final originalTree = tester
        .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
        .controller
        .data;

    final unchangedFileNode = originalTree.nodes['path:src/main.dart'];
    final changedParentNode = originalTree.nodes['path:src'];

    service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
      _file('src/main.dart'),
      _file('src/extra.dart'),
    ];
    service.emitWatchBatch(<String>['src']);
    await _pumpWatchedRefresh(tester);

    expect(service.nativeProjectionUpdates, projected + 1);
    expect(service.projectionCalls, 0);
    expect(
      tester
          .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
          .controller
          .data,
      isNot(same(originalTree)),
      reason: 'Adding a file still requires a new structural TreeData',
    );
    final newTree = tester
        .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
        .controller
        .data;
    expect(
      newTree.nodes['path:src/main.dart'],
      same(unchangedFileNode),
      reason: 'Structural Delta should reuse untouched Dart TreeNode objects',
    );
    expect(
      newTree.nodes['path:src'],
      isNot(same(changedParentNode)),
      reason: 'Changed folder child order must replace its Dart TreeNode',
    );
    expect(find.text('extra.dart'), findsOneWidget);
  });

  testWidgets(
    'bulk structural Delta reuses untouched nodes across 150 file adds and removals',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          _directory('src', hasChildrenHint: true),
          _file('persistent.md'),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/base.dart'),
        ];
      await _pumpExplorer(tester, service);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      final view = find.byType(tree.DirectoryTreeView);
      final before = tester
          .widget<tree.DirectoryTreeView>(view)
          .controller
          .data;
      final persistent = before.nodes['path:persistent.md'];
      final base = before.nodes['path:src/base.dart'];

      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/base.dart'),
        for (var i = 0; i < 150; i++) _file('src/batch_$i.dart'),
      ];
      service.emitWatchBatch(<String>['src']);
      await _pumpWatchedRefresh(tester);
      final added = tester.widget<tree.DirectoryTreeView>(view).controller.data;
      expect(added.nodes.length, before.nodes.length + 150);
      expect(added.nodes['path:persistent.md'], same(persistent));
      expect(added.nodes['path:src/base.dart'], same(base));
      expect(service.projectionCalls, 0);

      service.childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/base.dart'),
      ];
      service.emitWatchBatch(<String>['src']);
      await _pumpWatchedRefresh(tester);
      final removed = tester
          .widget<tree.DirectoryTreeView>(view)
          .controller
          .data;
      expect(removed.nodes.length, before.nodes.length);
      expect(removed.nodes.containsKey('path:src/batch_0.dart'), isFalse);
      expect(removed.nodes['path:persistent.md'], same(persistent));
      expect(removed.nodes['path:src/base.dart'], same(base));
      expect(service.projectionCalls, 0);
    },
  );

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
      await _pumpWatchedRefresh(tester);
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
    final beforeTree = tester
        .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
        .controller
        .data;
    final siblingNode = beforeTree.nodes['path:b/stable.dart'];
    service.childrenByDirectory['a'] = <native.WorkspaceFileEntry>[
      _file('a/new.dart'),
    ];
    service.emitWatchBatch(<String>['a']);
    await _pumpWatchedRefresh(tester);
    final afterTree = tester
        .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
        .controller
        .data;
    expect(afterTree.nodes['path:b/stable.dart'], same(siblingNode));
    expect(afterTree.nodes.containsKey('path:a/old.dart'), isFalse);
    expect(afterTree.nodes.containsKey('path:a/new.dart'), isTrue);
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
    final originalTree = tester
        .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
        .controller
        .data;

    // Native listing returns new instances even when all fields are equal.
    service.childrenByDirectory['src'] = [_file('src/main.dart')];
    service.emitWatchBatch(<String>['src']);
    await _pumpWatchedRefresh(tester);

    expect(service.watchedPathUpdates, hasLength(updatesAfterExpand));
    expect(
      tester
          .widget<tree.DirectoryTreeView>(find.byType(tree.DirectoryTreeView))
          .controller
          .data,
      same(originalTree),
      reason: 'Unchanged watcher events must not rebuild TreeData',
    );
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
