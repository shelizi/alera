import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_semantic_adapter_factory.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_semantic_provider_adapter.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'shared server syncs each document with its own language id and version',
    () async {
      final transport = _FakeSemanticTransport();
      final adapter = await _adapter(transport);

      await adapter.openDocument(
        language: LanguageId('typescript'),
        path: r'C:\repo\src\main.ts',
        text: 'const value = 1;',
      );
      await adapter.openDocument(
        language: LanguageId('javascript'),
        path: r'C:\repo\src\legacy.js',
        text: 'const value = 2;',
      );
      await adapter.replaceDocument(
        path: r'C:\repo\src\main.ts',
        text: 'const value = 3;',
      );
      await adapter.saveDocument(path: r'C:\repo\src\main.ts');
      await adapter.closeDocument(path: r'C:\repo\src\legacy.js');

      final opens = transport.notifications
          .where((item) => item.method == 'textDocument/didOpen')
          .toList();
      expect(opens, hasLength(2));
      expect(
        (opens[0].params['textDocument'] as Map)['languageId'],
        'typescript',
      );
      expect(
        (opens[1].params['textDocument'] as Map)['languageId'],
        'javascript',
      );
      expect((opens[0].params['textDocument'] as Map)['version'], 1);

      final change = transport.notifications.firstWhere(
        (item) => item.method == 'textDocument/didChange',
      );
      expect((change.params['textDocument'] as Map)['version'], 2);
      expect(
        ((change.params['contentChanges'] as List).single as Map)['text'],
        'const value = 3;',
      );
      expect(
        transport.notifications.map((item) => item.method),
        containsAll(<String>['textDocument/didSave', 'textDocument/didClose']),
      );
    },
  );

  test('definition converts scalar cursor to UTF-16 and all targets back to scalar', () async {
    final transport = _FakeSemanticTransport();
    final adapter = await _adapter(
      transport,
      sourceTextReader: (path) async {
        if (path.toLowerCase().endsWith(r'\target.ts')) {
          return '😀target();\n';
        }
        if (path.toLowerCase().endsWith(r'\other.ts')) {
          return 'let 😀other = 1;\n';
        }
        throw StateError('unexpected source read: $path');
      },
    );
    await adapter.openDocument(
      language: LanguageId('typescript'),
      path: r'C:\repo\src\main.ts',
      text: 'a😀target();\n',
    );
    transport.nextResult = <Object?>[
      <String, Object?>{
        'uri': 'file:///C:/repo/src/target.ts',
        'range': <String, Object?>{
          'start': <String, int>{'line': 0, 'character': 2},
          'end': <String, int>{'line': 0, 'character': 8},
        },
      },
      <String, Object?>{
        'targetUri': 'file:///C:/repo/src/other.ts',
        'targetSelectionRange': <String, Object?>{
          'start': <String, int>{'line': 0, 'character': 6},
          'end': <String, int>{'line': 0, 'character': 11},
        },
        'targetRange': <String, Object?>{
          'start': <String, int>{'line': 0, 'character': 0},
          'end': <String, int>{'line': 0, 'character': 14},
        },
      },
    ];

    final locations = await adapter.definition(
      path: r'C:\repo\src\main.ts',
      position: const SourcePosition(line: 0, scalarColumn: 2),
    );

    final request = transport.requests.single;
    expect(request.method, 'textDocument/definition');
    expect((request.params['position'] as Map)['character'], 3);
    expect(locations, hasLength(2));
    expect(locations[0].range.start.scalarColumn, 1);
    expect(locations[0].range.end.scalarColumn, 7);
    expect(locations[1].range.start.scalarColumn, 5);
    expect(locations[1].range.end.scalarColumn, 10);
  });

  test(
    'references include declaration context and ignore non-file locations',
    () async {
      final transport = _FakeSemanticTransport();
      final adapter = await _adapter(transport);
      await adapter.openDocument(
        language: LanguageId('rust'),
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}\n',
      );
      transport.nextResult = <Object?>[
        <String, Object?>{
          'uri': 'file:///C:/repo/src/main.rs',
          'range': <String, Object?>{
            'start': <String, int>{'line': 0, 'character': 3},
            'end': <String, int>{'line': 0, 'character': 7},
          },
        },
        <String, Object?>{
          'uri': 'rust-analyzer://metadata/core',
          'range': <String, Object?>{
            'start': <String, int>{'line': 0, 'character': 0},
            'end': <String, int>{'line': 0, 'character': 1},
          },
        },
      ];

      final locations = await adapter.references(
        path: r'C:\repo\src\main.rs',
        position: const SourcePosition(line: 0, scalarColumn: 3),
        includeDeclaration: false,
      );

      expect(locations, hasLength(1));
      expect(locations.single.range.start.scalarColumn, 3);
      expect(
        (transport.requests.single.params['context']
            as Map)['includeDeclaration'],
        isFalse,
      );
    },
  );

  test('navigation methods use provider-neutral LSP methods without CodeForge helpers', () async {
    final transport = _FakeSemanticTransport()..nextResult = <Object?>[];
    final adapter = await _adapter(transport);
    await adapter.openDocument(
      language: LanguageId('csharp'),
      path: r'C:\repo\Program.cs',
      text: 'class Program {}\n',
    );
    const position = SourcePosition(line: 0, scalarColumn: 1);

    await adapter.declaration(path: r'C:\repo\Program.cs', position: position);
    await adapter.typeDefinition(
      path: r'C:\repo\Program.cs',
      position: position,
    );
    await adapter.implementation(
      path: r'C:\repo\Program.cs',
      position: position,
    );

    expect(transport.requests.map((request) => request.method), <String>[
      'textDocument/declaration',
      'textDocument/typeDefinition',
      'textDocument/implementation',
    ]);
  });

  test(
    'production semantic factory binds both narrow ports to one adapter',
    () async {
      final transport = _FakeSemanticTransport();
      final runtime = CodeForgeLanguageServerRuntime(
        transportFactory: (_) async => transport,
      );
      final provider = LanguageProviderDescriptor(
        id: 'rust-semantic',
        kind: LanguageProviderKind.semanticServer,
        languages: <LanguageId>{LanguageId('rust')},
        capabilities: const <LanguageCapability>{LanguageCapability.definition},
        processScope: LanguageProviderProcessScope.workspace,
        launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
        executableResolutionPolicy:
            LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
        executableCandidates: const <String>['test-server'],
      );
      final session = await runtime.start(
        LanguageServerRuntimeStartRequest(
          provider: provider,
          executable: 'test-server',
          workspaceRoot: r'C:\repo',
          target: LanguageServerTarget.localWorkspace,
        ),
      );

      final binding = const CodeForgeSemanticAdapterFactory(isWindows: true)
          .create(
            workspaceId: 'workspace-a',
            provider: provider,
            session: session,
          );

      expect(binding.documents, isA<CodeForgeSemanticProviderAdapter>());
      expect(binding.navigation, same(binding.documents));
    },
  );
}

