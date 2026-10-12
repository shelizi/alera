enum MobileMarkdownLineKind { heading, list, quote, code, normal }

class MobileMarkdownLine {
  const MobileMarkdownLine({
    required this.text,
    required this.kind,
    this.level = 0,
  });

  final String text;
  final MobileMarkdownLineKind kind;
  final int level;
}

/// Render a small, intentionally passive Markdown preview without opening
/// remote images, following links, or executing workspace HTML/JS.
List<MobileMarkdownLine> parsePassiveMarkdown(String source) {
  var inFence = false;
  return <MobileMarkdownLine>[
    for (final line in source.split('\n'))
      _passiveMarkdownLine(line, () => inFence, (value) => inFence = value),
  ];
}

MobileMarkdownLine _passiveMarkdownLine(
  String line,
  bool Function() fenced,
  void Function(bool) setFenced,
) {
  final trimmed = line.trimLeft();
  if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
    setFenced(!fenced());
    return const MobileMarkdownLine(text: '', kind: .code);
  }
  if (fenced()) return MobileMarkdownLine(text: line, kind: .code);
  final heading = RegExp(r'^(#{1,6})\s+(.+)$').firstMatch(trimmed);
  if (heading != null) {
    return MobileMarkdownLine(
      text: heading.group(2)!,
      kind: .heading,
      level: heading.group(1)!.length,
    );
  }
  if (RegExp(r'^([-*+]\s|\d+\.\s)').hasMatch(trimmed)) {
    return MobileMarkdownLine(text: trimmed, kind: .list);
  }
  if (trimmed.startsWith('> ')) {
    return MobileMarkdownLine(text: trimmed.substring(2), kind: .quote);
  }
  return MobileMarkdownLine(text: line, kind: .normal);
}

bool isMarkdownWorkspaceFile(String path) {
  final normalized = path.toLowerCase();
  return normalized.endsWith('.md') || normalized.endsWith('.markdown');
}
