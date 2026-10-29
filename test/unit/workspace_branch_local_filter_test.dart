import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('remote-tracking branches are excluded without hiding similarly named local branches', () {
    final result = filterWorkspaceLocalBranches(
      branches: const [
        'origin/main',
        'originator/dev',
        'feature/z',
        'upstream/stable',
        'feature/a',
        'feature/z',
      ],
      currentBranch: 'feature/z',
      remoteNames: const ['origin', 'upstream'],
    );
    expect(result, ['feature/a', 'feature/z', 'originator/dev']);
  });

  test('current local checkout is included even if branch discovery did not list it', () {
    expect(
      filterWorkspaceLocalBranches(
        branches: const ['origin/main', 'main'],
        currentBranch: 'current/feature',
        remoteNames: const ['origin'],
      ),
      ['current/feature', 'main'],
    );
  });

  test('detached HEAD is not synthesized and empty remote names do not filter locals', () {
    expect(
      filterWorkspaceLocalBranches(
        branches: const ['aaa', 'bbb'],
        currentBranch: 'HEAD',
        remoteNames: const ['', '  '],
      ),
      ['aaa', 'bbb'],
    );
  });
}
