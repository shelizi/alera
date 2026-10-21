import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator_provider.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/workspace_folder_opener.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workbench_dialog_launchers.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;

void main() {
  testWidgets(
    'Ctrl+P results share Explorer file menu and retain Enter opening',
    (tester) async {
      String? copied;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied =
                (call.arguments as Map<Object?, Object?>)['text'] as String?;
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );

      final workspace = _workspace('workspace-1', 'Main', '/repo/main');
      final controller = _QuickOpenTestController(_state(workspace));
      await _pumpQuickOpen(
        tester,
        controller: controller,
        service: _QuickOpenFileService(
          entries: <String>['lib/first.dart', 'lib/second.dart'],
        ),
        folderOpener: _QuickOpenFolderOpener(),
        externalEditorLauncher: _QuickOpenExternalEditorLauncher(),
      );
      await tester.tap(find.text('Open Quick Open'));
      await tester.pumpAndSettle();

      await tester.tap(
        find.text('lib/second.dart'),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      for (final label in <String>[
        'New file',
        'New folder',
        'Copy',
        'Cut',
        'Paste',
        'Copy path',
        'Copy relative path',
        'Duplicate',
      ]) {
        expect(find.text(label), findsOneWidget, reason: label);
      }
      await _quickOpenMenuScrollTo(tester, 'Open in Alera');
      expect(find.text('Open in Zed'), findsOneWidget);
      expect(find.text('Open in Alera'), findsOneWidget);
      await _quickOpenMenuScrollTo(tester, 'View File History');
      expect(find.text('View File History'), findsOneWidget);
      await _quickOpenMenuScrollTo(tester, 'Compare With Branch...');
      expect(find.text('Compare With Branch...'), findsOneWidget);
      await _quickOpenMenuScrollTo(tester, 'Delete');
      expect(find.text('Delete'), findsOneWidget);
      // Tapping outside the menu must not close Quick Open.
      await tester.tapAt(const Offset(760, 120));
      await tester.pumpAndSettle();

      await tester.tap(
        find.text('lib/second.dart'),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Copy relative path'));
      await tester.pumpAndSettle();
      expect(copied, 'lib/second.dart');
      expect(find.text('Quick Open'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(controller.openedFiles, <String>['lib/second.dart']);
    },
  );

  testWidgets('Ctrl+P can explicitly open file in Alera from right-click', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final launcher = _QuickOpenExternalEditorLauncher();
    await _pumpQuickOpen(
      tester,
      controller: controller,
      service: _QuickOpenFileService(entries: <String>['deep/main.dart']),
      externalEditorLauncher: launcher,
    );
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    await tester.tap(
      find.text('deep/main.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await _quickOpenMenuScrollTo(tester, 'Open in Alera');
    await tester.tap(find.text('Open in Alera'));
    await tester.pumpAndSettle();
    expect(controller.openedFiles, <String>['deep/main.dart']);
    expect(launcher.fileRequests, isEmpty);
  });

  testWidgets('Ctrl+P context copy/paste supports multi-selected files', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      entries: <String>['src/a.dart', 'src/b.dart', 'dest/c.dart'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('src/a.dart'));
    await tester.tap(find.text('src/b.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    expect(controller.openedFiles, isEmpty);

    await tester.tap(find.text('src/a.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Copy'));
    await tester.pumpAndSettle();
    expect(service.clipboardWrittenPaths.map(p.basename), ['a.dart', 'b.dart']);

    await tester.tap(find.text('dest/c.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Paste'));
    await tester.pumpAndSettle();
    expect(service.copiedEntries, <String>[
      'src/a.dart->dest',
      'src/b.dart->dest',
    ]);
    expect(service.refreshedWorkspacePaths, contains('/repo/main'));
  });

  testWidgets('Ctrl+P rename and delete use original nested file paths', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      entries: <String>['lib/deep/before.dart', 'lib/deep/remove.dart'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('lib/deep/before.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await _quickOpenMenuScrollTo(tester, 'Rename');
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextField),
      ),
      'after.dart',
    );
    await tester.tap(find.text('Create'));
    await tester.pumpAndSettle();
    expect(service.renamedEntries, <String>[
      'lib/deep/before.dart->after.dart',
    ]);
    expect(controller.movedFileTabs, <String>[
      'lib/deep/before.dart->lib/deep/after.dart',
    ]);

    await tester.tap(
      find.text('lib/deep/remove.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await _quickOpenMenuScrollTo(tester, 'Delete');
    await tester.tap(find.text('Delete'));
    await tester.pumpAndSettle();
    expect(find.text('Delete remove.dart'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();
    expect(service.deletedEntries, <String>['lib/deep/remove.dart']);
    expect(service.refreshedWorkspacePaths, hasLength(2));
  });

  testWidgets('Ctrl+P menu opens system app and selected external editor', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      entries: <String>['lib/deep/entry.dart'],
    );
    final folderOpener = _QuickOpenFolderOpener();
    final launcher = _QuickOpenExternalEditorLauncher();
    await _pumpQuickOpen(
      tester,
      controller: controller,
      service: service,
      folderOpener: folderOpener,
      externalEditorLauncher: launcher,
    );
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('lib/deep/entry.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await _quickOpenMenuScrollTo(tester, 'Open with Default Application');
    await tester.tap(find.text('Open with Default Application'));
    await tester.pumpAndSettle();
    expect(
      folderOpener.openedPaths.single,
      p.join('/repo/main', 'lib', 'deep', 'entry.dart'),
    );

    await tester.tap(
      find.text('lib/deep/entry.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await _quickOpenMenuScrollTo(tester, 'Open in Zed');
    await tester.tap(find.text('Open in Zed'));
    await tester.pumpAndSettle();
    expect(launcher.fileRequests, hasLength(1));
    expect(
      launcher.fileRequests.single.filePath,
      p.join('/repo/main', 'lib', 'deep', 'entry.dart'),
    );
  });

  testWidgets('Ctrl+P creates a file beside selected search result', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      entries: <String>['lib/deep/existing.dart'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    await tester.tap(
      find.text('lib/deep/existing.dart'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('New file'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextField),
      ),
      'added.dart',
    );
    await tester.tap(find.text('Create'));
    await tester.pumpAndSettle();
    expect(service.createdEntries, <String>['lib/deep/added.dart']);
  });

  testWidgets('shows loading, filters files, and opens the selected file', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final session = Completer<native.WorkspaceQuickOpenSession>();
    final service = _QuickOpenFileService(
      session: session,
      entries: <String>['lib/main.dart', 'lib/main_test.dart', 'notes.txt'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pump();
    expect(find.text('Loading workspace files...'), findsOneWidget);

    session.complete(_session('workspace-1', 3));
    await tester.pumpAndSettle();
    expect(find.text('lib/main.dart'), findsOneWidget);
    expect(find.text('lib/main_test.dart'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'main_test.dart');
    await tester.pump();
    expect(find.text('lib/main.dart'), findsNothing);
    expect(find.text('lib/main_test.dart'), findsOneWidget);

    await tester.sendKeyEvent(.enter);
    await tester.pumpAndSettle();
    expect(controller.openedFiles, <String>['lib/main_test.dart']);
    expect(controller.openedPreviews, <bool>[false]);
  });

  testWidgets('shows cached results while the background index refreshes', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final refresh = Completer<List<native.WorkspaceQuickOpenMatch>>();
    final service = _QuickOpenFileService(
      entries: <String>['fresh.dart'],
      cachedEntries: <String>['cached.dart'],
      searchGates: <String, Completer<List<native.WorkspaceQuickOpenMatch>>>{
        '': refresh,
      },
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pump();

    expect(find.text('cached.dart'), findsOneWidget);
    expect(find.text('Loading workspace files...'), findsNothing);

    refresh.complete(<native.WorkspaceQuickOpenMatch>[_match('fresh.dart')]);
    await tester.pumpAndSettle();

    expect(find.text('cached.dart'), findsNothing);
    expect(find.text('fresh.dart'), findsOneWidget);
  });

  testWidgets('keeps cached results usable when the background index fails', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      cachedEntries: const <String>['cached.dart'],
      error: const native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.io,
        context: 'Temporary indexing failure',
      ),
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();

    expect(find.text('cached.dart'), findsOneWidget);
    expect(find.text('Could not load workspace files.'), findsNothing);
  });

  testWidgets(
    'routes editable files externally when it is the default target',
    (tester) async {
      final workspace = _workspace('workspace-1', 'Main', r'C:\repo\main');
      final controller = _QuickOpenTestController(_state(workspace));
      final launcher = _QuickOpenExternalEditorLauncher();
      final coordinator = WorkspaceFileOpenCoordinator(
        externalEditorLauncher: launcher,
        defaultTargetReader: () => CodeOpenTarget.external,
      );
      await _pumpQuickOpen(
        tester,
        controller: controller,
        service: _QuickOpenFileService(entries: <String>['lib/main.dart']),
        coordinator: coordinator,
      );

      await tester.tap(find.text('Open Quick Open'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(.enter);
      await tester.pumpAndSettle();

      expect(controller.openedFiles, isEmpty);
      expect(launcher.fileRequests, hasLength(1));
      expect(
        launcher.fileRequests.single.filePath,
        r'C:\repo\main\lib\main.dart',
      );
    },
  );

  testWidgets('ignores stale searches', (tester) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final oldSearch = Completer<List<native.WorkspaceQuickOpenMatch>>();
    final newSearch = Completer<List<native.WorkspaceQuickOpenMatch>>();
    final service = _QuickOpenFileService(
      entries: <String>['old.dart', 'new.dart'],
      searchGates: <String, Completer<List<native.WorkspaceQuickOpenMatch>>>{
        'old': oldSearch,
        'new': newSearch,
      },
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'old');
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'new');
    await tester.pump();

    oldSearch.complete(<native.WorkspaceQuickOpenMatch>[_match('old.dart')]);
    await tester.pump();
    expect(find.text('old.dart'), findsNothing);

    newSearch.complete(<native.WorkspaceQuickOpenMatch>[_match('new.dart')]);
    await tester.pumpAndSettle();
    expect(find.text('new.dart'), findsOneWidget);
  });

  testWidgets('ignores a cached session whose start completes after disposal', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final session = Completer<native.WorkspaceQuickOpenSession>();
    final service = _QuickOpenFileService(session: session);
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pump();
    await tester.sendKeyEvent(.escape);
    await tester.pumpAndSettle();

    session.complete(_session('late-session', 0));
    await tester.pump();
    expect(service.stoppedSessionIds, isEmpty);
  });

  testWidgets('arrow navigation, Escape, and focus restoration work', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final anchorFocus = FocusNode();
    addTearDown(anchorFocus.dispose);
    await _pumpQuickOpen(
      tester,
      controller: controller,
      service: _QuickOpenFileService(entries: <String>['a.dart', 'b.dart']),
      anchorFocus: anchorFocus,
    );
    anchorFocus.requestFocus();
    await tester.pump();

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(.arrowDown);
    await tester.sendKeyEvent(.enter);
    await tester.pumpAndSettle();
    expect(controller.openedFiles, <String>['b.dart']);
    expect(anchorFocus.hasFocus, isTrue);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(.escape);
    await tester.pumpAndSettle();
    expect(find.text('Quick Open'), findsNothing);
    expect(anchorFocus.hasFocus, isTrue);
  });

  testWidgets('scrolls the selected result into view', (tester) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      entries: <String>[for (var i = 0; i < 50; i++) 'file_$i.dart'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    for (var i = 0; i < 49; i++) {
      await tester.sendKeyEvent(.arrowDown);
    }
    await tester.pumpAndSettle();

    expect(find.text('file_49.dart'), findsOneWidget);
  });

  testWidgets('shows empty and error states', (tester) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    await _pumpQuickOpen(
      tester,
      controller: controller,
      service: _QuickOpenFileService(entries: const <String>[]),
    );

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(
      find.text('No files are available in this workspace.'),
      findsOneWidget,
    );
    await tester.sendKeyEvent(.escape);
    await tester.pumpAndSettle();

    await _pumpQuickOpen(
      tester,
      controller: controller,
      service: _QuickOpenFileService(error: StateError('enumeration failed')),
    );
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(find.text('Could not load workspace files.'), findsOneWidget);
  });

  testWidgets('displays the native error context and retries failed indexing', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(
      error: const native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.io,
        context: 'Cannot read the workspace directory',
      ),
      failOnce: true,
      entries: const <String>['lib/main.dart'],
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(find.text('Could not load workspace files.'), findsOneWidget);
    expect(find.text('Cannot read the workspace directory'), findsOneWidget);
    expect(find.text("Instance of 'WorkspaceFileError'"), findsNothing);

    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();
    expect(find.text('lib/main.dart'), findsOneWidget);
    expect(find.text('Could not load workspace files.'), findsNothing);
  });

  testWidgets('restarts the search when the active workspace changes', (
    tester,
  ) async {
    final first = _workspace('workspace-1', 'Main', '/repo/main');
    final second = _workspace('workspace-2', 'Feature', '/repo/feature');
    final controller = _QuickOpenTestController(
      _state(first, extraWorkspaces: <Workspace>[second]),
    );
    final service = _QuickOpenFileService(
      entriesByWorkspacePath: <String, List<String>>{
        first.path: <String>['old.dart'],
        second.path: <String>['new.dart'],
      },
    );
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(find.text('old.dart'), findsOneWidget);

    controller.switchWorkspace(second.id);
    await tester.pumpAndSettle();
    expect(find.text('old.dart'), findsNothing);
    expect(find.text('new.dart'), findsOneWidget);
  });

  testWidgets('remembers include Git-ignored files and reuses the index', (
    tester,
  ) async {
    final workspace = _workspace('workspace-1', 'Main', '/repo/main');
    final controller = _QuickOpenTestController(_state(workspace));
    final service = _QuickOpenFileService(entries: <String>['lib/main.dart']);
    await _pumpQuickOpen(tester, controller: controller, service: service);

    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(service.startedWorkspacePaths, <String>[workspace.path]);
    expect(
      service.excludedDirectoryRequests.single,
      EditorSettings.defaultQuickOpenExcludedDirectories,
    );
    expect(service.includeGitignoredRequests, <bool>[false]);
    expect(controller.state.viewPrefs.quickOpenIncludeGitignored, isFalse);

    await tester.tap(find.text('Include Git-ignored files'));
    await tester.pumpAndSettle();
    expect(controller.state.viewPrefs.quickOpenIncludeGitignored, isTrue);
    expect(service.startedWorkspacePaths, <String>[workspace.path]);
    expect(service.includeGitignoredRequests, <bool>[false, true]);

    await tester.sendKeyEvent(.escape);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open Quick Open'));
    await tester.pumpAndSettle();
    expect(service.startedWorkspacePaths, <String>[workspace.path]);
    expect(service.includeGitignoredRequests, <bool>[false, true, true]);
  });
}

Future<void> _quickOpenMenuScrollTo(WidgetTester tester, String label) async {
  await tester.scrollUntilVisible(
    find.text(label),
    100,
    scrollable: find.byType(Scrollable).last,
  );
  await tester.pumpAndSettle();
}

Future<void> _pumpQuickOpen(
  WidgetTester tester, {
  required _QuickOpenTestController controller,
  required _QuickOpenFileService service,
  FocusNode? anchorFocus,
  WorkspaceFileOpenCoordinator? coordinator,
  WorkspaceFolderOpener? folderOpener,
  _QuickOpenExternalEditorLauncher? externalEditorLauncher,
}) async {
  final launcher = externalEditorLauncher ?? _QuickOpenExternalEditorLauncher();
  final effectiveCoordinator =
      coordinator ??
      WorkspaceFileOpenCoordinator(
        externalEditorLauncher: launcher,
        defaultTargetReader: () => CodeOpenTarget.alera,
      );
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        settingsControllerProvider.overrideWithValue(AleraSettings.defaults),
        workbenchControllerProvider.overrideWith(() => controller),
        workspaceFileServiceProvider.overrideWithValue(service),
        if (folderOpener != null)
          workspaceFolderOpenerProvider.overrideWithValue(folderOpener),
        if (externalEditorLauncher != null) ...[
          installedExternalEditorsProvider.overrideWith(
            (ref) async => <ExternalEditorSpec>[
              externalEditorSpecs[ExternalEditorKind.zed]!,
            ],
          ),
          resolvedExternalEditorProvider.overrideWith(
            (ref) async => externalEditorSpecs[ExternalEditorKind.zed],
          ),
          externalEditorLauncherForProvider(.zed)
              .overrideWithValue(externalEditorLauncher),
        ],
        workspaceFileOpenCoordinatorProvider.overrideWithValue(
          effectiveCoordinator,
        ),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: Focus(
            focusNode: anchorFocus,
            child: Consumer(
              builder: (context, ref, _) => FilledButton(
                onPressed: () => showQuickOpenFlow(context, ref),
                child: const Text('Open Quick Open'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
}

WorkbenchState _state(
  Workspace active, {
  List<Workspace> extraWorkspaces = const <Workspace>[],
}) {
  return WorkbenchState(
    workspacesByProject: <String, List<Workspace>>{
      active.projectId: <Workspace>[active, ...extraWorkspaces],
    },
    activeWorkspaceId: active.id,
  );
}

Workspace _workspace(String id, String name, String path) {
  final now = DateTime.utc(2026);
  return Workspace(
    id: id,
    projectId: 'project-1',
    name: name,
    path: path,
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}

native.WorkspaceQuickOpenMatch _match(String relativePath) {
  return native.WorkspaceQuickOpenMatch(
    relativePath: relativePath,
    score: 0,
    isDirectory: false,
  );
}

native.WorkspaceQuickOpenSession _session(String id, int indexedFileCount) {
  return native.WorkspaceQuickOpenSession(
    id: id,
    indexedFileCount: indexedFileCount,
  );
}

class _QuickOpenFileService({
  final List<String> entries = const <String>[],
  final List<String> cachedEntries = const <String>[],
  final Object? error,
  final bool failOnce = false,
  final Completer<native.WorkspaceQuickOpenSession>? session,
  final Map<String, List<String>> entriesByWorkspacePath =
      const <String, List<String>>{},
  final Map<String, Completer<List<native.WorkspaceQuickOpenMatch>>>
      searchGates =
      const <String, Completer<List<native.WorkspaceQuickOpenMatch>>>{},
}) extends WorkspaceFileService {
  final StreamController<String> _refreshes =
      StreamController<String>.broadcast(sync: true);
  final List<String> refreshedWorkspacePaths = <String>[];
  final List<String> clipboardWrittenPaths = <String>[];
  final List<String> copiedEntries = <String>[];
  final List<String> movedEntries = <String>[];
  final List<String> renamedEntries = <String>[];
  final List<String> deletedEntries = <String>[];
  final List<String> createdEntries = <String>[];
  WorkspaceFileClipboardPayload? systemClipboard;
  int _clipboardSequence = 0;

  @override
  Stream<String> get quickOpenRefreshes => _refreshes.stream;

  void emitRefresh(String workspacePath) => _refreshes.add(workspacePath);

  @override
  void scheduleQuickOpenRefresh({
    required String workspacePath,
    Duration debounce = const Duration(milliseconds: 500),
  }) {
    refreshedWorkspacePaths.add(workspacePath);
  }

  @override
  Future<int?> writeSystemFileClipboard({
    required List<String> paths,
    required WorkspaceFileClipboardOperation operation,
  }) async {
    clipboardWrittenPaths
      ..clear()
      ..addAll(paths);
    systemClipboard = WorkspaceFileClipboardPayload(
      paths: List<String>.of(paths),
      operation: operation,
      sequenceNumber: ++_clipboardSequence,
    );
    return _clipboardSequence;
  }

  @override
  Future<WorkspaceFileClipboardPayload?> readSystemFileClipboard() async =>
      systemClipboard;

  @override
  Future<bool> clearSystemFileClipboardIfSequence(int sequenceNumber) async {
    if (_clipboardSequence != sequenceNumber) return false;
    systemClipboard = null;
    _clipboardSequence++;
    return true;
  }

  @override
  Future<native.WorkspaceFileEntry> copyEntry({
    required String workspacePath,
    required String relativePath,
    required String targetParentRelativePath,
  }) async {
    copiedEntries.add('$relativePath->$targetParentRelativePath');
    return _quickOpenFileEntry(
      '$targetParentRelativePath/${p.basename(relativePath)} copy',
    );
  }

  @override
  Future<native.WorkspaceFileEntry> moveEntry({
    required String workspacePath,
    required String relativePath,
    required String targetParentRelativePath,
  }) async {
    movedEntries.add('$relativePath->$targetParentRelativePath');
    return _quickOpenFileEntry(
      '$targetParentRelativePath/${p.basename(relativePath)}',
    );
  }

  @override
  Future<native.WorkspaceFileEntry> renameEntry({
    required String workspacePath,
    required String relativePath,
    required String newName,
  }) async {
    renamedEntries.add('$relativePath->$newName');
    final parent = p.posix.dirname(relativePath);
    return _quickOpenFileEntry(parent == '.' ? newName : '$parent/$newName');
  }

  @override
  Future<void> deleteEntry({
    required String workspacePath,
    required String relativePath,
    bool useTrash = true,
  }) async {
    deletedEntries.add(relativePath);
  }

  @override
  Future<native.WorkspaceFileEntry> createFile({
    required String workspacePath,
    required String parentRelativePath,
    required String name,
  }) async {
    final path = parentRelativePath.isEmpty
        ? name
        : '$parentRelativePath/$name';
    createdEntries.add(path);
    return _quickOpenFileEntry(path);
  }

  @override
  Future<native.WorkspaceFileEntry> createDirectory({
    required String workspacePath,
    required String parentRelativePath,
    required String name,
  }) async {
    final path = parentRelativePath.isEmpty
        ? name
        : '$parentRelativePath/$name';
    createdEntries.add('$path/');
    return _quickOpenFileEntry(path);
  }

  final List<String> stoppedSessionIds = <String>[];
  final List<String> startedWorkspacePaths = <String>[];
  final List<List<String>> excludedDirectoryRequests = <List<String>>[];
  final List<bool> includeGitignoredRequests = <bool>[];
  final Map<String, List<String>> _entriesBySessionId =
      <String, List<String>>{};
  final Map<String, native.WorkspaceQuickOpenSession>
  _cachedSessionByWorkspacePath = <String, native.WorkspaceQuickOpenSession>{};
  int _failedStarts = 0;

  @override
  List<native.WorkspaceQuickOpenMatch>? peekQuickOpenMatches({
    required String workspacePath,
    required List<String> excludedDirectories,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) {
    if (cachedEntries.isEmpty) {
      return null;
    }
    final normalizedQuery = query.trim().toLowerCase();
    return cachedEntries
        .where(
          (path) =>
              normalizedQuery.isEmpty ||
              path.toLowerCase().contains(normalizedQuery),
        )
        .take(limit)
        .map(_match)
        .toList(growable: false);
  }

  @override
  Future<List<native.WorkspaceQuickOpenMatch>> searchQuickOpenCached({
    required String workspacePath,
    required List<String> excludedDirectories,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) async {
    var session = _cachedSessionByWorkspacePath[workspacePath];
    if (session == null) {
      session = await startQuickOpenSession(
        workspacePath: workspacePath,
        excludedDirectories: excludedDirectories,
      );
      _cachedSessionByWorkspacePath[workspacePath] = session;
    }
    return searchQuickOpenSession(
      session: session,
      query: query,
      includeGitignored: includeGitignored,
      limit: limit,
      includeDirectories: includeDirectories,
    );
  }

  @override
  Future<native.WorkspaceQuickOpenSession> startQuickOpenSession({
    required String workspacePath,
    required List<String> excludedDirectories,
  }) {
    startedWorkspacePaths.add(workspacePath);
    excludedDirectoryRequests.add(List<String>.of(excludedDirectories));
    if (error != null && (!failOnce || _failedStarts++ == 0)) {
      return Future<native.WorkspaceQuickOpenSession>.error(error!);
    }
    if (session != null) {
      return session!.future.then((value) {
        _entriesBySessionId[value.id] =
            entriesByWorkspacePath[workspacePath] ?? entries;
        return value;
      });
    }
    final value = _session(
      workspacePath,
      entriesByWorkspacePath[workspacePath]?.length ?? entries.length,
    );
    _entriesBySessionId[value.id] =
        entriesByWorkspacePath[workspacePath] ?? entries;
    return Future.value(value);
  }

  @override
  Future<List<native.WorkspaceQuickOpenMatch>> searchQuickOpenSession({
    required native.WorkspaceQuickOpenSession session,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) {
    includeGitignoredRequests.add(includeGitignored);
    final gate = searchGates[query];
    if (gate != null) {
      return gate.future;
    }
    final normalizedQuery = query.trim().toLowerCase();
    final paths = _entriesBySessionId[session.id] ?? entries;
    final matches =
        paths
            .where(
              (path) =>
                  normalizedQuery.isEmpty ||
                  path.toLowerCase().contains(normalizedQuery),
            )
            .toList()
          ..sort();
    return Future.value(
      matches.take(limit).map(_match).toList(growable: false),
    );
  }

  @override
  Future<void> stopQuickOpenSession({
    required native.WorkspaceQuickOpenSession session,
  }) async {
    stoppedSessionIds.add(session.id);
  }
}

class _QuickOpenTestController(final WorkbenchState _seed)
    extends WorkbenchController {
  final List<String> openedFiles = <String>[];
  final List<bool> openedPreviews = <bool>[];
  final List<String> movedFileTabs = <String>[];

  @override
  Future<void> syncFileTabsAfterPathMove({
    required Workspace workspace,
    required String oldRelativePath,
    required String newRelativePath,
  }) async {
    movedFileTabs.add('$oldRelativePath->$newRelativePath');
  }

  @override
  WorkbenchState build() => _seed;

  @override
  Future<void> bootstrap() async {}

  @override
  Future<WorkspaceTabRecord> openFileTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) async {
    openedFiles.add(relativePath);
    openedPreviews.add(preview);
    final now = DateTime.utc(2026);
    return WorkspaceTabRecord(
      id: 'tab-${openedFiles.length}',
      workspaceId: workspace.id,
      kind: .editor,
      title: relativePath,
      createdAt: now,
      updatedAt: now,
    );
  }

  void switchWorkspace(String workspaceId) {
    state = state.copyWith(activeWorkspaceId: workspaceId);
  }
}

class _QuickOpenExternalEditorLauncher implements ExternalEditorLauncher {
  final List<ExternalEditorOpenRequest> fileRequests =
      <ExternalEditorOpenRequest>[];

  @override
  Future<ExternalEditorAvailability> checkAvailability() async =>
      const ExternalEditorAvailability(available: true);

  @override
  Future<bool> isInstalled() async => true;

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async {
    fileRequests.add(request);
    return ExternalEditorLaunchResultFactories.opened;
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async => ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(
    String workspacePath,
  ) async => ExternalEditorLaunchResultFactories.opened;
}

native.WorkspaceFileEntry _quickOpenFileEntry(String relativePath) =>
    native.WorkspaceFileEntry(
      relativePath: relativePath,
      name: p.posix.basename(relativePath),
      kind: native.WorkspaceFileKind.file,
      size: BigInt.zero,
      modifiedMillis: 0,
      contentToken: '',
      isIgnored: false,
      isHidden: false,
      isSymlink: false,
      isProtected: false,
      hasChildrenHint: false,
    );

class _QuickOpenFolderOpener() extends WorkspaceFolderOpener {
  this
    : super(
        processRunner: _QuickOpenNoopProcessRunner(),
        platform: WorkspaceFolderPlatform.macos,
      );

  final List<String> revealedPaths = <String>[];
  final List<String> openedPaths = <String>[];

  @override
  Future<WorkspaceFolderOpenResult> reveal(String path) async {
    revealedPaths.add(path);
    return const WorkspaceFolderOpenResult.success();
  }

  @override
  Future<WorkspaceFolderOpenResult> openWithDefaultApplication(
    String path,
  ) async {
    openedPaths.add(path);
    return const WorkspaceFolderOpenResult.success();
  }
}

class _QuickOpenNoopProcessRunner implements ProcessRunner {
  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) async => const ProcessRunOutput(exitCode: 0, stdout: '', stderr: '');

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) => throw UnimplementedError();
}
