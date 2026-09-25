import 'dart:async';
import 'dart:io';

import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:path/path.dart' as p;

import '../application/language_document_session_port.dart';
import '../application/language_navigation_port.dart';
import '../domain/language_id.dart';
import '../domain/source_location.dart';
import 'code_forge_language_server_runtime.dart';

typedef CodeForgeSourceTextReader = Future<String> Function(String path);

/// Upper bound for one navigation request; large workspaces can take several
/// seconds for a first references query while the server warms up.
const Duration navigationRequestTimeout = Duration(seconds: 30);

// JSON-RPC ContentModified (-32801) and ServerCancelled (-32802).
const Set<int> _retryableNavigationErrorCodes = <int>{-32801, -32802};
const List<Duration> _navigationRetryDelays = <Duration>[
  Duration(milliseconds: 300),
  Duration(milliseconds: 800),
  Duration(seconds: 2),
];

final class CodeForgeSemanticProviderAdapter
    implements LanguageDocumentSessionPort, LanguageNavigationPort {
  factory CodeForgeSemanticProviderAdapter({
    required String workspaceId,
    required CodeForgeLanguageServerSession session,
    CodeForgeSourceTextReader? sourceTextReader,
    bool? isWindows,
  }) {
    final windows = isWindows ?? Platform.isWindows;
    return CodeForgeSemanticProviderAdapter._(
      workspaceId: workspaceId,
      transport: session.transport,
      sourceTextReader: sourceTextReader ?? _readSourceText,
      isWindows: windows,
      pathContext: p.Context(style: windows ? p.Style.windows : p.Style.posix),
    );
  }

  CodeForgeSemanticProviderAdapter._({
    required this.workspaceId,
    required this._transport,
    required this._sourceTextReader,
    required this._isWindows,
    required this._pathContext,
  });

  final String workspaceId;
  final CodeForgeLanguageServerTransport _transport;
  final CodeForgeSourceTextReader _sourceTextReader;
  final bool _isWindows;
  final p.Context _pathContext;
  final Map<String, _OpenDocument> _documents = <String, _OpenDocument>{};

  @override
  Future<void> openDocument({
    required LanguageId language,
    required String path,
    required String text,
  }) async {
    final key = _documentKey(path);
    final existing = _documents[key];
    if (existing != null) {
      if (existing.language == language) {
        await replaceDocument(path: path, text: text);
        return;
      }
      await closeDocument(path: path);
    }

    const version = 1;
    final normalizedPath = _pathContext.normalize(path);
    await _transport.sendNotification(
      method: 'textDocument/didOpen',
      params: <String, dynamic>{
        'textDocument': <String, dynamic>{
          'uri': _fileUri(normalizedPath),
          'languageId': language.value,
          'version': version,
          'text': text,
        },
      },
    );
    _documents[key] = _OpenDocument(
      path: normalizedPath,
      language: language,
      text: text,
      version: version,
    );
  }

  @override
  Future<void> replaceDocument({
    required String path,
    required String text,
  }) async {
    final key = _documentKey(path);
    final document = _documents[key];
    if (document == null) {
      throw StateError('Language-server document is not open: $path');
    }
    final nextVersion = document.version + 1;
    await _transport.sendNotification(
      method: 'textDocument/didChange',
      params: <String, dynamic>{
        'textDocument': <String, dynamic>{
          'uri': _fileUri(document.path),
          'version': nextVersion,
        },
        'contentChanges': <Map<String, dynamic>>[
          <String, dynamic>{'text': text},
        ],
      },
    );
    document
      ..text = text
      ..version = nextVersion;
  }

  @override
  Future<void> saveDocument({required String path}) async {
    final document = _documents[_documentKey(path)];
    if (document == null) {
      throw StateError('Language-server document is not open: $path');
    }
    await _transport.sendNotification(
      method: 'textDocument/didSave',
      params: <String, dynamic>{
        'textDocument': <String, dynamic>{'uri': _fileUri(document.path)},
        'text': document.text,
      },
    );
  }

  @override
  Future<void> closeDocument({required String path}) async {
    final key = _documentKey(path);
    final document = _documents[key];
    if (document == null) {
      return;
    }
    await _transport.sendNotification(
      method: 'textDocument/didClose',
      params: <String, dynamic>{
        'textDocument': <String, dynamic>{'uri': _fileUri(document.path)},
      },
    );
    _documents.remove(key);
  }

  @override
  Future<List<SourceLocation>> definition({
    required String path,
    required SourcePosition position,
  }) async {
    const maxAliasHops = 4;
    var requestPath = path;
    var locations = await _navigationRequest(
      method: 'textDocument/definition',
      path: requestPath,
      position: position,
    );
    final visited = <String>{_definitionPositionKey(requestPath, position)};

    for (var hop = 0; hop < maxAliasHops; hop += 1) {
      if (locations.length != 1) {
        return locations;
      }
      final target = locations.single;
      if (_documentKey(target.path) != _documentKey(requestPath)) {
        return locations;
      }
      final targetKey = _definitionPositionKey(target.path, target.range.start);
      if (!visited.add(targetKey)) {
        return locations;
      }

      final List<SourceLocation> next;
      try {
        next = await _navigationRequest(
          method: 'textDocument/definition',
          path: target.path,
          position: target.range.start,
        );
      } on LanguageServerRequestException {
        return locations;
      }
      if (next.isEmpty) {
        return locations;
      }
      locations = next;
      requestPath = target.path;
    }
    return locations;
  }

  @override
  Future<List<SourceLocation>> declaration({
    required String path,
    required SourcePosition position,
  }) => _navigationRequest(
    method: 'textDocument/declaration',
    path: path,
    position: position,
  );

  @override
  Future<List<SourceLocation>> typeDefinition({
    required String path,
    required SourcePosition position,
  }) => _navigationRequest(
    method: 'textDocument/typeDefinition',
    path: path,
    position: position,
  );

  @override
  Future<List<SourceLocation>> implementation({
    required String path,
    required SourcePosition position,
  }) => _navigationRequest(
    method: 'textDocument/implementation',
    path: path,
    position: position,
  );

  @override
  Future<List<SourceLocation>> references({
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  }) => _navigationRequest(
    method: 'textDocument/references',
    path: path,
    position: position,
    extraParams: <String, dynamic>{
      'context': <String, bool>{'includeDeclaration': includeDeclaration},
    },
  );

  Future<List<SourceLocation>> _navigationRequest({
    required String method,
    required String path,
    required SourcePosition position,
    Map<String, dynamic> extraParams = const <String, dynamic>{},
  }) async {
    final text = await _textForPath(path);
    final utf16Character = _scalarToUtf16Column(
      text,
      position.line,
      position.scalarColumn,
    );
    final normalizedPath = _pathContext.normalize(path);
    final params = <String, dynamic>{
      'textDocument': <String, dynamic>{'uri': _fileUri(normalizedPath)},
      'position': <String, int>{
        'line': position.line,
        'character': utf16Character,
      },
      ...extraParams,
    };
    for (var attempt = 0; ; attempt += 1) {
      // The stdio transport waits indefinitely for a reply, so a server that
      // never answers would leave the command silently pending.
      final response = await _transport
          .sendRequest(method: method, params: params)
          .timeout(navigationRequestTimeout);
      final error = response['error'];
      if (error is! Map) {
        return _normalizeLocations(response['result']);
      }
      final code = error['code'] is int ? error['code'] as int : null;
      // rust-analyzer and gopls answer ContentModified or ServerCancelled
      // while they are still indexing; LSP defines both as "ask again".
      if (_retryableNavigationErrorCodes.contains(code) &&
          attempt < _navigationRetryDelays.length) {
        await Future<void>.delayed(_navigationRetryDelays[attempt]);
        continue;
      }
      throw LanguageServerRequestException(
        method: method,
        code: code,
        message: error['message']?.toString() ?? 'Unknown error',
      );
    }
  }

  Future<List<SourceLocation>> _normalizeLocations(Object? result) async {
    final rawLocations = switch (result) {
      null => const <Object?>[],
      List<Object?> items => items,
      Map<Object?, Object?> item => <Object?>[item],
      _ => const <Object?>[],
    };
    final locations = <SourceLocation>[];
    for (final raw in rawLocations) {
      if (raw is! Map) {
        continue;
      }
      final location = await _normalizeLocation(raw);
      if (location != null) {
        locations.add(location);
      }
    }
    return List<SourceLocation>.unmodifiable(locations);
  }

  Future<SourceLocation?> _normalizeLocation(Map raw) async {
    final uriValue = raw['targetUri'] ?? raw['uri'];
    if (uriValue is! String) {
      return null;
    }
    final targetPath = _pathFromFileUri(uriValue);
    if (targetPath == null) {
      return null;
    }
    final rawRange =
        raw['targetSelectionRange'] ?? raw['range'] ?? raw['targetRange'];
    if (rawRange is! Map) {
      return null;
    }
    final start = rawRange['start'];
    final end = rawRange['end'];
    if (start is! Map || end is! Map) {
      return null;
    }
    final startLine = start['line'];
    final startCharacter = start['character'];
    final endLine = end['line'];
    final endCharacter = end['character'];
    if (startLine is! int ||
        startCharacter is! int ||
        endLine is! int ||
        endCharacter is! int ||
        startLine < 0 ||
        startCharacter < 0 ||
        endLine < 0 ||
        endCharacter < 0) {
      return null;
    }

    final text = await _tryTextForPath(targetPath);
    if (text == null) {
      return null;
    }
    final scalarStart = _utf16ToScalarPosition(text, startLine, startCharacter);
    final scalarEnd = _utf16ToScalarPosition(text, endLine, endCharacter);
    if (scalarStart == null || scalarEnd == null) {
      return null;
    }
    return SourceLocation(
      workspaceId: workspaceId,
      path: _pathContext.normalize(targetPath),
      range: SourceRange(start: scalarStart, end: scalarEnd),
    );
  }

  Future<String> _textForPath(String path) async {
    final document = _documents[_documentKey(path)];
    if (document != null) {
      return document.text;
    }
    return _sourceTextReader(_pathContext.normalize(path));
  }

  Future<String?> _tryTextForPath(String path) async {
    try {
      return await _textForPath(path);
    } catch (_) {
      return null;
    }
  }

  String _fileUri(String path) =>
      Uri.file(path, windows: _isWindows).toString();

  String? _pathFromFileUri(String value) {
    try {
      final uri = Uri.parse(value);
      if (uri.scheme != 'file') {
        return null;
      }
      return _pathContext.normalize(uri.toFilePath(windows: _isWindows));
    } on FormatException {
      return null;
    } on UnsupportedError {
      return null;
    }
  }

  String _documentKey(String path) {
    // Editor paths keep the stored `\\?\` root while server locations do not;
    // both must resolve to the same open document.
    final normalized = comparablePath(path, pathContext: _pathContext);
    return _isWindows ? normalized.toLowerCase() : normalized;
  }

  String _definitionPositionKey(String path, SourcePosition position) =>
      '${_documentKey(path)}:${position.line}:${position.scalarColumn}';
}

