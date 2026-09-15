import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:alera/src/app/providers.dart'
    show WorkbenchController, workbenchControllerProvider;
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_agent_runner.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_errors.dart';
import 'package:alera/src/features/reading_diff/application/reading_diff_cache.dart';
import 'package:alera/src/features/reading_diff/application/reading_diff_generation_progress.dart';
import 'package:alera/src/features/reading_diff/application/reading_diff_providers.dart';
import 'package:alera/src/features/reading_diff/application/reading_diff_service.dart';
import 'package:alera/src/features/reading_diff/domain/reading_diff_models.dart';
import 'package:alera/src/rust/api/reading_diff.dart' as rust;
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart'
    show workspaceFileServiceProvider;
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import '../unit/fake_git_backend.dart';

part 'workspace_git_diff_surface_pull_request_cases.dart';
part 'workspace_git_diff_surface_open_path_cases.dart';
part 'workspace_git_diff_surface_reading_diff_cases.dart';
part 'workspace_git_diff_surface_reading_diff_support.dart';
part 'workspace_git_diff_surface_test_support.dart';

void main() {
  _registerWorkspaceGitDiffSurfacePullRequestTests();
  _registerWorkspaceGitDiffSurfaceReadingDiffTests();
  testWidgets('diff surface caps rendered line previews', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/large.dart',
            area: .unstaged,
            status: .modified,
            lines: List<GitDiffLine>.generate(
              5000,
              (index) => GitDiffLine.addition('+line $index'),
            ),
            added: 6005,
            removed: 0,
            linePreviewTruncated: true,
          ),
        ],
      );

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pumpAndSettle();

    expect(find.text('+line 0'), findsOneWidget);
    expect(find.text('+line 5000'), findsNothing);
    expect(find.text('+line 6000'), findsNothing);
    expect(find.text('+6005'), findsOneWidget);
    expect(find.text('-0'), findsNothing);

    await tester.dragUntilVisible(
      find.text('Diff line preview truncated.'),
      find.byType(ListView),
      const Offset(0, -1600),
      maxIteration: 80,
    );
    await tester.pumpAndSettle();

    expect(find.text('Diff line preview truncated.'), findsOneWidget);
    expect(find.text('+line 5000'), findsNothing);
    expect(find.text('+line 6000'), findsNothing);
  });

  testWidgets('diff preview renders before full-file hydration completes', (
    tester,
  ) async {
    final backend = _BlockingDiffBlobBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/first.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+first diff')],
          ),
          GitDiffFile(
            path: 'lib/second.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+second diff')],
          ),
        ],
      );
    final firstOldSideGate = Completer<void>();
    backend.gates[(filePath: 'lib/first.dart', oldSide: true)] =
        firstOldSideGate;

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pump();

    expect(find.text('+first diff'), findsOneWidget);
    expect(find.text('+second diff'), findsOneWidget);
    expect(
      backend.started,
      contains((filePath: 'lib/first.dart', oldSide: true)),
    );
    expect(
      backend.started.where((request) => request.filePath == 'lib/second.dart'),
      isEmpty,
    );

    firstOldSideGate.complete();
    await tester.pumpAndSettle();

    expect(
      backend.started.where((request) => request.filePath == 'lib/second.dart'),
      isNotEmpty,
    );
  });

  testWidgets('all diffs render the first file before later files load', (
    tester,
  ) async {
    final backend = _ProgressiveAllDiffBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/first.dart',
            area: .unstaged,
            status: .modified,
          ),
          GitChangeEntry(
            path: 'lib/second.dart',
            area: .unstaged,
            status: .modified,
          ),
        ],
      )
      ..diffByFile['lib/first.dart'] = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/first.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+first diff')],
          ),
        ],
      )
      ..diffByFile['lib/second.dart'] = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/second.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+second diff')],
          ),
        ],
      );
    final secondFileGate = Completer<void>();
    backend.gates['lib/second.dart'] = secondFileGate;

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        scope: .all,
        filePath: null,
        area: null,
        title: 'all changes',
      ),
    );
    await tester.pump();

    expect(find.text('+first diff'), findsOneWidget);
    expect(find.text('+second diff'), findsNothing);
    expect(backend.completedFilePaths, <String>['lib/first.dart']);
    expect(
      backend.calls
          .where((call) => call.method == 'diffAllPage')
          .single
          .args['filePaths'],
      <String>['lib/first.dart'],
    );

    secondFileGate.complete();
    await tester.pumpAndSettle();

    expect(find.text('+second diff'), findsOneWidget);
    expect(backend.completedFilePaths, <String>[
      'lib/first.dart',
      'lib/second.dart',
    ]);
    expect(
      backend.calls
          .where((call) => call.method == 'diffAllPage')
          .last
          .args['filePaths'],
      <String>['lib/second.dart'],
    );
  });

  testWidgets('full-file view includes unchanged lines outside diff hunks', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/large.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -3,2 +3,2 @@'),
              GitDiffLine.deletion('-old three'),
              GitDiffLine.addition('+new three'),
              GitDiffLine.context(' line four'),
            ],
            added: 1,
            removed: 1,
          ),
        ],
      )
      ..diffBlobBytesBySide[(
        filePath: 'lib/large.dart',
        oldSide: false,
      )] = Uint8List.fromList(
        'line one\nline two\nnew three\nline four\nline five\n'.codeUnits,
      );

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pumpAndSettle();

    expect(find.text('line one'), findsOneWidget);
    expect(find.text('line two'), findsOneWidget);
    expect(find.text('old three'), findsOneWidget);
    expect(find.text('new three'), findsOneWidget);
    expect(find.text('line four'), findsOneWidget);
    expect(find.text('line five'), findsOneWidget);
    expect(
      backend.calls
          .where(
            (call) =>
                call.method == 'diffBlobBytes' && call.args['oldSide'] == false,
          )
          .single
          .args,
      containsPair('oldSide', false),
    );
  });

  testWidgets('encoding switch re-decodes cached blobs for diff-only view', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/large.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -1 +1 @@'),
              GitDiffLine.deletion('-garbled old'),
              GitDiffLine.addition('+garbled new'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/large.dart', oldSide: true)] =
          Uint8List.fromList(<int>[1])
      ..diffBlobBytesBySide[(filePath: 'lib/large.dart', oldSide: false)] =
          Uint8List.fromList(<int>[2]);
    final fileService = _DiffEncodingFileService(
      decode: (bytes, encoding) {
        final oldSide = bytes.single == 1;
        final isBig5 = encoding == native.WorkspaceTextEncoding.big5;
        return native.WorkspaceDecodedText(
          content: isBig5
              ? (oldSide ? '舊內容\n' : '新內容\n')
              : (oldSide ? 'auto old\n' : 'auto new\n'),
          encoding: encoding ?? native.WorkspaceTextEncoding.utf8,
        );
      },
    );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: fileService,
    );
    await tester.pumpAndSettle();
    final blobReadsBeforeSwitch = backend.calls
        .where((call) => call.method == 'diffBlobBytes')
        .length;
    expect(find.text('Auto (UTF-8)'), findsOneWidget);

    await tester.tap(find.byTooltip('Switch to Diff Only'));
    await tester.pump();
    expect(find.text('-auto old'), findsOneWidget);
    expect(find.text('+auto new'), findsOneWidget);

    await tester.tap(find.byTooltip('Diff Encoding'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Big5').last);
    await tester.pumpAndSettle();

    expect(find.text('-舊內容'), findsOneWidget);
    expect(find.text('+新內容'), findsOneWidget);
    expect(find.text('Big5'), findsOneWidget);
    expect(
      backend.calls.where((call) => call.method == 'diffBlobBytes').length,
      blobReadsBeforeSwitch,
    );
    expect(fileService.decodeCalls.length, 4);
  });

  testWidgets('encoding switch preserves files hydrated during re-decode', (
    tester,
  ) async {
    final secondBlobGate = Completer<void>();
    final manualDecodeGate = Completer<void>();
    final manualDecodeStarted = Completer<void>();
    final backend = _BlockingDiffBlobBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/first.dart',
            area: .unstaged,
            status: .untracked,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -0,0 +1 @@'),
              GitDiffLine.addition('+garbled first'),
            ],
          ),
          GitDiffFile(
            path: 'lib/second.dart',
            area: .unstaged,
            status: .untracked,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -0,0 +1 @@'),
              GitDiffLine.addition('+garbled second'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/first.dart', oldSide: false)] =
          Uint8List.fromList(<int>[1])
      ..diffBlobBytesBySide[(filePath: 'lib/second.dart', oldSide: false)] =
          Uint8List.fromList(<int>[2])
      ..gates[(filePath: 'lib/second.dart', oldSide: false)] = secondBlobGate;
    final fileService = _DiffEncodingFileService(
      decode: (bytes, encoding) async {
        final isBig5 = encoding == native.WorkspaceTextEncoding.big5;
        if (isBig5 && bytes.single == 1) {
          if (!manualDecodeStarted.isCompleted) manualDecodeStarted.complete();
          await manualDecodeGate.future;
        }
        return native.WorkspaceDecodedText(
          content: isBig5
              ? (bytes.single == 1 ? '第一個\n' : '第二個\n')
              : (bytes.single == 1 ? 'auto first\n' : 'auto second\n'),
          encoding: encoding ?? native.WorkspaceTextEncoding.utf8,
        );
      },
    );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: fileService,
    );
    for (var i = 0; i < 10; i += 1) {
      await tester.pump();
      if (backend.started.contains((
        filePath: 'lib/second.dart',
        oldSide: false,
      ))) {
        break;
      }
    }
    expect(
      backend.started,
      contains((filePath: 'lib/second.dart', oldSide: false)),
    );

    await tester.tap(find.byTooltip('Diff Encoding'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Big5').last);
    for (var i = 0; i < 10 && !manualDecodeStarted.isCompleted; i += 1) {
      await tester.pump();
    }
    expect(manualDecodeStarted.isCompleted, isTrue);

    secondBlobGate.complete();
    for (var i = 0; i < 10; i += 1) {
      await tester.pump();
      if (fileService.decodeCalls.any(
        (call) =>
            call.bytes.single == 2 &&
            call.encoding == native.WorkspaceTextEncoding.big5,
      )) {
        break;
      }
    }
    manualDecodeGate.complete();
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Switch to Diff Only'));
    await tester.pump();
    expect(find.text('+第一個'), findsOneWidget);
    expect(find.text('+第二個'), findsOneWidget);
    expect(find.text('+garbled second'), findsNothing);
  });

  _registerWorkspaceGitDiffSurfaceOpenPathTests();

  testWidgets('diff surface hides zero-valued header stats', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/added.dart',
            area: .untracked,
            status: .untracked,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 3,
            removed: 0,
          ),
          GitDiffFile(
            path: 'lib/deleted.dart',
            area: .staged,
            status: .deleted,
            lines: <GitDiffLine>[GitDiffLine.deletion('-old')],
            added: 0,
            removed: 2,
          ),
        ],
      );

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pumpAndSettle();

    expect(find.text('+3'), findsOneWidget);
    expect(find.text('-2'), findsOneWidget);
    expect(find.text('+0'), findsNothing);
    expect(find.text('-0'), findsNothing);
  });

  testWidgets('diff surface loads commit diffs from commit payload', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitCommitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .staged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            sourceLabel: 'Commit',
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        source: .commit,
        filePath: 'packages/app/lib/main.dart',
        title: 'main.dart abc1234',
        scope: .file,
        area: null,
        gitDiffRoot: 'packages/app',
        commitOid: 'abc123456789',
        parentOid: 'def987654321',
        compareRef: 'abc1234',
      ),
    );
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'commitDiff').single.args,
      <String, Object?>{
        'path': p.join('/tmp/project', 'packages', 'app'),
        'commitOid': 'abc123456789',
        'parentOid': 'def987654321',
        'filePath': 'lib/main.dart',
        'oldPath': null,
      },
    );
    expect(find.text('Commit · lib/main.dart'), findsOneWidget);
    expect(_openFileButton(tester).onPressed, isNull);
  });

  testWidgets('diff content scope and layout toggle independently', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -3,2 +3,2 @@'),
              GitDiffLine.deletion('-old three'),
              GitDiffLine.addition('+new three'),
              GitDiffLine.context(' line four'),
            ],
            added: 1,
            removed: 1,
          ),
        ],
      )
      ..diffBlobBytesBySide[(
        filePath: 'lib/main.dart',
        oldSide: true,
      )] = Uint8List.fromList(
        'line one\nline two\nold three\nline four\nline five\n'.codeUnits,
      )
      ..diffBlobBytesBySide[(
        filePath: 'lib/main.dart',
        oldSide: false,
      )] = Uint8List.fromList(
        'line one\nline two\nnew three\nline four\nline five\n'.codeUnits,
      );

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pumpAndSettle();

    // Full file + single column (defaults).
    expect(find.byTooltip('Switch to Diff Only'), findsOneWidget);
    expect(find.byTooltip('Switch to Side-by-Side View'), findsOneWidget);
    expect(find.text('line one'), findsOneWidget);
    expect(find.text('line five'), findsOneWidget);
    expect(find.text('old three'), findsOneWidget);
    expect(find.text('new three'), findsOneWidget);
    expect(find.text('Original'), findsNothing);

    // Diff only + single column.
    await tester.tap(find.byTooltip('Switch to Diff Only'));
    await tester.pump();
    expect(find.byTooltip('Switch to Full File View'), findsOneWidget);
    expect(find.byTooltip('Switch to Side-by-Side View'), findsOneWidget);
    expect(find.text('line one'), findsNothing);
    expect(find.text('line five'), findsNothing);
    expect(find.text('-old three'), findsOneWidget);
    expect(find.text('+new three'), findsOneWidget);
    expect(find.text('Original'), findsNothing);

    // Diff only + side-by-side.
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pump();
    expect(find.byTooltip('Switch to Full File View'), findsOneWidget);
    expect(find.byTooltip('Switch to Single-Column View'), findsOneWidget);
    expect(find.text('Original'), findsOneWidget);
    expect(find.text('Modified'), findsOneWidget);
    expect(find.text('line one'), findsNothing);
    expect(find.text('line five'), findsNothing);
    expect(find.text('old three'), findsOneWidget);
    expect(find.text('new three'), findsOneWidget);

    // Full file + side-by-side.
    await tester.tap(find.byTooltip('Switch to Full File View'));
    await tester.pump();
    expect(find.byTooltip('Switch to Diff Only'), findsOneWidget);
    expect(find.byTooltip('Switch to Single-Column View'), findsOneWidget);
    expect(find.text('Original'), findsOneWidget);
    expect(find.text('Modified'), findsOneWidget);
    expect(find.text('line one'), findsNWidgets(2));
    expect(find.text('line five'), findsNWidgets(2));
    expect(find.text('old three'), findsOneWidget);
    expect(find.text('new three'), findsOneWidget);

    // Full file + single column again.
    await tester.tap(find.byTooltip('Switch to Single-Column View'));
    await tester.pump();
    expect(find.byTooltip('Switch to Diff Only'), findsOneWidget);
    expect(find.byTooltip('Switch to Side-by-Side View'), findsOneWidget);
    expect(find.text('Original'), findsNothing);
    expect(find.text('line one'), findsOneWidget);
    expect(find.text('line five'), findsOneWidget);
  });
}
