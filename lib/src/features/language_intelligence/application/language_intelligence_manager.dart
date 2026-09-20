import '../domain/language_capability.dart';
import '../domain/language_id.dart';
import '../domain/language_intelligence_settings.dart';
import '../domain/language_server_session_state.dart';
import '../domain/source_location.dart';
import 'language_navigation_port.dart';
import 'language_provider_registry.dart';
import 'language_semantic_adapter_factory.dart';
import 'language_server_runtime.dart';
import 'language_server_session_manager.dart';

final class LanguageIntelligenceDocumentState {
  const LanguageIntelligenceDocumentState({
    required this.language,
    required this.providerId,
    required this.session,
  });

  const LanguageIntelligenceDocumentState.unsupported()
    : language = null,
      providerId = null,
      session = null;

  final LanguageId? language;
  final String? providerId;
  final LanguageServerSessionSnapshot? session;

  bool get semanticReady => session?.state == LanguageServerSessionState.ready;
}

final class LanguageIntelligenceManager {
  factory LanguageIntelligenceManager({
    required LanguageExtensionRegistry registry,
    required LanguageServerSessionManager sessions,
    required LanguageSemanticAdapterFactory semanticAdapterFactory,
  }) => LanguageIntelligenceManager._(
    registry: registry,
    sessions: sessions,
    semanticAdapterFactory: semanticAdapterFactory,
  );

  LanguageIntelligenceManager._({
    required this._registry,
    required this._sessions,
    required this._semanticAdapterFactory,
  });

  final LanguageExtensionRegistry _registry;
  final LanguageServerSessionManager _sessions;
  final LanguageSemanticAdapterFactory _semanticAdapterFactory;
  final Map<_DocumentKey, _TrackedDocument> _documents =
      <_DocumentKey, _TrackedDocument>{};
  final Map<_BindingKey, _SemanticBindingCache> _bindings =
      <_BindingKey, _SemanticBindingCache>{};

  Future<LanguageIntelligenceDocumentState> openDocument({
    required String workspaceId,
    required String workspaceRoot,
    required String path,
    required String text,
    required LanguageIntelligenceSettings settings,
    required LanguageServerTarget target,
    Map<String, String> environment = const <String, String>{},
  }) async {
    final language = _registry.languageForPath(path)?.id;
    if (language == null) {
      return const LanguageIntelligenceDocumentState.unsupported();
    }

    final activation = settings.forLanguage(language);
    final semanticProvider = _registry.semanticProviderFor(
      language,
      preferredProviderId: activation.semanticProviderId,
    );
    final providerId = semanticProvider?.id;
    final key = _DocumentKey(workspaceId, path);
    final previous = _documents[key];
    if (!activation.enabled || providerId == null) {
      if (previous != null) {
        _documents.remove(key);
        await _detachTrackedDocument(previous, stopWhenUnused: true);
      }
      final snapshot = await _sessions.attachDocument(
        workspaceId: workspaceId,
        workspaceRoot: workspaceRoot,
        language: language,
        documentId: path,
        settings: settings,
        target: target,
        environment: environment,
      );
      return LanguageIntelligenceDocumentState(
        language: language,
        providerId: providerId,
        session: snapshot,
      );
    }

    if (previous != null && previous.providerId != providerId) {
      _documents.remove(key);
      await _detachTrackedDocument(previous, stopWhenUnused: true);
    }
    final snapshot = await _sessions.attachDocument(
      workspaceId: workspaceId,
      workspaceRoot: workspaceRoot,
      language: language,
      documentId: path,
      settings: settings,
      target: target,
      environment: environment,
    );
    final retained = _documents[key];
    final _TrackedDocument document;
    if (retained != null && retained.providerId == providerId) {
      retained
        ..text = text
        ..revision += 1;
      document = retained;
    } else {
      document = _TrackedDocument(
        key: key,
        path: path,
        language: language,
        providerId: providerId,
        text: text,
      );
      _documents[key] = document;
    }

    final binding = await _ensureBinding(document);
    if (binding != null) {
      await _syncDocument(binding, document);
    }
    return LanguageIntelligenceDocumentState(
      language: language,
      providerId: providerId,
      session: snapshot,
    );
  }

  Future<bool> replaceDocument({
    required String workspaceId,
    required String path,
    required String text,
  }) async {
    final document = _documents[_DocumentKey(workspaceId, path)];
    if (document == null) {
      return false;
    }
    document
      ..text = text
      ..revision += 1;
    final binding = await _ensureBinding(document);
    if (binding == null) {
      return false;
    }
    await _syncDocument(binding, document);
    return true;
  }

  Future<bool> saveDocument({
    required String workspaceId,
    required String path,
  }) async {
    final document = _documents[_DocumentKey(workspaceId, path)];
    if (document == null) {
      return false;
    }
    final binding = await _ensureBinding(document);
    if (binding == null) {
      return false;
    }
    await _syncDocument(binding, document);
    await binding.binding.documents.saveDocument(path: document.path);
    return true;
  }

  Future<void> closeDocument({
    required String workspaceId,
    required String path,
  }) async {
    final document = _documents.remove(_DocumentKey(workspaceId, path));
    if (document == null) {
      return;
    }
    await _detachTrackedDocument(document);
  }

