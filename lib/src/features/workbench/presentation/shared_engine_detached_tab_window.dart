import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:multiview_desktop/multiview_desktop.dart';

/// Experimental Windows-only backend. Registered only if the application's
/// root is runMultiApp and all views share the same root ProviderScope.
///
/// DO NOT use for terminal, Git Diff, PDF or other tab types until their native
/// per-view resources have been audited. Those retain the original new-process
/// path, including the explicit-save requirement.
final class SharedEngineDetachedTabWindows extends ChangeNotifier
    implements SharedEngineDetachedTabBackend, WindowObserver {
  SharedEngineDetachedTabWindows({
    Future<int> Function(Widget Function(BuildContext, int), WindowOptions)?
    openNativeWindow,
    Future<void> Function()? sourceFrameBarrier,
    Future<bool> Function(int)? closeNativeWindow,
  }) : _openNativeWindow =
           openNativeWindow ??
           ((builder, options) => openWindow(builder, options: options)),
       _sourceFrameBarrier =
           sourceFrameBarrier ?? (() => WidgetsBinding.instance.endOfFrame),
       _closeNativeWindow =
           closeNativeWindow ??
           ((viewId) => MultiViewDesktop.fromId(viewId).closeWindow());

  final Future<int> Function(Widget Function(BuildContext, int), WindowOptions)
  _openNativeWindow;
  final Future<void> Function() _sourceFrameBarrier;
  final Future<bool> Function(int) _closeNativeWindow;

  final Map<String, _SharedTabWindow> _active = <String, _SharedTabWindow>{};
  final Set<String> _pending = <String>{};
  bool _disposed = false;

  @override
  Set<String> get detachedTabIds => Set.unmodifiable(_active.keys);

  @override
  bool isDetached(String tabId) => _active.containsKey(tabId);

  @override
  bool isLaunching(String tabId) => _pending.contains(tabId);

  @override
  bool supports(DetachedTabDescriptor descriptor) =>
      Platform.isWindows && descriptor.tab.kind == WorkspaceTabKind.editor;

  bool canRender(String tabId, int viewId) {
    final entry = _active[tabId];
    return entry != null && entry.viewId == viewId && entry.contentMayMount;
  }

  @override
  Future<bool> detach(DetachedTabDescriptor descriptor) async {
    if (_disposed || !supports(descriptor)) return false;
    final id = descriptor.tab.id;
    if (_active.containsKey(id) || !_pending.add(id)) return false;
    notifyListeners();

    _SharedTabWindow? entry;
    try {
      // The native window initially shows only a lightweight placeholder.
      // Reparenting an Editor's CodeForgeController directly is not supported.
      // After the new HWND exists, hide the old widget, wait for dispose to
      // snapshot the dirty document into EditorSessionRegistry, then build a
      // fresh Editor widget under the *same* registry/ProviderScope.
      final windowId = await _openNativeWindow(
        (context, nativeId) => _SharedDetachedEditorWindow(
          descriptor: descriptor,
          viewId: nativeId,
          controller: this,
        ),
        WindowOptions(
          title: 'Alera - ${descriptor.tab.title}',
          size: const Size(1050, 720),
        ),
      );
      entry = _SharedTabWindow(descriptor, windowId);
      if (_disposed || !_pending.contains(id)) {
        await _closeNativeWindow(windowId);
        return false;
      }
      _active[id] = entry;
      notifyListeners(); // Workbench hides and unmounts its source tab.
      await _sourceFrameBarrier();
      if (_disposed || !identical(_active[id], entry)) {
        return false; // OS window closed while source was unmounting.
      }
      entry.contentMayMount = true;
      notifyListeners(); // Only now is the child allowed to build the Editor.
      await entry.contentMounted.future.timeout(const Duration(seconds: 15));
      return identical(_active[id], entry) && !_disposed;
    } catch (error, stack) {
      Logger(
        'SharedEngineDetachedTabWindows',
      ).warning('Failed to detach editor ${descriptor.tab.id}', error, stack);
      if (entry != null && identical(_active[id], entry)) {
        _active.remove(id);
        notifyListeners();
        try {
          await _closeNativeWindow(entry.viewId);
        } catch (_) {}
      }
      rethrow;
    } finally {
      _pending.remove(id);
      if (!_disposed) notifyListeners();
    }
  }

  void reportContentMounted(String tabId, int viewId) {
    final entry = _active[tabId];
    if (entry == null ||
        entry.viewId != viewId ||
        !entry.contentMayMount ||
        entry.contentMounted.isCompleted) {
      return;
    }
    entry.contentMounted.complete();
  }

  Future<bool> closeTabWindow(String tabId, int viewId) async {
    final entry = _active[tabId];
    if (entry == null || entry.viewId != viewId) return false;
    return _closeNativeWindow(viewId);
  }

  @override
  void onWindowOpened(int viewId, {int? parentViewId}) {}

  @override
  void onDialogOpened(int dialogId, {required int parentViewId}) {}

  @override
  void onDialogClose(int dialogId) {}

  @override
  void onAnchorChanged(int? previousViewId, int? newViewId) {}

  @override
  void onWindowEvent(int viewId, String eventName) {}

  @override
  void onDialogEvent(int viewId, String eventName) {}

  @override
  void onWindowClosed(int viewId) {
    final tabId = _active.entries
        .where((e) => e.value.viewId == viewId)
        .map((e) => e.key)
        .firstOrNull;
    if (tabId == null) return;
    final entry = _active.remove(tabId)!;
    if (!entry.contentMounted.isCompleted) {
      // A close may arrive *before* detach() awaits this completer at the
      // source-unmount barrier. Completing with an error at that point emits
      // an unhandled zone exception. Wake the waiter normally and check the
      // removed entry to report a failed handoff without losing the tab.
      entry.contentMounted.complete();
    }
    // The same EditorSessionRegistry is still alive in globalScope. This
    // notification remounts the original tab without discarding its buffer.
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    for (final entry in _active.values) {
      if (!entry.contentMounted.isCompleted) {
        entry.contentMounted.complete();
      }
    }
    _active.clear();
    _pending.clear();
    super.dispose();
  }
}

