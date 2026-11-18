import 'dart:collection';

import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:flutter/foundation.dart';

/// Source text from an assistant Markdown code block, never a PTY guess.
final class ClaudeSourceCodeBlock {
  const ClaudeSourceCodeBlock({required this.messageId, required this.code});

  final String messageId;
  final String code;
}

/// In-memory, bounded, per-terminal source for opt-in Claude code copying.
final class ClaudeCodeMessageStore extends ChangeNotifier {
  static const _messageLimit = 12;
  static const _characterLimit = 1024 * 1024;
  final Map<String, _ClaudeTerminalSource> _terminals = {};

  bool hasClaudeSession(String terminalId) =>
      _terminals.containsKey(terminalId);

  List<ClaudeSourceCodeBlock> blocksForTerminal(String terminalId) {
    final session = _terminals[terminalId];
    if (session == null) return const [];
    return [
      for (final message in session.messages.values.toList().reversed)
        ...message.blocks,
    ];
  }

  void clearTerminal(String terminalId) {
    if (_terminals.remove(terminalId) != null) notifyListeners();
  }

  void clearAll() {
    if (_terminals.isEmpty) return;
    _terminals.clear();
    notifyListeners();
  }

  void observeHookEvent(AgentHookEvent event) {
    if (event.agentType != AgentType.claude ||
        event.terminalSessionId.isEmpty) {
      return;
    }
    final name = event.hookEventName ?? event.payload['hook_event_name'];
    if (name != 'SessionStart' && name != 'MessageDisplay' && name != 'Stop') {
      return;
    }
    final sessionId = event.payload['session_id'];
    final transcript = event.payload['transcript_path'];
    if (sessionId is! String ||
        sessionId.isEmpty ||
        transcript is! String ||
        transcript.isEmpty) {
      return;
    }

    var session = _terminals[event.terminalSessionId];
    if (session == null ||
        session.sessionId != sessionId ||
        session.transcript != transcript ||
        session.workspaceId != event.workspaceId ||
        session.tabId != event.tabId) {
      session = _ClaudeTerminalSource(
        sessionId: sessionId,
        transcript: transcript,
        workspaceId: event.workspaceId,
        tabId: event.tabId,
      );
      _terminals[event.terminalSessionId] = session;
      notifyListeners();
    }
    if (name == 'SessionStart') return;

    if (name == 'MessageDisplay') {
      final messageId = event.payload['message_id'];
      final index = event.payload['index'];
      final delta = event.payload['delta'];
      if (messageId is! String ||
          messageId.isEmpty ||
          index is! int ||
          index < 0 ||
          delta is! String ||
          delta.isEmpty) {
        return;
      }
      final message = session.messages.putIfAbsent(
        messageId,
        _ClaudeSourceMessage.new,
      );
      message.chunks.putIfAbsent(index, () => delta);
      if (event.payload['final'] == true) message.finalIndex = index;
      final end = message.finalIndex;
      if (end != null &&
          message.chunks.length == end + 1 &&
          message.chunks.keys.first == 0 &&
          message.chunks.keys.last == end) {
        final text = message.chunks.values.join();
        if (text.length <= _characterLimit) {
          message.blocks = _parseCanonicalCodeBlocks(text, messageId);
        }
        message.chunks.clear();
        message.finalIndex = null;
        _trimMessages(session);
        notifyListeners();
      } else if (message.chunks.values.fold<int>(
            0,
            (total, value) => total + value.length,
          ) >
          _characterLimit) {
        session.messages.remove(messageId);
      }
      return;
    }

    final text = event.payload['last_assistant_message'];
    if (text is! String || text.isEmpty || text.length > _characterLimit) {
      return;
    }
    final blocks = _parseCanonicalCodeBlocks(text, 'stop');
    if (blocks.isEmpty) return;
    if (session.messages.values.any(
      (message) => _sameCodeBlocks(message.blocks, blocks),
    )) {
      return;
    }
    session.messages['stop:' +
        DateTime.now().microsecondsSinceEpoch
            .toString()] = _ClaudeSourceMessage()
      ..blocks = blocks;
    _trimMessages(session);
    notifyListeners();
  }

