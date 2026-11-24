import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'canonical snapshots share a typed view and isolate mutable raw input',
    () {
      final now = DateTime.utc(2026, 11, 24);
      final event = <String, Object?>{
        'kind': 'report',
        'text': 'Original report',
        'at': now.toIso8601String(),
      };
      final raw = <String, dynamic>{
        'goal': ' Original goal ',
        'timeline': [event],
        'unknown': 'not part of the context',
      };
      final snapshot = AgentContextOverview.snapshotFromRaw(raw);
      final overview = AgentContextOverview.fromRaw(snapshot);
      expect(overview.goal, 'Original goal');
      expect(AgentContextOverview.snapshotFromRaw(snapshot), same(snapshot));
      expect(AgentContextOverview.fromRaw(snapshot), same(overview));
      expect(snapshot, isNot(contains('unknown')));
      expect(() => snapshot['goal'] = 'Changed', throwsUnsupportedError);
      final timeline = snapshot['timeline'] as List;
      expect(() => timeline.clear(), throwsUnsupportedError);
      expect(
        () => (timeline.single as Map)['text'] = 'Changed',
        throwsUnsupportedError,
      );
      event['text'] = 'New report';
      raw['goal'] = 'New goal';
      expect(AgentContextOverview.fromRaw(raw).goal, 'New goal');
      expect(
        AgentContextOverview.fromRaw(raw).timeline.single.text,
        'New report',
      );
      expect(AgentContextOverview.fromRaw(snapshot).goal, 'Original goal');
      expect(
        AgentContextOverview.fromRaw(snapshot).timeline.single.text,
        'Original report',
      );

      final equivalent = AgentContextOverview.fromRaw(overview.toJson());
      expect(equivalent, overview);
      expect(equivalent.hashCode, overview.hashCode);
    },
  );

  test('legacy snapshots initialize missing anchors before no-op reuse', () {
    final now = DateTime.utc(2026, 11, 24);
    final previous = <String, dynamic>{'goal': 'Existing goal'};
    final next = AgentContextOverview.observeSnapshot(
      previous: previous,
      previousPrompt: 'Existing goal',
      previousAssistant: null,
      prompt: 'Existing goal',
      assistant: null,
      at: now,
    );
    expect(next['initialGoal'], 'Existing goal');
    final unchanged = AgentContextOverview.observeSnapshot(
      previous: next,
      previousPrompt: 'Existing goal',
      previousAssistant: null,
      prompt: 'Existing goal',
      assistant: null,
      at: now,
    );
    expect(unchanged, same(next));
    final missing = AgentContextOverview.observeSnapshot(
      previous: const <String, dynamic>{},
      previousPrompt: 'New task',
      previousAssistant: null,
      prompt: 'New task',
      assistant: null,
      at: now,
    );
    expect(missing['goal'], 'New task');
    expect(AgentContextOverview.fromRaw(missing).timeline.single.kind, 'goal');
  });

  test('records completed report even when working status already had the same text', () {
    final at = DateTime.utc(2026, 11, 24);
    final working = AgentContextOverview.observe(
      previous: const AgentContextOverview(),
      previousPrompt: null,
      previousAssistant: null,
      prompt: 'Fix native hook',
      assistant: null,
      at: at,
    );
    final completed = AgentContextOverview.observe(
      previous: working,
      previousPrompt: 'Fix native hook',
      previousAssistant: 'Raw report',
      prompt: 'Fix native hook',
      assistant: 'Raw report',
      at: at.add(const Duration(seconds: 1)),
    );
    expect(completed.progress, 'Raw report');
    expect(completed.reportSummary, isEmpty);
    expect(completed.timeline.first.kind, 'report');
    final duplicate = AgentContextOverview.observe(
      previous: completed,
      previousPrompt: 'Fix native hook',
      previousAssistant: 'Raw report',
      prompt: 'Fix native hook',
      assistant: 'Raw report',
      at: at.add(const Duration(seconds: 2)),
    );
    expect(duplicate.toJson(), completed.toJson());
  });

  test(
    'AI summary never overwrites original report and resets on fresh report',
    () {
      final previous = AgentContextOverview.fromRaw(<String, Object?>{
        'progress': 'Original detailed response',
        'reportSummary': 'Short summary',
      });
      expect(previous.displayProgress, 'Short summary');
      expect(previous.progress, 'Original detailed response');
      expect(
        AgentContextOverview.fromRaw(previous.toJson()).displayProgress,
        'Short summary',
      );
      final next = AgentContextOverview.observe(
        previous: previous,
        previousPrompt: null,
        previousAssistant: previous.progress,
        prompt: '',
        assistant: 'New raw report',
        at: DateTime.utc(2026, 11, 24),
      );
      expect(next.displayProgress, 'New raw report');
      expect(next.reportSummary, isEmpty);
    },
  );
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
      expect(updated.goal, contains(first.goal));
      expect(updated.goal, contains('先把儲存按鈕修好'));
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
      expect(same.goal, updated.goal);
    },
  );

  test(
    'multi-turn goal keeps the anchor, ignores continue, and honors changes',
    () {
      final at = DateTime.utc(2026, 11, 22, 8);
      final first = AgentContextOverview.observe(
        previous: const AgentContextOverview(),
        previousPrompt: null,
        previousAssistant: null,
        prompt: '完成 GPUI 與 Classic 的設定對齊',
        assistant: null,
        at: at,
      );
      final second = AgentContextOverview.observe(
        previous: first,
        previousPrompt: first.initialGoal,
        previousAssistant: null,
        prompt: '修正字體選擇和 Terminal 配色',
        assistant: '字體設定完成，Terminal 配色驗證中',
        at: at.add(const Duration(minutes: 1)),
      );
      expect(second.goal, contains('完成 GPUI 與 Classic 的設定對齊'));
      expect(second.goal, contains('修正字體選擇和 Terminal 配色'));
      expect(second.progress, '字體設定完成，Terminal 配色驗證中');
      expect(second.timeline.first.kind, 'report');
      final continuation = AgentContextOverview.observe(
        previous: second,
        previousPrompt: second.currentFocus,
        previousAssistant: second.progress,
        prompt: '繼續',
        assistant: null,
        at: at.add(const Duration(minutes: 2)),
      );
      expect(continuation.goal, second.goal);
      final changed = AgentContextOverview.observe(
        previous: continuation,
        previousPrompt: '繼續',
        previousAssistant: second.progress,
        prompt: '改成修正 Agent Dashboard 的歷史檢視',
        assistant: null,
        at: at.add(const Duration(minutes: 3)),
      );
      expect(changed.goal, contains('Agent Dashboard'));
      expect(changed.goal, isNot(contains('Terminal 配色')));
      expect(AgentContextOverview.fromRaw(changed.toJson()).goal, changed.goal);
    },
  );

  test('late follow-up stays in timeline without overwriting newer goal', () {
    final later = DateTime.utc(2026, 11, 22, 10);
    final current = AgentContextOverview(
      goal: '最初目標｜目前重點：新工作',
      initialGoal: '最初目標',
      currentFocus: '新工作',
      focusUpdatedAt: later,
    );
    final result = AgentContextOverview.observe(
      previous: current,
      previousPrompt: '先前要求',
      previousAssistant: null,
      prompt: '延遲到達的舊要求',
      assistant: null,
      at: later.subtract(const Duration(minutes: 2)),
    );
    expect(result.goal, current.goal);
    expect(result.timeline.last.text, contains('延遲到達'));
  });

  test(
    'long Agent final replies retain their latest conclusion in reports',
    () {
      final start = DateTime.utc(2026, 11, 22);
      final firstReply =
          '工作匯報。${List<String>.filled(150, '檢查過程').join()}最後結論：測試成功';
      final first = AgentContextOverview.observe(
        previous: const AgentContextOverview(),
        previousPrompt: null,
        previousAssistant: null,
        prompt: '修正狀態面板',
        assistant: firstReply,
        at: start,
      );
      expect(first.progress, contains('最後結論：測試成功'));
      expect(first.timeline.first.text, contains('最後結論：測試成功'));
      final newReply =
          '工作匯報。${List<String>.filled(150, '檢查過程').join()}最後結論：修正完成';
      final next = AgentContextOverview.observe(
        previous: first,
        previousPrompt: '修正狀態面板',
        previousAssistant: firstReply,
        prompt: '修正狀態面板',
        assistant: newReply,
        at: start.add(const Duration(seconds: 1)),
      );
      expect(next.progress, contains('最後結論：修正完成'));
      expect(next.timeline.first.kind, 'report');
      expect(next.timeline, hasLength(3));
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
