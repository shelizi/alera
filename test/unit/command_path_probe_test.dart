import 'package:alera/src/shared/infra/process/command_path_probe.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('commandResolvesOnPath', () {
    bool probe(
      String command,
      Map<String, String> environment, {
      required bool isWindows,
      required Set<String> existing,
    }) {
      return commandResolvesOnPath(
        command,
        environment: environment,
        isWindows: isWindows,
        executableExists: existing.contains,
      );
    }

    test('finds a command in a PATH segment', () {
      expect(
        probe(
          'claude',
          <String, String>{'PATH': '/usr/bin:/opt/agents'},
          isWindows: false,
          existing: <String>{'/opt/agents/claude'},
        ),
        isTrue,
      );
    });

    test('misses a command that no segment carries', () {
      expect(
        probe(
          'claude',
          <String, String>{'PATH': '/usr/bin:/opt/agents'},
          isWindows: false,
          existing: <String>{'/usr/bin/codex'},
        ),
        isFalse,
      );
    });

    test('misses when PATH is absent or empty', () {
      expect(
        probe(
          'claude',
          <String, String>{},
          isWindows: false,
          existing: <String>{'/usr/bin/claude'},
        ),
        isFalse,
      );
      expect(
        probe(
          'claude',
          <String, String>{'PATH': ''},
          isWindows: false,
          existing: <String>{'/usr/bin/claude'},
        ),
        isFalse,
      );
    });

    test('rejects names that are already paths', () {
      expect(
        probe(
          '/opt/agents/claude',
          <String, String>{'PATH': '/usr/bin'},
          isWindows: false,
          existing: <String>{'/opt/agents/claude'},
        ),
        isFalse,
      );
      expect(
        probe(
          r'agents\claude',
          <String, String>{'Path': r'C:\tools'},
          isWindows: true,
          existing: <String>{r'C:\tools\agents\claude.exe'},
        ),
        isFalse,
      );
    });

    test('matches PATHEXT shims on Windows', () {
      final environment = <String, String>{
        'Path': r'C:\tools;D:\bin',
        'PATHEXT': '.COM;.EXE;.BAT;.CMD',
      };
      expect(
        probe(
          'claude',
          environment,
          isWindows: true,
          existing: <String>{r'C:\tools\claude.cmd'},
        ),
        isTrue,
      );
      expect(
        probe(
          'codex',
          environment,
          isWindows: true,
          existing: <String>{r'D:\bin\codex.exe'},
        ),
        isTrue,
      );
    });

    test('falls back to the default PATHEXT set on Windows', () {
      expect(
        probe(
          'opencode',
          <String, String>{'Path': r'C:\tools'},
          isWindows: true,
          existing: <String>{r'C:\tools\opencode.cmd'},
        ),
        isTrue,
      );
    });

    test('reads the PATH key case-insensitively on Windows', () {
      expect(
        probe(
          'claude',
          <String, String>{'pAtH': r'C:\tools'},
          isWindows: true,
          existing: <String>{r'C:\tools\claude.exe'},
        ),
        isTrue,
      );
    });

    test('skips empty and quoted PATH segments', () {
      expect(
        probe(
          'claude',
          <String, String>{'Path': r'"";"C:\Program Files\agents"'},
          isWindows: true,
          existing: <String>{r'C:\Program Files\agents\claude.exe'},
        ),
        isTrue,
      );
      expect(
        probe(
          'claude',
          <String, String>{'PATH': ':/usr/bin'},
          isWindows: false,
          existing: <String>{'claude'},
        ),
        isFalse,
      );
    });
  });
}
