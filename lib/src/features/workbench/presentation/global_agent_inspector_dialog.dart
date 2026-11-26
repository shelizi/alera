import 'dart:async';
import 'dart:math' as math;

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/terminal_surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// The modal itself never interprets Escape as a request to close.
@visibleForTesting
bool isAgentInspectorEscape(KeyEvent event) =>
    event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.escape;

/// Use nearly the whole Alera window for reading long Agent/TUI output.
/// The dimensions remain bounded for large multi-monitor desktops.
@visibleForTesting
Size globalAgentInspectorDefaultSize(Size viewport) => Size(
  // Reserve a 32px tappable backdrop on every edge for outside dismissal.
  math.max(0.0, math.min(2400.0, viewport.width - 64)),
  math.max(0.0, math.min(1400.0, viewport.height - 64)),
);

/// Inspect the *existing* Terminal session without selecting another Tab.
///
/// A single TerminalSurface owns the live renderer while the shell suspends
/// its hidden workbench view; the Host PTY remains unchanged.
class GlobalAgentInspectorDialog extends ConsumerStatefulWidget {
  const GlobalAgentInspectorDialog({
    super.key,
    required this.row,
    required this.session,
  });

  final GlobalAgentLiveRow row;
  final TerminalSessionHandle session;

  @override
  ConsumerState<GlobalAgentInspectorDialog> createState() =>
      _GlobalAgentInspectorDialogState();
}