  Future<List<SourceLocation>> definition({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) => _navigate(
    workspaceId: workspaceId,
    path: path,
    capability: LanguageCapability.definition,
    invoke: (binding) => binding.definition(path: path, position: position),
  );

  Future<List<SourceLocation>> declaration({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) => _navigate(
    workspaceId: workspaceId,
    path: path,
    capability: LanguageCapability.declaration,
    invoke: (binding) => binding.declaration(path: path, position: position),
  );

  Future<List<SourceLocation>> typeDefinition({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) => _navigate(
    workspaceId: workspaceId,
    path: path,
    capability: LanguageCapability.typeDefinition,
    invoke: (binding) => binding.typeDefinition(path: path, position: position),
  );

  Future<List<SourceLocation>> implementation({
    required String workspaceId,
    required String path,
    required SourcePosition position,
  }) => _navigate(
    workspaceId: workspaceId,
    path: path,
    capability: LanguageCapability.implementation,
    invoke: (binding) => binding.implementation(path: path, position: position),
  );

  Future<List<SourceLocation>> references({
    required String workspaceId,
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  }) => _navigate(
    workspaceId: workspaceId,
    path: path,
    capability: LanguageCapability.references,
    invoke: (binding) => binding.references(
      path: path,
      position: position,
      includeDeclaration: includeDeclaration,
    ),
  );

  Future<List<SourceLocation>> _navigate({
    required String workspaceId,
    required String path,
    required LanguageCapability capability,
    required Future<List<SourceLocation>> Function(
      LanguageNavigationPort navigation,
    )
    invoke,
  }) async {
    final document = _documents[_DocumentKey(workspaceId, path)];
    if (document == null) {
      return const <SourceLocation>[];
    }
    final provider = _registry.provider(document.providerId);
    if (provider == null || !provider.capabilities.contains(capability)) {
      return const <SourceLocation>[];
    }
    final binding = await _ensureBinding(document);
    if (binding == null) {
      return const <SourceLocation>[];
    }
    await _syncDocument(binding, document);
    return invoke(binding.binding.navigation);
  }

  Future<_SemanticBindingCache?> _ensureBinding(
    _TrackedDocument document,
  ) async {
    final ready = _sessions.readySessionFor(
      document.key.workspaceId,
      document.providerId,
    );
    if (ready == null) {
      return null;
    }
    final key = _BindingKey(document.key.workspaceId, document.providerId);
    final cached = _bindings[key];
    if (cached != null &&
        cached.generation == ready.generation &&
        identical(cached.session, ready.session)) {
      return cached;
    }

    final replacement = _SemanticBindingCache(
      generation: ready.generation,
      session: ready.session,
      binding: _semanticAdapterFactory.create(
        workspaceId: ready.workspaceId,
        provider: ready.provider,
        session: ready.session,
      ),
    );
    _bindings[key] = replacement;
    final documentsToResync = _documents.values
        .where(
          (candidate) =>
              candidate.key.workspaceId == document.key.workspaceId &&
              candidate.providerId == document.providerId,
        )
        .toList(growable: false);
    for (final candidate in documentsToResync) {
      await _syncDocument(replacement, candidate);
    }
    return replacement;
  }

  Future<void> _syncDocument(
    _SemanticBindingCache cache,
    _TrackedDocument document,
  ) async {
    final syncedRevision = cache.syncedRevisions[document.key];
    if (syncedRevision == null) {
      await cache.binding.documents.openDocument(
        language: document.language,
        path: document.path,
        text: document.text,
      );
      cache.syncedRevisions[document.key] = document.revision;
      return;
    }
    if (syncedRevision == document.revision) {
      return;
    }
    await cache.binding.documents.replaceDocument(
      path: document.path,
      text: document.text,
    );
    cache.syncedRevisions[document.key] = document.revision;
  }

  Future<void> _detachTrackedDocument(
    _TrackedDocument document, {
    bool stopWhenUnused = false,
  }) async {
    final bindingKey = _BindingKey(
      document.key.workspaceId,
      document.providerId,
    );
    final cache = _bindings[bindingKey];
    try {
      if (cache?.syncedRevisions.containsKey(document.key) ?? false) {
        await cache!.binding.documents.closeDocument(path: document.path);
        cache.syncedRevisions.remove(document.key);
      }
    } finally {
      await _sessions.releaseDocument(
        workspaceId: document.key.workspaceId,
        providerId: document.providerId,
        documentId: document.path,
        stopWhenUnused: stopWhenUnused,
      );
      final stillUsed = _documents.values.any(
        (candidate) =>
            candidate.key.workspaceId == document.key.workspaceId &&
            candidate.providerId == document.providerId,
      );
      if (!stillUsed) {
        _bindings.remove(bindingKey);
      }
    }
  }
}

final class _DocumentKey {
  const _DocumentKey(this.workspaceId, this.path);

  final String workspaceId;
  final String path;

  @override
  bool operator ==(Object other) =>
      other is _DocumentKey &&
      other.workspaceId == workspaceId &&
      other.path == path;

  @override
  int get hashCode => Object.hash(workspaceId, path);
}

final class _BindingKey {
  const _BindingKey(this.workspaceId, this.providerId);

  final String workspaceId;
  final String providerId;

  @override
  bool operator ==(Object other) =>
      other is _BindingKey &&
      other.workspaceId == workspaceId &&
      other.providerId == providerId;

  @override
  int get hashCode => Object.hash(workspaceId, providerId);
}

final class _TrackedDocument {
  _TrackedDocument({
    required this.key,
    required this.path,
    required this.language,
    required this.providerId,
    required this.text,
  });

  final _DocumentKey key;
  final String path;
  final LanguageId language;
  final String providerId;
  String text;
  int revision = 1;
}

final class _SemanticBindingCache {
  _SemanticBindingCache({
    required this.generation,
    required this.session,
    required this.binding,
  });

  final int generation;
  final LanguageServerRuntimeSession session;
  final LanguageSemanticProviderBinding binding;
  final Map<_DocumentKey, int> syncedRevisions = <_DocumentKey, int>{};
}
