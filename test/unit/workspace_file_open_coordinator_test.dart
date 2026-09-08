import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late _FakeExternalEditorLauncher launcher;
  late List<String> internalOpens;
  late List<String> notices;
  var defaultTarget = CodeOpenTarget.alera;

  setUp(() {
    launcher = _FakeExternalEditorLauncher();
    internalOpens = <String>[];
    notices = <String>[];
    defaultTarget = CodeOpenTarget.alera;
  });

  WorkspaceFileOpenCoordinator coordinator() => WorkspaceFileOpenCoordinator(
    externalEditorLauncher: launcher,
    defaultTargetReader: () => defaultTarget,
    implicitFailureNotice: notices.add,
  );

  Future<String> openInternal({
    required Workspace workspace,
    required String relativePath,
    required bool preview,
  }) async {
    internalOpens.add('$relativePath:$preview');
    return relativePath;
  }

  test('Alera remains the default source-file target', () async {
    final result = await coordinator().open<String>(
      workspace: _workspace,
      relativePath: 'lib/main.dart',
      preview: true,
      openInAlera: openInternal,
    );

    expect(result.status, WorkspaceFileOpenStatus.opened);
    expect(result.destination, WorkspaceFileOpenDestination.alera);
    expect(result.internalValue, 'lib/main.dart');
    expect(internalOpens, <String>['lib/main.dart:true']);
    expect(launcher.files, isEmpty);
  });

  test('Zed default routes ordinary editable files externally', () async {
    defaultTarget = CodeOpenTarget.zed;

    final result = await coordinator().open<String>(
      workspace: _workspace,
      relativePath: 'lib/main.dart',
      openInAlera: openInternal,
      line: 12,
      column: 4,
    );

    expect(result.status, WorkspaceFileOpenStatus.opened);
    expect(result.destination, WorkspaceFileOpenDestination.zed);
    expect(internalOpens, isEmpty);
    expect(launcher.files.single.filePath, endsWith('lib\\main.dart'));
    expect(launcher.files.single.line, 12);
    expect(launcher.files.single.column, 4);
  });

  test('dedicated Alera preview file types stay internal', () async {
    defaultTarget = CodeOpenTarget.zed;
    final service = coordinator();

    for (final path in <String>[
      'README.md',
      'docs/manual.pdf',
      'assets/logo.png',
      'docs/diagram.mmd',
    ]) {
      final result = await service.open<String>(
        workspace: _workspace,
        relativePath: path,
        openInAlera: openInternal,
      );
      expect(result.status, WorkspaceFileOpenStatus.opened);
      expect(result.destination, WorkspaceFileOpenDestination.alera);
    }

    expect(launcher.files, isEmpty);
    expect(internalOpens, hasLength(4));
  });

  test(
    'explicit target override beats the default and preview routing',
    () async {
      defaultTarget = CodeOpenTarget.zed;
      final service = coordinator();

      await service.open<String>(
        workspace: _workspace,
        relativePath: 'lib/main.dart',
        targetOverride: CodeOpenTarget.alera,
        openInAlera: openInternal,
      );
      await service.open<String>(
        workspace: _workspace,
        relativePath: 'README.md',
        targetOverride: CodeOpenTarget.zed,
        openInAlera: openInternal,
      );

      expect(internalOpens, <String>['lib/main.dart:false']);
      expect(launcher.files.single.filePath, endsWith('README.md'));
    },
  );

  test('implicit Zed failure falls back to Alera and notifies once', () async {
    defaultTarget = CodeOpenTarget.zed;
    launcher.nextResult = externalEditorLaunchFailure(
      ExternalEditorLaunchFailureKind.unavailable,
      'Zed is unavailable.',
    );
    final service = coordinator();

    final first = await service.open<String>(
      workspace: _workspace,
      relativePath: 'lib/one.dart',
      openInAlera: openInternal,
    );
    final second = await service.open<String>(
      workspace: _workspace,
      relativePath: 'lib/two.dart',
      openInAlera: openInternal,
    );

    expect(first.status, WorkspaceFileOpenStatus.fellBack);
    expect(second.status, WorkspaceFileOpenStatus.fellBack);
    expect(first.ok, isTrue);
    expect(first.fellBack, isTrue);
    expect(second.fellBack, isTrue);
    expect(first.destination, WorkspaceFileOpenDestination.alera);
    expect(internalOpens, <String>['lib/one.dart:false', 'lib/two.dart:false']);
    expect(notices, hasLength(1));
    expect(notices.single, contains('Opened in Alera instead'));
  });

  test('explicit Zed failure never falls back to Alera', () async {
    launcher.nextResult = externalEditorLaunchFailure(
      ExternalEditorLaunchFailureKind.unavailable,
      'Zed is unavailable.',
    );

    final result = await coordinator().open<String>(
      workspace: _workspace,
      relativePath: 'lib/main.dart',
      targetOverride: CodeOpenTarget.zed,
      openInAlera: openInternal,
    );

    expect(result.status, WorkspaceFileOpenStatus.failed);
    expect(result.ok, isFalse);
    expect(result.destination, isNull);
    expect(result.message, 'Zed is unavailable.');
    expect(result.fellBack, isFalse);
    expect(internalOpens, isEmpty);
    expect(notices, isEmpty);
  });

  test(
    'a successful implicit launch resets the one-shot failure notice',
    () async {
      defaultTarget = CodeOpenTarget.zed;
      launcher.nextResult = externalEditorLaunchFailure(
        ExternalEditorLaunchFailureKind.unavailable,
        'Zed is unavailable.',
      );
      final service = coordinator();

      await service.open<String>(
        workspace: _workspace,
        relativePath: 'lib/one.dart',
        openInAlera: openInternal,
      );
      launcher.nextResult = ExternalEditorLaunchResultFactories.opened;
      await service.open<String>(
        workspace: _workspace,
        relativePath: 'lib/two.dart',
        openInAlera: openInternal,
      );
      launcher.nextResult = externalEditorLaunchFailure(
        ExternalEditorLaunchFailureKind.unavailable,
        'Zed is unavailable again.',
      );
      await service.open<String>(
        workspace: _workspace,
        relativePath: 'lib/three.dart',
        openInAlera: openInternal,
      );

      expect(notices, hasLength(2));
    },
  );
}

final _workspace = Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: r'C:\repo',
  createdAt: DateTime.utc(2026),
  updatedAt: DateTime.utc(2026),
  kind: WorkspaceKind.main,
  status: WorkspaceStatus.active,
);

class _FakeExternalEditorLauncher implements ExternalEditorLauncher {
  final List<ExternalEditorOpenRequest> files = <ExternalEditorOpenRequest>[];
  ExternalEditorLaunchResult nextResult =
      ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorAvailability> checkAvailability() async =>
      const ExternalEditorAvailability(available: true);

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async {
    files.add(request);
    return nextResult;
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async => ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(
    String workspacePath,
  ) async => nextResult;
}
