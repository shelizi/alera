import 'dart:async';

import 'package:alera/src/features/language_intelligence/infra/process_runner_lsp_process.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('starts language servers through the process runner', () async {
    final runner = _RecordingProcessRunner();
    final starter = processRunnerLspProcessStarter(runner);

    final process = await starter(
      r'C:\servers\typescript-language-server.cmd',
      const <String>['--stdio'],
      workingDirectory: r'E:\work\app',
      environment: const <String, String>{'NODE_OPTIONS': '--max-old-space'},
    );

    expect(runner.executable, r'C:\servers\typescript-language-server.cmd');
    expect(runner.arguments, <String>['--stdio']);
    expect(runner.workingDirectory, r'E:\work\app');
    expect(runner.environment, <String, String>{
      'NODE_OPTIONS': '--max-old-space',
    });
    expect(process.pid, 4242);
  });

  test('forwards stdio and ends the server through the runner kill', () async {
    final runner = _RecordingProcessRunner();
    final process = await processRunnerLspProcessStarter(runner)(
      'gopls',
      const <String>[],
    );

    process.writeStdin(const <int>[1, 2, 3]);
    await process.flushStdin();
    final stdout = process.stdout.first;
    runner.stdout.add(const <int>[7]);

    expect(runner.stdinWrites, <List<int>>[
      <int>[1, 2, 3],
    ]);
    expect(await stdout, <int>[7]);
    expect(process.kill(), isTrue);
    expect(runner.killCount, 1);

    runner.exitCode.complete(0);
    expect(await process.exitCode, 0);
  });
}

final class _RecordingProcessRunner implements ProcessRunner {
  final StreamController<List<int>> stdout = StreamController<List<int>>();
  final Completer<int> exitCode = Completer<int>();
  final List<List<int>> stdinWrites = <List<int>>[];
  String? executable;
  List<String>? arguments;
  String? workingDirectory;
  Map<String, String>? environment;
  int killCount = 0;

  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) => throw UnimplementedError();

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) async {
    this.executable = executable;
    this.arguments = arguments;
    this.workingDirectory = workingDirectory;
    this.environment = environment;
    return StartedProcess(
      stdinWrite: stdinWrites.add,
      stdout: stdout.stream,
      stderr: const Stream<List<int>>.empty(),
      pid: 4242,
      exitCode: exitCode.future,
      kill: ([signal]) {
        killCount += 1;
        return true;
      },
    );
  }
}
