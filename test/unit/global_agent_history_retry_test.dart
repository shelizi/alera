import 'package:alera/src/features/workbench/application/global_agent_history_retry.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const original = (
    provider: 'codex',
    accountId: 'secondary',
    projectId: 'project',
    workspaceId: 'workspace',
    providerSessionId: '550e8400-e29b-41d4-a716-446655440001',
    profileId: 'codex-profile',
  );

  test('lost acknowledgement reuses the original mutation UUID', () {
    final tracker = ExactHistoryResumeRetryTracker();
    var created = 0;
    String fresh() => 'mutation-${++created}';
    expect(tracker.reserve(original, fresh), 'mutation-1');
    expect(tracker.hasUnconfirmed(original), isTrue);
    // No success acknowledgement: a deliberate next attempt is idempotent.
    expect(tracker.reserve(original, fresh), 'mutation-1');
    expect(created, 1);
  });

  test('account, project, worktree and Profile never share retry IDs', () {
    final tracker = ExactHistoryResumeRetryTracker();
    var created = 0;
    String fresh() => 'mutation-${++created}';
    ExactHistoryResumeIdentity variation({
      String? accountId,
      String? projectId,
      String? workspaceId,
      String? providerSessionId,
      String? profileId,
    }) => (
      provider: original.provider,
      accountId: accountId ?? original.accountId,
      projectId: projectId ?? original.projectId,
      workspaceId: workspaceId ?? original.workspaceId,
      providerSessionId: providerSessionId ?? original.providerSessionId,
      profileId: profileId ?? original.profileId,
    );
    final variants = [
      original,
      variation(accountId: 'different'),
      variation(projectId: 'different'),
      variation(workspaceId: 'different'),
      variation(providerSessionId: '550e8400-e29b-41d4-a716-446655440099'),
      variation(profileId: 'different'),
    ];
    for (final identity in variants) {
      tracker.reserve(identity, fresh);
    }
    expect(created, variants.length);
    expect(tracker.reserve(original, fresh), 'mutation-1');
  });

  test('acknowledged result clears only its original retry identity', () {
    final tracker = ExactHistoryResumeRetryTracker();
    final other = (
      provider: original.provider,
      accountId: original.accountId,
      projectId: original.projectId,
      workspaceId: original.workspaceId,
      providerSessionId: original.providerSessionId,
      profileId: 'claude-profile',
    );
    tracker.reserve(original, () => 'mutation-one');
    tracker.reserve(other, () => 'mutation-other');
    tracker.acknowledge(original);
    expect(tracker.hasUnconfirmed(original), isFalse);
    expect(tracker.hasUnconfirmed(other), isTrue);
    expect(tracker.reserve(original, () => 'mutation-new'), 'mutation-new');
    expect(tracker.reserve(other, () => 'unexpected'), 'mutation-other');
  });
}
