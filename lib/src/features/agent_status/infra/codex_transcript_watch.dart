part of 'codex_transcript_status_watcher.dart';

class _CodexTranscriptWatch({
  required final AgentStatusSink _statusSink,
  required final String terminalSessionId,
  required final String workspaceId,
  required final String tabId,
  required final String transcriptPath,
  required final String? turnId,
  required final Duration watchdogInterval,
  required final AppForeground appForeground,
}) {
  final Map<String, String> _pendingToolsByCallId = <String, String>{};
  final Set<String> _emittedCallIds = <String>{};
  StreamSubscription<FileSystemEvent>? _subscription;
  StreamSubscription<bool>? _foregroundSubscription;
  Timer? _watchdogTimer;
  Timer? _pollTimer;

  /// Whether polling was the active strategy when the app went to the
  /// background, so returning restores the same one rather than upgrading a
  /// degraded watch back to a file-event watch that already failed.
  var _polling = false;
  int? _offset;
  final _partialLine = BytesBuilder(copy: false);
  var _armed = false;
  var _initialScan = true;
  var _disposed = false;
  var _scanning = false;
  var _scanAgain = false;
  Completer<void>? _activeScan;

  void start() {
    _foregroundSubscription = appForeground.changes.listen(_applyForeground);
    final file = File(transcriptPath);
    try {
      // The first async scan determines the initial tail window. Avoid
      // synchronous file existence and size probes on the Hook/UI path.
      _subscription = file
          .watch(
            events:
                FileSystemEvent.create |
                FileSystemEvent.modify |
                FileSystemEvent.move,
          )
          .listen(
            (_) {
              _armWatchdog();
              _scheduleScan();
            },
            onError: (_) => _enablePolling(),
            onDone: _enablePolling,
          );
      _armWatchdog();
    } catch (_) {
      // coverage:ignore-start
      // File watch setup can fail on platform/filesystem races; scan polling
      // still handles the transcript and is covered by watcher tests.
      _enablePolling();
      // coverage:ignore-end
    }
    _scheduleScan();
  }

  Future<void> scan() async {
    if (_disposed) {
      return;
    }
    if (_scanning) {
      _scanAgain = true;
      await _activeScan?.future;
      return;
    }
    _scanning = true;
    final activeScan = Completer<void>();
    _activeScan = activeScan;
    try {
      do {
        _scanAgain = false;
        await _scanOnce();
      } while (_scanAgain && !_disposed);
    } finally {
      _scanning = false;
      _activeScan = null;
      activeScan.complete();
    }
  }

  void dispose() {
    _disposed = true;
    _partialLine.clear();
    _pendingToolsByCallId.clear();
    _emittedCallIds.clear();
    unawaited(_subscription?.cancel());
    unawaited(_foregroundSubscription?.cancel());
    _watchdogTimer?.cancel();
    _pollTimer?.cancel();
    _subscription = null;
    _foregroundSubscription = null;
    _watchdogTimer = null;
    _pollTimer = null;
  }

  /// Park the timers while the app is hidden, and catch up on return.
  ///
  /// This is the one poller in the app that scales with how many agents are
  /// running: a stat plus an incremental read per transcript, every interval,
  /// whether or not anyone can see the status it produces.
  ///
  /// The file watch keeps running while parked. It is event driven, so it costs
  /// nothing idle, and leaving it subscribed means a transcript that changes
  /// while hidden is still noticed. Only the safety net stops. Nothing is lost
  /// either way: scanning resumes from the same byte offset, so returning to
  /// the foreground reads whatever accumulated.
  void _applyForeground(bool isForeground) {
    if (_disposed) {
      return;
    }
    if (!isForeground) {
      _watchdogTimer?.cancel();
      _watchdogTimer = null;
      _pollTimer?.cancel();
      _pollTimer = null;
      return;
    }
    if (_polling) {
      _startPollTimer();
    } else {
      _armWatchdog();
    }
    _scheduleScan();
  }

  void _scheduleScan() {
    unawaited(scan());
  }

  void _armWatchdog() {
    if (_disposed || _polling || !appForeground.isForeground) {
      return;
    }
    _watchdogTimer?.cancel();
    _watchdogTimer = Timer(watchdogInterval, () {
      _scheduleScan();
      _armWatchdog();
    });
  }

  void _enablePolling() {
    if (_disposed || _polling) {
      return;
    }
    _polling = true;
    _watchdogTimer?.cancel();
    _watchdogTimer = null;
    unawaited(_subscription?.cancel());
    _subscription = null;
    _startPollTimer();
  }

  void _startPollTimer() {
    if (_disposed || _pollTimer != null || !appForeground.isForeground) {
      return;
    }
    _pollTimer = Timer.periodic(watchdogInterval, (_) => _scheduleScan());
  }

  Future<void> _scanOnce() async {
    final file = File(transcriptPath);
    int length;
    try {
      length = await file.length();
    } catch (_) {
      return;
    }
    if (_disposed) return;

    var offset = _offset ?? math.max(0, length - _initialTranscriptScanBytes);
    if (length < offset) {
      // A truncated transcript invalidates the byte cursor and any unfinished
      // line. Keep call IDs for deduplication within this observed turn.
      offset = 0;
      _partialLine.clear();
      if (turnId == null) {
        // Without an explicit turn identity, replaying rewritten historical
        // records could falsely complete the current turn. Re-arm only after
        // reading this replacement snapshot, as on the initial scan.
        _armed = false;
        _initialScan = true;
      }
    }
    _offset = offset;
    if (length == offset) {
      _finishInitialScanIfNeeded();
      return;
    }

    RandomAccessFile? handle;
    try {
      handle = await file.open();
      if (_disposed) return;
      await handle.setPosition(offset);
      while (offset < length && !_disposed) {
        final bytes = await handle.read(
          math.min(length - offset, _transcriptReadBlockBytes),
        );
        if (_disposed || bytes.isEmpty) break;
        // Only advance for bytes actually received. A short read can occur
        // if the provider truncates or replaces the file during the scan.
        offset += bytes.length;
        _offset = offset;
        _processBytes(bytes);
      }
    } catch (_) {
      return;
    } finally {
      try {
        await handle?.close();
      } catch (_) {
        // The provider may have removed the file during the read.
      }
      _finishInitialScanIfNeeded();
    }
  }

  void _processBytes(Uint8List bytes) {
    var start = 0;
    for (var index = 0; index < bytes.length && !_disposed; index++) {
      if (bytes[index] != 0x0a) continue;
      final segment = Uint8List.sublistView(bytes, start, index);
      Uint8List lineBytes;
      if (_partialLine.isEmpty) {
        lineBytes = segment;
      } else {
        _partialLine.add(segment);
        lineBytes = _partialLine.takeBytes();
      }
      final line = utf8.decode(lineBytes, allowMalformed: true).trim();
      if (line.isNotEmpty) _processLine(line);
      start = index + 1;
    }
    if (!_disposed && start < bytes.length) {
      _partialLine.add(Uint8List.sublistView(bytes, start));
    }
  }

  void _finishInitialScanIfNeeded() {
    if (!_initialScan) {
      return;
    }
    _initialScan = false;
    _armed = true;
  }

  void _processLine(String line) {
    final record = _jsonObject(line);
    if (record == null) {
      return;
    }
    final payload = record['payload'];
    if (payload is! Map<String, Object?>) {
      return;
    }
    final recordType = record['type'];
    final payloadType = payload['type'];
    if (turnId != null &&
        payload['turn_id'] == turnId &&
        (payloadType == 'task_started' || recordType == 'turn_context')) {
      _armed = true;
      return;
    }
    if (!_armed) {
      return;
    }

    final completion = _turnCompletion(recordType, payloadType, payload);
    if (completion != null) {
      _emitStop(completion);
      dispose();
      return;
    }

    final pending = _pendingFunctionCall(recordType, payloadType, payload);
    if (pending != null) {
      _emitWaiting(pending);
      return;
    }

    final completedCallId =
        recordType == 'response_item' && payloadType == 'function_call_output'
        ? _string(payload['call_id'])
        : null;
    if (completedCallId == null) {
      return;
    }
    final toolName = _pendingToolsByCallId.remove(completedCallId);
    if (toolName == null) {
      return;
    }
    _statusSink.applyHookEvent(
      AgentHookEvent(
        terminalSessionId: terminalSessionId,
        workspaceId: workspaceId,
        tabId: tabId,
        agentType: .codex,
        hookEventName: 'PostToolUse',
        version: 'codex-transcript',
        payload: <String, Object?>{
          'hook_event_name': 'PostToolUse',
          'tool_name': toolName,
          'tool_use_id': completedCallId,
          'transcript_path': transcriptPath,
        },
      ),
    );
  }

  _TranscriptTurnCompletion? _turnCompletion(
    Object? recordType,
    Object? payloadType,
    Map<String, Object?> payload,
  ) {
    if (recordType != 'event_msg') {
      return null;
    }
    if (turnId != null && payload['turn_id'] != turnId) {
      return null;
    }
    if (payloadType == 'task_complete') {
      return _TranscriptTurnCompletion(
        interrupted: false,
        lastAssistantMessage: _string(payload['last_agent_message']),
      );
    }
    if (payloadType == 'turn_aborted') {
      return const _TranscriptTurnCompletion(interrupted: true);
    }
    return null;
  }

  void _emitStop(_TranscriptTurnCompletion completion) {
    _statusSink.applyHookEvent(
      AgentHookEvent(
        terminalSessionId: terminalSessionId,
        workspaceId: workspaceId,
        tabId: tabId,
        agentType: .codex,
        hookEventName: 'Stop',
        version: 'codex-transcript',
        payload: <String, Object?>{
          'hook_event_name': 'Stop',
          'transcript_path': transcriptPath,
          'is_interrupt': completion.interrupted,
          if (completion.lastAssistantMessage != null)
            'last_assistant_message': completion.lastAssistantMessage,
        },
      ),
    );
  }

  _PendingFunctionCall? _pendingFunctionCall(
    Object? recordType,
    Object? payloadType,
    Map<String, Object?> payload,
  ) {
    if (recordType == 'response_item' && payloadType == 'function_call') {
      final toolName = _string(payload['name']);
      if (!_isCodexTranscriptWaitingTool(toolName)) {
        return null;
      }
      final callId = _string(payload['call_id']) ?? _string(payload['id']);
      // Replayed call IDs cannot emit another waiting event. Avoid parsing
      // their potentially large argument JSON a second time.
      if (callId != null && _emittedCallIds.contains(callId)) {
        return null;
      }
      return _PendingFunctionCall(
        toolName: toolName!,
        callId: callId,
        toolInput: _parseArguments(payload['arguments']),
      );
    }
    if (recordType == 'event_msg' && payloadType == 'request_user_input') {
      return _PendingFunctionCall(
        toolName: 'request_user_input',
        callId: _string(payload['call_id']),
        toolInput: <String, Object?>{'questions': payload['questions']},
      );
    }
    if (recordType == 'event_msg' && payloadType == 'request_permissions') {
      return _PendingFunctionCall(
        toolName: 'request_permissions',
        callId: _string(payload['call_id']),
        toolInput: payload,
      );
    }
    return null;
  }

  void _emitWaiting(_PendingFunctionCall call) {
    final callKey = call.callId ?? '${call.toolName}:${call.toolInput}';
    if (!_emittedCallIds.add(callKey)) {
      return;
    }
    if (call.callId != null) {
      _pendingToolsByCallId[call.callId!] = call.toolName;
    }
    _statusSink.applyHookEvent(
      AgentHookEvent(
        terminalSessionId: terminalSessionId,
        workspaceId: workspaceId,
        tabId: tabId,
        agentType: .codex,
        hookEventName: 'PreToolUse',
        version: 'codex-transcript',
        payload: <String, Object?>{
          'hook_event_name': 'PreToolUse',
          'tool_name': call.toolName,
          if (call.callId != null) 'tool_use_id': call.callId,
          if (call.toolInput != null) 'tool_input': call.toolInput,
          'transcript_path': transcriptPath,
        },
      ),
    );
  }
}

