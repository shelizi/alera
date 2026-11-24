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

  static String _clean(Object? value, int limit) {
    if (value is! String) return '';
    return value
        .trim()
        .replaceAll(RegExp(r'[\x00-\x1F\x7F]+'), ' ')
        .replaceAll(RegExp(r'\s+'), ' ')
        .trim()
        .charactersTake(limit);
  }

  static bool _substantive(String prompt) {
    final lower = prompt
        .trim()
        .replaceAll(RegExp(r'^[\s。，？！.!?,]+|[\s。，？！.!?,]+$'), '')
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
    if (report.isNotEmpty && previousAssistant?.trim() != assistant?.trim()) {
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

/// String extension avoids splitting UTF-16 surrogate pairs.
extension _ContextTakeCharacters on String {
  String charactersTake(int count) {
    var total = 0;
    final buffer = StringBuffer();
    for (final rune in runes) {
      if (total++ >= count) break;
      buffer.writeCharCode(rune);
    }
    return buffer.toString();
  }
}
