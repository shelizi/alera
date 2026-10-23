import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:multiview_desktop/multiview_desktop.dart';

import 'persistent_windows_shell_host.dart';
import 'tab_lease_registry.dart';
import 'view_window_controller.dart';

final HandoffLifecycleObserver handoffWindowObserver =
    HandoffLifecycleObserver();

final class HandoffLifecycleObserver extends WindowObserver {
  void Function(int viewId)? onClosed;
  final List<int> closedViewIds = <int>[];

  @override
  void onWindowClosed(int viewId) {
    closedViewIds.add(viewId);
    onClosed?.call(viewId);
  }
}

/// Standalone native GUI handoff smoke test. No production Alera files,
/// Rust FFI, ConPTY, Riverpod or disk-backed editor sessions are imported.
final class HandoffHarness extends StatefulWidget {
  const HandoffHarness({
    super.key,
    required this.mainViewId,
    required this.cycles,
    this.requireExternalWmClose = false,
  });

  final int mainViewId;
  final int cycles;
  final bool requireExternalWmClose;

  @override
  State<HandoffHarness> createState() => _HandoffHarnessState();
}

final class _HandoffHarnessState extends State<HandoffHarness> {
  late final TabLeaseRegistry _leases;
  late final DemoTabSession _editor;
  late final DemoTabSession _terminal;
  late final PersistentWindowsShellHost _host;
  final ScopedNativeWindowController _windows = ScopedNativeWindowController();
  final List<int> _ownedNativeChildren = <int>[];
  String _status = 'Preparing';
  bool _running = false;
  bool _shuttingDown = false;
  int _automaticRecoveries = 0;

