import 'package:flutter/material.dart';

enum TabContentKind { editor, terminal }

/// An object representing the long-lived content; native window and widget
/// lifecycle must never dispose this object during a tab move.
final class DemoTabSession {
  DemoTabSession({
    required this.tabId,
    required this.kind,
    required String initialText,
  }) : editorText = TextEditingController(text: initialText),
       _savedText = initialText;

  final String tabId;
  final TabContentKind kind;
  final TextEditingController editorText;
  final List<String> terminalTranscript = <String>[];
  final ValueNotifier<int> terminalRevision = ValueNotifier<int>(0);
  String _savedText;
  int terminalHostPid = 0;
  int terminalHostStartCount = 0;
  int saveCount = 0;

  bool get isDirty => editorText.text != _savedText;
  String get savedText => _savedText;

  void saveSnapshot() {
    _savedText = editorText.text;
    saveCount++;
  }

  void appendTerminalOutput(String text) {
    terminalTranscript.add(text);
    terminalRevision.value++;
  }

  void dispose() {
    editorText.dispose();
    terminalRevision.dispose();
  }
}

/// A single-use transfer ticket. A stale ticket cannot steal ownership if a
/// window closed, a transfer was cancelled, or another handoff began.
final class TabTransferTicket {
  const TabTransferTicket({
    required this.tabId,
    required this.generation,
    required this.fromViewId,
    required this.toViewId,
  });

  final String tabId;
  final int generation;
  final int fromViewId;
  final int toViewId;
}

final class _TabSlot {
  _TabSlot(this.session, this.ownerViewId);

  final DemoTabSession session;
  int? ownerViewId;
  int? mountedViewId;
  int generation = 0;
  TabTransferTicket? pending;
}

/// A transfer must first unmount the source view, then commit to the target.
/// No second widget is allowed to bind the same TextEditingController / PTY.
/// Source remains recoverable until a successfully committed ticket.
final class TabLeaseRegistry extends ChangeNotifier {
  TabLeaseRegistry({required this.mainViewId}) {
    if (mainViewId <= 0) {
      throw ArgumentError.value(mainViewId, 'mainViewId');
    }
  }

  final int mainViewId;
  final Map<String, _TabSlot> _slots = <String, _TabSlot>{};
  bool _disposed = false;

  Iterable<DemoTabSession> get sessions => _slots.values.map((e) => e.session);

  void register(DemoTabSession session) {
    _checkNotDisposed();
    if (_slots.containsKey(session.tabId)) {
      throw StateError('Duplicate tab ID ${session.tabId}');
    }
    _slots[session.tabId] = _TabSlot(session, mainViewId);
    notifyListeners();
  }

  DemoTabSession session(String tabId) => _slot(tabId).session;
  int? ownerOf(String tabId) => _slot(tabId).ownerViewId;
  int? mountedIn(String tabId) => _slot(tabId).mountedViewId;
  bool inTransit(String tabId) => _slot(tabId).pending != null;

  bool canMount(String tabId, int viewId) {
    final slot = _slot(tabId);
    return slot.ownerViewId == viewId && slot.pending == null;
  }

  void claimMount(String tabId, int viewId) {
    final slot = _slot(tabId);
    if (!canMount(tabId, viewId)) {
      throw StateError('Unauthorized tab mount: $tabId in view $viewId');
    }
    if (slot.mountedViewId != null) {
      throw StateError('Tab $tabId is still mounted in ${slot.mountedViewId}');
    }
    slot.mountedViewId = viewId;
    // Do not notify in initState/build; avoids setState during build.
  }

  void releaseMount(String tabId, int viewId) {
    final slot = _slot(tabId);
    if (slot.mountedViewId == viewId) {
      slot.mountedViewId = null;
    }
    // Do not notify during dispose.
  }

  TabTransferTicket beginTransfer(String tabId, int toViewId) {
    _checkNotDisposed();
    if (toViewId <= 0) throw ArgumentError.value(toViewId, 'toViewId');
    final slot = _slot(tabId);
    final source = slot.ownerViewId;
    if (slot.pending != null) {
      throw StateError('Tab $tabId is already transferring');
    }
    if (source == null || source == toViewId) {
      throw StateError('Invalid transfer from $source to $toViewId');
    }
    final ticket = TabTransferTicket(
      tabId: tabId,
      generation: ++slot.generation,
      fromViewId: source,
      toViewId: toViewId,
    );
    slot.pending = ticket;
    slot.ownerViewId = null; // removes source from widget tree
    notifyListeners();
    return ticket;
  }

