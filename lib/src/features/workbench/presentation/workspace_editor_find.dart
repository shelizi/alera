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

class _WorkspaceEditorFindBar extends StatelessWidget
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

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) {
      return KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      onClose();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.f3) {
      if (HardwareKeyboard.instance.isShiftPressed) {
        controller.previous();
      } else {
        controller.next();
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
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
