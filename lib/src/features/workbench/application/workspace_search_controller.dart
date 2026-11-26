import 'dart:async';

import 'package:alera/src/features/workbench/application/retired_workspace_invalidation.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/workspace_search_projection.dart';
import 'package:alera/src/features/workbench/application/workspace_search_service.dart';
import 'package:alera/src/rust/api/workspace_search.dart' as native;
import 'package:riverpod_annotation/riverpod_annotation.dart';

export 'package:alera/src/features/workbench/application/workspace_search_projection.dart'
    show
        workspaceSearchCollapsibleNodeKeys,
        workspaceSearchDirectoryNodeKey,
        workspaceSearchDirectoryPaths,
        workspaceSearchFileNodeKey;

part 'workspace_search_controller.g.dart';

const int _workspaceSearchMaxResults = 2000;
const Duration _workspaceSearchDebounce = Duration(milliseconds: 250);
const Duration _workspaceLiteralDebounce = Duration(milliseconds: 120);
const Duration _workspaceSlowSearchThreshold = Duration(milliseconds: 400);

class const WorkspaceSearchState({
  final String query = '',
  final String replacement = '',
  final String includePattern = '',
  final String excludePattern = '',
  final bool caseSensitive = false,
  final bool wholeWord = false,
  final bool useRegex = false,
  final bool preserveCase = false,
  final bool includeIgnored = false,
  final bool loading = false,
  final bool replacing = false,
  final bool viewAsTree = false,
  final String? error,
  final native.WorkspaceSearchResult? result,
  final Set<String> collapsedResultNodeKeys = const <String>{},
}) {
  Set<String> get collapsedFilePaths => collapsedResultNodeKeys;

  bool get hasQuery => query.isNotEmpty;

  bool get hasReplacement => replacement.isNotEmpty;

  WorkspaceSearchState copyWith({
    String? query,
    String? replacement,
    String? includePattern,
    String? excludePattern,
    bool? caseSensitive,
    bool? wholeWord,
    bool? useRegex,
    bool? preserveCase,
    bool? includeIgnored,
    bool? loading,
    bool? replacing,
    bool? viewAsTree,
    Object? error = _sentinel,
    Object? result = _sentinel,
    Set<String>? collapsedResultNodeKeys,
  }) {
    return WorkspaceSearchState(
      query: query ?? this.query,
      replacement: replacement ?? this.replacement,
      includePattern: includePattern ?? this.includePattern,
      excludePattern: excludePattern ?? this.excludePattern,
      caseSensitive: caseSensitive ?? this.caseSensitive,
      wholeWord: wholeWord ?? this.wholeWord,
      useRegex: useRegex ?? this.useRegex,
      preserveCase: preserveCase ?? this.preserveCase,
      includeIgnored: includeIgnored ?? this.includeIgnored,
      loading: loading ?? this.loading,
      replacing: replacing ?? this.replacing,
      viewAsTree: viewAsTree ?? this.viewAsTree,
      error: identical(error, _sentinel) ? this.error : error as String?,
      result: identical(result, _sentinel)
          ? this.result
          : result as native.WorkspaceSearchResult?,
      collapsedResultNodeKeys:
          collapsedResultNodeKeys ?? this.collapsedResultNodeKeys,
    );
  }
}

const Object _sentinel = Object();

class const WorkspaceSearchDirtyFilesException(final List<String> paths)
    implements Exception {
  @override
  String toString() => 'Unsaved files: ${paths.join(', ')}';
}

@Riverpod(keepAlive: true)
class WorkspaceSearchController extends _$WorkspaceSearchController {
  Timer? _debounce;
  Duration? _lastSearchDuration;
  int _requestGeneration = 0;
  String? _activeRequestId;
  StreamIterator<native.WorkspaceSearchBatch>? _activeSearchIterator;
  WorkspaceSearchService? _searchService;
  native.WorkspaceSearchResult? _projectedResult;
  WorkspaceSearchProjection? _resultProjection;

  @override
  WorkspaceSearchState build(String workspaceId) {
    invalidateWhenWorkspaceRetired(ref, workspaceId);
    _searchService = ref.read(workspaceSearchServiceProvider);
    ref.onDispose(() {
      _debounce?.cancel();
      _cancelActiveSearch();
      _requestGeneration += 1;
    });
    return const WorkspaceSearchState();
  }

