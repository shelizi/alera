part of 'terminal_runtime_native_test.dart';

void _registerTerminalRuntimeViewBufferOwnerTests() {
  group('TerminalRuntimeViewBufferOwner', () {
    test('unbounded budget does not evict any sessions', () {
      final host = _FakeViewBufferOwnerHost();
      host.terminalSettings = TerminalSettings.defaults.copyWith(
        bufferBudgetMegabytes: 0,
      );
      final s1 = _TestBufferSessionHandle(
        tabId: 'tab-1',
        workspaceId: 'ws-1',
        bufferBytes: 50 * 1024 * 1024,
      );
      final s2 = _TestBufferSessionHandle(
        tabId: 'tab-2',
        workspaceId: 'ws-1',
        bufferBytes: 50 * 1024 * 1024,
      );
      host.sessions.addAll(<TerminalSessionHandle>[s1, s2]);

      final owner = TerminalRuntimeViewBufferOwner(host);
      owner.enforceBufferBudget();

      expect(host.evictedTabIds, isEmpty);
    });

    test('evicts coldest unpinned sessions when budget is exceeded', () {
      final host = _FakeViewBufferOwnerHost();
      host.terminalSettings = TerminalSettings.defaults.copyWith(
        bufferBudgetMegabytes: 1, // 1 MB
      );
      final now = DateTime.now();
      final cold = _TestBufferSessionHandle(
        tabId: 'cold-tab',
        workspaceId: 'ws-1',
        bufferBytes: 800 * 1024,
        lastVisibleAt: now.subtract(const Duration(minutes: 10)),
      );
      final warm = _TestBufferSessionHandle(
        tabId: 'warm-tab',
        workspaceId: 'ws-1',
        bufferBytes: 800 * 1024,
        lastVisibleAt: now.subtract(const Duration(minutes: 1)),
      );
      host.sessions.addAll(<TerminalSessionHandle>[cold, warm]);

      final owner = TerminalRuntimeViewBufferOwner(host);
      owner.enforceBufferBudget();

      expect(host.evictedTabIds, equals(<String>['cold-tab']));
    });

    test('pinned visible sessions are never evicted', () {
      final host = _FakeViewBufferOwnerHost();
      host.terminalSettings = TerminalSettings.defaults.copyWith(
        bufferBudgetMegabytes: 1,
      );
      final visible = _TestBufferSessionHandle(
        tabId: 'visible-tab',
        workspaceId: 'ws-1',
        initialVisible: true,
        bufferBytes: 2 * 1024 * 1024,
      );
      host.sessions.add(visible);

      final owner = TerminalRuntimeViewBufferOwner(host);
      owner.enforceBufferBudget();

      expect(host.evictedTabIds, isEmpty);
    });

    test(
      'setActiveWorkspace updates activeWorkspaceId and triggers eviction',
      () {
        final host = _FakeViewBufferOwnerHost();
        host.terminalSettings = TerminalSettings.defaults.copyWith(
          bufferBudgetMegabytes: 1,
        );
        final session = _TestBufferSessionHandle(
          tabId: 'tab-1',
          workspaceId: 'ws-1',
          bufferBytes: 2 * 1024 * 1024,
        );
        host.sessions.add(session);

        final owner = TerminalRuntimeViewBufferOwner(host);
        expect(owner.activeWorkspaceId, isNull);

        owner.setActiveWorkspace('ws-1');
        expect(owner.activeWorkspaceId, 'ws-1');
        expect(host.evictedTabIds, contains('tab-1'));
      },
    );

    test('setAppForeground updates isAppForeground', () {
      final host = _FakeViewBufferOwnerHost();
      final owner = TerminalRuntimeViewBufferOwner(host);
      expect(owner.isAppForeground, isTrue);

      owner.setAppForeground(false);
      expect(owner.isAppForeground, isFalse);

      owner.setAppForeground(true);
      expect(owner.isAppForeground, isTrue);
    });

    test(
      'handleVisibilityChanged ignores visible and triggers eviction on hidden',
      () {
        final host = _FakeViewBufferOwnerHost();
        host.terminalSettings = TerminalSettings.defaults.copyWith(
          bufferBudgetMegabytes: 1,
        );
        final session = _TestBufferSessionHandle(
          tabId: 'tab-1',
          workspaceId: 'ws-1',
          bufferBytes: 2 * 1024 * 1024,
          initialVisible: true,
        );
        host.sessions.add(session);

        final owner = TerminalRuntimeViewBufferOwner(host);
        owner.handleVisibilityChanged(session);
        expect(host.evictedTabIds, isEmpty);

        session._visible = false;
        owner.handleVisibilityChanged(session);
        expect(host.evictedTabIds, contains('tab-1'));
      },
    );
  });
}

final class _FakeViewBufferOwnerHost
    implements TerminalRuntimeViewBufferOwnerHost {
  @override
  TerminalSettings terminalSettings = TerminalSettings.defaults;
  final List<TerminalSessionHandle> sessions = <TerminalSessionHandle>[];
  final List<String> evictedTabIds = <String>[];

  @override
  Iterable<TerminalSessionHandle> get liveSessions => sessions;

  @override
  void evictSession(String tabId) {
    evictedTabIds.add(tabId);
  }
}

final class _TestBufferSessionHandle extends TerminalSessionHandle {
  _TestBufferSessionHandle({
    required this.tabId,
    required this.workspaceId,
    bool initialVisible = false,
    this.bufferBytes = 1000,
    this.lastVisibleAt,
  }) : _visible = initialVisible;

  @override
  final String tabId;
  @override
  final String workspaceId;
  final int bufferBytes;
  final DateTime? lastVisibleAt;
  bool _visible;

  @override
  bool get isVisible => _visible;

  @override
  TerminalBufferUsage get bufferUsage => TerminalBufferUsage(
    tabId: tabId,
    bytes: bufferBytes,
    lastVisibleAt: lastVisibleAt,
  );

  @override
  String get displayTitle => 'Test';

  @override
  ValueListenable<String> get titleListenable => ValueNotifier<String>('Test');

  @override
  bool get isRunning => true;

  @override
  bool get isStarting => false;

  @override
  String? get errorMessage => null;

  @override
  Future<void> ensureStarted() async {}

  @override
  Future<void> restart() async {}

  @override
  TerminalVisibilityLease acquireVisibility() =>
      const NoopTerminalVisibilityLease();

  @override
  Widget buildView({
    Key? key,
    bool autofocus = false,
    FocusOnKeyEventCallback? onKeyEvent,
  }) => const SizedBox.shrink();

  @override
  void requestFocus() {}
}