  void _trimMessages(_ClaudeTerminalSource session) {
    while (session.messages.length > _messageLimit) {
      session.messages.remove(session.messages.keys.first);
    }
  }
}

final class _ClaudeTerminalSource {
  _ClaudeTerminalSource({
    required this.sessionId,
    required this.transcript,
    required this.workspaceId,
    required this.tabId,
  });
  final String sessionId;
  final String transcript;
  final String workspaceId;
  final String tabId;
  final LinkedHashMap<String, _ClaudeSourceMessage> messages = LinkedHashMap();
}

final class _ClaudeSourceMessage {
  final SplayTreeMap<int, String> chunks = SplayTreeMap<int, String>();
  int? finalIndex;
  List<ClaudeSourceCodeBlock> blocks = const [];
}

bool _sameCodeBlocks(
  List<ClaudeSourceCodeBlock> left,
  List<ClaudeSourceCodeBlock> right,
) {
  if (left.length != right.length) return false;
  for (var i = 0; i < left.length; i++) {
    if (left[i].code != right[i].code) return false;
  }
  return true;
}

/// Parse Claude's canonical Markdown without interpreting the rendered PTY.
/// Shared by live Hooks and the read-only local transcript fallback.
List<ClaudeSourceCodeBlock> parseClaudeCodeBlocksFromMarkdown({
  required String messageId,
  required String markdown,
}) => _parseCanonicalCodeBlocks(markdown, messageId);

/// A structural Markdown scanner preserves code bytes exactly: Markdown AST
/// textContent escapes '<', '>' and '&' for HTML rendering, which would corrupt
/// copied Swift/Objective-C/C++ source. There are no language-specific rules.
List<ClaudeSourceCodeBlock> _parseCanonicalCodeBlocks(
  String text,
  String messageId,
) {
  final lines = text
      .replaceAll('\r\n', '\n')
      .replaceAll('\r', '\n')
      .split('\n');
  final opener = RegExp(r'^( {0,3})(\x60{3,}|~{3,})(.*)$');
  final blocks = <ClaudeSourceCodeBlock>[];

  void append(List<String> code) {
    final source = code.join('\n');
    if (source.trim().isNotEmpty) {
      blocks.add(ClaudeSourceCodeBlock(messageId: messageId, code: source));
    }
  }

  var index = 0;
  while (index < lines.length) {
    final line = lines[index];
    final match = opener.firstMatch(line);
    if (match != null) {
      final marker = match.group(2)!;
      final info = match.group(3)!;
      // Backtick fences cannot contain backticks in their info strings.
      if (marker.codeUnitAt(0) == 96 &&
          info.contains(String.fromCharCode(96))) {
        index++;
        continue;
      }
      final indent = match.group(1)!.length;
      final close = RegExp(
        '^( {0,3})' +
            RegExp.escape(marker[0]) +
            '{' +
            marker.length.toString() +
            ',}[ \\t]*' +
            r'$',
      );
      final code = <String>[];
      index++;
      var closed = false;
      while (index < lines.length) {
        final raw = lines[index];
        if (close.hasMatch(raw)) {
          closed = true;
          index++;
          break;
        }
        // CommonMark removes up to the opening fence's indentation.
        var leading = 0;
        while (leading < raw.length &&
            leading < indent &&
            raw.codeUnitAt(leading) == 32) {
          leading++;
        }
        code.add(raw.substring(leading));
        index++;
      }
      // Never copy a partial response whose closing delimiter has not arrived.
      if (closed) append(code);
      continue;
    }

    // Standard indented Markdown code blocks are structural too, and need no
    // source-language recognition.
    final indent = line.startsWith('    ') || line.startsWith('\t');
    final precededByBlank = index == 0 || lines[index - 1].trim().isEmpty;
    if (indent && precededByBlank) {
      final code = <String>[];
      while (index < lines.length) {
        final current = lines[index];
        if (current.startsWith('    ')) {
          code.add(current.substring(4));
        } else if (current.startsWith('\t')) {
          code.add(current.substring(1));
        } else if (current.trim().isEmpty) {
          code.add('');
        } else {
          break;
        }
        index++;
      }
      while (code.isNotEmpty && code.last.isEmpty) {
        code.removeLast();
      }
      append(code);
      continue;
    }
    index++;
  }
  return blocks;
}
