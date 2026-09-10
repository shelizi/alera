import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('clean preview is offered as the replacement candidate', () async {
    final events = <String>[];
    final preview = _tab('preview', path: 'lib/a.dart', preview: true);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[preview.id],
    );
    final editorSessions = _FakeEditorSessions(events: events);
    final coordinator = WorkbenchReplaceableTabOpenCoordinator(
      editorSessions: editorSessions,
    );

    final result = await coordinator.open(
      workspaceId: 'workspace',
      previousTabs: <WorkspaceTabRecord>[preview],
      layout: layout,
      targetGroupId: layout.activeGroupId,
      preview: true,
      keepPreviewTab: (tabId) async {
        events.add('pin:$tabId');
        return preview.copyWith(payload: const <String, Object?>{});
      },
      onPreviewPinned: (tab) {
        events.add('pinned:${tab.id}');
        return <WorkspaceTabRecord>[tab];
      },
      createTab:
          ({
            required workspaceId,
            required preview,
            replacePreviewTabId,
          }) async {
            events.add('create:$replacePreviewTabId');
            return _tab('preview', path: 'lib/b.dart', preview: true);
          },
    );

    expect(events, <String>[
      'dirty:preview',
      'create:preview',
      'forget:preview',
    ]);
    expect(result.tab.filePath, 'lib/b.dart');
    expect(result.plan.tabs.single.filePath, 'lib/b.dart');
  });

  test('dirty preview is pinned before create and is not replaced', () async {
    final events = <String>[];
    final preview = _tab('preview', path: 'lib/a.dart', preview: true);
    final pinned = _tab('preview', path: 'lib/a.dart');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[preview.id],
    );
    final editorSessions = _FakeEditorSessions(
      events: events,
      dirtyTabIds: <String>{preview.id},
    );
    final coordinator = WorkbenchReplaceableTabOpenCoordinator(
      editorSessions: editorSessions,
    );

    final result = await coordinator.open(
      workspaceId: 'workspace',
      previousTabs: <WorkspaceTabRecord>[preview],
      layout: layout,
      targetGroupId: layout.activeGroupId,
      preview: true,
      keepPreviewTab: (tabId) async {
        events.add('pin:$tabId');
        return pinned;
      },
      onPreviewPinned: (tab) {
        events.add('pinned:${tab.id}');
        return <WorkspaceTabRecord>[tab];
      },
      createTab:
          ({
            required workspaceId,
            required preview,
            replacePreviewTabId,
          }) async {
            events.add('create:$replacePreviewTabId');
            return _tab('new-tab', path: 'lib/b.dart', preview: true);
          },
    );

    expect(events, <String>[
      'dirty:preview',
      'pin:preview',
      'pinned:preview',
      'create:null',
    ]);
    expect(result.plan.tabs.map((tab) => tab.id), <String>[
      'preview',
      'new-tab',
    ]);
    expect(result.plan.tabs.first.isPreview, isFalse);
  });

  test('dirty preview remains pinned when later create fails', () async {
    final events = <String>[];
    final preview = _tab('preview', path: 'lib/a.dart', preview: true);
    final pinned = _tab('preview', path: 'lib/a.dart');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[preview.id],
    );
    final coordinator = WorkbenchReplaceableTabOpenCoordinator(
      editorSessions: _FakeEditorSessions(
        events: events,
        dirtyTabIds: <String>{preview.id},
      ),
    );

    await expectLater(
      coordinator.open(
        workspaceId: 'workspace',
        previousTabs: <WorkspaceTabRecord>[preview],
        layout: layout,
        targetGroupId: layout.activeGroupId,
        preview: true,
        keepPreviewTab: (tabId) async {
          events.add('pin:$tabId');
          return pinned;
        },
        onPreviewPinned: (tab) {
          events.add('pinned:${tab.id}');
          return <WorkspaceTabRecord>[tab];
        },
        createTab:
            ({
              required workspaceId,
              required preview,
              replacePreviewTabId,
            }) async {
              events.add('create:$replacePreviewTabId');
              throw StateError('create failed');
            },
      ),
      throwsStateError,
    );

    expect(events, <String>[
      'dirty:preview',
      'pin:preview',
      'pinned:preview',
      'create:null',
    ]);
  });

  test(
    'retargeting a reused tab forgets its previous editor session',
    () async {
      final events = <String>[];
      final existing = _tab('preview', path: 'lib/a.dart', preview: true);
      final layout = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[existing.id],
      );
      final coordinator = WorkbenchReplaceableTabOpenCoordinator(
        editorSessions: _FakeEditorSessions(events: events),
      );

      final result = await coordinator.open(
        workspaceId: 'workspace',
        previousTabs: <WorkspaceTabRecord>[existing],
        layout: layout,
        targetGroupId: layout.activeGroupId,
        preview: true,
        keepPreviewTab: (_) async => existing,
        onPreviewPinned: (_) => <WorkspaceTabRecord>[existing],
        createTab:
            ({
              required workspaceId,
              required preview,
              replacePreviewTabId,
            }) async {
              return _tab('preview', path: 'lib/b.dart', preview: true);
            },
      );

      expect(events, <String>['dirty:preview', 'forget:preview']);
      expect(result.plan.shouldForgetEditorSession, isTrue);
    },
  );

  test('permanent open never inspects or replaces a preview slot', () async {
    final events = <String>[];
    final preview = _tab('preview', path: 'lib/a.dart', preview: true);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[preview.id],
    );
    final coordinator = WorkbenchReplaceableTabOpenCoordinator(
      editorSessions: _FakeEditorSessions(
        events: events,
        dirtyTabIds: <String>{preview.id},
      ),
    );

    await coordinator.open(
      workspaceId: 'workspace',
      previousTabs: <WorkspaceTabRecord>[preview],
      layout: layout,
      targetGroupId: layout.activeGroupId,
      preview: false,
      keepPreviewTab: (tabId) async {
        events.add('pin:$tabId');
        return preview;
      },
      onPreviewPinned: (tab) {
        events.add('pinned:${tab.id}');
        return <WorkspaceTabRecord>[tab];
      },
      createTab:
          ({
            required workspaceId,
            required preview,
            replacePreviewTabId,
          }) async {
            events.add('create:$replacePreviewTabId');
            return _tab('new-tab', path: 'lib/b.dart');
          },
    );

    expect(events, <String>['create:null']);
  });
}

final class _FakeEditorSessions
    implements WorkbenchReplaceableTabEditorSessions {
  _FakeEditorSessions({
    required this.events,
    this.dirtyTabIds = const <String>{},
  });

  final List<String> events;
  final Set<String> dirtyTabIds;

  @override
  bool isDirty(String tabId) {
    events.add('dirty:$tabId');
    return dirtyTabIds.contains(tabId);
  }

  @override
  void forget(String tabId) => events.add('forget:$tabId');
}

WorkspaceTabRecord _tab(
  String id, {
  required String path,
  bool preview = false,
}) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    kind: WorkspaceTabKind.editor,
    title: id,
    createdAt: now,
    updatedAt: now,
    payload: <String, Object?>{
      workspaceTabFilePathPayloadKey: path,
      if (preview) workspaceTabPreviewPayloadKey: true,
    },
  );
}
