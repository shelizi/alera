import 'dart:collection';
import 'dart:typed_data';

import 'terminal_xterm_worker.dart';

/// Words per cell in the worker's packed full-buffer payload.
const int terminalXtermPackedCellStride = 8;

const int _widthWord = 0;
const int _foregroundWord = 1;
const int _backgroundWord = 2;
const int _attributesWord = 3;
const int _underlineColorWord = 4;
const int _contentWord = 5;
const int _hyperlinkWord = 6;
const int _semanticWord = 7;

/// One row's cells read in place from a packed full-repaint payload.
///
/// A full repaint carries every scrollback row, so building a
/// [TerminalXtermWorkerRenderCell] per cell on the UI isolate cost more than
/// applying the rows themselves (a 10,000-line resize allocated 1.6M of them).
/// Consumers that recognize this type read the words directly; any other
/// consumer still gets cells, materialized on access.
final class TerminalXtermWorkerPackedCells
    extends ListBase<TerminalXtermWorkerRenderCell> {
  TerminalXtermWorkerPackedCells(
    this._words,
    this._start,
    this._length,
    this._combining,
  );

  final Uint32List _words;
  final int _start;
  final int _length;
  final Map<int, String>? _combining;

  @override
  int get length => _length;

  @override
  set length(int newLength) =>
      throw UnsupportedError('Packed terminal cells are read-only.');

  @override
  void operator []=(int index, TerminalXtermWorkerRenderCell value) =>
      throw UnsupportedError('Packed terminal cells are read-only.');

  @override
  TerminalXtermWorkerRenderCell operator [](int index) {
    RangeError.checkValidIndex(index, this, 'index', _length);
    return TerminalXtermWorkerRenderCell(
      width: widthAt(index),
      foreground: foregroundAt(index),
      background: backgroundAt(index),
      attributes: attributesAt(index),
      underlineColor: underlineColorAt(index),
      content: contentAt(index),
      hyperlinkId: hyperlinkIdAt(index),
      semanticAttributes: semanticAttributesAt(index),
      combiningCharacters: combiningCharactersAt(index),
    );
  }

  int _word(int index, int word) =>
      _words[_start + index * terminalXtermPackedCellStride + word];

  int widthAt(int index) => _word(index, _widthWord);
  int foregroundAt(int index) => _word(index, _foregroundWord);
  int backgroundAt(int index) => _word(index, _backgroundWord);
  int attributesAt(int index) => _word(index, _attributesWord);
  int underlineColorAt(int index) => _word(index, _underlineColorWord);
  int contentAt(int index) => _word(index, _contentWord);
  int hyperlinkIdAt(int index) => _word(index, _hyperlinkWord);
  int semanticAttributesAt(int index) => _word(index, _semanticWord);
  String? combiningCharactersAt(int index) => _combining?[index];
}
