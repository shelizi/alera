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
  // Copy actions are derived from the visible terminal buffer, not mouse hover.
  List<_VisibleCodeCopyTarget> _visibleCodeBlocks = const [];
  xterm.Terminal? _observedTerminal;
  bool _copyRefreshScheduled = false;
  String? _primaryCopyBlockId;
  String? _copiedCodeBlockId;
  double? _lastCopyViewportTop;
  Timer? _copyFeedbackTimer;
  Timer? _copyDismissTimer;
  bool _copyButtonsDismissed = false;
  bool _pointerOverCodeBlock = false;
  bool _pointerOverCopyButton = false;
  List<ClaudeSourceCodeBlock> _transcriptCodeBlocks = const [];
  DateTime? _nextClaudeTranscriptRead;
  bool _loadingClaudeTranscript = false;
  int _transcriptReadGeneration = 0;

  @override
  void initState() {
    super.initState();
    widget.session._scrollController.addListener(_scheduleCopyButtonsRefresh);
    widget.session._claudeCodeMessageStore?.addListener(_onClaudeSourceUpdate);
    _observeTerminal(widget.session._terminal);
    _scheduleCopyButtonsRefresh();
  }

  @override
  void dispose() {
    widget.session._scrollController.removeListener(
      _scheduleCopyButtonsRefresh,
    );
    widget.session._claudeCodeMessageStore?.removeListener(
      _onClaudeSourceUpdate,
    );
    _observedTerminal?.removeListener(_scheduleCopyButtonsRefresh);
    _copyFeedbackTimer?.cancel();
    _copyDismissTimer?.cancel();
    _transcriptReadGeneration++;
    super.dispose();
  }

  @override
  void didUpdateWidget(covariant _InteractiveTerminalView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.showCodeBlockCopyButton != widget.showCodeBlockCopyButton) {
      _copyFeedbackTimer?.cancel();
      _cancelCopyDismissTimer();
      _copyButtonsDismissed = false;
      _pointerOverCodeBlock = false;
      _pointerOverCopyButton = false;
      _visibleCodeBlocks = const [];
      _lastCopyViewportTop = null;
      _primaryCopyBlockId = null;
      _copiedCodeBlockId = null;
      _lastHoverOffset = null;
      _transcriptCodeBlocks = const [];
      _nextClaudeTranscriptRead = null;
      _transcriptReadGeneration++;
      _loadingClaudeTranscript = false;
    }
    if (oldWidget.session != widget.session) {
      oldWidget.session._scrollController.removeListener(
        _scheduleCopyButtonsRefresh,
      );
      oldWidget.session._claudeCodeMessageStore?.removeListener(
        _onClaudeSourceUpdate,
      );
      widget.session._scrollController.addListener(_scheduleCopyButtonsRefresh);
      widget.session._claudeCodeMessageStore?.addListener(
        _onClaudeSourceUpdate,
      );
      _transcriptCodeBlocks = const [];
      _nextClaudeTranscriptRead = null;
      _transcriptReadGeneration++;
      _loadingClaudeTranscript = false;
      _clearHoverState();
    }
    _observeTerminal(widget.session._terminal);
    _scheduleCopyButtonsRefresh();
  }

  void _observeTerminal(xterm.Terminal terminal) {
    if (identical(terminal, _observedTerminal)) return;
    _observedTerminal?.removeListener(_scheduleCopyButtonsRefresh);
    _observedTerminal = terminal;
    terminal.addListener(_scheduleCopyButtonsRefresh);
    _visibleCodeBlocks = const [];
    _lastCopyViewportTop = null;
  }

  void _scheduleCopyButtonsRefresh() {
    if (!mounted || !widget.showCodeBlockCopyButton || _copyRefreshScheduled) {
      return;
    }
    _copyRefreshScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _copyRefreshScheduled = false;
      if (mounted && widget.showCodeBlockCopyButton) {
        _refreshVisibleCodeBlocks();
      }
    });
  }

  /// Scan only the rendered viewport once per frame, skipping each recognized
  /// block after finding a witness. Do not scan all terminal scrollback or
  /// resolve code on each mouse movement.
  void _refreshVisibleCodeBlocks() {
    final view = widget.session._terminalViewKey.currentState;
    if (view == null) return;
    final renderer = view.renderTerminal;
    if (!renderer.attached || !renderer.hasSize) return;
    final lines = widget.session._terminal.buffer.lines;
    if (lines.length == 0) return;
    final firstRow = renderer.getCellOffset(Offset.zero).y;
    final bottomY = renderer.size.height > 1 ? renderer.size.height - 1 : 0.0;
    final lastRow = renderer.getCellOffset(Offset(0, bottomY)).y;
    final viewportTop = renderer.getOffset(xterm.CellOffset(0, firstRow)).dy;
    final visible = <_VisibleCodeCopyTarget>[];
    for (var row = firstRow; row <= lastRow && visible.length < 24; row++) {
      if (lines[row].getText().trim().isEmpty) continue;
      final block = _resolveCodeBlockAt(xterm.CellOffset(0, row));
      if (block == null || !block.containsRow(row)) continue;
      visible.add(_VisibleCodeCopyTarget(block: block, witnessRow: row));
      // Long source blocks need only one button after their last line.
      if (block.closingRow > row) row = block.closingRow;
    }
    if (visible.length == _visibleCodeBlocks.length) {
      var unchanged = true;
      for (var i = 0; i < visible.length; i++) {
        if (visible[i].id != _visibleCodeBlocks[i].id ||
            visible[i].witnessRow != _visibleCodeBlocks[i].witnessRow) {
          unchanged = false;
          break;
        }
      }
      if (unchanged && _lastCopyViewportTop == viewportTop) return;
    }
    setState(() {
      _lastCopyViewportTop = viewportTop;
      _visibleCodeBlocks = visible;
      if (!visible.any((target) => target.id == _copiedCodeBlockId)) {
        _copiedCodeBlockId = null;
        _copyFeedbackTimer?.cancel();
      }
      if (!visible.any((target) => target.id == _primaryCopyBlockId)) {
        _primaryCopyBlockId = visible.isEmpty ? null : visible.first.id;
      }
    });
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
          if (widget.showCodeBlockCopyButton && !_copyButtonsDismissed)
            for (final target in _visibleCodeBlocks)
              Positioned(
                top: _copyTopBelowCode(target.block),
                right: 8,
                child: MouseRegion(
                  onEnter: (_) => _handleCopyButtonEnter(),
                  onExit: (_) => _handleCopyButtonExit(),
                  child: Material(
                    elevation: 2,
                    color: Theme.of(context).colorScheme.surfaceContainerHigh,
                    borderRadius: BorderRadius.circular(6),
                    child: IconButton(
                      key: ValueKey<String>(
                        target.id == _primaryCopyBlockId
                            ? 'terminal-copy-code-button'
                            : 'terminal-copy-code-button-${target.block.openingRow}',
                      ),
                      tooltip: context.tr(
                        _copiedCodeBlockId == target.id
                            ? 'Copied'
                            : 'Copy code',
                      ),
                      onPressed: () => unawaited(_copyCodeBlock(target)),
                      icon: Icon(
                        _copiedCodeBlockId == target.id
                            ? Icons.check
                            : Icons.content_copy,
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
    final viewState = widget.session._terminalViewKey.currentState;
    if (viewState == null) return;
    final localPosition = viewState.renderTerminal.globalToLocal(
      event.position,
    );
    final offset = viewState.renderTerminal.getCellOffset(localPosition);
    if (widget.showCodeBlockCopyButton) {
      _maybeLoadClaudeTranscriptSource(offset);
    }
    if (widget.showCodeBlockCopyButton && !_pointerOverCopyButton) {
      _VisibleCodeCopyTarget? target;
      for (final candidate in _visibleCodeBlocks) {
        if (!candidate.block.containsRow(offset.y)) continue;
        target = candidate;
        break;
      }
      final selected = target;
      if (selected != null) {
        _pointerOverCodeBlock = true;
        _cancelCopyDismissTimer();
        if (_copyButtonsDismissed || _primaryCopyBlockId != selected.id) {
          setState(() {
            _copyButtonsDismissed = false;
            _primaryCopyBlockId = selected.id;
          });
        }
      } else if (_pointerOverCodeBlock) {
        _pointerOverCodeBlock = false;
        _startCopyDismissTimer();
      }
    }
    if (_lastHoverOffset == offset) return;
    _lastHoverOffset = offset;
    _setHoveredLink(widget.session._linkAt(offset));
  }

  void _handleCopyButtonEnter() {
    _pointerOverCopyButton = true;
    _cancelCopyDismissTimer();
  }

  void _handleCopyButtonExit() {
    _pointerOverCopyButton = false;
    _pointerOverCodeBlock = false;
    _startCopyDismissTimer();
  }

  void _cancelCopyDismissTimer() {
    _copyDismissTimer?.cancel();
    _copyDismissTimer = null;
  }

  void _startCopyDismissTimer() {
    if (!widget.showCodeBlockCopyButton ||
        _copyButtonsDismissed ||
        _pointerOverCodeBlock ||
        _pointerOverCopyButton ||
        _copyDismissTimer != null ||
        _visibleCodeBlocks.isEmpty) {
      return;
    }
    _copyDismissTimer = Timer(const Duration(seconds: 10), () {
      _copyDismissTimer = null;
      if (!mounted ||
          !widget.showCodeBlockCopyButton ||
          _pointerOverCodeBlock ||
          _pointerOverCopyButton) {
        return;
      }
      setState(() => _copyButtonsDismissed = true);
    });
  }

  /// Manually launched Claude does not receive Alera's optional hook args.
  /// Read original same-workspace Markdown instead of guessing from PTY text.
  void _maybeLoadClaudeTranscriptSource(xterm.CellOffset offset) {
    if (!widget.showCodeBlockCopyButton ||
        widget.session._claudeCodeMessageStore == null ||
        _loadingClaudeTranscript) {
      return;
    }
    final id = widget.session.terminalSessionId;
    final hooked = widget.session._claudeCodeMessageStore!.blocksForTerminal(
      id,
    );
    // A user can switch from a hooked Claude run to a manual one in the
    // same terminal. Old hook source should not block transcript recovery.
    if (hooked.isNotEmpty &&
        resolveClaudeSourceCodeBlockAt(
              terminal: widget.session._terminal,
              offset: offset,
              sourceBlocks: hooked,
            ) !=
            null) {
      return;
    }
    final now = DateTime.now();
    if (_nextClaudeTranscriptRead case final next? when now.isBefore(next)) {
      return;
    }
    final workspacePath = widget.session.workspacePath;
    if (workspacePath.isEmpty) return;
    _nextClaudeTranscriptRead = now.add(const Duration(seconds: 8));
    _loadingClaudeTranscript = true;
    final sourceSession = widget.session;
    final generation = ++_transcriptReadGeneration;
    unawaited(() async {
      try {
        final blocks = await sourceSession._claudeTranscriptSourceLoader(
          workspacePath,
        );
        if (!mounted ||
            !widget.showCodeBlockCopyButton ||
            !identical(widget.session, sourceSession) ||
            generation != _transcriptReadGeneration) {
          return;
        }
        _transcriptCodeBlocks = blocks;
        _refreshCodeButtonsAfterSourceUpdate();
      } catch (_) {
        // Failure to read an optional local transcript must not affect PTY.
      } finally {
        if (generation == _transcriptReadGeneration) {
          _loadingClaudeTranscript = false;
        }
      }
    }());
  }

  void _onClaudeSourceUpdate() => _refreshCodeButtonsAfterSourceUpdate();

  void _refreshCodeButtonsAfterSourceUpdate() {
    if (!mounted || !widget.showCodeBlockCopyButton) return;
    // Hook/transcript source can arrive with no subsequent terminal output.
    if (SchedulerBinding.instance.schedulerPhase ==
        SchedulerPhase.persistentCallbacks) {
      _scheduleCopyButtonsRefresh();
    } else {
      _refreshVisibleCodeBlocks();
    }
  }

  TerminalCodeBlock? _resolveCodeBlockAt(xterm.CellOffset offset) {
    final store = widget.session._claudeCodeMessageStore;
    if (store != null) {
      final id = widget.session.terminalSessionId;
      final originals = <ClaudeSourceCodeBlock>[
        ...store.blocksForTerminal(id),
        ..._transcriptCodeBlocks,
      ];
      if (originals.isNotEmpty) {
        final match = resolveClaudeSourceCodeBlockAt(
          terminal: widget.session._terminal,
          offset: offset,
          sourceBlocks: originals,
        );
        if (match != null) return match;
      }
      // A normal shell can still copy explicit fenced Markdown. Never use
      // Claude-specific syntax guessing in the production Classic terminal.
      return resolveTerminalCodeBlockAt(
        terminal: widget.session._terminal,
        offset: offset,
        includeClaudeHeuristics: false,
      );
    }
    return resolveTerminalCodeBlockAt(
      terminal: widget.session._terminal,
      offset: offset,
    );
  }

  /// Anchor the copy action *after* the rendered block, not at the mouse.
  /// xterm's render box owns viewport/scrollback positioning, so convert its
  /// cell coordinates into this overlay's coordinates after scrolling.
  double _copyTopBelowCode(TerminalCodeBlock block) {
    final viewState = widget.session._terminalViewKey.currentState;
    final overlay = context.findRenderObject();
    if (viewState == null || overlay is! RenderBox || !overlay.attached) {
      return 8;
    }
    final renderer = viewState.renderTerminal;
    final lastLineTop = renderer.getOffset(
      xterm.CellOffset(0, block.closingRow),
    );
    final afterLastLine = renderer.localToGlobal(
      lastLineTop + Offset(0, renderer.cellSize.height),
    );
    final idealTop = overlay.globalToLocal(afterLastLine).dy + 4;
    final maxTop = (overlay.size.height - 36).clamp(4.0, double.infinity);
    // The end can be outside the viewport, or at its last line. Keep the
    // action reachable at the bottom edge in those cases, never above the
    // last rendered code row when there is room below it.
    return idealTop.clamp(4.0, maxTop).toDouble();
  }

  Future<void> _copyCodeBlock(_VisibleCodeCopyTarget target) async {
    if (!widget.showCodeBlockCopyButton) return;
    // Revalidate the buffer before copying; streaming TUI redraws must never
    // copy stale contents from a row that used to contain another answer.
    final current = _resolveCodeBlockAt(xterm.CellOffset(0, target.witnessRow));
    if (current == null ||
        current.openingRow != target.block.openingRow ||
        current.closingRow != target.block.closingRow ||
        current.code != target.block.code) {
      _scheduleCopyButtonsRefresh();
      return;
    }
    try {
      await widget.session._launchInputOwner.clipboard.writeText(current.code);
      if (!mounted ||
          !_visibleCodeBlocks.any((block) => block.id == target.id)) {
        return;
      }
      _copyFeedbackTimer?.cancel();
      setState(() => _copiedCodeBlockId = target.id);
      _copyFeedbackTimer = Timer(const Duration(milliseconds: 1200), () {
        _copyFeedbackTimer = null;
        if (!mounted ||
            !widget.showCodeBlockCopyButton ||
            _copiedCodeBlockId != target.id) {
          return;
        }
        setState(() => _copiedCodeBlockId = null);
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
    _lastHoverOffset = null;
    _setHoveredLink(null);
    _pointerOverCodeBlock = false;
    _startCopyDismissTimer();
  }

  void _clearHoverState() {
    _copyFeedbackTimer?.cancel();
    _cancelCopyDismissTimer();
    _copyButtonsDismissed = false;
    _pointerOverCodeBlock = false;
    _pointerOverCopyButton = false;
    _lastHoverOffset = null;
    _primaryCopyBlockId = null;
    _copiedCodeBlockId = null;
    _visibleCodeBlocks = const [];
    _lastCopyViewportTop = null;
    if (_hoveredLink != null) setState(() => _hoveredLink = null);
  }
}

final class _VisibleCodeCopyTarget {
  const _VisibleCodeCopyTarget({required this.block, required this.witnessRow});

  final TerminalCodeBlock block;
  final int witnessRow;

  String get id =>
      '${block.openingRow}:${block.closingRow}:${block.code.hashCode}';
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
