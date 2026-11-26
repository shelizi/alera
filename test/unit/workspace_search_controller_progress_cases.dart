part of 'workspace_search_controller_test.dart';

void _registerWorkspaceSearchProgressTests() {
  test('partial batches appear before completion and cannot replace', () async {
    final service = _StreamingSearchService();
    final container = ProviderContainer(
      overrides: [workspaceSearchServiceProvider.overrideWithValue(service)],
    );
    addTearDown(container.dispose);
    final provider = workspaceSearchControllerProvider('workspace-1');
    final controller = container.read(provider.notifier);
    controller.setQuery('/workspace', 'needle');
    final pending = controller.searchNow('/workspace');
    final stream = service.streams.single;
    stream.add(
      native.WorkspaceSearchBatch(
        files: [_multiFileSearchResult.files.first],
        totalMatches: 1,
        truncated: false,
        done: false,
      ),
    );
    await Future<void>.delayed(Duration.zero);
    final first = container.read(provider);
    expect(first.loading, isTrue);
    expect(first.result?.totalMatches, 1);
    expect(first.result?.files.single.relativePath, 'lib/main.dart');
    await expectLater(
      controller.replaceMatches(
        workspacePath: '/workspace',
        matchIds: const [],
        editorSessions: EditorSessionRegistry(),
      ),
      throwsStateError,
    );
    expect(service.replaceCalls, 0);
    stream.add(
      native.WorkspaceSearchBatch(
        files: [_multiFileSearchResult.files.last],
        totalMatches: 2,
        truncated: false,
        done: false,
      ),
    );
    await Future<void>.delayed(Duration.zero);
    expect(container.read(provider).result?.totalMatches, 2);
    expect(
      first.result?.files,
      hasLength(1),
      reason: 'earlier state remains immutable',
    );
    controller.toggleFileCollapsed('lib/main.dart');
    stream.add(
      const native.WorkspaceSearchBatch(
        files: [],
        totalMatches: 2,
        truncated: true,
        done: true,
      ),
    );
    await Future<void>.delayed(Duration.zero);
    await pending.timeout(const Duration(seconds: 5));
    final result = container.read(provider);
    expect(result.loading, isFalse);
    expect(result.result?.truncated, isTrue);
    expect(result.result?.files, hasLength(2));
    expect(
      result.collapsedResultNodeKeys,
      contains(workspaceSearchFileNodeKey('lib/main.dart')),
    );
    expect(service.cancelledSubscriptions, [0]);
    await stream.close();
  });

  test(
    'changing query cancels stream and never accepts late old batches',
    () async {
      final service = _StreamingSearchService();
      final container = ProviderContainer(
        overrides: [workspaceSearchServiceProvider.overrideWithValue(service)],
      );
      addTearDown(container.dispose);
      final provider = workspaceSearchControllerProvider('workspace-1');
      final controller = container.read(provider.notifier);
      controller.setQuery('/workspace', 'first');
      final old = controller.searchNow('/workspace');
      service.streams.first.add(
        native.WorkspaceSearchBatch(
          files: [_multiFileSearchResult.files.first],
          totalMatches: 1,
          truncated: false,
          done: false,
        ),
      );
      await Future<void>.delayed(Duration.zero);
      expect(container.read(provider).result?.totalMatches, 1);
      controller.setQuery('/workspace', 'second');
      expect(container.read(provider).result, isNull);
      final current = controller.searchNow('/workspace');
      service.streams.first.add(
        native.WorkspaceSearchBatch(
          files: [_multiFileSearchResult.files.last],
          totalMatches: 2,
          truncated: false,
          done: true,
        ),
      );
      service.streams.last.add(
        native.WorkspaceSearchBatch(
          files: [_multiFileSearchResult.files.last],
          totalMatches: 1,
          truncated: false,
          done: true,
        ),
      );
      await Future<void>.delayed(Duration.zero);
      await old.timeout(const Duration(seconds: 5));
      await current.timeout(const Duration(seconds: 5));
      expect(
        container.read(provider).result?.files.single.relativePath,
        'lib/other.dart',
      );
      expect(container.read(provider).loading, isFalse);
      expect(service.cancelledRequests, [service.requestIds.first]);
      expect(service.cancelledSubscriptions, [0, 1]);
      for (final stream in service.streams) {
        await stream.close();
      }
    },
  );

  test(
    'terminal result does not wait for slow subscription teardown',
    () async {
      final service = _StreamingSearchService()..cancelGate = Completer<void>();
      final container = ProviderContainer(
        overrides: [workspaceSearchServiceProvider.overrideWithValue(service)],
      );
      addTearDown(container.dispose);
      final provider = workspaceSearchControllerProvider('workspace-1');
      final controller = container.read(provider.notifier);
      controller.setQuery('/workspace', 'needle');
      final pending = controller.searchNow('/workspace');
      final stream = service.streams.single;
      stream.add(
        const native.WorkspaceSearchBatch(
          files: [],
          totalMatches: 0,
          truncated: false,
          done: true,
        ),
      );
      await pending.timeout(const Duration(seconds: 5));
      expect(container.read(provider).loading, isFalse);
      expect(service.cancelledSubscriptions, [0]);
      expect(service.cancelGate!.isCompleted, isFalse);
      service.cancelGate!.complete();
      await stream.close();
    },
  );

  for (final scenario in [
    'missing terminal',
    'native error',
    'duplicate',
    'wrong count',
  ]) {
    test('partial results are cleared after $scenario', () async {
      final service = _StreamingSearchService();
      final container = ProviderContainer(
        overrides: [workspaceSearchServiceProvider.overrideWithValue(service)],
      );
      addTearDown(container.dispose);
      final provider = workspaceSearchControllerProvider('workspace-1');
      final controller = container.read(provider.notifier);
      controller.setQuery('/workspace', 'needle');
      final pending = controller.searchNow('/workspace');
      final stream = service.streams.single;
      final first = _multiFileSearchResult.files.first;
      stream.add(
        native.WorkspaceSearchBatch(
          files: [first],
          totalMatches: 1,
          truncated: false,
          done: false,
        ),
      );
      await Future<void>.delayed(Duration.zero);
      expect(container.read(provider).result?.totalMatches, 1);
      switch (scenario) {
        case 'missing terminal':
          unawaited(stream.close());
        case 'native error':
          stream.add(
            const native.WorkspaceSearchBatch(
              files: [],
              totalMatches: 0,
              truncated: false,
              done: true,
              error: native.WorkspaceSearchError(
                kind: native.WorkspaceSearchErrorKind.io,
                context: 'read failed',
              ),
            ),
          );
        case 'duplicate':
          stream.add(
            native.WorkspaceSearchBatch(
              files: [first],
              totalMatches: 2,
              truncated: false,
              done: true,
            ),
          );
        case 'wrong count':
          stream.add(
            const native.WorkspaceSearchBatch(
              files: [],
              totalMatches: 2,
              truncated: false,
              done: true,
            ),
          );
      }
      await Future<void>.delayed(Duration.zero);
      await pending.timeout(const Duration(seconds: 5));
      final state = container.read(provider);
      expect(state.result, isNull);
      expect(state.error, isNotNull);
      expect(state.loading, isFalse);
      await stream.close();
    });
  }

  test('empty terminal completes a no-match search', () async {
    final service = _StreamingSearchService();
    final container = ProviderContainer(
      overrides: [workspaceSearchServiceProvider.overrideWithValue(service)],
    );
    addTearDown(container.dispose);
    final provider = workspaceSearchControllerProvider('workspace-1');
    final controller = container.read(provider.notifier);
    controller.setQuery('/workspace', 'needle');
    final pending = controller.searchNow('/workspace');
    service.streams.single.add(
      const native.WorkspaceSearchBatch(
        files: [],
        totalMatches: 0,
        truncated: false,
        done: true,
      ),
    );
    await Future<void>.delayed(Duration.zero);
    await pending.timeout(const Duration(seconds: 5));
    expect(container.read(provider).result?.files, isEmpty);
    expect(container.read(provider).loading, isFalse);
    expect(container.read(provider).error, isNull);
    await service.streams.single.close();
  });
}

class _StreamingSearchService extends _FakeWorkspaceSearchService {
  final streams = <StreamController<native.WorkspaceSearchBatch>>[];
  final requestIds = <String>[];
  final cancelledRequests = <String>[];
  final cancelledSubscriptions = <int>[];
  Completer<void>? cancelGate;

  @override
  Stream<native.WorkspaceSearchBatch> searchBatches({
    required native.WorkspaceSearchOptions options,
    required String requestId,
  }) {
    final index = streams.length;
    final stream = StreamController<native.WorkspaceSearchBatch>(
      onCancel: () {
        cancelledSubscriptions.add(index);
        return cancelGate?.future;
      },
    );
    streams.add(stream);
    requestIds.add(requestId);
    return stream.stream;
  }

  @override
  Future<void> cancel({required String requestId}) async {
    cancelledRequests.add(requestId);
  }
}
