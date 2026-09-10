import 'dart:io' show Platform;

/// Merges terminal-launch environment values while treating wrapper paths as a
/// platform path list rather than filesystem path segments.
void mergeTerminalLaunchEnvironment(
  Map<String, String> target,
  Map<String, String>? source,
) {
  if (source == null || source.isEmpty) {
    return;
  }
  final wrapperEntries = <String>[
    ..._splitPathList(target['ALERA_AGENT_WRAPPER_PATH']),
    ..._splitPathList(source['ALERA_AGENT_WRAPPER_PATH']),
  ];
  target.addAll(source);
  if (wrapperEntries.isEmpty) {
    target.remove('ALERA_AGENT_WRAPPER_PATH');
    return;
  }
  final seen = <String>{};
  target['ALERA_AGENT_WRAPPER_PATH'] = wrapperEntries
      .where((entry) => entry.isNotEmpty && seen.add(entry))
      .join(_pathListSeparator);
}

List<String> _splitPathList(String? value) {
  if (value == null || value.isEmpty) {
    return const <String>[];
  }
  return value
      .split(_pathListSeparator)
      .where((entry) => entry.isNotEmpty)
      .toList(growable: false);
}

String get _pathListSeparator => Platform.isWindows ? ';' : ':';
