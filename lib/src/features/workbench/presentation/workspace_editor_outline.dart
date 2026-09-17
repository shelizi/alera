part of 'workspace_editor_surface.dart';

const Duration _workspaceEditorOutlineRefreshDebounce = Duration(
  milliseconds: 350,
);

extension _WorkspaceEditorOutline on _WorkspaceEditorSurfaceState {
  void _toggleOutline() {
    if (_outlineOpen) {
      _outlineRefreshTimer?.cancel();
      _outlineRequestId += 1;
      setState(() {
        _outlineOpen = false;
        _outlineLoading = false;
      });
      return;
    }

    setState(() {
      _outlineOpen = true;
      _outlineLoading = true;
    });
    unawaited(_loadOutline());
  }

  void _refreshOutline() {
    if (!_outlineOpen || _loading || _loadError != null) return;
    unawaited(_loadOutline());
  }

  void _scheduleOutlineRefresh({bool immediate = false}) {
    if (!_outlineOpen || _loading || _loadError != null) return;
    _outlineRefreshTimer?.cancel();
    if (immediate) {
      unawaited(_loadOutline());
      return;
    }
    _outlineRefreshTimer = Timer(
      _workspaceEditorOutlineRefreshDebounce,
      () => unawaited(_loadOutline()),
    );
  }

  void _resetOutlineForDocumentChange() {
    _outlineRefreshTimer?.cancel();
    _outlineRequestId += 1;
    _outlineSymbols = const [];
    _outlineLoading = _outlineOpen;
    _outlineTruncated = false;
    _outlineSource = null;
  }

  Future<void> _loadOutline() async {
    if (!_outlineOpen || _loading || _loadError != null) return;
    _outlineRefreshTimer?.cancel();
    final requestId = ++_outlineRequestId;
    if (!_outlineLoading && mounted) {
      setState(() => _outlineLoading = true);
    }

    final result = await _controller.queryDocumentSymbols(maxSymbols: 1000);
    if (!mounted || !_outlineOpen || requestId != _outlineRequestId) return;

    setState(() {
      _outlineLoading = false;
      _outlineSymbols = result?.symbols ?? const [];
      _outlineTruncated = result?.truncated ?? false;
      _outlineSource = result?.source;
    });
  }

  void _navigateToOutlineSymbol(code_forge.CodeForgeDocumentSymbol symbol) {
    _focusNode.requestFocus();
    _controller.navigateToDocumentSymbol(symbol);
  }
}
