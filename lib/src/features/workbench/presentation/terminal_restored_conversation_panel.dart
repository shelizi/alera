import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// A provider-native turn. This does not represent PTY output and must
/// never be written into the interactive terminal emulator.
class RestoredNativeConversationMessage {
  const RestoredNativeConversationMessage(this.role, this.text, this.timestamp);

  final String role;
  final String text;
  final String? timestamp;
}

class RestoredNativeConversationPage {
  const RestoredNativeConversationPage(this.messages, this.before);

  final List<RestoredNativeConversationMessage> messages;
  final int? before;

  /// Fail closed on malformed or oversized/untrusted Host responses.
  static RestoredNativeConversationPage parse(
    Map<String, Object?> raw,
    int? requestedBefore,
  ) {
    if (raw['readOnly'] != true || raw['source'] != 'nativeProvider') {
      throw const FormatException('Native history source was not verified');
    }
    final source = raw['messages'];
    if (source is! List || source.length > 60) {
      throw const FormatException('Unexpected native history message count');
    }
    final messages = <RestoredNativeConversationMessage>[];
    for (final item in source) {
      if (item is! Map || item['role'] is! String || item['text'] is! String) {
        throw const FormatException('Malformed native message');
      }
      final role = item['role'] as String;
      final text = item['text'] as String;
      if ((role != 'user' && role != 'assistant') || text.runes.length > 8000) {
        throw const FormatException('Untrusted native message role or length');
      }
      final stamp = item['timestamp'] is String
          ? (item['timestamp'] as String)
          : null;
      messages.add(
        RestoredNativeConversationMessage(
          role,
          text,
          stamp != null && stamp.length <= 64 ? stamp : null,
        ),
      );
    }
    final hasMore = raw['hasMore'] == true;
    final cursor = raw['nextBefore'];
    if (hasMore &&
        (cursor is! int ||
            cursor <= 0 ||
            (requestedBefore != null && cursor >= requestedBefore))) {
      throw const FormatException('Invalid native history page cursor');
    }
    return RestoredNativeConversationPage(
      messages,
      hasMore ? cursor as int : null,
    );
  }
}

/// Shows the recovered Claude/Codex dialogue INSIDE its original Classic Tab,
/// above (not inside) the live PTY. On reattach, no Agent is restarted or fed
/// transcript text. A single read is attempted only for a saved provider Tab;
/// the Host must prove its native UUID, account and original Worktree.
class RestoredTerminalConversationPanel extends ConsumerStatefulWidget {
  const RestoredTerminalConversationPanel({super.key, required this.session});

  final TerminalSessionHandle session;

  @override
  ConsumerState<RestoredTerminalConversationPanel> createState() =>
      _RestoredTerminalConversationPanelState();
}