  @override
  void initState() {
    super.initState();
    _leases = TabLeaseRegistry(mainViewId: widget.mainViewId);
    _editor = DemoTabSession(
      tabId: 'source/sample.dart',
      kind: TabContentKind.editor,
      initialText: 'class Sample { int saved = 1; }',
    );
    _terminal = DemoTabSession(
      tabId: 'terminal/one',
      kind: TabContentKind.terminal,
      initialText: '',
    );
    _leases.register(_editor);
    _leases.register(_terminal);
    _host = PersistentWindowsShellHost(_terminal);
    handoffWindowObserver.onClosed = (viewId) {
      if (!mounted || _shuttingDown || viewId == widget.mainViewId) return;
      _ownedNativeChildren.remove(viewId);
      _leases.acknowledgeWindowUnmount(viewId);
      _leases.recoverClosedWindow(viewId);
      _automaticRecoveries++;
    };
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => unawaited(_runAutomaticHandoff()),
    );
  }

  void _updateStatus(String status) {
    if (mounted) setState(() => _status = status);
  }

  Future<void> _afterFrame() async {
    await WidgetsBinding.instance.endOfFrame;
    await Future<void>.delayed(const Duration(milliseconds: 24));
  }

  void _verifyMountedIn(int viewId) {
    if (_leases.mountedIn(_editor.tabId) != viewId ||
        _leases.mountedIn(_terminal.tabId) != viewId) {
      throw StateError(
        'Wrong mounted view: expected $viewId, editor=${_leases.mountedIn(_editor.tabId)}, '
        'terminal=${_leases.mountedIn(_terminal.tabId)}',
      );
    }
  }

  Future<void> _moveBoth(int destination) async {
    final editorTicket = _leases.beginTransfer(_editor.tabId, destination);
    final terminalTicket = _leases.beginTransfer(_terminal.tabId, destination);
    await _afterFrame();
    if (_leases.mountedIn(_editor.tabId) != null ||
        _leases.mountedIn(_terminal.tabId) != null) {
      throw StateError('Source widget still mounted after frame barrier');
    }
    if (!_leases.commitTransfer(editorTicket) ||
        !_leases.commitTransfer(terminalTicket)) {
      throw StateError('Transfer ticket rejected');
    }
    await _afterFrame();
    _verifyMountedIn(destination);
  }

  Future<int> _openChild(int cycle) async {
    final id = await openWindow(
      (_, viewId) => HandoffChildWindow(registry: _leases, viewId: viewId),
      options: WindowOptions(
        title: 'Handoff child $cycle',
        size: const Size(750, 540),
      ),
    );
    _ownedNativeChildren.add(id);
    _windows.setTitle(id, 'Handoff child $cycle');
    if (_windows.getTitle(id) != 'Handoff child $cycle') {
      throw StateError('Native scoped window title mismatch viewId=$id');
    }
    return id;
  }

  Future<void> _closeChild(int id) async {
    if (!await _windows.close(id)) {
      throw StateError('Native close was refused for $id');
    }
    _ownedNativeChildren.remove(id);
    await _afterFrame();
  }

  Future<void> _runAutomaticHandoff() async {
    if (_running) return;
    _running = true;
    final stopwatch = Stopwatch()..start();
    final report = <String, Object?>{
      'pid': pid,
      'mainWindowId': widget.mainViewId,
      'cyclesRequested': widget.cycles,
      'passed': false,
      'handoffTimings': <Map<String, Object?>>[],
    };
    try {
      await _afterFrame();
      _verifyMountedIn(widget.mainViewId);
      report['initialEditorOwner'] = _leases.ownerOf(_editor.tabId);
      _editor.editorText.value = const TextEditingValue(
        text: 'class Sample { int unsaved = 42; } // initial dirty',
        selection: TextSelection.collapsed(offset: 28),
      );
      final editorIdentity = _editor.editorText;
      if (!_editor.isDirty) {
        throw StateError('Initial editor was not dirty');
      }

      await _host.start();
      final shellPid = _host.pid!;
      report['terminalProcessPid'] = shellPid;
      if (_terminal.terminalHostStartCount != 1 ||
          !_host.output.contains('ALERA_SHELL_STARTED')) {
        throw StateError('Could not start persistent cmd.exe');
      }

      for (var i = 0; i < widget.cycles; i++) {
        _updateStatus('Detach/dock cycle ${i + 1}/${widget.cycles}');
        final start = Stopwatch()..start();
        final childId = await _openChild(i);
        final windowCreatedMs = start.elapsedMilliseconds;
        await _moveBoth(childId);
        final editorInChildMs = start.elapsedMilliseconds;

        final token = 'ALERA_IN_CHILD_$i';
        await _host.sendEcho(token);
        if (_host.pid != shellPid || _terminal.terminalHostStartCount != 1) {
          throw StateError('Terminal was restarted during detach');
        }

        final nextText = 'unsaved editing round $i • persists across windows';
        _editor.editorText.value = TextEditingValue(
          text: nextText,
          selection: TextSelection.collapsed(offset: nextText.length),
        );
        if (!identical(_editor.editorText, editorIdentity) ||
            !_editor.isDirty) {
          throw StateError('Editor buffer was reset while detached');
        }

        await _moveBoth(widget.mainViewId);
        final editorBackInMainMs = start.elapsedMilliseconds;
        await _closeChild(childId);
        _verifyMountedIn(widget.mainViewId);
        if (_editor.editorText.text != nextText ||
            _editor.editorText.selection.baseOffset != nextText.length ||
            !_terminal.terminalTranscript.join().contains(token)) {
          throw StateError('Editor/terminal state changed on dock cycle $i');
        }
        (report['handoffTimings']! as List<Map<String, Object?>>).add({
          'cycle': i,
          'childWindowId': childId,
          'nativeWindowCreatedMs': windowCreatedMs,
          'editorMountedInChildMs': editorInChildMs,
          'editorMountedBackInMainMs': editorBackInMainMs,
        });
      }

      // Native close dispatches onWindowClosed after children dispose. That
      // observer must restore ownership without explicit caller recovery.
      final abruptChildId = await _openChild(9001);
      await _moveBoth(abruptChildId);
      await _host.sendEcho('ALERA_BEFORE_UNEXPECTED_CLOSE');
      final recoveriesBeforeClose = _automaticRecoveries;
      if (widget.requireExternalWmClose) {
        await File('poc_handoff_waiting_wm_close.json').writeAsString(
          jsonEncode(<String, Object?>{
            'pid': pid,
            'targetViewId': abruptChildId,
            'targetWindowTitle': 'Handoff child 9001',
          }),
          flush: true,
        );
        final deadline = DateTime.now().add(const Duration(seconds: 12));
        while (!handoffWindowObserver.closedViewIds.contains(abruptChildId)) {
          if (DateTime.now().isAfter(deadline)) {
            throw StateError('External WM_CLOSE was not received');
          }
          await Future<void>.delayed(const Duration(milliseconds: 40));
        }
        report['externalWmCloseObserved'] = true;
      } else {
        await _closeChild(abruptChildId);
      }
      await _afterFrame();
      _verifyMountedIn(widget.mainViewId);
      if (_automaticRecoveries <= recoveriesBeforeClose ||
          !handoffWindowObserver.closedViewIds.contains(abruptChildId)) {
        throw StateError('Native close observer failed to recover tab owner');
      }
      report['unexpectedCloseRecoveredByNativeObserver'] = true;

      // A pending open/transfer that fails must rollback without resurrecting
      // a stale handoff, while preserving the dirty editor.
      final cancelledId = await _openChild(9002);
      final pendingEditor = _leases.beginTransfer(_editor.tabId, cancelledId);
      final pendingTerminal = _leases.beginTransfer(
        _terminal.tabId,
        cancelledId,
      );
      await _afterFrame();
      await _closeChild(cancelledId);
      await _afterFrame();
      _verifyMountedIn(widget.mainViewId);
      if (_leases.commitTransfer(pendingEditor) ||
          _leases.commitTransfer(pendingTerminal)) {
        throw StateError('Stale transfer resurrected a closed child');
      }
      report['failedTransferRecovered'] = true;

      await _host.sendEcho('ALERA_AFTER_REDOCK');
      if (_host.pid != shellPid ||
          !_terminal.terminalTranscript.join().contains('ALERA_AFTER_REDOCK')) {
        throw StateError('Terminal session lost after native close');
      }
      final draft = _editor.editorText.text;
      if (!_editor.isDirty) throw StateError('Dirty state was dropped');
      final savedFile = File('poc_handoff_saved_file.txt');
      await savedFile.writeAsString(draft, flush: true);
      _editor.saveSnapshot();
      if (_editor.isDirty ||
          await savedFile.readAsString() != draft ||
          !identical(_editor.editorText, editorIdentity)) {
        throw StateError('Post-handoff Ctrl+S equivalent failed');
      }
      report.addAll({
        'savedText': draft,
        'saveCount': _editor.saveCount,
        'dirtyAfterSave': _editor.isDirty,
        'editorSelectionOffset': _editor.editorText.selection.baseOffset,
        'terminalHostPidAfterAllMoves': _host.pid,
        'terminalHostStartCount': _terminal.terminalHostStartCount,
        'terminalOutputChunks': _terminal.terminalTranscript.length,
        'finalEditorOwner': _leases.ownerOf(_editor.tabId),
        'finalTerminalOwner': _leases.ownerOf(_terminal.tabId),
        'finalMountedEditorView': _leases.mountedIn(_editor.tabId),
        'finalMountedTerminalView': _leases.mountedIn(_terminal.tabId),
        'nativeChildWindowsRemaining': _ownedNativeChildren.length,
        'nativeCloseEvents': handoffWindowObserver.closedViewIds.length,
        'automaticRecoveries': _automaticRecoveries,
        'elapsedMs': stopwatch.elapsedMilliseconds,
      });
      report['passed'] = true;
      _updateStatus('PASS ${widget.cycles} detach/dock cycles');
    } catch (error, stack) {
      report['error'] = '$error\n$stack';
      _updateStatus('FAIL: $error');
    } finally {
      _shuttingDown = true;
      handoffWindowObserver.onClosed = null;
      // Always tear down the stand-alone cmd.exe host; no orphan processes.
      try {
        await _host.stop();
      } catch (error) {
        report['hostCleanupError'] = '$error';
        report['passed'] = false;
      }
      for (final id in _ownedNativeChildren.toList().reversed) {
        try {
          await _windows.close(id);
          _ownedNativeChildren.remove(id);
        } catch (_) {}
      }
      report['totalElapsedMs'] = stopwatch.elapsedMilliseconds;
      await File('poc_handoff_report.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
      await Future<void>.delayed(const Duration(milliseconds: 400));
      await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
    }
  }

  @override
  void dispose() {
    handoffWindowObserver.onClosed = null;
    _leases.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text('Alera tab handoff PoC • $_status')),
    body: HandoffWindowBody(registry: _leases, viewId: widget.mainViewId),
  );
}

