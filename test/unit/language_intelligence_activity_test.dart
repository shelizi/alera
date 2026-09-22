import 'package:alera/src/features/language_intelligence/application/language_intelligence_activity.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_server_session_state.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('aggregates download, server, and parser activity', () {
    final store = LanguageIntelligenceActivityStore();
    addTearDown(store.dispose);
    final rust = LanguageId('rust');

    store.reportManagedServerAcquisition(
      const ManagedLanguageServerAcquisitionSnapshot(
        providerId: 'rust.rust-analyzer',
        state: ManagedLanguageServerAcquisitionState.installing,
        version: '1.2.3',
        detail: 'Installing',
      ),
    );
    store.reportServerSession(
      const LanguageServerSessionSnapshot(
        workspaceId: 'workspace-a',
        providerId: 'rust.rust-analyzer',
        state: LanguageServerSessionState.ready,
        activeDocumentCount: 2,
        generation: 1,
        restartAttempts: 0,
        executable: 'rust-analyzer',
      ),
    );
    store.reportServerSession(
      const LanguageServerSessionSnapshot(
        workspaceId: 'workspace-b',
        providerId: 'rust.rust-analyzer',
        state: LanguageServerSessionState.starting,
        activeDocumentCount: 1,
        generation: 2,
        restartAttempts: 1,
      ),
    );
    store.reportStructuralParserDocument(
      StructuralParserDocumentSnapshot(
        language: rust,
        documentId: 'main.rs',
        state: StructuralParserActivityState.ready,
        revision: 4,
      ),
    );
    store.reportStructuralParserDocument(
      StructuralParserDocumentSnapshot(
        language: rust,
        documentId: 'lib.rs',
        state: StructuralParserActivityState.parsing,
        revision: 7,
      ),
    );

    final snapshot = store.snapshot;
    expect(
      snapshot.acquisitionFor('rust.rust-analyzer')?.state,
      ManagedLanguageServerAcquisitionState.installing,
    );

    final server = snapshot.serverFor('rust.rust-analyzer');
    expect(server, isNotNull);
    expect(server!.state, LanguageServerSessionState.starting);
    expect(server.sessionCount, 2);
    expect(server.activeDocumentCount, 3);
    expect(server.restartAttempts, 1);
    expect(server.executable, 'rust-analyzer');

    final parser = snapshot.parserFor(rust);
    expect(parser, isNotNull);
    expect(parser!.state, StructuralParserActivityState.parsing);
    expect(parser.activeDocumentCount, 2);
    expect(parser.revision, 7);
  });

  test('publishes changes and removes closed parser documents', () async {
    final store = LanguageIntelligenceActivityStore();
    addTearDown(store.dispose);
    final python = LanguageId('python');
    final emitted = <LanguageIntelligenceActivitySnapshot>[];
    final subscription = store.changes.listen(emitted.add);
    addTearDown(subscription.cancel);

    store.reportStructuralParserDocument(
      StructuralParserDocumentSnapshot(
        language: python,
        documentId: 'main.py',
        state: StructuralParserActivityState.parsing,
        revision: 1,
      ),
    );
    store.reportStructuralParserDocument(
      StructuralParserDocumentSnapshot(
        language: python,
        documentId: 'main.py',
        state: StructuralParserActivityState.ready,
        revision: 1,
      ),
    );
    store.removeStructuralParserDocument(
      language: python,
      documentId: 'main.py',
    );

    expect(emitted, hasLength(3));
    expect(
      emitted[0].parserFor(python)?.state,
      StructuralParserActivityState.parsing,
    );
    expect(
      emitted[1].parserFor(python)?.state,
      StructuralParserActivityState.ready,
    );
    expect(emitted[2].parserFor(python), isNull);
  });
}
