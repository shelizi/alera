part of 'workspace_git_diff_surface.dart';

final class _DiffSyntaxStyle {
  _DiffSyntaxStyle._({
    required this.languageId,
    required this.themeName,
    required this.textStyle,
    required code_forge_syntax.SyntaxHighlighter highlighter,
  }) : _highlighter = highlighter;

  factory _DiffSyntaxStyle.forPath({
    required String filePath,
    required String themeName,
    required LanguageExtensionRegistry registry,
  }) {
    final languageId = workspaceEditorSyntaxLanguageIdForPath(
      filePath: filePath,
      registry: registry,
    );
    final editorTheme = editorSyntaxThemeForName(themeName);
    final rootStyle = editorSyntaxRootStyleForName(themeName);
    final textStyle = AleraTokens.monoStyle.copyWith(
      fontSize: 12,
      color: rootStyle.color ?? AleraTokens.foreground,
      height: 1.5,
    );
    return _DiffSyntaxStyle._(
      languageId: languageId,
      themeName: themeName,
      textStyle: textStyle,
      highlighter: code_forge_syntax.SyntaxHighlighter(
        language: workspaceEditorSyntaxModeForPath(
          filePath: filePath,
          registry: registry,
          syntaxLanguageId: languageId,
        ),
        languageId: languageId,
        editorTheme: editorTheme,
        baseTextStyle: textStyle,
      ),
    );
  }

  final String languageId;
  final String themeName;
  final TextStyle textStyle;
  final code_forge_syntax.SyntaxHighlighter _highlighter;

  TextSpan lineSpan(String text, {int lineIndex = 0}) =>
      _highlighter.getLineSpan(lineIndex, text) ??
      TextSpan(text: text, style: textStyle);

  TextSpan documentSpan(String text, {TextStyle? style}) {
    final baseStyle = style ?? textStyle;
    if (text.isEmpty) {
      return TextSpan(text: '', style: baseStyle);
    }
    final children = <InlineSpan>[];
    var start = 0;
    var lineIndex = 0;
    for (final match in RegExp(r'\r\n|\n|\r').allMatches(text)) {
      children.add(
        lineSpan(text.substring(start, match.start), lineIndex: lineIndex),
      );
      children.add(TextSpan(text: match.group(0), style: baseStyle));
      start = match.end;
      lineIndex += 1;
    }
    children.add(lineSpan(text.substring(start), lineIndex: lineIndex));
    return TextSpan(style: baseStyle, children: children);
  }

  void dispose() => _highlighter.dispose();
}

class _DiffSyntaxTextEditingController extends TextEditingController {
  _DiffSyntaxTextEditingController({
    required String text,
    required _DiffSyntaxStyle syntax,
  }) : _syntax = syntax,
       super(text: text);

  _DiffSyntaxStyle _syntax;

  void updateSyntax(_DiffSyntaxStyle syntax) {
    if (identical(_syntax, syntax)) return;
    _syntax = syntax;
    notifyListeners();
  }

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    if (withComposing &&
        value.composing.isValid &&
        !value.composing.isCollapsed) {
      return super.buildTextSpan(
        context: context,
        style: style,
        withComposing: withComposing,
      );
    }
    return _syntax.documentSpan(text, style: style);
  }
}

class _DiffSyntaxScope extends InheritedWidget {
  const _DiffSyntaxScope({required this.syntax, required super.child});

  final _DiffSyntaxStyle syntax;

  static _DiffSyntaxStyle? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_DiffSyntaxScope>()?.syntax;

  @override
  bool updateShouldNotify(covariant _DiffSyntaxScope oldWidget) =>
      !identical(syntax, oldWidget.syntax);
}

class _DiffSyntaxScopedRow extends _DiffRow {
  const _DiffSyntaxScopedRow({required this.row, required this.syntax});

  final _DiffRow row;
  final _DiffSyntaxStyle syntax;

  @override
  Widget build(BuildContext context) => _DiffSyntaxScope(
    syntax: syntax,
    child: Builder(builder: row.build),
  );

  @override
  Widget buildSideBySidePane(BuildContext context, {required bool isLeft}) =>
      _DiffSyntaxScope(
        syntax: syntax,
        child: Builder(
          builder: (context) =>
              row.buildSideBySidePane(context, isLeft: isLeft),
        ),
      );
}

class _DiffSyntaxScopedRowSpan implements _DiffRowSpan {
  const _DiffSyntaxScopedRowSpan({required this.span, required this.syntax});

  final _DiffRowSpan span;
  final _DiffSyntaxStyle syntax;

  @override
  int get length => span.length;

  @override
  _DiffRow rowAt(int index) =>
      _DiffSyntaxScopedRow(row: span.rowAt(index), syntax: syntax);
}
