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
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/foundation.dart'
    show debugDefaultTargetPlatformOverride;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';
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
        'whitespaceMode': GitDiffWhitespaceMode.normal,
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
            sideBySideRows: <GitDiffSideBySideRow>[
              GitDiffSideBySideRow(kind: .passthrough, lineIndex: 0),
              GitDiffSideBySideRow(
                kind: .pair,
                leftLineIndex: 1,
                leftLineNumber: 33,
                rightLineIndex: 2,
                rightLineNumber: 33,
              ),
              GitDiffSideBySideRow(
                kind: .pair,
                leftLineIndex: 3,
                leftLineNumber: 34,
                rightLineIndex: 3,
                rightLineNumber: 34,
              ),
            ],
            fullFileRows: <GitDiffFullFileRow>[
              // Intentionally omit full-file line index 1 so the default
              // single-column view proves it consumes the native plan.
              GitDiffFullFileRow(
                kind: .contextRange,
                startIndex: 0,
                endIndex: 1,
              ),
              GitDiffFullFileRow(kind: .line, diffLineIndex: 1, lineNumber: 3),
              GitDiffFullFileRow(
                kind: .line,
                fullLineIndex: 2,
                diffLineIndex: 2,
                lineNumber: 3,
              ),
              GitDiffFullFileRow(
                kind: .line,
                fullLineIndex: 3,
                diffLineIndex: 3,
                lineNumber: 4,
              ),
              GitDiffFullFileRow(kind: .contextRange, startIndex: 4),
            ],
            fullFileSideBySideRows: <GitDiffFullFileSideBySideRow>[
              // Intentionally omit full-file line index 1. The full-file
              // side-by-side assertion below proves this native plan is used
              // instead of rebuilding alignment from hunks on the UI isolate.
              GitDiffFullFileSideBySideRow(
                kind: .contextRange,
                oldStartIndex: 0,
                oldEndIndex: 1,
                newStartIndex: 0,
                newEndIndex: 1,
              ),
              GitDiffFullFileSideBySideRow(
                kind: .pair,
                oldStartIndex: 2,
                newStartIndex: 2,
                leftDiffLineIndex: 1,
                rightDiffLineIndex: 2,
              ),
              GitDiffFullFileSideBySideRow(
                kind: .pair,
                oldStartIndex: 3,
                newStartIndex: 3,
                leftDiffLineIndex: 3,
                rightDiffLineIndex: 3,
              ),
              GitDiffFullFileSideBySideRow(
                kind: .contextRange,
                oldStartIndex: 4,
                newStartIndex: 4,
              ),
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
    expect(find.text('line two'), findsNothing);
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
    // Non-default line numbers prove the native/index projection was consumed
    // instead of rebuilding side-by-side alignment on the UI isolate.
    expect(find.text('33'), findsNWidgets(2));

    await tester.tap(find.byTooltip('Switch to Full File View'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Switch to Diff Only'), findsOneWidget);
    expect(find.byTooltip('Switch to Single-Column View'), findsOneWidget);
    expect(find.text('Original'), findsOneWidget);
    expect(find.text('Modified'), findsOneWidget);
    expect(find.text('line one'), findsNWidgets(2));
    expect(find.text('line five'), findsNWidgets(2));
    expect(find.text('line two'), findsNothing);
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
    expect(find.text('line two'), findsNothing);
  });

  testWidgets('oversized tracked full-file preview falls back to diff only', (
    tester,
  ) async {
    final oversized = Uint8List(2 * 1024 * 1024 + 1);
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/huge.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -1 +1 @@'),
              GitDiffLine.deletion('-old value'),
              GitDiffLine.addition('+new value'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/huge.dart', oldSide: true)] =
          oversized
      ..diffBlobBytesBySide[(filePath: 'lib/huge.dart', oldSide: false)] =
          oversized;
    final fileService = _DiffEncodingFileService();

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: fileService,
      tab: _diffTab(filePath: 'lib/huge.dart', title: 'huge.dart unstaged'),
    );
    await tester.pumpAndSettle();

    expect(
      find.text(
        'Full file preview is limited for large files. Showing diff only.',
      ),
      findsOneWidget,
    );
    expect(find.text('-old value'), findsOneWidget);
    expect(find.text('+new value'), findsOneWidget);
    expect(fileService.decodeCalls, isEmpty);
    expect(
      find.byKey(
        const ValueKey<String>('git-diff-working-tree-editor-lib/huge.dart'),
      ),
      findsNothing,
    );
  });

  testWidgets('whitespace comparison selection is sticky and reloads diff', (
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
              GitDiffLine.deletion('-old'),
              GitDiffLine.addition('+new'),
            ],
          ),
        ],
      );
    final controller = _GitDiffSurfaceTestController();

    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
    );
    await tester.pumpAndSettle();

    expect(find.text('Normal'), findsOneWidget);
    await tester.tap(find.byTooltip('Whitespace Comparison'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Ignore Whitespace Changes').last);
    await tester.pumpAndSettle();

    expect(
      controller.state.viewPrefs.gitDiffWhitespaceMode,
      GitDiffWhitespaceMode.ignoreChanges.name,
    );
    expect(
      backend.calls
          .where((call) => call.method == 'diff')
          .last
          .args['whitespaceMode'],
      GitDiffWhitespaceMode.ignoreChanges,
    );
    expect(find.text('Ignore Whitespace Changes'), findsOneWidget);
  });

  testWidgets('new diff surface reuses persisted whitespace comparison mode', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
          ),
        ],
      );
    final controller = _GitDiffSurfaceTestController(
      initialViewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        gitDiffWhitespaceMode: GitDiffWhitespaceMode.ignoreAll.name,
      ),
    );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
    );
    await tester.pumpAndSettle();

    expect(find.text('Ignore All Whitespace'), findsOneWidget);
    expect(
      backend.calls
          .where((call) => call.method == 'diff')
          .single
          .args['whitespaceMode'],
      GitDiffWhitespaceMode.ignoreAll,
    );

    controller.setGitDiffWhitespaceMode(GitDiffWhitespaceMode.ignoreEol.name);
    await tester.pumpAndSettle();

    expect(find.text('Ignore End-of-Line Whitespace'), findsOneWidget);
    expect(
      backend.calls
          .where((call) => call.method == 'diff')
          .last
          .args['whitespaceMode'],
      GitDiffWhitespaceMode.ignoreEol,
    );
  });

  testWidgets('working-tree side-by-side right pane edits and saves file', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.windows;
    addTearDown(() => debugDefaultTargetPlatformOverride = null);
    String? clipboardText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          clipboardText =
              (call.arguments as Map<Object?, Object?>)['text'] as String?;
        }
        if (call.method == 'Clipboard.getData') {
          return <String, Object?>{'text': clipboardText};
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
              GitDiffLine.deletion('-old line'),
              GitDiffLine.addition('+new line'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
          Uint8List.fromList('old line\n'.codeUnits)
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
          Uint8List.fromList('new line\n'.codeUnits);
    final files = _EditableDiffFileService(content: 'new line\n');

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: files,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();

    final editor = find.byKey(
      const ValueKey<String>('git-diff-working-tree-editor-lib/main.dart'),
    );
    final original = find.byKey(
      const ValueKey<String>('git-diff-working-tree-original-lib/main.dart'),
    );
    expect(editor, findsOneWidget);
    expect(original, findsOneWidget);
    expect(tester.widget<TextField>(original).readOnly, isTrue);
    expect(find.text('Workspace · Editable'), findsOneWidget);
    expect(tester.getSize(editor).height, greaterThan(400));
    expect(
      find.byKey(
        const ValueKey<String>(
          'git-diff-working-tree-original-deletion-lib/main.dart-0',
        ),
      ),
      findsOneWidget,
    );
    final rightHighlight = find.byKey(
      const ValueKey<String>(
        'git-diff-working-tree-editor-addition-lib/main.dart-0',
      ),
    );
    expect(rightHighlight, findsOneWidget);
    final rightHighlightStack = tester.widget<Stack>(
      find.ancestor(of: rightHighlight, matching: find.byType(Stack)).first,
    );
    expect(
      rightHighlightStack.children.last.key,
      const ValueKey<String>(
        'git-diff-working-tree-editor-addition-lib/main.dart-0',
      ),
    );

    final originalController = tester.widget<TextField>(original).controller!;
    final originalEditable = tester.widget<EditableText>(
      find.descendant(of: original, matching: find.byType(EditableText)),
    );
    originalEditable.focusNode.requestFocus();
    originalController.selection = const TextSelection(
      baseOffset: 0,
      extentOffset: 8,
    );
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyC);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyC);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(clipboardText, 'old line');

    await tester.enterText(editor, 'edited line\n');
    await tester.pump();
    expect(find.text('+1'), findsWidgets);
    expect(find.text('-1'), findsWidgets);

    await tester.tap(find.byTooltip('Save File'));
    await tester.pumpAndSettle();

    expect(files.writes, hasLength(1));
    expect(files.writes.single.currentDisplayContent, 'edited line\n');
    expect(files.writes.single.expectedContentToken, 'token-1');
    expect(files.writes.single.overwriteIfChanged, isFalse);
    expect(files.writes.single.encoding, native.WorkspaceTextEncoding.utf8);
  });

  testWidgets('editable side-by-side shows x scrollbars and syncs x/y scroll', (
    tester,
  ) async {
    final longLine = List<String>.filled(180, 'x').join();
    final oldContent = List<String>.generate(
      80,
      (index) => 'old $index $longLine',
    ).join('\n');
    final newContent = List<String>.generate(
      80,
      (index) => 'new $index $longLine',
    ).join('\n');
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
              GitDiffLine.deletion('-old line'),
              GitDiffLine.addition('+new line'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
          Uint8List.fromList('$oldContent\n'.codeUnits)
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
          Uint8List.fromList('$newContent\n'.codeUnits);
    final files = _EditableDiffFileService(content: '$newContent\n');

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: files,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();

    final leftXFinder = find.byKey(
      const ValueKey<String>(
        'git-diff-working-tree-original-x-scrollbar-lib/main.dart',
      ),
    );
    final rightXFinder = find.byKey(
      const ValueKey<String>(
        'git-diff-working-tree-editor-x-scrollbar-lib/main.dart',
      ),
    );
    final leftYFinder = find.byKey(
      const ValueKey<String>(
        'git-diff-working-tree-original-y-scrollbar-lib/main.dart',
      ),
    );
    final rightYFinder = find.byKey(
      const ValueKey<String>(
        'git-diff-working-tree-editor-y-scrollbar-lib/main.dart',
      ),
    );
    final overviewFinder = find.byKey(
      const ValueKey<String>('git-diff-working-tree-overview-lib/main.dart'),
    );

    expect(leftXFinder, findsOneWidget);
    expect(rightXFinder, findsOneWidget);
    expect(leftYFinder, findsOneWidget);
    expect(rightYFinder, findsOneWidget);
    expect(overviewFinder, findsOneWidget);
    expect(find.byTooltip('Diff Overview'), findsOneWidget);

    final leftX = tester.widget<Scrollbar>(leftXFinder).controller!;
    final rightX = tester.widget<Scrollbar>(rightXFinder).controller!;
    final leftY = tester.widget<Scrollbar>(leftYFinder).controller!;
    final rightY = tester.widget<Scrollbar>(rightYFinder).controller!;

    expect(tester.widget<Scrollbar>(leftXFinder).thumbVisibility, isTrue);
    expect(tester.widget<Scrollbar>(rightXFinder).thumbVisibility, isTrue);

    leftX.jumpTo(120);
    await tester.pump();
    expect(rightX.offset, closeTo(leftX.offset, 0.5));

    leftY.jumpTo(180);
    await tester.pump();
    expect(rightY.offset, closeTo(leftY.offset, 0.5));

    rightY.jumpTo(0);
    await tester.pump();
    final overviewTopLeft = tester.getTopLeft(overviewFinder);
    final overviewSize = tester.getSize(overviewFinder);
    await tester.tapAt(
      overviewTopLeft +
          Offset(overviewSize.width / 2, overviewSize.height * 0.75),
    );
    await tester.pump();
    expect(rightY.offset, greaterThan(0));
    expect(leftY.offset, closeTo(rightY.offset, 0.5));
  });

  testWidgets('single-column diff shows interactive overview ruler', (
    tester,
  ) async {
    String? copiedText;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copiedText =
              (call.arguments as Map<Object?, Object?>)['text'] as String?;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final lines = <GitDiffLine>[
      const GitDiffLine.hunk('@@ -1,80 +1,80 @@'),
      for (var index = 0; index < 80; index += 1)
        if (index == 10)
          const GitDiffLine.deletion('-old changed')
        else if (index == 11)
          const GitDiffLine.addition('+new changed')
        else
          GitDiffLine.context(' line $index'),
    ];
    final backend = FakeGitBackend()
      ..gitDiffResult = GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: lines,
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Switch to Diff Only'));
    await tester.pumpAndSettle();

    final listFinder = find.byKey(
      const ValueKey<String>('git-diff-single-column-list'),
    );
    final overviewFinder = find.byKey(
      const ValueKey<String>('git-diff-single-column-overview'),
    );
    expect(listFinder, findsOneWidget);
    expect(overviewFinder, findsOneWidget);
    expect(find.byTooltip('Diff Overview'), findsOneWidget);
    expect(find.byType(SelectionArea), findsOneWidget);

    await tester.tap(find.text('+new changed'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyA);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyC);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(copiedText, isNotNull);
    expect(copiedText, contains('+new changed'));

    final list = tester.widget<ListView>(listFinder);
    final controller = list.controller!;
    expect(controller.hasClients, isTrue);
    expect(controller.position.maxScrollExtent, greaterThan(0));

    controller.jumpTo(0);
    await tester.pump();
    final overviewTopLeft = tester.getTopLeft(overviewFinder);
    final overviewSize = tester.getSize(overviewFinder);
    await tester.tapAt(
      overviewTopLeft +
          Offset(overviewSize.width / 2, overviewSize.height * 0.8),
    );
    await tester.pump();
    expect(controller.offset, greaterThan(0));
  });

  testWidgets(
    'editable side-by-side selection keeps manual horizontal scroll position',
    (tester) async {
      final longLine = List<String>.filled(220, 'x').join();
      final content = '$longLine\nshort\n';
      final backend = FakeGitBackend()
        ..gitDiffResult = const GitDiffResult(
          files: <GitDiffFile>[
            GitDiffFile(
              path: 'lib/main.dart',
              area: .unstaged,
              status: .modified,
              lines: <GitDiffLine>[
                GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
                GitDiffLine.deletion('-old line'),
                GitDiffLine.addition('+new line'),
              ],
            ),
          ],
        )
        ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
            Uint8List.fromList(content.codeUnits)
        ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
            Uint8List.fromList(content.codeUnits);
      final files = _EditableDiffFileService(content: content);

      await _pumpDiffSurface(
        tester,
        backend: backend,
        workspaceFileService: files,
        tab: _diffTab(filePath: 'lib/main.dart', title: 'main.dart unstaged'),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
      await tester.pumpAndSettle();

      final editorFinder = find.byKey(
        const ValueKey<String>('git-diff-working-tree-editor-lib/main.dart'),
      );
      final rightXFinder = find.byKey(
        const ValueKey<String>(
          'git-diff-working-tree-editor-x-scrollbar-lib/main.dart',
        ),
      );
      final editor = tester.widget<TextField>(editorFinder);
      final controller = editor.controller!;
      final rightX = tester.widget<Scrollbar>(rightXFinder).controller!;
      final editable = tester.widget<EditableText>(
        find.descendant(of: editorFinder, matching: find.byType(EditableText)),
      );

      editable.focusNode.requestFocus();
      await tester.pump();
      expect(editable.focusNode.hasFocus, isTrue);
      rightX.jumpTo(180);
      await tester.pump();
      expect(rightX.offset, closeTo(180, 0.5));

      controller.selection = TextSelection.collapsed(offset: longLine.length);
      await tester.pumpAndSettle();
      expect(rightX.offset, closeTo(180, 0.5));

      controller.selection = TextSelection.collapsed(
        offset: longLine.length + 1 + 'short'.length,
      );
      await tester.pumpAndSettle();
      expect(rightX.offset, closeTo(180, 0.5));
    },
  );

  testWidgets('staged side-by-side diff stays read only', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .staged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.hunk('@@ -1,1 +1,1 @@'),
              GitDiffLine.deletion('-old line'),
              GitDiffLine.addition('+staged line'),
            ],
          ),
        ],
      )
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: true)] =
          Uint8List.fromList('old line\n'.codeUnits)
      ..diffBlobBytesBySide[(filePath: 'lib/main.dart', oldSide: false)] =
          Uint8List.fromList('staged line\n'.codeUnits);
    final files = _EditableDiffFileService(content: 'workspace line\n');

    await _pumpDiffSurface(
      tester,
      backend: backend,
      workspaceFileService: files,
      tab: _diffTab(
        filePath: 'lib/main.dart',
        title: 'main.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Switch to Side-by-Side View'));
    await tester.pumpAndSettle();

    expect(
      find.byKey(
        const ValueKey<String>('git-diff-working-tree-editor-lib/main.dart'),
      ),
      findsNothing,
    );
    expect(find.byType(SelectionArea), findsOneWidget);
    expect(files.readCount, 0);
    expect(find.text('Modified'), findsOneWidget);
  });
}