class _RestoredTerminalConversationPanelState
    extends ConsumerState<RestoredTerminalConversationPanel> {
  final List<RestoredNativeConversationMessage> _messages = [];
  bool _attempted = false;
  bool _reading = false;
  bool _expanded = true;
  int _ticket = 0;
  int? _before;
  ({String? sessionId, String workspaceId, String tabId, String? provider})?
  _identity;

  ({String? sessionId, String workspaceId, String tabId, String? provider})
  get _currentIdentity => (
    sessionId: widget.session.terminalSessionId,
    workspaceId: widget.session.workspaceId,
    tabId: widget.session.tabId,
    provider: widget.session.nativeHistoryProvider,
  );

  @override
  void initState() {
    super.initState();
    widget.session.addListener(_maybeRead);
    WidgetsBinding.instance.addPostFrameCallback((_) => _maybeRead());
  }

  @override
  void didUpdateWidget(RestoredTerminalConversationPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.session == widget.session) return;
    oldWidget.session.removeListener(_maybeRead);
    _ticket++;
    _messages.clear();
    _before = null;
    _attempted = false;
    _reading = false;
    _expanded = true;
    _identity = null;
    widget.session.addListener(_maybeRead);
    WidgetsBinding.instance.addPostFrameCallback((_) => _maybeRead());
  }

  @override
  void dispose() {
    _ticket++;
    widget.session.removeListener(_maybeRead);
    _messages.clear();
    super.dispose();
  }

  void _maybeRead() {
    if (!mounted) return;
    final next = _currentIdentity;
    if (_identity != next) {
      // A Host tab may be transferred or reused with a different Agent while
      // its Flutter session object stays the same. Do not leak old messages.
      final previouslyVisible = _messages.isNotEmpty;
      _identity = next;
      _ticket++;
      _messages.clear();
      _before = null;
      _attempted = false;
      _reading = false;
      _expanded = true;
      if (previouslyVisible) setState(() {});
    }
    if (_attempted ||
        _reading ||
        !widget.session.isRunning ||
        widget.session.nativeHistoryProvider == null ||
        widget.session.terminalSessionId == null) {
      return;
    }
    _attempted = true;
    unawaited(_load(older: false));
  }

  Future<void> _load({required bool older}) async {
    if (_reading || (older && _before == null)) return;
    final session = widget.session;
    final identity = _currentIdentity;
    final requestedBefore = older ? _before : null;
    final ticket = ++_ticket;
    _reading = true;
    try {
      final client = ref.read(terminalHostClientProvider);
      if (client is! TerminalHostAgentHistoryMessagesClient) return;
      final raw = await (client as TerminalHostAgentHistoryMessagesClient)
          .readRestoredAgentMessages(
            workspaceId: session.workspaceId,
            tabId: session.tabId,
            before: requestedBefore,
          );
      if (!mounted ||
          widget.session != session ||
          identity != _currentIdentity ||
          ticket != _ticket) {
        return;
      }
      final page = RestoredNativeConversationPage.parse(raw, requestedBefore);
      setState(() {
        if (!older) {
          _messages
            ..clear()
            ..addAll(page.messages);
        } else {
          _messages.insertAll(0, page.messages);
          if (_messages.length > 360) {
            _messages.removeRange(360, _messages.length);
          }
        }
        _before = page.before;
      });
    } catch (_) {
      // Old, offline and unverified Hosts intentionally do not expose this
      // panel. Never fall back to guessing paths or altering the live Agent.
    } finally {
      if (mounted && ticket == _ticket) {
        setState(() => _reading = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    if ((_messages.isEmpty && _before == null) ||
        _identity != _currentIdentity) {
      return const SizedBox.shrink();
    }
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final label = widget.session.nativeHistoryProvider == 'claude'
        ? 'Claude'
        : 'Codex';
    return DecoratedBox(
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border(bottom: BorderSide(color: scheme.outlineVariant)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          SizedBox(
            height: 32,
            child: Row(
              children: <Widget>[
                const SizedBox(width: 12),
                Icon(Icons.history, size: 16, color: scheme.onSurfaceVariant),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '$label 原始對話 · 唯讀 · 下方 Terminal 可繼續操作',
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.labelSmall,
                  ),
                ),
                TextButton(
                  onPressed: () => setState(() => _expanded = !_expanded),
                  child: Text(_expanded ? '收合' : '展開'),
                ),
              ],
            ),
          ),
          if (_expanded)
            SizedBox(
              height: _messages.isEmpty
                  ? 64.0
                  : (MediaQuery.sizeOf(context).height * 0.25).clamp(
                      120.0,
                      230.0,
                    ),
              child: ListView.builder(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 4,
                ),
                itemCount: _messages.length + (_before != null ? 1 : 0),
                itemBuilder: (context, index) {
                  if (_before != null && index == 0) {
                    return TextButton(
                      onPressed: _reading
                          ? null
                          : () => unawaited(_load(older: true)),
                      child: Text(_reading ? '讀取中…' : '載入更早對話'),
                    );
                  }
                  final message = _messages[index - (_before != null ? 1 : 0)];
                  return Padding(
                    padding: const EdgeInsets.symmetric(vertical: 5),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: <Widget>[
                        Text(
                          (message.role == 'user' ? '你' : label) +
                              (message.timestamp == null
                                  ? ''
                                  : ' · ${message.timestamp!}'),
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: scheme.primary,
                          ),
                        ),
                        const SizedBox(height: 3),
                        SelectableText(
                          message.text,
                          style: theme.textTheme.bodySmall,
                        ),
                      ],
                    ),
                  );
                },
              ),
            ),
        ],
      ),
    );
  }
}
