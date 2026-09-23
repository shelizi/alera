import 'dart:async';

import '../domain/language_id.dart';
import '../domain/language_server_session_state.dart';

enum ManagedLanguageServerAcquisitionState {
  checking,
  installing,
  verifying,
  ready,
  missing,
  failed,
}

final class ManagedLanguageServerAcquisitionSnapshot {
  const ManagedLanguageServerAcquisitionSnapshot({
    required this.providerId,
    required this.state,
    this.version,
    this.executable,
    this.detail,
  });

  final String providerId;
  final ManagedLanguageServerAcquisitionState state;
  final String? version;
  final String? executable;
  final String? detail;
}

enum StructuralParserActivityState { parsing, ready, unsupported, failed }

final class StructuralParserDocumentSnapshot {
  const StructuralParserDocumentSnapshot({
    required this.language,
    required this.documentId,
    required this.state,
    this.revision,
    this.currentByteOffset,
    this.totalBytes,
    this.detail,
  });

  final LanguageId language;
  final String documentId;
  final StructuralParserActivityState state;
  final int? revision;
  final int? currentByteOffset;
  final int? totalBytes;
  final String? detail;

  double? get progressFraction {
    final current = currentByteOffset;
    final total = totalBytes;
    if (current == null || total == null || total <= 0) return null;
    return (current / total).clamp(0.0, 1.0);
  }
}

final class SemanticServerActivitySnapshot {
  const SemanticServerActivitySnapshot({
    required this.providerId,
    required this.state,
    required this.sessionCount,
    required this.activeDocumentCount,
    required this.restartAttempts,
    this.executable,
    this.lastError,
  });

  final String providerId;
  final LanguageServerSessionState state;
  final int sessionCount;
  final int activeDocumentCount;
  final int restartAttempts;
  final String? executable;
  final String? lastError;
}

final class LanguageServerProgressSnapshot {
  const LanguageServerProgressSnapshot({
    required this.workspaceId,
    required this.providerId,
    required this.token,
    this.title,
    this.message,
    this.percentage,
  });

  final String workspaceId;
  final String providerId;
  final String token;
  final String? title;
  final String? message;
  final double? percentage;
}

final class StructuralParserActivitySnapshot {
  const StructuralParserActivitySnapshot({
    required this.language,
    required this.state,
    required this.activeDocumentCount,
    this.revision,
    this.currentByteOffset,
    this.totalBytes,
    this.detail,
  });

  final LanguageId language;
  final StructuralParserActivityState state;
  final int activeDocumentCount;
  final int? revision;
  final int? currentByteOffset;
  final int? totalBytes;
  final String? detail;

  double? get progressFraction {
    final current = currentByteOffset;
    final total = totalBytes;
    if (current == null || total == null || total <= 0) return null;
    return (current / total).clamp(0.0, 1.0);
  }
}

final class LanguageIntelligenceActivitySnapshot {
  const LanguageIntelligenceActivitySnapshot._({
    required this.acquisitions,
    required this.serverSessions,
    required this.serverProgress,
    required this.parserDocuments,
  });

  const LanguageIntelligenceActivitySnapshot.empty()
    : acquisitions = const <String, ManagedLanguageServerAcquisitionSnapshot>{},
      serverSessions = const <String, LanguageServerSessionSnapshot>{},
      serverProgress = const <String, LanguageServerProgressSnapshot>{},
      parserDocuments = const <String, StructuralParserDocumentSnapshot>{};

  final Map<String, ManagedLanguageServerAcquisitionSnapshot> acquisitions;
  final Map<String, LanguageServerSessionSnapshot> serverSessions;
  final Map<String, LanguageServerProgressSnapshot> serverProgress;
  final Map<String, StructuralParserDocumentSnapshot> parserDocuments;

  ManagedLanguageServerAcquisitionSnapshot? acquisitionFor(String providerId) =>
      acquisitions[providerId];

