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
    this.timeline = const <AgentContextMilestone>[],
  });

  final String goal;
  final String progress;

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
    if (goal.isEmpty && currentPrompt.isNotEmpty) {
      goal = currentPrompt;
      record('goal', _clean(currentPrompt, 260));
    } else if (currentPrompt.isNotEmpty &&
        previousPrompt != null &&
        _clean(previousPrompt, 600) != currentPrompt) {
      record('instruction', _clean(currentPrompt, 260));
    }
    final report = _clean(assistant, 420);
    if (report.isNotEmpty && _clean(previousAssistant, 420) != report) {
      final recentReports = timeline.where((event) => event.kind == 'report');
      final latest = recentReports.isEmpty ? null : recentReports.first.at;
      // A delayed Hook must never replace a newer status already displayed.
      if (latest == null || !at.isBefore(latest)) {
        progress = report;
      }
      record('report', _clean(report, 260));
    }
    return AgentContextOverview(
      goal: goal,
      progress: progress,
      timeline: List<AgentContextMilestone>.unmodifiable(timeline),
    );
  }

  Map<String, dynamic> toJson() => <String, dynamic>{
    'goal': goal,
    'progress': progress,
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
