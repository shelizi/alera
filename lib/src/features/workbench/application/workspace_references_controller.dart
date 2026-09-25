import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_intelligence_manager.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_providers.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/workbench/application/retired_workspace_invalidation.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:path/path.dart' as p;
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workspace_references_controller.g.dart';

class const WorkspaceReferenceMatch({
  required final String relativePath,
  required final int line,
  required final int column,
  required final int matchLength,
  required final String linePreview,
});

class const WorkspaceReferenceFileGroup({
  required final String relativePath,
  required final List<WorkspaceReferenceMatch> matches,
});

class const WorkspaceReferencesResult({
  required final List<WorkspaceReferenceFileGroup> files,
  required final String? providerId,
  final bool truncated = false,
});

class const WorkspaceReferencesState({
  final bool loading = false,
  final String? error,
  final WorkspaceReferencesResult? result,
});

const Object _workspaceReferencesSentinel = Object();

extension on WorkspaceReferencesState {
  WorkspaceReferencesState copyWith({
    bool? loading,
    Object? error = _workspaceReferencesSentinel,
    Object? result = _workspaceReferencesSentinel,
  }) => WorkspaceReferencesState(
    loading: loading ?? this.loading,
    error: identical(error, _workspaceReferencesSentinel)
        ? this.error
        : error as String?,
    result: identical(result, _workspaceReferencesSentinel)
        ? this.result
        : result as WorkspaceReferencesResult?,
  );
}

class const WorkspaceReferencesQueryResult({
  required final List<SourceLocation> locations,
  required final String? providerId,
  final bool truncated = false,
});

abstract interface class WorkspaceReferencesQueryPort {
  Future<WorkspaceReferencesQueryResult> references({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  });
}

final class _LanguageIntelligenceWorkspaceReferencesQuery
    implements WorkspaceReferencesQueryPort {
  const _LanguageIntelligenceWorkspaceReferencesQuery(this._manager);

  final LanguageIntelligenceManager _manager;

  @override
  Future<WorkspaceReferencesQueryResult> references({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) async {
    final providerId = _manager.providerIdFor(
      workspaceId: workspaceId,
      path: path,
      capability: LanguageCapability.references,
    );
    final locations = await _manager.references(
      workspaceId: workspaceId,
      path: path,
      position: position,
    );
    return WorkspaceReferencesQueryResult(
      locations: locations,
      providerId: providerId,
    );
  }
}

@Riverpod(keepAlive: true)
WorkspaceReferencesQueryPort workspaceReferencesQuery(Ref ref) =>
    _LanguageIntelligenceWorkspaceReferencesQuery(
      ref.watch(languageIntelligenceManagerProvider),
    );

@Riverpod(keepAlive: true)
class WorkspaceReferencesController extends _$WorkspaceReferencesController {
  int _generation = 0;

  @override
  WorkspaceReferencesState build(String workspaceId) {
    invalidateWhenWorkspaceRetired(ref, workspaceId);
    ref.onDispose(() => _generation += 1);
    return const WorkspaceReferencesState();
  }

  Future<void> search({
    required String workspacePath,
    required String sourcePath,
    required SourcePosition position,
  }) async {
    final generation = ++_generation;
    state = state.copyWith(loading: true, error: null, result: null);
    try {
      final query = await ref
          .read(workspaceReferencesQueryProvider)
          .references(
            workspaceId: workspaceId,
            path: sourcePath,
            position: position,
          );
      if (generation != _generation) {
        return;
      }
      final result = await _projectResult(
        workspacePath: workspacePath,
        query: query,
        generation: generation,
      );
      if (generation != _generation) {
        return;
      }
      state = state.copyWith(loading: false, result: result, error: null);
    } catch (error) {
      if (generation != _generation) {
        return;
      }
      state = state.copyWith(
        loading: false,
        result: null,
        error: error is TimeoutException
            ? 'Language server did not respond'
            : error.toString(),
      );
    }
  }

  void clear() {
    _generation += 1;
    state = const WorkspaceReferencesState();
  }

  Future<WorkspaceReferencesResult> _projectResult({
    required String workspacePath,
    required WorkspaceReferencesQueryResult query,
    required int generation,
  }) async {
    final root = p.normalize(p.absolute(workspacePath));
    final locationsByPath = <String, List<SourceLocation>>{};
    for (final location in query.locations) {
      if (location.workspaceId != workspaceId) {
        continue;
      }
      final absolutePath = p.normalize(
        p.isAbsolute(location.path)
            ? location.path
            : p.join(root, location.path),
      );
      final relativePath = workspaceRelativePath(
        workspacePath: root,
        filePath: absolutePath,
      );
      if (relativePath == null) {
        continue;
      }
      locationsByPath
          .putIfAbsent(relativePath, () => <SourceLocation>[])
          .add(location);
    }

    final files = <WorkspaceReferenceFileGroup>[];
    final fileService = ref.read(workspaceFileServiceProvider);
    for (final entry in locationsByPath.entries) {
      if (generation != _generation) {
        return WorkspaceReferencesResult(
          files: files,
          providerId: query.providerId,
          truncated: query.truncated,
        );
      }
      String? content;
      try {
        content = (await fileService.readTextFile(
          workspacePath: workspacePath,
          relativePath: entry.key,
        )).content;
      } catch (_) {
        content = null;
      }
      final lines = content?.split(RegExp(r'\r\n|\r|\n'));
      final matches = <WorkspaceReferenceMatch>[
        for (final location in entry.value)
          _matchFor(relativePath: entry.key, location: location, lines: lines),
      ];
      files.add(
        WorkspaceReferenceFileGroup(relativePath: entry.key, matches: matches),
      );
    }
    return WorkspaceReferencesResult(
      files: files,
      providerId: query.providerId,
      truncated: query.truncated,
    );
  }

  WorkspaceReferenceMatch _matchFor({
    required String relativePath,
    required SourceLocation location,
    required List<String>? lines,
  }) {
    final start = location.range.start;
    final end = location.range.end;
    final matchLength = start.line == end.line
        ? (end.scalarColumn - start.scalarColumn).clamp(1, 1 << 30)
        : 1;
    final preview =
        lines != null && start.line >= 0 && start.line < lines.length
        ? lines[start.line].trim()
        : '';
    return WorkspaceReferenceMatch(
      relativePath: relativePath,
      line: start.line + 1,
      column: start.scalarColumn + 1,
      matchLength: matchLength,
      linePreview: preview,
    );
  }
}