  SemanticServerActivitySnapshot? serverFor(String providerId) {
    final sessions = serverSessions.values
        .where((session) => session.providerId == providerId)
        .toList(growable: false);
    if (sessions.isEmpty) return null;

    sessions.sort(
      (left, right) =>
          _serverStatePriority(right.state) - _serverStatePriority(left.state),
    );
    final representative = sessions.first;
    return SemanticServerActivitySnapshot(
      providerId: providerId,
      state: representative.state,
      sessionCount: sessions.length,
      activeDocumentCount: sessions.fold<int>(
        0,
        (total, session) => total + session.activeDocumentCount,
      ),
      restartAttempts: sessions.fold<int>(
        0,
        (highest, session) => session.restartAttempts > highest
            ? session.restartAttempts
            : highest,
      ),
      executable: _firstNonEmpty(sessions.map((session) => session.executable)),
      lastError: _firstNonEmpty(sessions.map((session) => session.lastError)),
    );
  }

  List<LanguageServerProgressSnapshot> progressForWorkspace(
    String workspaceId,
  ) => serverProgress.values
      .where((progress) => progress.workspaceId == workspaceId)
      .toList(growable: false);

  StructuralParserActivitySnapshot? parserFor(LanguageId language) {
    final documents = parserDocuments.values
        .where((document) => document.language == language)
        .toList(growable: false);
    if (documents.isEmpty) return null;

    documents.sort(
      (left, right) =>
          _parserStatePriority(right.state) - _parserStatePriority(left.state),
    );
    final representative = documents.first;
    int? highestRevision;
    var progressCurrent = 0;
    var progressTotal = 0;
    var hasProgress = false;
    for (final document in documents) {
      final revision = document.revision;
      if (revision != null &&
          (highestRevision == null || revision > highestRevision)) {
        highestRevision = revision;
      }
      if (document.state == StructuralParserActivityState.parsing) {
        final total = document.totalBytes;
        final current = document.currentByteOffset;
        if (total != null && total > 0 && current != null) {
          hasProgress = true;
          progressTotal += total;
          progressCurrent += current.clamp(0, total);
        }
      }
    }
    return StructuralParserActivitySnapshot(
      language: language,
      state: representative.state,
      activeDocumentCount: documents.length,
      revision: highestRevision,
      currentByteOffset: hasProgress ? progressCurrent : null,
      totalBytes: hasProgress ? progressTotal : null,
      detail: _firstNonEmpty(documents.map((document) => document.detail)),
    );
  }

  static int _serverStatePriority(LanguageServerSessionState state) =>
      switch (state) {
        LanguageServerSessionState.failed => 90,
        LanguageServerSessionState.missingExecutable => 80,
        LanguageServerSessionState.initializing => 75,
        LanguageServerSessionState.starting => 70,
        LanguageServerSessionState.resolvingExecutable => 65,
        LanguageServerSessionState.ready => 60,
        LanguageServerSessionState.stopping => 50,
        LanguageServerSessionState.available => 40,
        LanguageServerSessionState.disabled => 0,
      };

  static int _parserStatePriority(StructuralParserActivityState state) =>
      switch (state) {
        StructuralParserActivityState.failed => 40,
        StructuralParserActivityState.parsing => 30,
        StructuralParserActivityState.ready => 20,
        StructuralParserActivityState.unsupported => 10,
      };

  static String? _firstNonEmpty(Iterable<String?> values) {
    for (final value in values) {
      if (value != null && value.trim().isNotEmpty) return value;
    }
    return null;
  }
}

abstract interface class LanguageIntelligenceActivityPort {
  LanguageIntelligenceActivitySnapshot get snapshot;

  Stream<LanguageIntelligenceActivitySnapshot> get changes;
}

abstract interface class LanguageIntelligenceActivityReporter {
  void reportManagedServerAcquisition(
    ManagedLanguageServerAcquisitionSnapshot snapshot,
  );

  void reportServerSession(LanguageServerSessionSnapshot snapshot);

  void reportServerProgress(LanguageServerProgressSnapshot snapshot);

  void removeServerProgress({
    required String workspaceId,
    required String providerId,
    required String token,
  });

  void clearServerProgress({
    required String workspaceId,
    required String providerId,
  });

  void reportStructuralParserDocument(
    StructuralParserDocumentSnapshot snapshot,
  );

  void removeStructuralParserDocument({
    required LanguageId language,
    required String documentId,
  });
}