final class HandoffChildWindow extends StatelessWidget {
  const HandoffChildWindow({
    super.key,
    required this.registry,
    required this.viewId,
  });

  final TabLeaseRegistry registry;
  final int viewId;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text('Detached tabs • Window $viewId')),
    body: HandoffWindowBody(registry: registry, viewId: viewId),
  );
}

final class HandoffWindowBody extends StatelessWidget {
  const HandoffWindowBody({
    super.key,
    required this.registry,
    required this.viewId,
  });

  final TabLeaseRegistry registry;
  final int viewId;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: registry,
    builder: (context, _) => Column(
      children: <Widget>[
        Expanded(
          flex: 2,
          child: registry.canMount('source/sample.dart', viewId)
              ? EditorLeaseView(
                  key: ValueKey<String>('editor-in-$viewId'),
                  registry: registry,
                  tabId: 'source/sample.dart',
                  viewId: viewId,
                )
              : Center(child: Text('Editor is detached from View $viewId')),
        ),
        const Divider(height: 1),
        Expanded(
          flex: 1,
          child: registry.canMount('terminal/one', viewId)
              ? _TerminalLeaseView(
                  key: ValueKey<String>('terminal-in-$viewId'),
                  registry: registry,
                  viewId: viewId,
                )
              : Center(child: Text('Terminal is detached from View $viewId')),
        ),
      ],
    ),
  );
}

final class _TerminalLeaseView extends StatefulWidget {
  const _TerminalLeaseView({
    super.key,
    required this.registry,
    required this.viewId,
  });
  final TabLeaseRegistry registry;
  final int viewId;

  @override
  State<_TerminalLeaseView> createState() => _TerminalLeaseViewState();
}

final class _TerminalLeaseViewState extends State<_TerminalLeaseView> {
  @override
  void initState() {
    super.initState();
    widget.registry.claimMount('terminal/one', widget.viewId);
  }

  @override
  void dispose() {
    widget.registry.releaseMount('terminal/one', widget.viewId);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final terminal = widget.registry.session('terminal/one');
    return Column(
      children: <Widget>[
        Text(
          'Piped cmd.exe PID ${terminal.terminalHostPid} • View ${widget.viewId}',
        ),
        Expanded(
          child: ValueListenableBuilder<int>(
            valueListenable: terminal.terminalRevision,
            builder: (_, _, _) => SingleChildScrollView(
              child: Text(terminal.terminalTranscript.join()),
            ),
          ),
        ),
      ],
    );
  }
}
