final class SourcePosition {
  const SourcePosition({required this.line, required this.scalarColumn})
    : assert(line >= 0),
      assert(scalarColumn >= 0);

  final int line;
  final int scalarColumn;

  @override
  bool operator ==(Object other) =>
      other is SourcePosition &&
      other.line == line &&
      other.scalarColumn == scalarColumn;

  @override
  int get hashCode => Object.hash(line, scalarColumn);
}

final class SourceRange {
  const SourceRange({required this.start, required this.end});

  final SourcePosition start;
  final SourcePosition end;

  @override
  bool operator ==(Object other) =>
      other is SourceRange && other.start == start && other.end == end;

  @override
  int get hashCode => Object.hash(start, end);
}

final class SourceLocation {
  const SourceLocation({
    required this.workspaceId,
    required this.path,
    required this.range,
  });

  final String workspaceId;
  final String path;
  final SourceRange range;

  SourceLocation copyWith({
    String? workspaceId,
    String? path,
    SourceRange? range,
  }) => SourceLocation(
    workspaceId: workspaceId ?? this.workspaceId,
    path: path ?? this.path,
    range: range ?? this.range,
  );

  @override
  bool operator ==(Object other) =>
      other is SourceLocation &&
      other.workspaceId == workspaceId &&
      other.path == path &&
      other.range == range;

  @override
  int get hashCode => Object.hash(workspaceId, path, range);
}
