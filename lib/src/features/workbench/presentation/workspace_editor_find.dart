part of 'workspace_editor_surface.dart';

@visibleForTesting
code_forge.CodeForgeKeyboardShortcuts
workspaceEditorKeyboardShortcutsForPlatform(TargetPlatform platform) {
  final useMeta = platform == TargetPlatform.macOS;
  return code_forge.CodeForgeKeyboardShortcuts(
    showFindBar: SingleActivator(
      LogicalKeyboardKey.keyF,
      control: !useMeta,
      meta: useMeta,
    ),
  );
}

extension _WorkspaceEditorFindActions on _WorkspaceEditorSurfaceState {
  void _openGoToLine() {
    if (!_goToLineOpen) {
      _setGoToLineOpen(true);
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _goToLineFocusNode.requestFocus();
      _goToLineController.selection = TextSelection(
        baseOffset: 0,
        extentOffset: _goToLineController.text.length,
      );
    });
  }

  void _closeGoToLine() {
    if (_goToLineOpen) {
      _setGoToLineOpen(false);
    }
    _focusNode.requestFocus();
  }

  void _goToLine(String value) {
    final lineIndex = workspaceEditorGoToLineIndex(
      value,
      lineCount: _controller.lineCount,
    );
    if (lineIndex == null) return;
    final offset = _controller.getLineStartOffset(lineIndex);
    _controller.setSelectionSilently(TextSelection.collapsed(offset: offset));
    _controller.scrollToLine(lineIndex);
    _closeGoToLine();
  }

  void _closeEditorFind() {
    _findController.isReplaceMode = false;
    _findController.isActive = false;
    _focusNode.requestFocus();
  }

  KeyEventResult _handleEditorFindNavigationKey(
    FocusNode node,
    KeyEvent event,
  ) {
    if (event is! KeyDownEvent ||
        !_findController.isActive ||
        event.logicalKey != LogicalKeyboardKey.f3) {
      return KeyEventResult.ignored;
    }
    if (HardwareKeyboard.instance.isShiftPressed) {
      _findController.previous();
    } else {
      _findController.next();
    }
    return KeyEventResult.handled;
  }
}

@visibleForTesting
int? workspaceEditorGoToLineIndex(String value, {required int lineCount}) {
  final line = int.tryParse(value.trim());
  if (line == null || line < 1 || line > lineCount) return null;
  return line - 1;
}

class _WorkspaceEditorFindBar extends StatefulWidget
    implements PreferredSizeWidget {
  const _WorkspaceEditorFindBar({
    required this.controller,
    required this.onClose,
  });

  final code_forge.FindController controller;
  final VoidCallback onClose;

  @override
  Size get preferredSize => const Size.fromHeight(
    AleraTextField.defaultDenseHeight + AleraTokens.space8,
  );

  @override
  State<_WorkspaceEditorFindBar> createState() =>
      _WorkspaceEditorFindBarState();
}

class _WorkspaceEditorFindBarState extends State<_WorkspaceEditorFindBar> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final findController = widget.controller;
      findController.findInputFocusNode.requestFocus();
      findController.findInputController.selection = TextSelection(
        baseOffset: 0,
        extentOffset: findController.findInputController.text.length,
      );
    });
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) {
      return KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      widget.onClose();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.f3) {
      if (HardwareKeyboard.instance.isShiftPressed) {
        widget.controller.previous();
      } else {
        widget.controller.next();
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final query = controller.findInputController.text;
    final countLabel = query.isEmpty
        ? null
        : controller.matchCount == 0
        ? 'No results'
        : '${controller.currentMatchIndex + 1}/${controller.matchCount}';
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surface,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space4),
        child: Focus(
          onKeyEvent: _handleKey,
          child: Row(
            children: <Widget>[
              Expanded(
                child: AleraTextField(
                  controller: controller.findInputController,
                  focusNode: controller.findInputFocusNode,
                  hintText: 'Find in File',
                  dense: true,
                  fillColor: AleraTokens.surfaceVariant,
                ),
              ),
              if (countLabel != null) ...<Widget>[
                const SizedBox(width: AleraTokens.space8),
                Text(countLabel, style: AleraTokens.monoStyle),
              ],
              const SizedBox(width: AleraTokens.space4),
              AleraIconButton(
                tooltip: 'Previous Match',
                icon: AleraIcons.chevronUp,
                onPressed: controller.matchCount == 0
                    ? null
                    : controller.previous,
                minSize: AleraTokens.space32,
              ),
              AleraIconButton(
                tooltip: 'Next Match',
                icon: AleraIcons.chevronDown,
                onPressed: controller.matchCount == 0 ? null : controller.next,
                minSize: AleraTokens.space32,
              ),
              AleraIconButton(
                tooltip: 'Close Search',
                icon: AleraIcons.close,
                onPressed: widget.onClose,
                minSize: AleraTokens.space32,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class const _WorkspaceEditorGoToLineBar({
  required final TextEditingController controller,
  required final FocusNode focusNode,
  required final int lineCount,
  required final ValueChanged<String> onSubmit,
  required final VoidCallback onClose,
}) extends StatelessWidget {
  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      onClose();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surface,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space4),
        child: Focus(
          onKeyEvent: _handleKey,
          child: Row(
            children: <Widget>[
              Expanded(
                child: AleraTextField(
                  key: const ValueKey<String>(
                    'workspace-editor-go-to-line-input',
                  ),
                  controller: controller,
                  focusNode: focusNode,
                  hintText: 'Go to Line (1-$lineCount)',
                  dense: true,
                  fillColor: AleraTokens.surfaceVariant,
                  keyboardType: TextInputType.number,
                  onSubmitted: onSubmit,
                ),
              ),
              const SizedBox(width: AleraTokens.space4),
              AleraIconButton(
                tooltip: 'Close Go to Line',
                icon: AleraIcons.close,
                onPressed: onClose,
                minSize: AleraTokens.space32,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
