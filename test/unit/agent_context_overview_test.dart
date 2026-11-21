import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'keeps the overall objective when a later user prompt narrows the task',
    () {
      final now = DateTime.utc(2026, 11, 21, 4, 0);
      final first = AgentContextOverview.observe(
        previous: const AgentContextOverview(),
        previousPrompt: null,
        previousAssistant: null,
        prompt: '讓 GPUI 與 Classic 設定功能完整對齊',
        assistant: null,
        at: now,
      );
      final updated = AgentContextOverview.observe(
        previous: first,
        previousPrompt: '讓 GPUI 與 Classic 設定功能完整對齊',
        previousAssistant: null,
        prompt: '先把儲存按鈕修好',
        assistant: '已加入表單狀態驗證，尚需實機檢查',
        at: now.add(const Duration(minutes: 7)),
      );
      expect(updated.goal, first.goal);
      expect(updated.progress, '已加入表單狀態驗證，尚需實機檢查');
      expect(updated.timeline.map((e) => e.kind), <String>[
        'report',
        'instruction',
        'goal',
      ]);
      final same = AgentContextOverview.observe(
        previous: updated,
        previousPrompt: '先把儲存按鈕修好',
        previousAssistant: '已加入表單狀態驗證，尚需實機檢查',
        prompt: '先把儲存按鈕修好',
        assistant: '已加入表單狀態驗證，尚需實機檢查',
        at: now.add(const Duration(minutes: 8)),
      );
      expect(same.timeline.length, updated.timeline.length);
      expect(same.goal, first.goal);
    },
  );

  test('bounds history and text despite untrusted Host input', () {
    final now = DateTime.utc(2026, 11, 21).toIso8601String();
    final overview = AgentContextOverview.fromRaw(<String, Object?>{
      'goal': '任'.padRight(999, '務'),
      'progress': '進'.padRight(999, '度'),
      'timeline': <Object?>[
        for (var i = 0; i < 40; i++)
          <String, Object?>{
            'kind': i.isEven ? 'report' : 'instruction',
            'text': '事'.padRight(500, '項'),
            'at': now,
          },
      ],
    });
    expect(overview.goal.runes.length, 600);
    expect(overview.progress.runes.length, 420);
    expect(overview.timeline, hasLength(16));
    expect(overview.timeline.first.text.runes.length, 260);
    expect(overview.toJson()['goal'], overview.goal);
  });

  test(
    'delayed Hook cannot overwrite a newer report or break timeline order',
    () {
      final older = DateTime.utc(2026, 11, 21, 8, 0);
      final newer = older.add(const Duration(minutes: 5));
      final first = AgentContextOverview.observe(
        previous: const AgentContextOverview(),
        previousPrompt: null,
        previousAssistant: null,
        prompt: '完成跨版本設定對齊',
        assistant: '最新進度',
        at: newer,
      );
      final late = AgentContextOverview.observe(
        previous: first,
        previousPrompt: '完成跨版本設定對齊',
        previousAssistant: '最新進度',
        prompt: '完成跨版本設定對齊',
        assistant: '稍早的回報',
        at: older,
      );
      expect(late.progress, '最新進度');
      expect(late.timeline.first.text, '最新進度');
      expect(late.timeline.last.text, '稍早的回報');
    },
  );

  test('drops malformed and unknown event types', () {
    final overview = AgentContextOverview.fromRaw(<String, Object?>{
      'goal': '修正 Windows 問題',
      'timeline': <Object?>[
        <String, Object?>{
          'kind': 'toolInput',
          'text': 'SECRET',
          'at': '2026-11-21T12:00:00Z',
        },
        <String, Object?>{'kind': 'report', 'text': '正常回報', 'at': 'not-a-date'},
        <String, Object?>{
          'kind': 'report',
          'text': '有效回報',
          'at': '2026-11-21T12:01:00Z',
        },
      ],
    });
    expect(overview.timeline, hasLength(1));
    expect(overview.timeline.first.text, '有效回報');
    expect(AgentContextOverview.fromRaw(null).timeline, isEmpty);
  });
}
