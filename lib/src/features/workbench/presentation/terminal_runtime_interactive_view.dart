part of 'terminal_runtime.dart';

class const _InteractiveTerminalView({
  super.key,
  required final _XtermTerminalSessionHandle session,
  required final bool autofocus,
  required final bool showCodeBlockCopyButton,
  final FocusOnKeyEventCallback? onKeyEvent,
}) extends StatefulWidget {
  @override
  State<_InteractiveTerminalView> createState() =>
      _InteractiveTerminalViewState();
}

class _InteractiveTerminalViewState extends State<_InteractiveTerminalView> {
  TerminalLinkRange? _hoveredLink;
  xterm.CellOffset? _lastHoverOffset;
  TerminalLinkRange? _lastHoverLink;
  TerminalCodeBlock? _hoveredCodeBlock;
  int? _lastHoverCodeRow;
  double _copyButtonTop = 8;
  bool _copiedCode = false;
  bool _pointerOverCopyButton = false;
  Timer? _copyFeedbackTimer;

  @override
  void initState() {
    super.initState();
    widget.session._scrollController.addListener(_clearHoverOnScroll);
  }

  @override
  void dispose() {
    widget.session._scrollController.removeListener(_clearHoverOnScroll);
    _copyFeedbackTimer?.cancel();
    super.dispose();
  }

