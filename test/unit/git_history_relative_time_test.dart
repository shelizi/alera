import 'package:alera/src/features/workbench/presentation/git_history_relative_time.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime(2026, 10, 29, 12);
  test(
    'missing and future commit timestamps are handled without negative ages',
    () {
      expect(gitHistoryRelativeTime(null, now: now), isEmpty);
      expect(
        gitHistoryRelativeTime(now.add(const Duration(seconds: 40)), now: now),
        'just now',
      );
    },
  );
  test('Git graph relative ages preserve the minute/hour/day boundaries', () {
    expect(
      gitHistoryRelativeTime(
        now.subtract(const Duration(seconds: 59)),
        now: now,
      ),
      'just now',
    );
    expect(
      gitHistoryRelativeTime(
        now.subtract(const Duration(minutes: 1)),
        now: now,
      ),
      '1m ago',
    );
    expect(
      gitHistoryRelativeTime(
        now.subtract(const Duration(minutes: 59)),
        now: now,
      ),
      '59m ago',
    );
    expect(
      gitHistoryRelativeTime(now.subtract(const Duration(hours: 1)), now: now),
      '1h ago',
    );
    expect(
      gitHistoryRelativeTime(now.subtract(const Duration(hours: 23)), now: now),
      '23h ago',
    );
    expect(
      gitHistoryRelativeTime(now.subtract(const Duration(days: 1)), now: now),
      '1d ago',
    );
    expect(
      gitHistoryRelativeTime(now.subtract(const Duration(days: 29)), now: now),
      '29d ago',
    );
  });
  test('old Git commits use a stable yyyy-MM-dd local date', () {
    expect(
      gitHistoryRelativeTime(DateTime(2026, 2, 7, 12), now: now),
      '2026-02-07',
    );
  });
}
