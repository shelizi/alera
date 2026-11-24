/// Typed, bounded read model for one Host-authenticated Agent task overview.
/// Hook reports are not proof that implementation or testing succeeded.
class AgentContextMilestone {
  const AgentContextMilestone({
    required this.kind,
    required this.text,
    required this.at,
  });
  final String kind;
  final String text;
  final DateTime at;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is AgentContextMilestone &&
          kind == other.kind &&
          text == other.text &&
          at == other.at;

  @override
  int get hashCode => Object.hash(kind, text, at);

  Map<String, Object?> toJson() => <String, Object?>{
    'kind': kind,
    'text': text,
    'at': at.toUtc().toIso8601String(),
  };
}

class AgentContextOverview {
  const AgentContextOverview({
    this.goal = '',
    this.progress = '',
    this.reportSummary = '',
    this.initialGoal = '',
    this.currentFocus = '',
    this.focusUpdatedAt,
    this.timeline = const <AgentContextMilestone>[],
  });

  final String goal;
  final String progress;
  final String reportSummary;
  String get displayProgress =>
      reportSummary.isNotEmpty ? reportSummary : progress;

  /// Keep the original task anchor while later instructions change focus.
  final String initialGoal;
  final String currentFocus;
  final DateTime? focusUpdatedAt;

  /// Latest event first; only meaningful user instructions and Agent reports.
  final List<AgentContextMilestone> timeline;

  // Cache only snapshots we create and deep-freeze. Expando keys are weak:
  // a closed session or superseded snapshot cannot pin a history in memory.
  // Caller/Host maps are never retained; bounded scalar signatures allow
  // exact repeat detection without reparsing a validated snapshot.
  static final _snapshotViews = Expando<AgentContextOverview>();
  static final _snapshotInputs = Expando<_ContextSnapshotInput>();

  /// Reuse a prior immutable snapshot only when the relevant raw inputs
  /// are exactly equal, or their fully validated typed views are equal.
  /// Hosts can resend a fresh JSON map on every tool event; do not trust a
  /// contextChanged hint (a client may have missed a prior delta).
  static Map<String, dynamic> snapshotFromRaw(
    Object? raw, {
    Map<String, dynamic>? previous,
  }) {
    if (raw is Map<String, dynamic> && _snapshotViews[raw] != null) {
      return raw;
    }
    // The activation token is Host metadata, not visible report content.
    // Identical reports from two native Sessions must not share a snapshot.
    final rawToken = raw is Map ? raw['conversationId'] : null;
    final conversationId =
        rawToken is String && rawToken.isNotEmpty && rawToken.length <= 128
        ? rawToken
        : null;
    final previousView = previous == null ? null : _snapshotViews[previous];
    final sameConversation = previous?['conversationId'] == conversationId;
    if (previous != null &&
        previousView != null &&
        sameConversation &&
        raw is Map &&
        _snapshotInputs[previous]?.matches(raw) == true) {
      return previous;
    }
    final overview = AgentContextOverview.fromRaw(raw);
    if (previous != null &&
        previousView != null &&
        sameConversation &&
        overview == previousView) {
      final input = _ContextSnapshotInput.capture(raw);
      if (input != null) _snapshotInputs[previous] = input;
      return previous;
    }
    final json = overview.toJson();
    if (conversationId != null) json['conversationId'] = conversationId;
    json['timeline'] = List<Map<String, Object?>>.unmodifiable(
      (json['timeline'] as List<Map<String, Object?>>).map(
        (event) => Map<String, Object?>.unmodifiable(event),
      ),
    );
    final snapshot = Map<String, dynamic>.unmodifiable(json);
    _snapshotViews[snapshot] = overview;
    final input = _ContextSnapshotInput.capture(raw);
    if (input != null) _snapshotInputs[snapshot] = input;
    return snapshot;
  }

