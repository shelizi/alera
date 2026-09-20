final class LanguageId {
  factory LanguageId(String value) {
    final normalized = value.trim().toLowerCase();
    if (!_validPattern.hasMatch(normalized)) {
      throw ArgumentError.value(
        value,
        'value',
        'Language ids must use lowercase ASCII words separated by -, _, or .',
      );
    }
    return LanguageId._(normalized);
  }

  const LanguageId._(this.value);

  static final RegExp _validPattern = RegExp(
    r'^[a-z][a-z0-9]*(?:[-_.][a-z0-9]+)*$',
  );

  final String value;

  @override
  bool operator ==(Object other) => other is LanguageId && other.value == value;

  @override
  int get hashCode => value.hashCode;

  @override
  String toString() => value;
}
