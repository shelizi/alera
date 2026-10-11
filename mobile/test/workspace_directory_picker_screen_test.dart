import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_directory_picker_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _FolderClient implements MobileRuntimeClient {
  final List<String> loadedFolders = <String>[];

  @override
  Future<MobileWorkspaceDirectoryPage> listWorkspaceDirectory({
    required String workspaceId,
    String relativePath = '',
    int offset = 0,
    int limit = 100,
    bool showHidden = false,
  }) async {
    loadedFolders.add(relativePath);
    return MobileWorkspaceDirectoryPage(
      items: relativePath.isEmpty
          ? const <MobileWorkspaceDirectoryEntry>[
              MobileWorkspaceDirectoryEntry(
                name: 'src',
                relativePath: 'src',
                isDirectory: true,
                sizeBytes: 0,
              ),
              MobileWorkspaceDirectoryEntry(
                name: 'notes',
                relativePath: 'notes',
                isDirectory: true,
                sizeBytes: 0,
              ),
              MobileWorkspaceDirectoryEntry(
                name: 'secret.txt',
                relativePath: 'secret.txt',
                isDirectory: false,
                sizeBytes: 12,
              ),
            ]
          : const <MobileWorkspaceDirectoryEntry>[],
      nextOffset: relativePath.isEmpty ? 3 : 0,
      hasMore: false,
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _StaticConnection extends HostConnectionController {
  _StaticConnection(this.client);
  final MobileRuntimeClient client;

  @override
  Future<MobileRuntimeClient> build(String hostId) async => client;
}

void main() {
  testWidgets(
    'move destination picker lists only folders and returns selected directory',
    (tester) async {
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final client = _FolderClient();
      String? selected;
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            hostConnectionControllerProvider.overrideWith2(
              (_) => _StaticConnection(client),
            ),
          ],
          child: MaterialApp(
            home: Builder(
              builder: (context) => Scaffold(
                body: Center(
                  child: FilledButton(
                    onPressed: () async {
                      selected = await Navigator.of(context).push<String>(
                        MaterialPageRoute<String>(
                          builder: (_) => const WorkspaceDirectoryPickerScreen(
                            hostId: 'host',
                            workspaceId: 'ws',
                            sourceDirectory: 'notes',
                            fileName: 'demo.md',
                          ),
                        ),
                      );
                    },
                    child: const Text('Choose folder'),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Choose folder'));
      await tester.pumpAndSettle();
      expect(find.text('src'), findsOneWidget);
      expect(find.text('secret.txt'), findsNothing);
      expect(find.text('Destination: /demo.md'), findsOneWidget);
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      expect(find.text('Destination: /src/demo.md'), findsOneWidget);
      await tester.tap(find.text('Select this folder'));
      await tester.pumpAndSettle();
      expect(selected, 'src');
      expect(client.loadedFolders, containsAll(<String>['', 'src']));

      // The workspace root is a valid destination represented by ''.
      await tester.tap(find.text('Choose folder'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Select this folder'));
      await tester.pumpAndSettle();
      expect(selected, '');

      // Moving a file into its existing parent must not be offered.
      await tester.tap(find.text('Choose folder'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('notes'));
      await tester.pumpAndSettle();
      final disabled = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Select this folder'),
      );
      expect(disabled.onPressed, isNull);
    },
  );
}
