import 'package:alera/src/rust/api/workspace_files.dart' as native;

enum WorkspaceTextEncodingSelection {
  auto,
  utf8,
  utf8Bom,
  utf16Le,
  utf16Be,
  big5,
  gbk,
  shiftJis,
  eucJp,
  eucKr,
  windows1252;

  native.WorkspaceTextEncoding? get encoding => switch (this) {
    WorkspaceTextEncodingSelection.auto => null,
    WorkspaceTextEncodingSelection.utf8 => native.WorkspaceTextEncoding.utf8,
    WorkspaceTextEncodingSelection.utf8Bom =>
      native.WorkspaceTextEncoding.utf8Bom,
    WorkspaceTextEncodingSelection.utf16Le =>
      native.WorkspaceTextEncoding.utf16Le,
    WorkspaceTextEncodingSelection.utf16Be =>
      native.WorkspaceTextEncoding.utf16Be,
    WorkspaceTextEncodingSelection.big5 => native.WorkspaceTextEncoding.big5,
    WorkspaceTextEncodingSelection.gbk => native.WorkspaceTextEncoding.gbk,
    WorkspaceTextEncodingSelection.shiftJis =>
      native.WorkspaceTextEncoding.shiftJis,
    WorkspaceTextEncodingSelection.eucJp => native.WorkspaceTextEncoding.eucJp,
    WorkspaceTextEncodingSelection.eucKr => native.WorkspaceTextEncoding.eucKr,
    WorkspaceTextEncodingSelection.windows1252 =>
      native.WorkspaceTextEncoding.windows1252,
  };

  String get label => switch (this) {
    WorkspaceTextEncodingSelection.auto => 'Auto',
    WorkspaceTextEncodingSelection.utf8 => 'UTF-8',
    WorkspaceTextEncodingSelection.utf8Bom => 'UTF-8 BOM',
    WorkspaceTextEncodingSelection.utf16Le => 'UTF-16 LE',
    WorkspaceTextEncodingSelection.utf16Be => 'UTF-16 BE',
    WorkspaceTextEncodingSelection.big5 => 'Big5',
    WorkspaceTextEncodingSelection.gbk => 'GBK',
    WorkspaceTextEncodingSelection.shiftJis => 'Shift-JIS',
    WorkspaceTextEncodingSelection.eucJp => 'EUC-JP',
    WorkspaceTextEncodingSelection.eucKr => 'EUC-KR',
    WorkspaceTextEncodingSelection.windows1252 => 'Windows-1252',
  };

  static WorkspaceTextEncodingSelection fromEncoding(
    native.WorkspaceTextEncoding? encoding,
  ) {
    return switch (encoding) {
      null => WorkspaceTextEncodingSelection.auto,
      native.WorkspaceTextEncoding.utf8 => WorkspaceTextEncodingSelection.utf8,
      native.WorkspaceTextEncoding.utf8Bom =>
        WorkspaceTextEncodingSelection.utf8Bom,
      native.WorkspaceTextEncoding.utf16Le =>
        WorkspaceTextEncodingSelection.utf16Le,
      native.WorkspaceTextEncoding.utf16Be =>
        WorkspaceTextEncodingSelection.utf16Be,
      native.WorkspaceTextEncoding.big5 => WorkspaceTextEncodingSelection.big5,
      native.WorkspaceTextEncoding.gbk => WorkspaceTextEncodingSelection.gbk,
      native.WorkspaceTextEncoding.shiftJis =>
        WorkspaceTextEncodingSelection.shiftJis,
      native.WorkspaceTextEncoding.eucJp =>
        WorkspaceTextEncodingSelection.eucJp,
      native.WorkspaceTextEncoding.eucKr =>
        WorkspaceTextEncodingSelection.eucKr,
      native.WorkspaceTextEncoding.windows1252 =>
        WorkspaceTextEncodingSelection.windows1252,
    };
  }
}

String workspaceTextEncodingDisplayLabel({
  required WorkspaceTextEncodingSelection selection,
  native.WorkspaceTextEncoding? detectedEncoding,
}) {
  if (selection != WorkspaceTextEncodingSelection.auto ||
      detectedEncoding == null) {
    return selection.label;
  }
  return 'Auto (${WorkspaceTextEncodingSelection.fromEncoding(detectedEncoding).label})';
}
