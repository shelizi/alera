import 'dart:convert';
import 'dart:io';

import 'package:code_forge/code_forge/rope.dart' as code_forge;
import 'package:code_forge/src/rust/api/native_save_io.dart' as native_save;
import 'package:code_forge/src/rust/api/workspace_source.dart' as source;
import 'package:code_forge/src/rust/frb_generated.dart' as code_forge_rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';

/// Opt-in Windows FFI smoke test, using a DLL built from the current worktree:
/// cargo build --manifest-path third_party/code_forge/rust/Cargo.toml
/// ALERA_CODE_FORGE_TEST_DLL=D:\\c\\t\\debug\\code_forge.dll flutter test ...
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final dllPath = Platform.environment['ALERA_CODE_FORGE_TEST_DLL'];
  final canRun = Platform.isWindows && dllPath != null && dllPath.isNotEmpty;

  test(
    'native Rope Save preserves unchanged source and handles conflicts',
    () async {
      await code_forge_rust.RustLib.init(
        externalLibrary: ExternalLibrary.open(dllPath!),
      );

      final directory = Directory.systemTemp.createTempSync('alera-rope-ffi-');
      try {
        final file = File('${directory.path}\\document.txt');
        file.writeAsBytesSync(
          utf8.encode('\talpha\r\n\tbeta\r\n'),
          flush: true,
        );
        final loaded = await code_forge.Rope.openWorkspaceFile(
          workspacePath: directory.path,
          relativePath: 'document.txt',
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        final originalToken = loaded.sourceInfo!.contentToken;

        // The native Rope retains the original tab-expanded displayed text,
        // then inserts a new line; unchanged edges must keep original bytes.
        final edited = loaded.insertImmutable(loaded.length, 'new line');
        final saved = await edited.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'document.txt',
          expectedContentToken: originalToken,
          overwriteIfChanged: false,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(saved.status, native_save.SaveStatus.saved);
        expect(file.readAsStringSync(), '\talpha\r\n\tbeta\r\nnew line');
        expect(saved.size.toInt(), file.lengthSync());

        file.writeAsStringSync('external update is longer', flush: true);
        final conflicted = await edited.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'document.txt',
          expectedContentToken: saved.token,
          overwriteIfChanged: false,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(conflicted.status, native_save.SaveStatus.conflict);
        expect(file.readAsStringSync(), 'external update is longer');

        final overwritten = await edited.saveWorkspaceSnapshot(
          workspacePath: directory.path,
          relativePath: 'document.txt',
          expectedContentToken: saved.token,
          overwriteIfChanged: true,
          tabSize: 4,
          encoding: source.NativeWorkspaceTextEncoding.utf8,
        );
        expect(overwritten.status, native_save.SaveStatus.saved);
        expect(file.readAsStringSync(), '    alpha\r\n    beta\r\nnew line');

        // Verify explicit encoding survives the real Flutter -> CodeForge DLL
        // boundary. Neither opening nor saving may run charset detection when
        // the user selected a specific codec.
        final encodingCases =
            <
              ({
                String name,
                source.NativeWorkspaceTextEncoding encoding,
                List<int> original,
                List<int> expected,
              })
            >[
              (
                name: 'utf16-le',
                encoding: source.NativeWorkspaceTextEncoding.utf16Le,
                original: _utf16Bytes('\t中文\r\n\told\r\n', littleEndian: true),
                expected: _utf16Bytes(
                  '\t中文\r\n\told\r\nadded',
                  littleEndian: true,
                ),
              ),
              (
                name: 'utf16-be',
                encoding: source.NativeWorkspaceTextEncoding.utf16Be,
                original: _utf16Bytes('\t中文\r\n\told\r\n', littleEndian: false),
                expected: _utf16Bytes(
                  '\t中文\r\n\told\r\nadded',
                  littleEndian: false,
                ),
              ),
              (
                name: 'big5',
                encoding: source.NativeWorkspaceTextEncoding.big5,
                // Big5 for 中 is A4 A4.
                original: <int>[
                  9,
                  0xa4,
                  0xa4,
                  13,
                  10,
                  9,
                  111,
                  108,
                  100,
                  13,
                  10,
                ],
                expected: <int>[
                  9,
                  0xa4,
                  0xa4,
                  13,
                  10,
                  9,
                  111,
                  108,
                  100,
                  13,
                  10,
                  97,
                  100,
                  100,
                  101,
                  100,
                ],
              ),
              (
                name: 'windows1252',
                encoding: source.NativeWorkspaceTextEncoding.windows1252,
                original: latin1.encode('\tcafé\r\n\told\r\n'),
                expected: latin1.encode('\tcafé\r\n\told\r\nadded'),
              ),
            ];
        for (final caseData in encodingCases) {
          file.writeAsBytesSync(caseData.original, flush: true);
          final reopened = await code_forge.Rope.openWorkspaceFile(
            workspacePath: directory.path,
            relativePath: 'document.txt',
            tabSize: 4,
            encoding: caseData.encoding,
          );
          expect(
            reopened.sourceInfo!.encoding,
            caseData.encoding,
            reason: caseData.name,
          );
          final noOp = await reopened.saveWorkspaceSnapshot(
            workspacePath: directory.path,
            relativePath: 'document.txt',
            expectedContentToken: reopened.sourceInfo!.contentToken,
            overwriteIfChanged: false,
            tabSize: 4,
            encoding: caseData.encoding,
          );
          expect(
            noOp.status,
            native_save.SaveStatus.saved,
            reason: caseData.name,
          );
          expect(
            file.readAsBytesSync(),
            caseData.original,
            reason: caseData.name,
          );

          final withAppend = reopened.insertImmutable(reopened.length, 'added');
          final updated = await withAppend.saveWorkspaceSnapshot(
            workspacePath: directory.path,
            relativePath: 'document.txt',
            expectedContentToken: noOp.token,
            overwriteIfChanged: false,
            tabSize: 4,
            encoding: caseData.encoding,
          );
          expect(
            updated.status,
            native_save.SaveStatus.saved,
            reason: caseData.name,
          );
          expect(
            file.readAsBytesSync(),
            caseData.expected,
            reason: caseData.name,
          );
        }
      } finally {
        directory.deleteSync(recursive: true);
      }
    },
    skip: canRun ? false : 'requires Windows and ALERA_CODE_FORGE_TEST_DLL',
  );
}

List<int> _utf16Bytes(String text, {required bool littleEndian}) {
  final bytes = <int>[
    if (littleEndian) 0xff else 0xfe,
    if (littleEndian) 0xfe else 0xff,
  ];
  for (final unit in text.codeUnits) {
    if (littleEndian) {
      bytes
        ..add(unit & 0xff)
        ..add(unit >> 8);
    } else {
      bytes
        ..add(unit >> 8)
        ..add(unit & 0xff);
    }
  }
  return bytes;
}
