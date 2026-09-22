import 'dart:typed_data';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/reading_diff/application/reading_diff_generation_progress.dart';
import 'package:alera/src/features/reading_diff/domain/reading_diff_models.dart';
import 'package:alera/src/features/reading_diff/presentation/reading_diff_confirmation_dialog.dart';
import 'package:alera/src/features/reading_diff/presentation/reading_diff_failure_view.dart';
import 'package:alera/src/features/reading_diff/presentation/reading_diff_generation_progress_view.dart';
import 'package:alera/src/features/reading_diff/presentation/reading_diff_view.dart';
import 'package:alera/src/rust/api/reading_diff.dart' as rust;
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('confirmation discloses AI Assist usage and diff-only access', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: ReadingDiffConfirmationDialog(preparation: _preparation()),
      ),
    );

    expect(find.text('Generate Reading Diff'), findsNWidgets(2));
    expect(
      find.textContaining('may consume subscription quota'),
      findsOneWidget,
    );
    expect(find.textContaining('complete selected patch'), findsOneWidget);
    expect(find.textContaining('hidden by preview truncation'), findsOneWidget);
    expect(find.textContaining('behavioral overview'), findsOneWidget);
    expect(find.textContaining('not a bug or security review'), findsOneWidget);
    expect(find.text('Diff Only'), findsOneWidget);
    expect(find.text('Antigravity'), findsOneWidget);
    expect(find.text('1'), findsOneWidget);
  });

  testWidgets('reading view separates overview from the condensed diff', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: SizedBox(
          width: 800,
          height: 500,
          child: ReadingDiffView(
            result: ReadingDiffResult(
              diff: .fromList('+new\n-old\n'.codeUnits),
              summary: 'Keep the behavioral change.',
              changedLines: 4,
              retainedChangedLines: 2,
              agentLabel: 'Codex',
              model: 'gpt-5.5',
              effort: 'high',
              chunkSummaries: const <ReadingDiffChunkSummary>[
                ReadingDiffChunkSummary(
                  index: 0,
                  summary: 'Update the command flow.',
                ),
                ReadingDiffChunkSummary(
                  index: 1,
                  summary: 'Cover the updated flow with tests.',
                ),
              ],
              fromCache: true,
            ),
          ),
        ),
      ),
    );

    expect(find.text('Keep the behavioral change.'), findsOneWidget);
    expect(find.text('Codex'), findsOneWidget);
    expect(find.text('gpt-5.5'), findsOneWidget);
    expect(find.text('high Effort'), findsOneWidget);
    expect(find.text('2 Chunks'), findsOneWidget);
    expect(find.text('Kept 2/4 Changed Lines'), findsOneWidget);
    expect(find.text('Cached Result'), findsOneWidget);
    expect(find.text('Chunk Analysis'), findsOneWidget);
    expect(find.text('Chunk 1 of 2'), findsOneWidget);
    expect(find.text('Update the command flow.'), findsOneWidget);
    expect(find.textContaining('does not identify bugs'), findsOneWidget);
    expect(find.text('+new'), findsNothing);

    await tester.runAsync(() => Future.pause(.zero));
    await tester.tap(find.text('Condensed Diff'));
    await tester.pumpAndSettle();

    expect(find.text('+new'), findsOneWidget);
    expect(find.text('-old'), findsOneWidget);
  });

  testWidgets('reading diff progress shows the active agent chunk and model', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: ReadingDiffGenerationProgressView(
          progress: ReadingDiffGenerationProgress(
            stage: .generating,
            completedChunks: 1,
            totalChunks: 3,
            currentChunk: 2,
          ),
          agentLabel: 'Claude Code',
          model: 'sonnet',
        ),
      ),
    );

    expect(find.text('Generating chunk 2 of 3'), findsOneWidget);
    expect(
      find.text(
        'The agent is proposing safe elisions; Rust validates the plan.',
      ),
      findsOneWidget,
    );
    expect(find.text('Claude Code · sonnet'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });

  testWidgets('reading diff surfaces localize Traditional Chinese chrome', (
    tester,
  ) async {
    await tester.pumpWidget(
      _traditionalChineseApp(
        SizedBox(
          width: 800,
          height: 500,
          child: ReadingDiffView(
            result: ReadingDiffResult(
              diff: .fromList('+new\n-old\n'.codeUnits),
              summary: 'Keep the behavioral change.',
              changedLines: 4,
              retainedChangedLines: 2,
              agentLabel: 'Codex',
              model: 'gpt-5.5',
              effort: 'high',
              chunkSummaries: const <ReadingDiffChunkSummary>[
                ReadingDiffChunkSummary(index: 0, summary: 'First summary.'),
                ReadingDiffChunkSummary(index: 1, summary: 'Second summary.'),
              ],
              fromCache: true,
            ),
          ),
        ),
      ),
    );

    expect(find.text('總覽'), findsOneWidget);
    expect(find.text('精簡 Diff'), findsOneWidget);
    expect(find.text('推理強度：high'), findsOneWidget);
    expect(find.text('2 個區塊'), findsOneWidget);
    expect(find.text('保留 2/4 行變更'), findsOneWidget);
    expect(find.text('快取結果'), findsOneWidget);
    expect(find.text('區塊 1/2'), findsOneWidget);
    expect(find.text('Codex'), findsOneWidget);
    expect(find.text('gpt-5.5'), findsOneWidget);

    await tester.pumpWidget(
      _traditionalChineseApp(
        ReadingDiffGenerationProgressView(
          progress: const ReadingDiffGenerationProgress(
            stage: .generating,
            completedChunks: 1,
            totalChunks: 3,
            currentChunk: 2,
          ),
          agentLabel: 'Claude Code',
          model: 'sonnet',
        ),
      ),
    );
    expect(find.text('正在產生區塊 2/3'), findsOneWidget);
    expect(find.text('Agent 正在提出安全的省略方案；Rust 會驗證該方案。'), findsOneWidget);
    expect(find.text('Claude Code · sonnet'), findsOneWidget);

    await tester.pumpWidget(
      _traditionalChineseApp(
        ReadingDiffConfirmationDialog(preparation: _preparation()),
      ),
    );
    expect(find.text('產生閱讀 Diff'), findsNWidgets(2));
    expect(find.text('僅限 Diff'), findsOneWidget);
    expect(find.text('模型'), findsOneWidget);
    expect(find.text('存取權限'), findsOneWidget);
    expect(find.text('Antigravity'), findsOneWidget);

    const errorMessage = 'Codex failed: schema mismatch.';
    await tester.pumpWidget(
      _traditionalChineseApp(
        ReadingDiffFailureView(message: errorMessage, onDismiss: () {}),
      ),
    );
    expect(find.text('閱讀 Diff 產生失敗'), findsOneWidget);
    expect(find.text(errorMessage), findsOneWidget);
    expect(find.byTooltip('關閉錯誤'), findsOneWidget);
  });

  testWidgets('reading diff failures remain visible and selectable', (
    tester,
  ) async {
    var dismissed = false;
    const message =
        'Codex failed: Invalid schema for response_format: version must have a type key.';

    await tester.pumpWidget(
      MaterialApp(
        home: ReadingDiffFailureView(
          message: message,
          onDismiss: () => dismissed = true,
        ),
      ),
    );

    expect(find.text('Reading diff generation failed'), findsOneWidget);
    expect(find.text(message), findsOneWidget);
    expect(find.byType(SelectableText), findsOneWidget);

    await tester.tap(find.byTooltip('Dismiss Error'));
    expect(dismissed, isTrue);
  });
}

Widget _traditionalChineseApp(Widget home) => MaterialApp(
  locale: const Locale('zh', 'TW'),
  supportedLocales: supportedAleraLocales,
  localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
    AleraLocalizationsDelegate(),
    ...GlobalMaterialLocalizations.delegates,
  ],
  home: home,
);

ReadingDiffPreparation _preparation() {
  final request = ReadingDiffRequest(
    workspacePath: '/repo',
    settings: .defaults,
  );
  return ReadingDiffPreparation(
    request: request,
    rawDiff: .fromList(<int>[1]),
    compiler: rust.ReadingDiffPreparation(
      rawBytes: .from(1024),
      schemaVersion: 1,
      rubricVersion: 'rubric-v1',
      planSchema: '{}',
      chunks: <rust.ReadingDiffChunk>[
        rust.ReadingDiffChunk(
          index: 0,
          rawDiff: .fromList(<int>[1]),
          numberedDiff: '1|diff --git a/a b/a',
          continuationPreamble: Uint8List(0),
        ),
      ],
    ),
    agentType: AgentType.agy,
    model: 'agent-model',
    effort: 'medium',
    accessPolicy: .diffOnly,
    cacheKey: 'key',
  );
}