final class _OpenDocument {
  _OpenDocument({
    required this.path,
    required this.language,
    required this.text,
    required this.version,
  });

  final String path;
  final LanguageId language;
  String text;
  int version;
}

Future<String> _readSourceText(String path) => File(path).readAsString();

int _scalarToUtf16Column(String text, int line, int scalarColumn) {
  final content = _lineContent(text, line);
  if (content == null) {
    throw RangeError.range(line, 0, _lineCount(text) - 1, 'line');
  }
  if (scalarColumn < 0) {
    throw RangeError.value(scalarColumn, 'scalarColumn');
  }
  var scalar = 0;
  var utf16 = 0;
  for (final rune in content.runes) {
    if (scalar == scalarColumn) {
      return utf16;
    }
    scalar += 1;
    utf16 += rune > 0xFFFF ? 2 : 1;
  }
  if (scalar == scalarColumn) {
    return utf16;
  }
  throw RangeError.range(scalarColumn, 0, scalar, 'scalarColumn');
}

SourcePosition? _utf16ToScalarPosition(String text, int line, int utf16Column) {
  final content = _lineContent(text, line);
  if (content == null || utf16Column < 0) {
    return null;
  }
  var scalar = 0;
  var utf16 = 0;
  for (final rune in content.runes) {
    if (utf16 >= utf16Column) {
      break;
    }
    final next = utf16 + (rune > 0xFFFF ? 2 : 1);
    if (utf16Column < next) {
      break;
    }
    utf16 = next;
    scalar += 1;
  }
  return SourcePosition(line: line, scalarColumn: scalar);
}

String? _lineContent(String text, int line) {
  if (line < 0) {
    return null;
  }
  var currentLine = 0;
  var start = 0;
  for (var index = 0; index <= text.length; index += 1) {
    final atEnd = index == text.length;
    if (!atEnd && text.codeUnitAt(index) != 0x0A) {
      continue;
    }
    if (currentLine == line) {
      var end = index;
      if (end > start && text.codeUnitAt(end - 1) == 0x0D) {
        end -= 1;
      }
      return text.substring(start, end);
    }
    currentLine += 1;
    start = index + 1;
  }
  return null;
}

int _lineCount(String text) {
  var count = 1;
  for (final codeUnit in text.codeUnits) {
    if (codeUnit == 0x0A) count += 1;
  }
  return count;
}
