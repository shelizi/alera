import 'package:code_forge/code_forge.dart';

import '../../../shared/infra/process/process_runner.dart';

/// Starts language servers through [ProcessRunner] instead of `dart:io`.
///
/// The runner ends the whole invocation on kill (`taskkill /T /F` on Windows, a
/// process-group `SIGKILL` elsewhere). `Process.kill` reaches only the direct
/// child, which for a `.cmd` server is `cmd.exe`, so disposing a server that
/// ignored `exit` left its `node` processes running.
LspProcessStarter processRunnerLspProcessStarter(ProcessRunner runner) =>
    (executable, arguments, {workingDirectory, environment}) async =>
        ProcessRunnerLspProcess(
          await runner.start(
            executable,
            arguments,
            workingDirectory: workingDirectory,
            environment: environment,
          ),
        );

final class ProcessRunnerLspProcess implements LspProcess {
  ProcessRunnerLspProcess(this._process);

  final StartedProcess _process;

  @override
  int get pid => _process.pid;

  @override
  Stream<List<int>> get stdout => _process.stdout;

  @override
  Stream<List<int>> get stderr => _process.stderr;

  @override
  Future<int> get exitCode => _process.exitCode;

  @override
  void writeStdin(List<int> data) => _process.stdinWrite(data);

  // The runner queues writes in order and exposes no flush acknowledgement.
  @override
  Future<void> flushStdin() async {}

  @override
  bool kill() => _process.kill();
}
