import 'dart:convert';
import 'dart:io';

import 'package:code_forge/code_forge/rope.dart' as code_forge;
import 'package:code_forge/src/rust/api/native_save_io.dart' as save;
import 'package:code_forge/src/rust/api/workspace_source.dart' as source;
import 'package:code_forge/src/rust/frb_generated.dart' as rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// Opt-in Windows SMB test. Root must be a separately authorized test share.
/// e.g. ALERA_NATIVE_SMB_TEST_ROOT=\\localhost\D$\some\isolated\test-folder
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final dll = Platform.environment['ALERA_CODE_FORGE_TEST_DLL'];
  final root = Platform.environment['ALERA_NATIVE_SMB_TEST_ROOT'];
  final enabled =
      Platform.isWindows &&
      dll != null &&
      dll.isNotEmpty &&
      root != null &&
      root.startsWith(r'\\');

  test(
    'Windows native Rope Save keeps SMB bytes and conflict semantics',
    () async {
      await rust.RustLib.init(externalLibrary: ExternalLibrary.open(dll!));
      final share = Directory(root!);
      expect(share.existsSync(), isTrue);
      final directory = share.createTempSync('alera-unc-rope-');
      try {
        final file = File(p.join(directory.path, 'sample.txt'));
        file.writeAsBytesSync(
          utf8.encode('\talpha\r\n\tbeta\r\n'),
          flush: true,
        );
        final loaded = await code_forge.Rope.openWorkspaceFile(
          workspacePath: directory.path,
          relativePath: 'sample.txt',
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        final updated = loaded.insertImmutable(loaded.length, 'added');
        final saved = await updated.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'sample.txt',
          expectedContentToken: loaded.sourceInfo!.contentToken,
          overwriteIfChanged: false,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(saved.status, save.SaveStatus.saved);
        expect(file.readAsStringSync(), '\talpha\r\n\tbeta\r\nadded');

        file.writeAsStringSync('external SMB update longer', flush: true);
        final conflicted = await updated.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'sample.txt',
          expectedContentToken: saved.token,
          overwriteIfChanged: false,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(conflicted.status, save.SaveStatus.conflict);
        expect(file.readAsStringSync(), 'external SMB update longer');

        final overwritten = await updated.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'sample.txt',
          expectedContentToken: saved.token,
          overwriteIfChanged: true,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(overwritten.status, save.SaveStatus.saved);
        expect(file.readAsStringSync(), '    alpha\r\n    beta\r\nadded');
      } finally {
        directory.deleteSync(recursive: true);
      }
    },
    skip: enabled ? false : 'requires Windows Release DLL and test UNC root',
  );
}