final class LanguageIntelligenceActivityStore
    implements
        LanguageIntelligenceActivityPort,
        LanguageIntelligenceActivityReporter {
  final StreamController<LanguageIntelligenceActivitySnapshot> _changes =
      StreamController<LanguageIntelligenceActivitySnapshot>.broadcast(
        sync: true,
      );
  final Map<String, ManagedLanguageServerAcquisitionSnapshot> _acquisitions =
      <String, ManagedLanguageServerAcquisitionSnapshot>{};
  final Map<String, LanguageServerSessionSnapshot> _serverSessions =
      <String, LanguageServerSessionSnapshot>{};
  final Map<String, LanguageServerProgressSnapshot> _serverProgress =
      <String, LanguageServerProgressSnapshot>{};
  final Map<String, StructuralParserDocumentSnapshot> _parserDocuments =
      <String, StructuralParserDocumentSnapshot>{};
  bool _disposed = false;

  @override
  LanguageIntelligenceActivitySnapshot get snapshot =>
      LanguageIntelligenceActivitySnapshot._(
        acquisitions:
            Map<String, ManagedLanguageServerAcquisitionSnapshot>.unmodifiable(
              _acquisitions,
            ),
        serverSessions: Map<String, LanguageServerSessionSnapshot>.unmodifiable(
          _serverSessions,
        ),
        serverProgress:
            Map<String, LanguageServerProgressSnapshot>.unmodifiable(
              _serverProgress,
            ),
        parserDocuments:
            Map<String, StructuralParserDocumentSnapshot>.unmodifiable(
              _parserDocuments,
            ),
      );

  @override
  Stream<LanguageIntelligenceActivitySnapshot> get changes => _changes.stream;

  @override
  void reportManagedServerAcquisition(
    ManagedLanguageServerAcquisitionSnapshot snapshot,
  ) {
    _acquisitions[snapshot.providerId] = snapshot;
    _publish();
  }

  @override
  void reportServerSession(LanguageServerSessionSnapshot snapshot) {
    _serverSessions[_serverKey(snapshot.workspaceId, snapshot.providerId)] =
        snapshot;
    _publish();
  }

  @override
  void reportServerProgress(LanguageServerProgressSnapshot snapshot) {
    final key = _serverProgressKey(
      snapshot.workspaceId,
      snapshot.providerId,
      snapshot.token,
    );
    final previous = _serverProgress[key];
    _serverProgress[key] = LanguageServerProgressSnapshot(
      workspaceId: snapshot.workspaceId,
      providerId: snapshot.providerId,
      token: snapshot.token,
      title: snapshot.title ?? previous?.title,
      message: snapshot.message ?? previous?.message,
      percentage: snapshot.percentage ?? previous?.percentage,
    );
    _publish();
  }

  @override
  void removeServerProgress({
    required String workspaceId,
    required String providerId,
    required String token,
  }) {
    if (_serverProgress.remove(
          _serverProgressKey(workspaceId, providerId, token),
        ) !=
        null) {
      _publish();
    }
  }

  @override
  void clearServerProgress({
    required String workspaceId,
    required String providerId,
  }) {
    final prefix = '$workspaceId\u0000$providerId\u0000';
    final keys = _serverProgress.keys
        .where((key) => key.startsWith(prefix))
        .toList(growable: false);
    if (keys.isEmpty) return;
    for (final key in keys) {
      _serverProgress.remove(key);
    }
    _publish();
  }

  @override
  void reportStructuralParserDocument(
    StructuralParserDocumentSnapshot snapshot,
  ) {
    _parserDocuments[_parserKey(snapshot.language, snapshot.documentId)] =
        snapshot;
    _publish();
  }

  @override
  void removeStructuralParserDocument({
    required LanguageId language,
    required String documentId,
  }) {
    if (_parserDocuments.remove(_parserKey(language, documentId)) != null) {
      _publish();
    }
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _changes.close();
  }

  void _publish() {
    if (_disposed) return;
    _changes.add(snapshot);
  }

  static String _serverKey(String workspaceId, String providerId) =>
      '$workspaceId\u0000$providerId';

  static String _serverProgressKey(
    String workspaceId,
    String providerId,
    String token,
  ) => '$workspaceId\u0000$providerId\u0000$token';

  static String _parserKey(LanguageId language, String documentId) =>
      '${language.value}\u0000$documentId';
}