  @override
  void didUpdateWidget(covariant _InteractiveTerminalView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.showCodeBlockCopyButton != widget.showCodeBlockCopyButton) {
      _copyFeedbackTimer?.cancel();
      // A setting change must clear any stale copy overlay immediately.
      _hoveredCodeBlock = null;
      _lastHoverCodeRow = null;
      _lastHoverOffset = null;
      _copiedCode = false;
      _pointerOverCopyButton = false;
    }
    if (oldWidget.session != widget.session) {
      oldWidget.session._scrollController.removeListener(_clearHoverOnScroll);
      widget.session._scrollController.addListener(_clearHoverOnScroll);
      _clearHoverState();
    }
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onExit: _handleMouseExit,
      onHover: _handleHover,
      child: Stack(
        fit: StackFit.expand,
        children: <Widget>[
          widget.session._buildTerminalView(
            autofocus: widget.autofocus,
            onKeyEvent: widget.onKeyEvent,
            mouseCursor: _hoveredLink == null
                ? SystemMouseCursors.text
                : SystemMouseCursors.click,
            onTapUp: _handleTapUp,
          ),
          if (widget.showCodeBlockCopyButton && _hoveredCodeBlock != null)
            Positioned(
              top: _copyButtonTop,
              right: 8,
              child: MouseRegion(
                onEnter: (_) => _pointerOverCopyButton = true,
                onExit: (_) => _pointerOverCopyButton = false,
                child: Material(
                  elevation: 2,
                  color: Theme.of(context).colorScheme.surfaceContainerHigh,
                  borderRadius: BorderRadius.circular(6),
                  child: IconButton(
                    key: const ValueKey<String>('terminal-copy-code-button'),
                    tooltip: context.tr(_copiedCode ? 'Copied' : 'Copy code'),
                    onPressed: () => unawaited(_copyCodeBlock()),
                    icon: Icon(
                      _copiedCode ? Icons.check : Icons.content_copy,
                      size: 16,
                    ),
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints.tightFor(
                      width: 32,
                      height: 32,
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }

  void _handleHover(PointerHoverEvent event) {
    if (_pointerOverCopyButton) return;
    final viewState = widget.session._terminalViewKey.currentState;
    if (viewState == null) {
      return;
    }
    final localPosition = viewState.renderTerminal.globalToLocal(
      event.position,
    );
    final offset = viewState.renderTerminal.getCellOffset(localPosition);
    if (_lastHoverOffset == offset) {
      _setHoveredLink(_lastHoverLink);
      return;
    }
    final link = widget.session._linkAt(offset);
    _lastHoverOffset = offset;
    _lastHoverLink = link;
    _setHoveredLink(link);
    if (widget.showCodeBlockCopyButton) {
      _handleCodeHover(offset, event.localPosition.dy);
    }
  }

  void _handleCodeHover(xterm.CellOffset offset, double pointerY) {
    if (!widget.showCodeBlockCopyButton || _lastHoverCodeRow == offset.y)
      return;
    _lastHoverCodeRow = offset.y;
    final block = resolveTerminalCodeBlockAt(
      terminal: widget.session._terminal,
      offset: offset,
    );
    final height = context.size?.height ?? 0;
    final buttonTop = (pointerY - 16)
        .clamp(4.0, (height - 36).clamp(4.0, double.infinity))
        .toDouble();
    final previous = _hoveredCodeBlock;
    final sameBlock =
        previous?.openingRow == block?.openingRow &&
        previous?.closingRow == block?.closingRow &&
        previous?.code == block?.code;
    if (sameBlock && (block == null || _copyButtonTop == buttonTop)) return;
    setState(() {
      _hoveredCodeBlock = block;
      _copyButtonTop = buttonTop;
      if (!sameBlock) {
        _copyFeedbackTimer?.cancel();
        _copiedCode = false;
      }
    });
  }

  Future<void> _copyCodeBlock() async {
    if (!widget.showCodeBlockCopyButton) return;
    final target = _hoveredCodeBlock;
    if (target == null) return;
    // A streaming TUI can repaint or replace rows between hover and click.
    // Resolve the contents again instead of copying stale terminal output.
    final current = resolveTerminalCodeBlockAt(
      terminal: widget.session._terminal,
      offset: xterm.CellOffset(0, target.openingRow),
    );
    if (current == null ||
        current.openingRow != target.openingRow ||
        current.closingRow != target.closingRow ||
        current.code != target.code) {
      _clearHoverState();
      return;
    }
    try {
      // Claude's TUI may replace unshown source rows with a viewport hint.
      // When an exact same-workspace transcript match exists, copy the full
      // canonical code; otherwise retain the ordinary terminal selection.
      final completeCode = current.code.contains('Jump to bottom')
          ? await recoverCompleteClaudeCodeFromTranscript(
              workspacePath: widget.session.workspacePath,
              visibleCode: current.code,
            )
          : null;
      if (!mounted ||
          !widget.showCodeBlockCopyButton ||
          !identical(_hoveredCodeBlock, target))
        return;
      if (completeCode == null && current.code.contains('Jump to bottom')) {
        // A virtualized PTY buffer is not the full source. Do not silently
        // claim a successful copy of a truncated file with an injected hint.
        ScaffoldMessenger.maybeOf(context)?.showSnackBar(
          SnackBar(content: Text(context.tr('Could not copy code'))),
        );
        return;
      }
      await widget.session._launchInputOwner.clipboard.writeText(
        completeCode ?? current.code,
      );
      if (!mounted || !identical(_hoveredCodeBlock, target)) return;
      _copyFeedbackTimer?.cancel();
      setState(() => _copiedCode = true);
      // Do not leave a permanent checkmark that looks like the button vanished.
      // Keep the same button actionable for repeated copies without re-hovering.
      _copyFeedbackTimer = Timer(const Duration(milliseconds: 1200), () {
        _copyFeedbackTimer = null;
        if (!mounted || !widget.showCodeBlockCopyButton || !_copiedCode) return;
        setState(() => _copiedCode = false);
      });
    } catch (_) {
      if (!mounted) return;
      ScaffoldMessenger.maybeOf(context)?.showSnackBar(
        SnackBar(content: Text(context.tr('Could not copy code'))),
      );
    } finally {
      if (mounted) widget.session._requestFocusNow();
    }
  }

  void _handleTapUp(TapUpDetails _, xterm.CellOffset offset) {
    if (!isTerminalLinkActivation()) {
      return;
    }
    final link = widget.session._linkAt(offset);
    if (link == null) {
      return;
    }
    unawaited(_openLink(link.uri));
  }

  Future<void> _openLink(Uri uri) async {
    try {
      await widget.session._openLink(uri);
    } catch (_) {
      if (!mounted) {
        return;
      }
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${context.tr('Could not open link')}: $uri')),
      );
    }
  }

  void _setHoveredLink(TerminalLinkRange? link) {
    if (_hoveredLink == link) {
      return;
    }
    setState(() {
      _hoveredLink = link;
    });
  }

  void _handleMouseExit(PointerExitEvent event) {
    // Tooltips and overlay layers can change MouseRegion hit testing even
    // while the physical pointer stays inside the terminal. In that case,
    // do not dismiss a copy button that the user is actively clicking.
    final render = context.findRenderObject();
    if (render is RenderBox && render.attached) {
      final local = render.globalToLocal(event.position);
      if ((Offset.zero & render.size).contains(local)) return;
    }
    _clearHoverState();
  }

  void _clearHoverOnScroll() {
    // Restoring keyboard focus after copying can scroll xterm to its cursor.
    // Keep the button under the pointer so another click can copy again;
    // _copyCodeBlock revalidates the buffer before writing to the clipboard.
    if (_pointerOverCopyButton && _hoveredCodeBlock != null) return;
    _clearHoverState();
  }

  void _clearHoverState() {
    _copyFeedbackTimer?.cancel();
    _lastHoverOffset = null;
    _lastHoverLink = null;
    _lastHoverCodeRow = null;
    _pointerOverCopyButton = false;
    if (_hoveredLink == null && _hoveredCodeBlock == null) return;
    setState(() {
      _hoveredLink = null;
      _hoveredCodeBlock = null;
      _copiedCode = false;
    });
  }
}

class const _TerminalPtySize({
  required final int cols,
  required final int rows,
  required final int cellWidthPx,
  required final int cellHeightPx,
});

const Duration _ptyResizeDebounceDuration = Duration(milliseconds: 150);

final class _TerminalVisibilityLease(final VoidCallback _onDispose)
    implements TerminalVisibilityLease {
  bool _disposed = false;

  @override
  void dispose() {
    if (_disposed) {
      return;
    }
    _disposed = true;
    _onDispose();
  }
}