  /// Update a canonical Hook snapshot, without re-parsing or serializing a
  /// whole overview on tool-only events. Legacy/mutable input is normalized
  /// before it can be reused. A missing task anchor still needs initialization.
  static Map<String, dynamic> observeSnapshot({
    required Map<String, dynamic>? previous,
    required String? previousPrompt,
    required String? previousAssistant,
    required String prompt,
    required String? assistant,
    required DateTime at,
  }) {
    final snapshot = snapshotFromRaw(previous);
    final overview = AgentContextOverview.fromRaw(snapshot);
    if ((assistant == null || assistant.isEmpty) &&
        ((overview.initialGoal.isNotEmpty &&
                (prompt.isEmpty || prompt == previousPrompt)) ||
            (overview.initialGoal.isEmpty &&
                overview.goal.isEmpty &&
                prompt.isEmpty))) {
      return snapshot;
    }
    final observed = observe(
      previous: overview,
      previousPrompt: previousPrompt,
      previousAssistant: previousAssistant,
      prompt: prompt,
      assistant: assistant,
      at: at,
    );
    return observed == overview ? snapshot : snapshotFromRaw(observed.toJson());
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    if (other is! AgentContextOverview ||
        goal != other.goal ||
        progress != other.progress ||
        reportSummary != other.reportSummary ||
        initialGoal != other.initialGoal ||
        currentFocus != other.currentFocus ||
        focusUpdatedAt != other.focusUpdatedAt ||
        timeline.length != other.timeline.length) {
      return false;
    }
    for (var i = 0; i < timeline.length; i++) {
      if (timeline[i] != other.timeline[i]) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hash(
    goal,
    progress,
    reportSummary,
    initialGoal,
    currentFocus,
    focusUpdatedAt,
    Object.hashAll(timeline),
  );

  static final _controlCharacters = RegExp(r'[\x00-\x1F\x7F]+');
  static final _whitespace = RegExp(r'\s+');
  static final _continuationPunctuation = RegExp(
    r'^[\s。，？！.!?,]+|[\s。，？！.!?,]+$',
  );

  static String _clean(Object? value, int limit) {
    if (value is! String) return '';
    return value
        .trim()
        .replaceAll(_controlCharacters, ' ')
        .replaceAll(_whitespace, ' ')
        .trim()
        .charactersTake(limit);
  }

  static bool _substantive(String prompt) {
    final lower = prompt
        .trim()
        .replaceAll(_continuationPunctuation, '')
        .toLowerCase();
    return !const <String>{
      '',
      '繼續',
      '繼續吧',
      '嗯',
      '好',
      '好的',
      '做吧',
      '進度',
      'ok',
      'okay',
      'continue',
      'go on',
      'next',
    }.contains(lower);
  }

  static bool _changesGoal(String prompt) {
    final lower = prompt.toLowerCase();
    return const <String>[
      '改成',
      '改為',
      '新目標',
      '新的目標',
      '改做',
      'switch to',
      'change the goal',
      'instead',
    ].any(lower.startsWith);
  }

  static String _evolvingGoal(String base, String focus) {
    if (focus.isEmpty || base == focus) return _clean(base, 600);
    return _clean('${_clean(base, 370)}｜目前重點：${_clean(focus, 200)}', 600);
  }

  /// A bounded excerpt of the model's own final answer, not a fresh model call.
  /// Preserve its last conclusion when a long response opens with boilerplate.
  static String _reportExcerpt(String? response, int limit) {
    final value = _clean(response, 8000);
    final chars = value.runes.toList(growable: false);
    if (chars.length <= limit) return value;
    final head = limit * 3 ~/ 5;
    final tail = limit - head - 3;
    return '${String.fromCharCodes(chars.take(head))} … '
        '${String.fromCharCodes(chars.skip(chars.length - tail))}';
  }

  factory AgentContextOverview.fromRaw(Object? raw) {
    if (raw is! Map) return const AgentContextOverview();
    final cached = _snapshotViews[raw];
    if (cached != null) return cached;
    final rawTimeline = raw['timeline'];
    final timeline = <AgentContextMilestone>[];
    if (rawTimeline is List) {
      for (final item in rawTimeline.take(16)) {
        if (item is! Map) continue;
        final kind = item['kind'];
        if (kind is! String ||
            !const <String>['goal', 'instruction', 'report'].contains(kind)) {
          continue;
        }
        final text = _clean(item['text'], 260);
        final at = DateTime.tryParse(
          item['at'] is String ? item['at'] as String : '',
        );
        if (text.isEmpty || at == null) continue;
        timeline.add(AgentContextMilestone(kind: kind, text: text, at: at));
      }
    }
    timeline.sort((a, b) => b.at.compareTo(a.at));
    return AgentContextOverview(
      goal: _clean(raw['goal'], 600),
      progress: _clean(raw['progress'], 420),
      reportSummary: _clean(raw['reportSummary'], 420),
      initialGoal: _clean(raw['initialGoal'], 600),
      currentFocus: _clean(raw['currentFocus'], 600),
      focusUpdatedAt: DateTime.tryParse(
        raw['focusUpdatedAt'] is String ? raw['focusUpdatedAt'] as String : '',
      ),
      timeline: List<AgentContextMilestone>.unmodifiable(timeline),
    );
  }

  static AgentContextOverview observe({
    required AgentContextOverview previous,
    required String? previousPrompt,
    required String? previousAssistant,
    required String prompt,
    required String? assistant,
    required DateTime at,
  }) {
    var goal = previous.goal;
    var progress = previous.progress;
    var reportSummary = previous.reportSummary;
    var initialGoal = previous.initialGoal.isEmpty
        ? previous.goal
        : previous.initialGoal;
    var focus = previous.currentFocus;
    var focusUpdatedAt = previous.focusUpdatedAt;
    final timeline = <AgentContextMilestone>[...previous.timeline];
    void record(String kind, String text) {
      if (text.isEmpty) {
        return;
      }
      if (timeline.isNotEmpty &&
          timeline.first.kind == kind &&
          timeline.first.text == text) {
        return;
      }
      final position = timeline.indexWhere((event) => !event.at.isAfter(at));
      final update = AgentContextMilestone(kind: kind, text: text, at: at);
      if (position == -1) {
        timeline.add(update);
      } else {
        timeline.insert(position, update);
      }
      if (timeline.length > 16) {
        timeline.removeRange(16, timeline.length);
      }
    }

    final currentPrompt = _clean(prompt, 600);
    if (initialGoal.isEmpty && currentPrompt.isNotEmpty) {
      initialGoal = currentPrompt;
      goal = currentPrompt;
      record('goal', _clean(currentPrompt, 260));
    } else if (currentPrompt.isNotEmpty &&
        (previousPrompt == null ||
            _clean(previousPrompt, 600) != currentPrompt) &&
        currentPrompt != initialGoal &&
        currentPrompt != focus &&
        _substantive(currentPrompt)) {
      if (focusUpdatedAt == null || !at.isBefore(focusUpdatedAt)) {
        if (_changesGoal(currentPrompt)) {
          initialGoal = currentPrompt;
          focus = '';
          record('goal', _clean(currentPrompt, 260));
        } else {
          focus = currentPrompt;
          record('instruction', _clean(currentPrompt, 260));
        }
        focusUpdatedAt = at;
        goal = _evolvingGoal(initialGoal, focus);
      } else {
        record('instruction', _clean(currentPrompt, 260));
      }
    }
    final report = _reportExcerpt(assistant, 420);
    // The status field may already contain the assistant's text from a
    // working-state event; only the completed report is a dedupe baseline.
    if (report.isNotEmpty &&
        (progress != report ||
            previousAssistant?.trim() != assistant?.trim())) {
      final recentReports = timeline.where((event) => event.kind == 'report');
      final latest = recentReports.isEmpty ? null : recentReports.first.at;
      // A delayed Hook must never replace a newer status already displayed.
      if (latest == null || !at.isBefore(latest)) {
        progress = report;
        reportSummary = '';
      }
      record('report', _reportExcerpt(assistant, 260));
    }
    return AgentContextOverview(
      goal: goal,
      progress: progress,
      reportSummary: reportSummary,
      initialGoal: initialGoal,
      currentFocus: focus,
      focusUpdatedAt: focusUpdatedAt,
      timeline: List<AgentContextMilestone>.unmodifiable(timeline),
    );
  }

  Map<String, dynamic> toJson() => <String, dynamic>{
    'goal': goal,
    'progress': progress,
    if (reportSummary.isNotEmpty) 'reportSummary': reportSummary,
    'initialGoal': initialGoal,
    'currentFocus': currentFocus,
    'focusUpdatedAt': focusUpdatedAt?.toUtc().toIso8601String(),
    'timeline': <Map<String, Object?>>[
      for (final event in timeline) event.toJson(),
    ],
  };
}

/// Exact, bounded inputs for the normal Host shape. Store only the scalar
/// strings read by fromRaw, never the caller's mutable maps, arbitrary keys,
/// tool payloads or oversized malformed history. Weakly attached to the
/// canonical snapshot, so closing a session releases the source signature.
final class _ContextSnapshotInput {
  _ContextSnapshotInput(this.fields, this.timeline);

  static const _fieldLimits = <String, int>{
    'goal': 1200,
    'progress': 840,
    'reportSummary': 840,
    'initialGoal': 1200,
    'currentFocus': 1200,
    'focusUpdatedAt': 128,
    'conversationId': 128,
  };

  final List<String?> fields;
  final List<({String kind, String text, String at})> timeline;

  static _ContextSnapshotInput? capture(Object? raw) {
    if (raw is! Map) return null;
    final fields = <String?>[];
    for (final field in _fieldLimits.entries) {
      final value = raw[field.key];
      if (value != null && (value is! String || value.length > field.value)) {
        return null;
      }
      fields.add(value as String?);
    }
    final rawTimeline = raw['timeline'];
    if (rawTimeline is! List || rawTimeline.length > 16) return null;
    final timeline = <({String kind, String text, String at})>[];
    for (final item in rawTimeline) {
      if (item is! Map) return null;
      final kind = item['kind'];
      final text = item['text'];
      final at = item['at'];
      if (kind is! String ||
          kind.length > 16 ||
          text is! String ||
          text.length > 520 ||
          at is! String ||
          at.length > 128) {
        return null;
      }
      timeline.add((kind: kind, text: text, at: at));
    }
    return _ContextSnapshotInput(fields, timeline);
  }

  bool matches(Map raw) {
    var index = 0;
    for (final field in _fieldLimits.keys) {
      if (raw[field] != fields[index++]) return false;
    }
    final items = raw['timeline'];
    if (items is! List || items.length != timeline.length) return false;
    for (var i = 0; i < timeline.length; i++) {
      final item = items[i];
      final old = timeline[i];
      if (item is! Map ||
          item['kind'] != old.kind ||
          item['text'] != old.text ||
          item['at'] != old.at) {
        return false;
      }
    }
    return true;
  }
}

/// String extension avoids splitting UTF-16 surrogate pairs.
extension _ContextTakeCharacters on String {
  String charactersTake(int count) {
    // UTF-16 length is an upper bound on rune count. Short strings need
    // neither a rune iterator nor a StringBuffer.
    if (length <= count) return this;
    var total = 0;
    final buffer = StringBuffer();
    for (final rune in runes) {
      if (total++ >= count) break;
      buffer.writeCharCode(rune);
    }
    return buffer.toString();
  }
}
