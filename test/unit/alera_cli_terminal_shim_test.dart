import 'dart:io';

import 'package:alera/src/features/workbench/infra/alera_cli_terminal_shim.dart';
import 'package:alera/src/shared/infra/runtime/alera_cli_sidecar.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory root;
  late Directory support;
  late _FakeCliResolver resolver;
  late AleraCliTerminalShimService service;

  setUp(() async {
    root = await Directory.systemTemp.createTemp('alera-terminal-shim-');
    support = Directory(p.join(root.path, 'support'))
      ..createSync(recursive: true);
    resolver = _FakeCliResolver(
      const AleraCliCommand(executable: r'C:\tools\alera.exe'),
    );
    service = AleraCliTerminalShimService(
      cliResolver: resolver,
      applicationSupportDirectory: () async => support,
      operatingSystem: 'windows',
    );
  });

  tearDown(() async {
    if (await root.exists()) await root.delete(recursive: true);
  });

  test('unchanged Windows terminal shims are not rewritten', () async {
    await service.prepareForTerminalLaunch();
    final shimDirectory = p.join(support.path, 'terminal_tools', 'bin');
    final cmd = File(p.join(shimDirectory, 'alera.cmd'));
    final bat = File(p.join(shimDirectory, 'alera.bat'));
    final expected = await cmd.readAsString();
    expect(await bat.readAsString(), expected);

    // An unchanged second Tab should not replace either launcher, which can
    // otherwise trigger antivirus and filesystem watchers on every startup.
    final pinnedTime = DateTime.utc(2024, 1, 2, 3, 4, 5);
    await cmd.setLastModified(pinnedTime);
    await bat.setLastModified(pinnedTime);
    await service.prepareForTerminalLaunch();

    expect(await cmd.readAsString(), expected);
    expect(await bat.readAsString(), expected);
    expect((await cmd.lastModified()).isAtSameMomentAs(pinnedTime), isTrue);
    expect((await bat.lastModified()).isAtSameMomentAs(pinnedTime), isTrue);
  });

  test('Windows shim refreshes when the resolved executable changes', () async {
    await service.prepareForTerminalLaunch();
    resolver.command = const AleraCliCommand(
      executable: r'C:\tools\alera-new.exe',
    );
    await service.prepareForTerminalLaunch();
    final dir = p.join(support.path, 'terminal_tools', 'bin');
    for (final filename in <String>['alera.cmd', 'alera.bat']) {
      expect(
        await File(p.join(dir, filename)).readAsString(),
        contains(r'C:\tools\alera-new.exe'),
      );
    }
  });
}

class _FakeCliResolver implements AleraCliResolver {
  _FakeCliResolver(this.command);
  AleraCliCommand command;

  @override
  Future<AleraCliCommand> resolve({required String runtimeDir}) async =>
      command;
}