  void setQuery(String workspacePath, String value) {
    if (state.query == value) {
      return;
    }
    _applySearchInputChange(workspacePath, state.copyWith(query: value));
  }

  void setReplacement(String workspacePath, String value) {
    if (state.replacement == value) {
      return;
    }
    _applySearchInputChange(workspacePath, state.copyWith(replacement: value));
  }

  void setIncludePattern(String workspacePath, String value) {
    if (state.includePattern == value) {
      return;
    }
    _applySearchInputChange(
      workspacePath,
      state.copyWith(includePattern: value),
    );
  }

  void setExcludePattern(String workspacePath, String value) {
    if (state.excludePattern == value) {
      return;
    }
    _applySearchInputChange(
      workspacePath,
      state.copyWith(excludePattern: value),
    );
  }

  void toggleCaseSensitive(String workspacePath) {
    _applySearchInputChange(
      workspacePath,
      state.copyWith(caseSensitive: !state.caseSensitive),
    );
  }

  void toggleWholeWord(String workspacePath) {
    _applySearchInputChange(
      workspacePath,
      state.copyWith(wholeWord: !state.wholeWord),
    );
  }

  void toggleUseRegex(String workspacePath) {
    _applySearchInputChange(
      workspacePath,
      state.copyWith(useRegex: !state.useRegex),
    );
  }

  void togglePreserveCase(String workspacePath) {
    _applySearchInputChange(
      workspacePath,
      state.copyWith(preserveCase: !state.preserveCase),
    );
  }

  void toggleIncludeIgnored(String workspacePath) {
    _applySearchInputChange(
      workspacePath,
      state.copyWith(includeIgnored: !state.includeIgnored),
    );
  }

  void toggleResultNodeCollapsed(String nodeKey) {
    final next = Set<String>.from(state.collapsedResultNodeKeys);
    if (!next.add(nodeKey)) {
      next.remove(nodeKey);
    }
    state = state.copyWith(collapsedResultNodeKeys: next);
  }

  void toggleFileCollapsed(String relativePath) {
    toggleResultNodeCollapsed(workspaceSearchFileNodeKey(relativePath));
  }

  void toggleAllResultsCollapsed() {
    final keys =
        _projectionForCurrentResult()?.collapsibleNodeKeys(
          viewAsTree: state.viewAsTree,
        ) ??
        const <String>{};
    if (keys.isEmpty) {
      return;
    }
    final allCollapsed = keys.every(state.collapsedResultNodeKeys.contains);
    state = state.copyWith(
      collapsedResultNodeKeys: allCollapsed ? const <String>{} : keys,
    );
  }

  void toggleAllFilesCollapsed() {
    toggleAllResultsCollapsed();
  }

  void toggleViewAsTree() {
    state = state.copyWith(viewAsTree: !state.viewAsTree);
  }

  void clearSearchResults() {
    _debounce?.cancel();
    _cancelActiveSearch();
    _requestGeneration += 1;
    state = state.copyWith(
      query: '',
      replacement: '',
      includePattern: '',
      excludePattern: '',
      loading: false,
      error: null,
      result: null,
      collapsedResultNodeKeys: const <String>{},
    );
  }

  Future<void> searchNow(String workspacePath) async {
    _debounce?.cancel();
    _cancelActiveSearch();
    final query = state.query;
    final generation = ++_requestGeneration;
    if (query.isEmpty) {
      state = state.copyWith(loading: false, error: null, result: null);
      return;
    }
    final requestId = '$workspacePath:${identityHashCode(this)}:$generation';
    _activeRequestId = requestId;
    state = state.copyWith(loading: true, error: null, result: null);
    try {
      final stopwatch = Stopwatch()..start();
      final searchOptions = _searchOptions(workspacePath);
      final result = state.hasReplacement
          ? (await _service.previewReplace(
              options: native.WorkspaceReplaceOptions(
                search: searchOptions,
                replacement: state.replacement,
                preserveCase: state.preserveCase,
              ),
              requestId: requestId,
            )).result
          : await _searchBatches(searchOptions, requestId, generation);
      if (generation != _requestGeneration || result == null) {
        return;
      }
      state = state.copyWith(loading: false, result: result, error: null);
      _lastSearchDuration = stopwatch.elapsed;
    } catch (error) {
      if (generation != _requestGeneration) {
        return;
      }
      state = state.copyWith(
        loading: false,
        result: null,
        error: _messageFor(error),
      );
    } finally {
      if (_activeRequestId == requestId) {
        _activeRequestId = null;
      }
    }
  }