Future<CodeForgeSemanticProviderAdapter> _adapter(
  _FakeSemanticTransport transport, {
  CodeForgeSourceTextReader? sourceTextReader,
}) async {
  final runtime = CodeForgeLanguageServerRuntime(
    transportFactory: (_) async => transport,
  );
  final provider = LanguageProviderDescriptor(
    id: 'shared-semantic',
    kind: LanguageProviderKind.semanticServer,
    languages: <LanguageId>{
      LanguageId('typescript'),
      LanguageId('javascript'),
      LanguageId('rust'),
      LanguageId('csharp'),
    },
    capabilities: const <LanguageCapability>{
      LanguageCapability.definition,
      LanguageCapability.declaration,
      LanguageCapability.typeDefinition,
      LanguageCapability.implementation,
      LanguageCapability.references,
    },
    processScope: LanguageProviderProcessScope.sharedWorkspaceFamily,
    launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
    executableResolutionPolicy:
        LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
    executableCandidates: const <String>['test-server'],
  );
  final session = await runtime.start(
    LanguageServerRuntimeStartRequest(
      provider: provider,
      executable: 'test-server',
      workspaceRoot: r'C:\repo',
      target: LanguageServerTarget.localWorkspace,
    ),
  );
  return CodeForgeSemanticProviderAdapter(
    workspaceId: 'workspace-a',
    session: session as CodeForgeLanguageServerSession,
    sourceTextReader: sourceTextReader,
    isWindows: true,
  );
}

final class _RecordedMessage {
  const _RecordedMessage(this.method, this.params);

  final String method;
  final Map<String, dynamic> params;
}

final class _FakeSemanticTransport implements CodeForgeLanguageServerTransport {
  final List<_RecordedMessage> notifications = <_RecordedMessage>[];
  final List<_RecordedMessage> requests = <_RecordedMessage>[];
  final Completer<int> _exitCode = Completer<int>();
  Object? nextResult;

  @override
  Future<int> get processExitCode => _exitCode.future;

  @override
  Stream<Map<String, dynamic>> get responses => const Stream.empty();

  @override
  Future<void> initialize() async {}

  @override
  Future<void> shutdown() async {}

  @override
  Future<void> exitServer() async {}

  @override
  void dispose() {
    if (!_exitCode.isCompleted) _exitCode.complete(0);
  }

  @override
  Future<void> sendNotification({
    required String method,
    required Map<String, dynamic> params,
  }) async {
    notifications.add(_RecordedMessage(method, params));
  }

  @override
  Future<Map<String, dynamic>> sendRequest({
    required String method,
    required Map<String, dynamic> params,
  }) async {
    requests.add(_RecordedMessage(method, params));
    return <String, dynamic>{'result': nextResult};
  }
}
