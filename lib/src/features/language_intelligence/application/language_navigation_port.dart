import '../domain/source_location.dart';

abstract interface class LanguageNavigationPort {
  Future<List<SourceLocation>> definition({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> declaration({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> typeDefinition({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> implementation({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> references({
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  });
}
