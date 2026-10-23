import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'tab_lease_registry.dart';

/// A smoke-test *piped cmd.exe* host, not Alera's real ConPTY/PTY implementation.
/// Lives outside all Flutter Views to verify an OS process and its output
/// survive repeated detach/dock without being restarted.
final class PersistentWindowsShellHost {
  PersistentWindowsShellHost(this.session);

  final DemoTabSession session;
  Process? _process;
  StreamSubscription<String>? _stdoutSub;
  StreamSubscription<String>? _stderrSub;
  final Map<String, Completer<void>> _markers = <String, Completer<void>>{};
  final StringBuffer _output = StringBuffer();
  bool _closing = false;

  int? get pid => _process?.pid;
  String get output => _output.toString();

  Future<void> start() async {
    if (_process != null) throw StateError('Terminal host already running');
    if (!Platform.isWindows) throw UnsupportedError('Windows cmd.exe required');
    final proc = await Process.start('cmd.exe', <String>[
      '/D',
      '/Q',
    ], mode: ProcessStartMode.normal);
    _process = proc;
    session.terminalHostPid = proc.pid;
    session.terminalHostStartCount++;
    _stdoutSub = proc.stdout
        .transform(const Utf8Decoder(allowMalformed: true))
        .listen(_onOutput);
    _stderrSub = proc.stderr
        .transform(const Utf8Decoder(allowMalformed: true))
        .listen(_onOutput);
    unawaited(
      proc.exitCode.then((code) {
        if (!_closing) _onOutput('cmd.exe exited unexpectedly: $code');
      }),
    );
    await sendEcho('ALERA_SHELL_STARTED');
  }

  void _onOutput(String chunk) {
    _output.write(chunk);
    session.appendTerminalOutput(chunk);
    for (final entry in _markers.entries.toList(growable: false)) {
      if (_output.toString().contains(entry.key) && !entry.value.isCompleted) {
        entry.value.complete();
        _markers.remove(entry.key);
      }
    }
  }

  Future<void> sendEcho(String marker) async {
    if (_closing || _process == null) throw StateError('Terminal unavailable');
    if (!RegExp(r'^[A-Z0-9_]+$').hasMatch(marker)) {
      throw ArgumentError.value(
        marker,
        'marker',
        'Only safe echo tokens allowed',
      );
    }
    if (_output.toString().contains(marker)) {
      throw StateError('Marker already used: $marker');
    }
    final done = Completer<void>();
    _markers[marker] = done;
    _process!.stdin.writeln('echo $marker');
    await _process!.stdin.flush();
    await done.future.timeout(const Duration(seconds: 5));
  }

  Future<void> stop() async {
    if (_closing) return;
    _closing = true;
    final proc = _process;
    if (proc == null) return;
    try {
      proc.stdin.writeln('exit');
      await proc.stdin.flush();
      await proc.exitCode.timeout(const Duration(seconds: 2));
    } on Object {
      proc.kill();
      try {
        await proc.exitCode.timeout(const Duration(seconds: 2));
      } on Object {
        // Test environment can deny process exit, but no child should remain.
      }
    } finally {
      await _stdoutSub?.cancel();
      await _stderrSub?.cancel();
      _process = null;
      _markers.clear();
    }
  }
}