class _GlobalAgentInspectorDialogState
    extends ConsumerState<GlobalAgentInspectorDialog> {
  bool _terminating = false;
  bool _confirmTerminate = false;
  String? _feedback;

  String get _sessionId => widget.row.tab.terminalSessionId;
  String get _workspaceId => widget.row.status.workspaceId;
  String get _tabId => widget.row.tab.id;

  bool _isVerified() => globalAgentTargetIsCurrent(
    workbench: ref.read(workbenchControllerProvider),
    statuses: ref.read(agentStatusControllerProvider),
    sessionId: _sessionId,
    workspaceId: _workspaceId,
    tabId: _tabId,
  );

  /// Send a single raw ESC byte, never a bracketed paste or a modal pop.
  ///
  /// TerminalView handles its own key events when focused. This fallback
  /// covers the prompt/metadata controls, which do not own the PTY focus.
  Future<void> _sendEscape() async {
    if (!_isVerified()) {
      if (mounted) {
        setState(() => _feedback = 'Session changed. Escape was not sent.');
      }
      return;
    }
    try {
      await ref
          .read(terminalHostClientProvider)
          .write(sessionId: _sessionId, bytes: const <int>[0x1b]);
      if (mounted) setState(() => _feedback = 'Esc sent to this Agent.');
    } on Object {
      if (mounted) {
        setState(() => _feedback = 'Esc delivery could not be confirmed.');
      }
    }
  }

  KeyEventResult _onKeyEvent(FocusNode _, KeyEvent event) {
    if (isAgentInspectorEscape(event)) {
      unawaited(_sendEscape());
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  Future<void> _terminateSession() async {
    if (_terminating) return;
    if (!_confirmTerminate) {
      setState(() {
        _confirmTerminate = true;
        _feedback = 'Click Confirm terminate again to end this Host PTY.';
      });
      return;
    }
    if (!_isVerified()) {
      setState(() {
        _confirmTerminate = false;
        _feedback = 'Session changed. No action was taken.';
      });
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostVerifiedAgentLifecycleClient) {
      setState(() => _feedback = 'Host does not support verified termination.');
      return;
    }
    setState(() {
      _terminating = true;
      _confirmTerminate = false;
    });
    try {
      await (host as TerminalHostVerifiedAgentLifecycleClient)
          .controlVerifiedAgent(
            sessionId: _sessionId,
            workspaceId: _workspaceId,
            tabId: _tabId,
            terminate: true,
          );
      if (mounted)
        setState(() => _feedback = 'Host confirmed session termination.');
    } on Object {
      if (mounted) {
        setState(
          () => _feedback = 'Termination unconfirmed. Check Host status.',
        );
      }
    } finally {
      if (mounted) setState(() => _terminating = false);
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.session.requestFocus();
    });
  }

  Widget _contextPanel(BuildContext context, GlobalAgentLiveRow row) {
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return ListView(
      padding: const EdgeInsets.all(14),
      children: <Widget>[
        Text('Objective', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 8),
        SelectableText(row.goal.isEmpty ? row.tab.title : row.goal),
        const SizedBox(height: 18),
        Text('Latest progress', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 8),
        SelectableText(
          row.progress.isEmpty ? 'No progress update yet.' : row.progress,
          style: muted,
        ),
        const SizedBox(height: 18),
        Text(
          'Progress timeline (${row.timeline.length})',
          style: Theme.of(context).textTheme.titleSmall,
        ),
        const SizedBox(height: 8),
        if (row.timeline.isEmpty)
          Text('No verified milestones yet.', style: muted)
        else
          for (final event in row.timeline.take(24)) ...[
            Text('${event.kind} · ${event.at.toLocal()}', style: muted),
            const SizedBox(height: 3),
            SelectableText(
              event.kind == 'report' && event.summary.isNotEmpty
                  ? event.summary
                  : event.text,
            ),
            const Divider(height: 18),
          ],
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final size = MediaQuery.sizeOf(context);
    final matches = ref
        .watch(globalAgentLiveRowsProvider)
        .where(
          (row) =>
              row.tab.id == _tabId &&
              row.tab.terminalSessionId == _sessionId &&
              row.status.workspaceId == _workspaceId,
        );
    final current = matches.firstOrNull;
    final row = current ?? widget.row;
    final verified = current != null;
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    final inspectorSize = globalAgentInspectorDefaultSize(size);

    return Shortcuts(
      shortcuts: const <ShortcutActivator, Intent>{
        SingleActivator(LogicalKeyboardKey.escape): DoNothingIntent(),
      },
      child: PopScope(
        // Allow outside-barrier taps to dismiss this route. The Focus handler
        // below forwards Escape to the Agent and the shortcut fallback keeps
        // Escape from dismissing the modal when terminal focus changes.
        canPop: true,
        child: Focus(
          onKeyEvent: _onKeyEvent,
          canRequestFocus: false,
          child: Dialog(
            backgroundColor: AleraTokens.bg,
            insetPadding: const EdgeInsets.all(32),
            child: SizedBox(
              width: inspectorSize.width,
              height: inspectorSize.height,
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    Row(
                      children: <Widget>[
                        const Icon(Icons.smart_toy_outlined),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: <Widget>[
                              Text(
                                '${row.project} / ${row.workspace}',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: Theme.of(context).textTheme.titleMedium,
                              ),
                              Text(
                                '${row.status.agentType.key} · ${row.status.state.key} · Live Session',
                                style: muted,
                              ),
                            ],
                          ),
                        ),
                        PopupMenuButton<String>(
                          tooltip: 'Session options',
                          onSelected: (value) {
                            if (value == 'terminate')
                              unawaited(_terminateSession());
                          },
                          itemBuilder: (context) => <PopupMenuEntry<String>>[
                            PopupMenuItem<String>(
                              value: 'terminate',
                              enabled: verified && !_terminating,
                              child: Text(
                                _confirmTerminate
                                    ? 'Confirm terminate'
                                    : 'Terminate Session…',
                              ),
                            ),
                          ],
                        ),
                        IconButton(
                          tooltip: 'Close Agent inspector',
                          onPressed: () => Navigator.of(context).pop(),
                          icon: const Icon(Icons.close),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(
                      'Type directly in the Terminal. Esc reaches the Agent; close with ×.',
                      style: muted,
                    ),
                    if (!verified)
                      const Padding(
                        padding: EdgeInsets.only(top: 8),
                        child: Text(
                          'Agent identity is no longer verified; input is disabled.',
                        ),
                      ),
                    const SizedBox(height: 12),
                    Expanded(
                      child: LayoutBuilder(
                        builder: (context, constraints) {
                          final wide = constraints.maxWidth >= 830;
                          final terminal = DecoratedBox(
                            decoration: BoxDecoration(
                              border: Border.all(
                                color: AleraTokens.borderSubtle,
                              ),
                              borderRadius: BorderRadius.circular(8),
                            ),
                            child: ClipRRect(
                              borderRadius: BorderRadius.circular(7),
                              child: ExcludeFocus(
                                excluding: !verified,
                                child: IgnorePointer(
                                  ignoring: !verified,
                                  child: TerminalSurface(
                                    session: widget.session,
                                    autofocus: true,
                                  ),
                                ),
                              ),
                            ),
                          );
                          final contextInfo = DecoratedBox(
                            decoration: BoxDecoration(
                              color: AleraTokens.surface,
                              border: Border.all(
                                color: AleraTokens.borderSubtle,
                              ),
                              borderRadius: BorderRadius.circular(8),
                            ),
                            child: _contextPanel(context, row),
                          );
                          if (wide) {
                            return Row(
                              children: <Widget>[
                                Expanded(child: terminal),
                                const SizedBox(width: 12),
                                SizedBox(width: 310, child: contextInfo),
                              ],
                            );
                          }
                          return Column(
                            children: <Widget>[
                              SizedBox(
                                height: math.min(
                                  150,
                                  constraints.maxHeight * 0.28,
                                ),
                                child: contextInfo,
                              ),
                              const SizedBox(height: 10),
                              Expanded(child: terminal),
                            ],
                          );
                        },
                      ),
                    ),
                    if (_feedback != null) ...[
                      const SizedBox(height: 6),
                      Text(_feedback!, style: muted),
                    ],
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