class const _TranscriptTurnCompletion({
  required final bool interrupted,
  final String? lastAssistantMessage,
});

class const _PendingFunctionCall({
  required final String toolName,
  required final String? callId,
  required final Object? toolInput,
});

String? _hookEventName(AgentHookEvent event) {
  return event.hookEventName ??
      _readString(event.payload, const <String>[
        'hook_event_name',
        'hookEventName',
      ]);
}

Map<String, Object?>? _jsonObject(String line) {
  try {
    final decoded = jsonDecode(line);
    // jsonDecode already produces string-keyed maps. Neither the record
    // nor its payload needs a second allocation for read-only dispatch.
    return decoded is Map<String, Object?> ? decoded : null;
  } catch (_) {
    return null;
  }
}

Object? _parseArguments(Object? arguments) {
  if (arguments is! String) {
    return arguments;
  }
  try {
    final decoded = jsonDecode(arguments);
    return decoded;
  } catch (_) {
    return arguments;
  }
}

String? _readString(Map<String, Object?> payload, List<String> keys) {
  for (final key in keys) {
    final value = _string(payload[key]);
    if (value != null) {
      return value;
    }
  }
  return null;
}

String? _string(Object? value) {
  if (value is! String) {
    return null;
  }
  final trimmed = value.trim();
  return trimmed.isEmpty ? null : trimmed;
}

bool _isCodexTranscriptWaitingTool(String? toolName) {
  return toolName == 'request_user_input' ||
      toolName == 'functions.request_user_input' ||
      toolName == 'request_permissions' ||
      toolName == 'functions.request_permissions' ||
      toolName == 'request_approval' ||
      toolName == 'functions.request_approval';
}

const int _initialTranscriptScanBytes = 256 * 1024;
const int _transcriptReadBlockBytes = 64 * 1024;
