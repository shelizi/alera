import 'dart:async';

import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/workspace_references_controller.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('groups references by file, reads one preview snapshot per file, and filters outside workspace', () async {
    final root = p.join('C:', 'repo');
    final query = _FakeWorkspaceReferencesQueryPort(
      results: <WorkspaceReferencesQueryResult>[
        WorkspaceReferencesQueryResult(
          providerId: 'rust.rust-analyzer',
          locations: <SourceLocation>[
            SourceLocation(
              workspaceId: 'workspace-a',
              path: p.join(root, 'src', 'main.rs'),
              range: const SourceRange(
                start: SourcePosition(line: 1, scalarColumn: 4),
                end: SourcePosition(line: 1, scalarColumn: 8),
              ),
            ),
            SourceLocation(
              workspaceId: 'workspace-a',
              path: p.join(root, 'src', 'main.rs'),
              range: const SourceRange(
                start: SourcePosition(line: 2, scalarColumn: 2),
                end: SourcePosition(line: 2, scalarColumn: 5),
              ),
            ),
            SourceLocation(
              workspaceId: 'workspace-a',
              path: p.join(root, 'src', 'lib.rs'),
              range: const SourceRange(
                start: SourcePosition(line: 0, scalarColumn: 0),
                end: SourcePosition(line: 0, scalarColumn: 3),
              ),
            ),
            SourceLocation(
              workspaceId: 'workspace-a',
              path: p.join('C:', 'sdk', 'outside.rs'),
              range: const SourceRange(
                start: SourcePosition(line: 0, scalarColumn: 0),
                end: SourcePosition(line: 0, scalarColumn: 1),
              ),
            ),
            SourceLocation(
              workspaceId: 'workspace-b',
              path: p.join(root, 'src', 'other.rs'),
              range: const SourceRange(
                start: SourcePosition(line: 0, scalarColumn: 0),
                end: SourcePosition(line: 0, scalarColumn: 1),
              ),
            ),
          ],
        ),
      ],
    );
    final files = _FakeWorkspaceFileService(<String, String>{
      p.join('src', 'main.rs'):
          'fn main() {}\n    call_target();\n  target();\n',
      p.join('src', 'lib.rs'): 'lib\n',
    });
    final container = ProviderContainer(
      overrides: [
        workspaceReferencesQueryProvider.overrideWithValue(query),
        workspaceFileServiceProvider.overrideWithValue(files),
      ],
    );
    addTearDown(container.dispose);

    final provider = workspaceReferencesControllerProvider('workspace-a');
    await container
        .read(provider.notifier)
        .search(
          workspacePath: root,
          sourcePath: p.join(root, 'src', 'main.rs'),
          position: const SourcePosition(line: 1, scalarColumn: 4),
        );

    final state = container.read(provider);
    expect(state.error, isNull);
    expect(state.loading, isFalse);
    expect(state.result?.providerId, 'rust.rust-analyzer');
    expect(state.result?.files, hasLength(2));
    expect(
      state.result?.files.first.matches.map((match) => match.linePreview),
      <String>['call_target();', 'target();'],
    );
    expect(state.result?.files.first.matches.first.line, 2);
    expect(state.result?.files.first.matches.first.column, 5);
    expect(state.result?.files.first.matches.first.matchLength, 4);
    expect(files.readPaths, <String>[
      p.join('src', 'main.rs'),
      p.join('src', 'lib.rs'),
    ]);
  });

  test('newer references query suppresses stale older completion', () async {
    final first = Completer<WorkspaceReferencesQueryResult>();
    final query = _ControlledWorkspaceReferencesQueryPort(
      first: first,
      second: const WorkspaceReferencesQueryResult(
        providerId: 'new-provider',
        locations: <SourceLocation>[],
      ),
    );
    final container = ProviderContainer(
      overrides: [
        workspaceReferencesQueryProvider.overrideWithValue(query),
        workspaceFileServiceProvider.overrideWithValue(
          _FakeWorkspaceFileService(const <String, String>{}),
        ),
      ],
    );
    addTearDown(container.dispose);
    final provider = workspaceReferencesControllerProvider('workspace-a');
    final controller = container.read(provider.notifier);

    final older = controller.search(
      workspacePath: p.join('C:', 'repo'),
      sourcePath: p.join('C:', 'repo', 'old.rs'),
      position: const SourcePosition(line: 0, scalarColumn: 0),
    );
    await Future<void>.delayed(Duration.zero);
    await controller.search(
      workspacePath: p.join('C:', 'repo'),
      sourcePath: p.join('C:', 'repo', 'new.rs'),
      position: const SourcePosition(line: 0, scalarColumn: 0),
    );
    first.complete(
      const WorkspaceReferencesQueryResult(
        providerId: 'stale-provider',
        locations: <SourceLocation>[],
      ),
    );
    await older;

    expect(container.read(provider).result?.providerId, 'new-provider');
  });
}

final class _FakeWorkspaceReferencesQueryPort
    implements WorkspaceReferencesQueryPort {
  _FakeWorkspaceReferencesQueryPort({required this.results});

  final List<WorkspaceReferencesQueryResult> results;
  int calls = 0;

  @override
  Future<WorkspaceReferencesQueryResult> references({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) async => results[calls++];
}

final class _ControlledWorkspaceReferencesQueryPort
    implements WorkspaceReferencesQueryPort {
  _ControlledWorkspaceReferencesQueryPort({
    required this.first,
    required this.second,
  });

  final Completer<WorkspaceReferencesQueryResult> first;
  final WorkspaceReferencesQueryResult second;
  int calls = 0;

  @override
  Future<WorkspaceReferencesQueryResult> references({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) {
    calls += 1;
    return calls == 1 ? first.future : Future.value(second);
  }
}

final class _FakeWorkspaceFileService extends WorkspaceFileService {
  _FakeWorkspaceFileService(this.contents);

  final Map<String, String> contents;
  final List<String> readPaths = <String>[];

  @override
  Future<native.WorkspaceTextFile> readTextFile({
    required String workspacePath,
    required String relativePath,
  }) async {
    readPaths.add(relativePath);
    final content = contents[relativePath];
    if (content == null) {
      throw StateError('Missing fake file: $relativePath');
    }
    return native.WorkspaceTextFile(
      content: content,
      contentToken: 'token:$relativePath',
      modifiedMillis: 0,
      size: .from(content.length),
    );
  }
}