  /// Must be called after source widget dispose/endOfFrame. Never silently
  /// mount both windows while a TextEditingController is attached.
  bool commitTransfer(TabTransferTicket ticket) {
    _checkNotDisposed();
    final slot = _slot(ticket.tabId);
    if (!_isCurrent(slot, ticket)) return false;
    if (slot.mountedViewId != null) {
      throw StateError('Source still mounted: ${slot.mountedViewId}');
    }
    slot.pending = null;
    slot.ownerViewId = ticket.toViewId;
    notifyListeners();
    return true;
  }

  bool rollbackTransfer(TabTransferTicket ticket) {
    _checkNotDisposed();
    final slot = _slot(ticket.tabId);
    if (!_isCurrent(slot, ticket)) return false;
    if (slot.mountedViewId != null) {
      throw StateError('Rollback while tab is still mounted');
    }
    slot.pending = null;
    slot.ownerViewId = ticket.fromViewId;
    notifyListeners();
    return true;
  }

  /// Unexpected native close is not a 'save/discard'. Keep the backing editor
  /// and terminal session objects and restore their owner to the main view.
  void recoverClosedWindow(int viewId) {
    _checkNotDisposed();
    if (viewId == mainViewId) {
      // Preserving child views after main closes requires a different policy.
      throw StateError('Cannot auto-recover when the main window closes');
    }
    var changed = false;
    for (final slot in _slots.values) {
      if (slot.pending case final ticket?) {
        if (ticket.toViewId == viewId || ticket.fromViewId == viewId) {
          slot.pending = null;
          slot.generation++;
          slot.ownerViewId = ticket.fromViewId == viewId
              ? mainViewId
              : ticket.fromViewId;
          changed = true;
        }
      } else if (slot.ownerViewId == viewId) {
        slot.generation++;
        slot.ownerViewId = mainViewId;
        changed = true;
      }
    }
    if (changed) notifyListeners();
  }

  /// Used only when the target native window has already gone away, so any
  /// Widget dispose callbacks may not have reached Flutter yet.
  void acknowledgeWindowUnmount(int viewId) {
    for (final slot in _slots.values) {
      if (slot.mountedViewId == viewId) {
        slot.mountedViewId = null;
      }
    }
  }

  bool _isCurrent(_TabSlot slot, TabTransferTicket ticket) =>
      identical(slot.pending, ticket) && slot.generation == ticket.generation;

  _TabSlot _slot(String tabId) {
    _checkNotDisposed();
    final slot = _slots[tabId];
    if (slot == null) throw StateError('Unknown tab $tabId');
    return slot;
  }

  void _checkNotDisposed() {
    if (_disposed) throw StateError('TabLeaseRegistry is disposed');
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    for (final slot in _slots.values) {
      slot.session.dispose();
    }
    _slots.clear();
    super.dispose();
  }
}

/// Presents the same long-lived editor controller under exactly one Flutter
/// View. The state survives Widget disposal/rebuild across native windows.
final class EditorLeaseView extends StatefulWidget {
  const EditorLeaseView({
    super.key,
    required this.registry,
    required this.tabId,
    required this.viewId,
    this.onMounted,
    this.onUnmounted,
  });

  final TabLeaseRegistry registry;
  final String tabId;
  final int viewId;
  final VoidCallback? onMounted;
  final VoidCallback? onUnmounted;

  @override
  State<EditorLeaseView> createState() => _EditorLeaseViewState();
}

final class _EditorLeaseViewState extends State<EditorLeaseView> {
  @override
  void initState() {
    super.initState();
    widget.registry.claimMount(widget.tabId, widget.viewId);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onMounted?.call();
    });
  }

  @override
  void dispose() {
    widget.registry.releaseMount(widget.tabId, widget.viewId);
    widget.onUnmounted?.call();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final session = widget.registry.session(widget.tabId);
    return Column(
      children: <Widget>[
        Text('Editor: ${session.tabId} • View ${widget.viewId}'),
        Expanded(
          child: TextField(
            key: ValueKey<String>('editor-input-${widget.viewId}'),
            expands: true,
            maxLines: null,
            controller: session.editorText,
          ),
        ),
        Text('Unsaved changes: ${session.isDirty}'),
      ],
    );
  }
}
