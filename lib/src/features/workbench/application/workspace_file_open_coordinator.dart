import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/workbench/application/workspace_file_preview_kind.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:path/path.dart' as p;

typedef WorkspaceFileInternalOpener<T> = Future<T> Function({
  required Workspace workspace,
  required String relativePath,
  required bool preview,
});

enum WorkspaceFileOpenDestination { alera, zed }

enum WorkspaceFileOpenStatus { opened, fellBack, failed }

class const WorkspaceFileOpenResult<T>({
  required this.status,
  this.destination,
  this.internalValue,
  this.message,
}) {
  final WorkspaceFileOpenStatus status;
  final WorkspaceFileOpenDestination? destination;
  final T? internalValue;
  final String? message;

  bool get ok => status != WorkspaceFileOpenStatus.failed;
  bool get fellBack => status == WorkspaceFileOpenStatus.fellBack;
}

class WorkspaceFileOpenCoordinator {
  WorkspaceFileOpenCoordinator({
    required ExternalEditorLauncher externalEditorLauncher,
    required CodeOpenTarget Function() defaultTargetReader,
    void Function(String message)? implicitFailureNotice,
  }) : _externalEditorLauncher = externalEditorLauncher,
       _defaultTargetReader = defaultTargetReader,
       _implicitFailureNotice = implicitFailureNotice;

  final ExternalEditorLauncher _externalEditorLauncher;
  final CodeOpenTarget Function() _defaultTargetReader;
  final void Function(String message)? _implicitFailureNotice;
  bool _implicitFailureNotified = false;

  Future<WorkspaceFileOpenResult<T>> open<T>({
    required Workspace workspace,
    required String relativePath,
    required WorkspaceFileInternalOpener<T> openInAlera,
    bool preview = false,
    int? line,
    int? column,
    CodeOpenTarget? targetOverride,
  }) async {
    final explicitTarget = targetOverride;
    final target = explicitTarget ?? _implicitTargetFor(relativePath);
    if (target == CodeOpenTarget.alera) {
      return _openInternally(
        workspace: workspace,
        relativePath: relativePath,
        preview: preview,
        openInAlera: openInAlera,
      );
    }

    final result = await _externalEditorLauncher.openFile(
      ExternalEditorOpenRequest(
        workspacePath: workspace.path,
        filePath: p.normalize(p.join(workspace.path, relativePath)),
        line: line,
        column: column,
      ),
    );
    if (result.ok) {
      _implicitFailureNotified = false;
      return WorkspaceFileOpenResult<T>(
        status: WorkspaceFileOpenStatus.opened,
        destination: WorkspaceFileOpenDestination.zed,
      );
    }
    if (explicitTarget != null) {
      return WorkspaceFileOpenResult<T>(
        status: WorkspaceFileOpenStatus.failed,
        message: result.message,
      );
    }

    final message = result.message ?? 'Could not open file in Zed.';
    if (!_implicitFailureNotified) {
      _implicitFailureNotified = true;
      _implicitFailureNotice?.call('$message Opened in Alera instead.');
    }
    final fallback = await _openInternally(
      workspace: workspace,
      relativePath: relativePath,
      preview: preview,
      openInAlera: openInAlera,
    );
    return WorkspaceFileOpenResult<T>(
      status: WorkspaceFileOpenStatus.fellBack,
      destination: fallback.destination,
      internalValue: fallback.internalValue,
      message: message,
    );
  }

  CodeOpenTarget _implicitTargetFor(String relativePath) {
    if (_hasDedicatedAleraPreview(relativePath)) {
      return CodeOpenTarget.alera;
    }
    return _defaultTargetReader();
  }

  bool _hasDedicatedAleraPreview(String relativePath) {
    if (isWorkspaceMarkdownFilePath(relativePath)) {
      return true;
    }
    return switch (workspaceFilePreviewKindForPath(relativePath)) {
      WorkspaceFilePreviewKind.image ||
      WorkspaceFilePreviewKind.pdf ||
      WorkspaceFilePreviewKind.merman => true,
      WorkspaceFilePreviewKind.text => false,
    };
  }

  Future<WorkspaceFileOpenResult<T>> _openInternally<T>({
    required Workspace workspace,
    required String relativePath,
    required bool preview,
    required WorkspaceFileInternalOpener<T> openInAlera,
  }) async {
    final value = await openInAlera(
      workspace: workspace,
      relativePath: relativePath,
      preview: preview,
    );
    return WorkspaceFileOpenResult<T>(
      status: WorkspaceFileOpenStatus.opened,
      destination: WorkspaceFileOpenDestination.alera,
      internalValue: value,
    );
  }
}