  Future<native.WorkspaceSearchResult?> _searchBatches(
    native.WorkspaceSearchOptions options,
    String requestId,
    int generation,
  ) async {
    final iterator = StreamIterator(
      _service.searchBatches(options: options, requestId: requestId),
    );
    _activeSearchIterator = iterator;
    final files = <native.WorkspaceSearchFileResult>[];
    final paths = <String>{};
    var count = 0;
    var truncated = false;
    var receivedTerminal = false;
    try {
      while (await iterator.moveNext()) {
        // Check before touching state or accumulating an obsolete batch.
        if (generation != _requestGeneration) return null;
        final batch = iterator.current;
        if (batch.error case final error?) {
          receivedTerminal = batch.done;
          throw error;
        }
        for (final file in batch.files) {
          if (!paths.add(file.relativePath)) {
            throw StateError('Duplicate file in search stream');
          }
          count += file.matches.length;
          files.add(file);
        }
        if (batch.totalMatches != count ||
            count > _workspaceSearchMaxResults ||
            (truncated && !batch.truncated)) {
          throw StateError('Invalid search stream result count');
        }
        truncated = batch.truncated;
        final result = native.WorkspaceSearchResult(
          files: List<native.WorkspaceSearchFileResult>.unmodifiable(files),
          totalMatches: count,
          truncated: truncated,
        );
        if (batch.done) {
          receivedTerminal = true;
          return result;
        }
        // Keep loading=true until the authoritative final count/truncation
        // arrives. Partial results are navigable but never replaceable.
        if (batch.files.isNotEmpty) {
          state = state.copyWith(result: result, error: null);
        }
      }
      if (generation != _requestGeneration) return null;
      throw StateError('Search stream ended before completion');
    } finally {
      if (identical(_activeSearchIterator, iterator)) {
        _activeSearchIterator = null;
      }
      // Subscription cleanup must not hold the visible terminal state
      // hostage to a slow transport. Explicitly cancel native work if this
      // active stream ended malformed or without a terminal batch.
      unawaited(_cancelSearchIterator(iterator));
      if (!receivedTerminal &&
          generation == _requestGeneration &&
          _activeRequestId == requestId) {
        unawaited(_ignoreCancellationFailure(requestId));
      }
    }
  }

  Future<void> _cancelSearchIterator(
    StreamIterator<native.WorkspaceSearchBatch> iterator,
  ) async {
    try {
      await iterator.cancel();
    } catch (_) {
      // Native cancellation and generation guards are independent of the
      // transport's best-effort subscription cleanup.
    }
  }

  Future<native.WorkspaceReplaceResult> replaceMatches({
    required String workspacePath,
    required Iterable<String> matchIds,
    required EditorSessionRegistry editorSessions,
  }) async {
    if (state.loading) {
      const message = 'Wait until search completes before replacing.';
      state = state.copyWith(error: message);
      throw StateError(message);
    }
    final result = state.result;
    if (result == null) {
      const message = 'Run search before replacing.';
      state = state.copyWith(error: message);
      throw StateError(message);
    }
    final selectedMatchIds = matchIds.toSet();
    if (selectedMatchIds.isEmpty && result.truncated) {
      const message = 'Replace all is unavailable while results are truncated.';
      state = state.copyWith(error: message);
      throw StateError(message);
    }
    final affectedFiles = _affectedFiles(selectedMatchIds);
    final dirtyPaths = editorSessions.dirtyPathsFor(
      workspacePath: workspacePath,
      relativePaths: affectedFiles.map((file) => file.relativePath),
    );
    if (dirtyPaths.isNotEmpty) {
      state = state.copyWith(error: _dirtyMessage(dirtyPaths));
      throw WorkspaceSearchDirtyFilesException(dirtyPaths);
    }
    state = state.copyWith(replacing: true, error: null);
    try {
      final result = await _service.replaceMatches(
        request: native.WorkspaceReplaceRequest(
          options: native.WorkspaceReplaceOptions(
            search: _searchOptions(workspacePath),
            replacement: state.replacement,
            preserveCase: state.preserveCase,
          ),
          matchIds: selectedMatchIds.toList(growable: false),
          expectedFiles: <native.WorkspaceReplaceFileExpectation>[
            for (final file in affectedFiles)
              native.WorkspaceReplaceFileExpectation(
                relativePath: file.relativePath,
                contentToken: file.contentToken,
              ),
          ],
        ),
      );
      await editorSessions.reloadCleanFiles(
        workspacePath: workspacePath,
        relativePaths: affectedFiles.map((file) => file.relativePath),
      );
      state = state.copyWith(replacing: false);
      await searchNow(workspacePath);
      return result;
    } catch (error) {
      state = state.copyWith(replacing: false, error: _messageFor(error));
      rethrow;
    }
  }

