import 'dart:io';

import 'package:alera/src/features/ai_assist/application/ai_assist_agent_runner.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_merge_resolution.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_service.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/shared/infra/git/rust_git_backend.dart';
import 'package:alera/src/shared/infra/process/command_environment_resolver.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

/// Opt-in live model test. Requires ALERA_LLM_MERGE_E2E=1 and a temporary Git
/// conflict repository path in ALERA_LLM_MERGE_FIXTURE.
///
/// Uses Alera's real prompt, CLI agent task planning, restricted policy and
/// output parser. Dart Process replaces only the desktop-specific Rust bridge.
void main() {
  test(
    'live Claude resolves two-sided merge without touching the index or file',
    () async {
      final path = Platform.environment['ALERA_LLM_MERGE_FIXTURE'];
      expect(path, isNotNull);
      final file = File(path! + Platform.pathSeparator + 'total.js');
      expect(await file.exists(), isTrue);
      final before = await file.readAsBytes();
      final beforeStatus = await _git(path, ['status', '--porcelain']);
      expect(beforeStatus, contains('UU total.js'));
      final base = await _git(path, ['show', ':1:total.js']);
      final ours = await _git(path, ['show', ':2:total.js']);
      final theirs = await _git(path, ['show', ':3:total.js']);

      final runner = _IoProcessRunner();
      final cliRunner = CliAgentTaskRunner(
        processRunner: runner,
        commandEnvironmentResolver: _TestEnvironment(),
      );
      final service = CliAiAssistService(
        gitBackend: const RustGitBackend(),
        processRunner: runner,
        runner: cliRunner,
      );
      final request = AiAssistRequest(
        operation: .mergeConflicts,
        workspacePath: path,
        settings: const AiAssistSettings(
          agent: 'claude',
          timeoutSeconds: 180,
          selectedModelByAgent: {'claude': 'haiku'},
          selectedThinkingByOperation: {
            AiAssistOperation.mergeConflicts: {'haiku': 'low'},
          },
        ),
        mergeConflict: AiAssistMergeConflictContext(
          filePath: 'total.js',
          base: base,
          ours: ours,
          theirs: theirs,
        ),
      );

      final response = await service.generate(request);
      final suggested = parseMergeConflictsSuggestion(response.text);
      expect(suggested.merged, contains('Array.isArray(items)'));
      expect(suggested.merged, contains('console.info'));
      expect(suggested.merged, contains('return total'));
      expect(suggested.explanation, isNotEmpty);

      expect(await file.readAsBytes(), before);
      expect(await _git(path, ['status', '--porcelain']), beforeStatus);
      expect(
        await _git(path, ['diff', '--name-only', '--diff-filter=U']),
        contains('total.js'),
      );
      print(
        'LIVE_LLM_E2E_PASS agent=' +
            response.agentLabel +
            ' merged_chars=' +
            suggested.merged.length.toString() +
            ' index_unchanged=true worktree_unchanged=true',
      );
    },
    skip: Platform.environment['ALERA_LLM_MERGE_E2E'] != '1',
    timeout: const Timeout(Duration(minutes: 4)),
  );
}

Future<String> _git(String directory, List<String> args) async {
  final result = await Process.run('git', [
    '-C',
    directory,
    ...args,
  ], runInShell: Platform.isWindows);
  if (result.exitCode != 0) {
    throw StateError('git command failed: ' + result.stderr.toString());
  }
  return (result.stdout as String).replaceAll('\r\n', '\n');
}

class _TestEnvironment implements CommandEnvironmentResolver {
  @override
  Future<Map<String, String>> environment() async =>
      Map<String, String>.of(Platform.environment);

  @override
  Future<Map<String, String>> environmentVariables(List<String> names) async =>
      <String, String>{
        for (final name in names)
          if (Platform.environment.containsKey(name))
            name: Platform.environment[name]!,
      };
}

class _IoProcessRunner implements ProcessRunner {
  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) async {
    final result = await Process.run(
      executable,
      arguments,
      workingDirectory: workingDirectory,
      environment: environment,
      runInShell: Platform.isWindows,
    );
    return ProcessRunOutput(
      exitCode: result.exitCode,
      stdout: result.stdout as String,
      stderr: result.stderr as String,
    );
  }

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) async {
    final process = await Process.start(
      executable,
      arguments,
      workingDirectory: workingDirectory,
      environment: environment,
      includeParentEnvironment: includeParentEnvironment,
      runInShell: Platform.isWindows,
    );
    return StartedProcess(
      stdinWrite: process.stdin.add,
      stdinClose: () {
        process.stdin.close();
      },
      stdout: process.stdout,
      stderr: process.stderr,
      pid: process.pid,
      exitCode: process.exitCode,
      kill: ([signal]) => process.kill(signal ?? ProcessSignal.sigterm),
    );
  }
}
