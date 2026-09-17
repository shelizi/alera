import 'package:alera/src/features/workbench/domain/terminal_search.dart';
import 'package:alera/src/features/workbench/presentation/terminal_search_controller.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_buffer_model.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart' as xterm;

void main() {
  test('finds literal matches case-insensitively by default', () {
    final matches = findTerminalSearchMatches(const <TerminalSearchLine>[
      TerminalSearchLine(id: 'first', index: 3, text: 'A [literal] match'),
      TerminalSearchLine(id: 'second', index: 4, text: 'MATCH again'),
    ], '[literal]');

    expect(matches, hasLength(1));
    expect(matches.single.lineId, 'first');
    expect(matches.single.lineIndex, 3);
    expect(matches.single.start, 2);
    expect(matches.single.end, 11);
  });

  test('returns no results for an empty or missing query', () {
    const lines = <TerminalSearchLine>[
      TerminalSearchLine(id: 'line', index: 0, text: 'terminal output'),
    ];

    expect(findTerminalSearchMatches(lines, ''), isEmpty);
    expect(findTerminalSearchMatches(lines, 'missing'), isEmpty);
  });

  test('supports explicit case-sensitive matching', () {
    final matches = findTerminalSearchMatches(
      const <TerminalSearchLine>[
        TerminalSearchLine(id: 'line', index: 0, text: 'Match match'),
      ],
      'match',
      caseSensitive: true,
    );

    expect(matches, hasLength(1));
    expect(matches.single.start, 6);
  });

  test('navigates matches and wraps in both directions', () {
    final terminal = _terminal()..write('needle\r\nother\r\nNEEDLE\r\nthird');
    final visitedLines = <int>[];
    final controller = TerminalSearchController(
      terminal: terminal,
      scrollToLine: visitedLines.add,
    );
    addTearDown(controller.dispose);

    controller.open();
    controller.setQuery('needle');

    expect(controller.matchCount, 2);
    expect(controller.selectedMatchNumber, 1);
    expect(controller.selectedMatch?.lineIndex, 0);
    expect(visitedLines, <int>[0]);

    controller.next();
    expect(controller.selectedMatchNumber, 2);
    expect(controller.selectedMatch?.lineIndex, 2);

    controller.next();
    expect(controller.selectedMatchNumber, 1);
    expect(controller.selectedMatch?.lineIndex, 0);

    controller.previous();
    expect(controller.selectedMatchNumber, 2);
    expect(controller.selectedMatch?.lineIndex, 2);
  });

  test('updates matches after new output without rebuilding the query', () {
    final terminal = _terminal()..write('ready\r\n');
    final controller = TerminalSearchController(
      terminal: terminal,
      scrollToLine: (_) {},
    );
    addTearDown(controller.dispose);

    controller.open();
    controller.setQuery('result');
    expect(controller.matchCount, 0);

    terminal.write('new RESULT');

    expect(controller.matchCount, 1);
    expect(controller.selectedMatch?.lineIndex, 1);
    expect(controller.needsFullRefreshForTesting, isFalse);
  });

  test(
    'keeps matches ordered by line then column after incremental output',
    () {
      final terminal = _terminal()
        ..write('needle needle\r\nother\r\nneedle needle needle');
      final controller = TerminalSearchController(
        terminal: terminal,
        scrollToLine: (_) {},
      );
      addTearDown(controller.dispose);

      controller.open();
      controller.setQuery('needle');

      expect(
        controller.matches
            .map((match) => (match.lineIndex, match.start))
            .toList(),
        <(int, int)>[(0, 0), (0, 7), (2, 0), (2, 7), (2, 14)],
      );

      terminal.write('\r\ntail');

      expect(
        controller.matches
            .map((match) => (match.lineIndex, match.start))
            .toList(),
        <(int, int)>[(0, 0), (0, 7), (2, 0), (2, 7), (2, 14)],
      );
    },
  );

  test('only listens while search is open with a non-empty query', () {
    final terminal = _terminal();
    final controller = TerminalSearchController(
      terminal: terminal,
      scrollToLine: (_) {},
    );
    addTearDown(controller.dispose);

    expect(terminal.listeners, isEmpty);

    controller.open();
    expect(terminal.listeners, isEmpty);

    controller.setQuery('needle');
    expect(terminal.listeners, hasLength(1));

    controller.setQuery('');
    expect(terminal.listeners, isEmpty);

    controller.setQuery('needle');
    expect(terminal.listeners, hasLength(1));

    controller.close();
    expect(terminal.listeners, isEmpty);
  });

  test('releases the match index when the overlay closes', () {
    final terminal = _terminal()..write('needle\r\nother needle');
    final controller = TerminalSearchController(
      terminal: terminal,
      scrollToLine: (_) {},
    );
    addTearDown(controller.dispose);

    controller.open();
    controller.setQuery('needle');
    expect(controller.matchCount, 2);

    // Matches are one entry per scrollback hit; keeping them while the
    // overlay is hidden retains memory nobody can see.
    controller.close();
    expect(controller.matchCount, 0);
    expect(controller.selectedMatch, isNull);

    // Reopening rescans with the kept query, so nothing is lost.
    controller.open();
    expect(controller.matchCount, 2);
  });

  test('rechecks output that arrived while the overlay was closed', () {
    final terminal = _terminal()..write('first');
    final controller = TerminalSearchController(
      terminal: terminal,
      scrollToLine: (_) {},
    );
    addTearDown(controller.dispose);

    controller.open();
    controller.setQuery('later');
    expect(controller.matchCount, 0);

    controller.close();
    terminal.write('\r\nlater');
    controller.open();

    expect(controller.matchCount, 1);
    expect(controller.selectedMatch?.lineIndex, 1);
  });

  test(
    'searches the xterm worker mirror without isolate round trips',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 20,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();
      model.apply(await worker.writeBufferDelta('needle\r\nother\r\nNEEDLE'));
      final visitedLines = <int>[];
      final controller = TerminalSearchController.fromSource(
        source: model,
        scrollToLine: visitedLines.add,
      );
      addTearDown(controller.dispose);

      controller.open();
      controller.setQuery('needle');
      expect(controller.matchCount, 2);
      expect(controller.matches.map((match) => match.lineIndex), <int>[0, 2]);
      expect(visitedLines, <int>[0]);

      model.apply(await worker.writeBufferDelta('\r\ntail needle'));
      expect(controller.matchCount, 3);
      expect(controller.matches.map((match) => match.lineIndex), <int>[
        0,
        2,
        3,
      ]);
      expect(controller.needsFullRefreshForTesting, isFalse);
    },
  );

  test(
    'mirror search follows same-height TUI rewrites and keeps selection',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 20,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();
      model.apply(
        await worker.writeBufferDelta(
          'needle-first\r\nplain\r\nneedle-selected\r\nfooter',
        ),
      );
      final controller = TerminalSearchController.fromSource(
        source: model,
        scrollToLine: (_) {},
      );
      addTearDown(controller.dispose);

      controller
        ..open()
        ..setQuery('needle')
        ..next();
      expect(controller.selectedMatch?.lineIndex, 2);

      final rewrite = await worker.writeBufferDelta(
        '\x1b[1;1Hplain-now\x1b[K\x1b[2;1Hneedle-new\x1b[K',
      );
      expect(rewrite.fullRepaint, isFalse);
      expect(rewrite.bufferLength, model.bufferLength);
      model.apply(rewrite);

      expect(controller.matches.map((match) => match.lineIndex), <int>[1, 2]);
      expect(controller.selectedMatch?.lineIndex, 2);
      expect(controller.selectedMatchNumber, 2);
      expect(controller.needsFullRefreshForTesting, isFalse);
    },
  );

  test(
    'mirror search keeps matches correct across circular head trims',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 20,
        rows: 3,
        maxLines: 25,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();
      final initial = <String>[
        'needle-drop',
        'needle-keep',
        ...List<String>.generate(23, (index) => 'line-$index'),
      ].join('\r\n');
      model.apply(await worker.writeBufferDelta(initial));
      final controller = TerminalSearchController.fromSource(
        source: model,
        scrollToLine: (_) {},
      );
      addTearDown(controller.dispose);

      controller.open();
      controller.setQuery('needle');
      expect(controller.matches.map((match) => match.lineIndex), <int>[0, 1]);

      final trimmed = await worker.writeBufferDelta('\r\nneedle-tail');
      expect(trimmed.trimStart, 1);
      model.apply(trimmed);

      expect(controller.matchCount, 2);
      expect(controller.matches.map((match) => match.lineIndex), <int>[0, 24]);
      expect(controller.needsFullRefreshForTesting, isFalse);
    },
  );
}

xterm.Terminal _terminal() {
  return xterm.Terminal(maxLines: 64)..resize(80, 8);
}
