/// A portable single filename, never a path or a Windows alternate data
/// stream. The Rust host repeats these checks regardless of phone behavior.
bool isSafeMobileWorkspaceFileName(String name) {
  if (name.isEmpty ||
      name != name.trim() ||
      name == '.' ||
      name == '..' ||
      name.endsWith('.') ||
      name.toLowerCase().startsWith('.alera-mobile-')) {
    return false;
  }
  if (name.runes.any(
    (rune) =>
        rune < 32 ||
        rune == 127 ||
        const <int>[47, 92, 58, 42, 63, 34, 60, 62, 124].contains(rune),
  )) {
    return false;
  }
  if (const <String>{'.git', '.hg', '.svn'}.contains(name.toLowerCase())) {
    return false;
  }
  final stem = name.split('.').first.toUpperCase();
  const reservedNames = <String>{'CON', 'PRN', 'AUX', 'NUL', r'CLOCK$'};
  if (reservedNames.contains(stem)) {
    return false;
  }
  if (RegExp(r'^(COM|LPT)[1-9]$').hasMatch(stem)) {
    return false;
  }
  return true;
}
