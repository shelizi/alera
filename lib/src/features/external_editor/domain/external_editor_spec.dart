import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';

typedef ExternalEditorWorkspaceArgs = List<String> Function({
  required bool newWindow,
});
typedef ExternalEditorFileArgs = List<String> Function(
  String filePath, {
  int? line,
  int? column,
});
typedef ExternalEditorFilesArgs = List<String> Function(List<String> filePaths);
typedef ExternalEditorVersionParser = String? Function(String stdout);

/// CLI contract for one supported external editor. Everything the launcher,
/// providers, and menus need to know about an editor lives here, so adding a
/// new editor means adding an [ExternalEditorKind] value plus one entry in
/// [externalEditorSpecs] - no launcher or call-site changes.
class const ExternalEditorSpec({
  required this.kind,
  required this.displayName,
  required this.shortName,
  required this.commandCandidates,
  required this.workspaceArgs,
  required this.fileArgs,
  required this.filesArgs,
  this.fallbackPaths = const <String>[],
  this.parseVersion = _trimmedVersion,
}) {
  /// External editor this spec launches.
  final ExternalEditorKind kind;

  /// Full product name used in settings copy and error messages.
  final String displayName;

  /// Compact name used in menu labels such as `Open in {shortName}`.
  final String shortName;

  /// Bare command names probed on PATH in order; the first hit wins. On
  /// Windows the probe walks PATHEXT, so `code` also matches `code.cmd`.
  final List<String> commandCandidates;

  /// Absolute paths tried when no command resolves on PATH. `%VAR%` and
  /// `$VAR`/`${VAR}` tokens expand against the process environment.
  final List<String> fallbackPaths;

  /// Flags prepended to a workspace path argument.
  final ExternalEditorWorkspaceArgs workspaceArgs;

  /// Arguments for opening a single file, optionally at line/column.
  final ExternalEditorFileArgs fileArgs;

  /// Arguments for opening several files in one invocation.
  final ExternalEditorFilesArgs filesArgs;

  /// Extracts a displayable version from `--version` stdout; null when the
  /// output carries nothing worth showing.
  final ExternalEditorVersionParser parseVersion;
}

/// Registry of every supported external editor in preference order. The first
/// installed spec becomes the fallback when the configured editor is missing.
const externalEditorSpecs = <ExternalEditorKind, ExternalEditorSpec>{
  ExternalEditorKind.zed: _zedSpec,
  ExternalEditorKind.vscode: _vscodeSpec,
};

const _zedSpec = ExternalEditorSpec(
  kind: ExternalEditorKind.zed,
  displayName: 'Zed',
  shortName: 'Zed',
  commandCandidates: <String>['zed'],
  fallbackPaths: <String>['/Applications/Zed.app/Contents/MacOS/cli'],
  workspaceArgs: _zedWorkspaceArgs,
  fileArgs: _zedFileArgs,
  filesArgs: _identityFilesArgs,
);

const _vscodeSpec = ExternalEditorSpec(
  kind: ExternalEditorKind.vscode,
  displayName: 'Visual Studio Code',
  shortName: 'VS Code',
  commandCandidates: <String>['code', 'code.cmd'],
  fallbackPaths: <String>[
    '/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code',
    r'%LOCALAPPDATA%\Programs\Microsoft VS Code\bin\code.cmd',
  ],
  workspaceArgs: _vscodeWorkspaceArgs,
  fileArgs: _vscodeFileArgs,
  filesArgs: _identityFilesArgs,
  parseVersion: _vscodeVersion,
);

String? _trimmedVersion(String stdout) {
  final trimmed = stdout.trim();
  return trimmed.isEmpty ? null : trimmed;
}

String? _vscodeVersion(String stdout) {
  for (final line in stdout.split('\n')) {
    final trimmed = line.trim();
    if (trimmed.isNotEmpty) {
      return trimmed;
    }
  }
  return null;
}

List<String> _zedWorkspaceArgs({required bool newWindow}) =>
    newWindow ? const <String>['--new'] : const <String>[];

List<String> _vscodeWorkspaceArgs({required bool newWindow}) => newWindow
    ? const <String>['--new-window']
    : const <String>['--reuse-window'];

List<String> _zedFileArgs(String filePath, {int? line, int? column}) =>
    <String>[_fileTarget(filePath, line: line, column: column)];

List<String> _vscodeFileArgs(String filePath, {int? line, int? column}) =>
    <String>['--goto', _fileTarget(filePath, line: line, column: column)];

List<String> _identityFilesArgs(List<String> filePaths) => filePaths;

String _fileTarget(String filePath, {int? line, int? column}) {
  return switch ((line, column)) {
    (final int line, final int column) => '$filePath:$line:$column',
    (final int line, null) => '$filePath:$line',
    _ => filePath,
  };
}