final class _SharedTabWindow {
  _SharedTabWindow(this.descriptor, this.viewId);

  final DetachedTabDescriptor descriptor;
  final int viewId;
  final Completer<void> contentMounted = Completer<void>();
  bool contentMayMount = false;
}

/// Per-View MaterialApp only. No second ProviderScope or Rust/SQLite startup.
/// A new Editor widget shares the primary's EditorSessionRegistry and all
/// existing provider state, while a scoped native View ID controls close/title.
final class _SharedDetachedEditorWindow extends ConsumerWidget {
  const _SharedDetachedEditorWindow({
    required this.descriptor,
    required this.viewId,
    required this.controller,
  });

  final DetachedTabDescriptor descriptor;
  final int viewId;
  final SharedEngineDetachedTabWindows controller;

  @override
  Widget build(BuildContext context, WidgetRef ref) => MaterialApp(
    title: 'Alera - ${descriptor.tab.title}',
    debugShowCheckedModeBanner: false,
    theme: aleraDarkTheme,
    darkTheme: aleraDarkTheme,
    home: Scaffold(
      appBar: AppBar(
        title: Text(descriptor.tab.title),
        actions: <Widget>[
          IconButton(
            tooltip: 'Return tab to main window',
            icon: const Icon(Icons.call_merge),
            onPressed: () =>
                unawaited(controller.closeTabWindow(descriptor.tab.id, viewId)),
          ),
        ],
      ),
      body: ListenableBuilder(
        listenable: controller,
        builder: (context, _) {
          if (!controller.canRender(descriptor.tab.id, viewId)) {
            return const Center(child: CircularProgressIndicator());
          }
          return _SharedEditorFrame(
            key: ValueKey('shared-editor-${descriptor.tab.id}-$viewId'),
            descriptor: descriptor,
            controller: controller,
            viewId: viewId,
          );
        },
      ),
    ),
  );
}

final class _SharedEditorFrame extends ConsumerStatefulWidget {
  const _SharedEditorFrame({
    super.key,
    required this.descriptor,
    required this.controller,
    required this.viewId,
  });
  final DetachedTabDescriptor descriptor;
  final SharedEngineDetachedTabWindows controller;
  final int viewId;

  @override
  ConsumerState<_SharedEditorFrame> createState() => _SharedEditorFrameState();
}

final class _SharedEditorFrameState extends ConsumerState<_SharedEditorFrame> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        widget.controller.reportContentMounted(
          widget.descriptor.tab.id,
          widget.viewId,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) => DetachedWorkspaceTabSurface(
    workspace: widget.descriptor.workspace,
    tab: widget.descriptor.tab,
    terminalRuntime: ref.read(terminalRuntimeProvider),
    onOpenFile: (path) {
      final handler = detachedTabWindows.onOpenFile;
      if (handler != null) {
        unawaited(handler(widget.descriptor, path));
      }
    },
  );
}