  WorkspaceSearchService get _service => _searchService!;

  native.WorkspaceSearchOptions _searchOptions(String workspacePath) {
    return native.WorkspaceSearchOptions(
      workspacePath: workspacePath,
      query: state.query,
      caseSensitive: state.caseSensitive,
      wholeWord: state.wholeWord,
      useRegex: state.useRegex,
      includePattern: _nullablePattern(state.includePattern),
      excludePattern: _nullablePattern(state.excludePattern),
      includeIgnored: state.includeIgnored,
      maxResults: _workspaceSearchMaxResults,
    );
  }

  String? _nullablePattern(String value) {
    final trimmed = value.trim();
    return trimmed.isEmpty ? null : trimmed;
  }

  void _scheduleSearch(String workspacePath) {
    _debounce?.cancel();
    if (!state.hasQuery) {
      state = state.copyWith(loading: false, error: null, result: null);
      return;
    }
    // A sufficiently selective plain query can start sooner. Keep the
    // original delay for broad, expensive or recently slow searches.
    final fastLiteral =
        !state.useRegex &&
        !state.wholeWord &&
        !state.hasReplacement &&
        state.query.runes.length >= 3 &&
        (_lastSearchDuration ?? Duration.zero) <= _workspaceSlowSearchThreshold;
    _debounce = Timer(
      fastLiteral ? _workspaceLiteralDebounce : _workspaceSearchDebounce,
      () => unawaited(searchNow(workspacePath)),
    );
  }

  void _applySearchInputChange(
    String workspacePath,
    WorkspaceSearchState next,
  ) {
    _cancelActiveSearch();
    _requestGeneration += 1;
    state = next.copyWith(
      loading: next.hasQuery,
      error: null,
      result: null,
      collapsedResultNodeKeys: const <String>{},
    );
    _scheduleSearch(workspacePath);
  }

  void _cancelActiveSearch() {
    final iterator = _activeSearchIterator;
    _activeSearchIterator = null;
    if (iterator != null) {
      unawaited(_cancelSearchIterator(iterator));
    }
    final requestId = _activeRequestId;
    if (requestId == null) {
      return;
    }
    _activeRequestId = null;
    unawaited(_ignoreCancellationFailure(requestId));
  }

  Future<void> _ignoreCancellationFailure(String requestId) async {
    try {
      await _service.cancel(requestId: requestId);
    } on Object {
      // Cancellation is best effort because stale generations are also ignored.
    }
  }

  List<native.WorkspaceSearchFileResult> _affectedFiles(Set<String> matchIds) {
    final result = state.result;
    if (result == null) {
      return const <native.WorkspaceSearchFileResult>[];
    }
    if (matchIds.isEmpty) {
      return result.files;
    }
    return <native.WorkspaceSearchFileResult>[
      for (final file in result.files)
        if (file.matches.any((match) => matchIds.contains(match.id))) file,
    ];
  }

  WorkspaceSearchProjection? _projectionForCurrentResult() {
    final result = state.result;
    if (result == null) {
      _clearResultProjection();
      return null;
    }
    if (!identical(_projectedResult, result)) {
      _projectedResult = result;
      _resultProjection = WorkspaceSearchProjection(result);
    }
    return _resultProjection;
  }

  void _clearResultProjection() {
    _projectedResult = null;
    _resultProjection = null;
  }

  String _messageFor(Object error) {
    if (error is native.WorkspaceSearchError) {
      return error.context;
    }
    if (error is WorkspaceSearchDirtyFilesException) {
      return _dirtyMessage(error.paths);
    }
    return 'Search failed: $error';
  }

  String _dirtyMessage(List<String> paths) {
    if (paths.length == 1) {
      return 'Save or discard ${paths.single} before replacing.';
    }
    return 'Save or discard ${paths.length} open files before replacing.';
  }
}
