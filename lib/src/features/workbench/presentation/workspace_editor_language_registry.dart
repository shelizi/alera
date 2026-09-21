part of 'workspace_editor_surface.dart';

Mode get _plainTextLanguage => builtinAllLanguages['plaintext']!;

String workspaceEditorSyntaxLanguageIdForPath({
  required String filePath,
  required LanguageExtensionRegistry registry,
}) =>
    registry.syntaxLanguageIdForPath(filePath) ??
    _legacyLanguageIdForPath(filePath);

bool workspaceEditorNativeSyntaxEnabled({
  required String filePath,
  required LanguageExtensionRegistry registry,
  required LanguageIntelligenceSettings settings,
}) {
  final language = registry.languageForPath(filePath);
  if (language == null) {
    return true;
  }
  if (language.parserProviderId == null) {
    return false;
  }
  return settings
      .forLanguage(
        language.id,
        structuralParserDefaultEnabled: language.structuralParserDefaultEnabled,
      )
      .structuralParserEnabled;
}

Mode workspaceEditorSyntaxModeForPath({
  required String filePath,
  required LanguageExtensionRegistry registry,
  required String syntaxLanguageId,
}) {
  final registeredLanguage = registry.languageForPath(filePath);
  return builtinAllLanguages[syntaxLanguageId] ??
      (registeredLanguage == null
          ? null
          : builtinAllLanguages[registeredLanguage.id.value]) ??
      builtinAllLanguages[_legacyLanguageIdForPath(filePath)] ??
      _plainTextLanguage;
}

String _legacyLanguageIdForPath(String filePath) {
  final extension = p.extension(filePath).toLowerCase();
  final basename = p.basename(filePath).toLowerCase();
  final byName = _languageIdsByBasename[basename];
  if (byName != null) {
    return byName;
  }
  return _languageIdsByExtension[extension] ?? 'plaintext';
}

const Map<String, String> _languageIdsByBasename = <String, String>{
  'dockerfile': 'dockerfile',
  'makefile': 'makefile',
  'gemfile': 'ruby',
  'rakefile': 'ruby',
  'podfile': 'ruby',
  'cmakelists.txt': 'cmake',
};

const Map<String, String> _languageIdsByExtension = <String, String>{
  '.1c': '1c',
  '.abnf': 'abnf',
  '.accesslog': 'accesslog',
  '.ada': 'ada',
  '.adb': 'ada',
  '.ads': 'ada',
  '.apacheconf': 'apache',
  '.applescript': 'applescript',
  '.ino': 'arduino',
  '.as': 'actionscript',
  '.asc': 'asciidoc',
  '.asciidoc': 'asciidoc',
  '.ahk': 'autohotkey',
  '.au3': 'autoit',
  '.awk': 'awk',
  '.bash': 'bash',
  '.bat': 'dos',
  '.bf': 'brainfuck',
  '.bnf': 'bnf',
  '.c': 'c',
  '.h': 'c',
  '.cc': 'cpp',
  '.cxx': 'cpp',
  '.cpp': 'cpp',
  '.hpp': 'cpp',
  '.cs': 'csharp',
  '.css': 'css',
  '.scss': 'scss',
  '.sass': 'scss',
  '.clj': 'clojure',
  '.cljs': 'clojure',
  '.cmake': 'cmake',
  '.coffee': 'coffeescript',
  '.cr': 'crystal',
  '.d': 'd',
  '.dart': 'dart',
  '.diff': 'diff',
  '.patch': 'diff',
  '.Dockerfile': 'dockerfile',
  '.dockerfile': 'dockerfile',
  '.erl': 'erlang',
  '.ex': 'elixir',
  '.exs': 'elixir',
  '.elm': 'elm',
  '.erb': 'erb',
  '.fs': 'fsharp',
  '.fsx': 'fsharp',
  '.f90': 'fortran',
  '.f95': 'fortran',
  '.gcode': 'gcode',
  '.feature': 'gherkin',
  '.glsl': 'glsl',
  '.go': 'go',
  '.graphql': 'graphql',
  '.gql': 'graphql',
  '.gradle': 'gradle',
  '.groovy': 'groovy',
  '.haml': 'haml',
  '.hs': 'haskell',
  '.hx': 'haxe',
  '.html': 'xml',
  '.htm': 'xml',
  '.xml': 'xml',
  '.svg': 'xml',
  '.http': 'http',
  '.ini': 'ini',
  '.toml': 'ini',
  '.java': 'java',
  '.js': 'javascript',
  '.cjs': 'javascript',
  '.mjs': 'javascript',
  '.jsx': 'javascript',
  '.json': 'json',
  '.jsonc': 'json',
  '.jl': 'julia',
  '.kt': 'kotlin',
  '.kts': 'kotlin',
  '.latex': 'latex',
  '.tex': 'latex',
  '.less': 'less',
  '.lisp': 'lisp',
  '.lua': 'lua',
  '.md': 'markdown',
  '.markdown': 'markdown',
  '.matlab': 'matlab',
  '.m': 'objectivec',
  '.mm': 'objectivec',
  '.ml': 'ocaml',
  '.mli': 'ocaml',
  '.nginxconf': 'nginx',
  '.nim': 'nim',
  '.nix': 'nix',
  '.php': 'php',
  '.pl': 'perl',
  '.pm': 'perl',
  '.ps1': 'powershell',
  '.proto': 'protobuf',
  '.properties': 'properties',
  '.py': 'python',
  '.pyw': 'python',
  '.r': 'r',
  '.rb': 'ruby',
  '.rs': 'rust',
  '.scala': 'scala',
  '.scm': 'scheme',
  '.sh': 'bash',
  '.zsh': 'bash',
  '.sql': 'sql',
  '.swift': 'swift',
  '.ts': 'typescript',
  '.tsx': 'typescript',
  '.twig': 'twig',
  '.vb': 'vbnet',
  '.vbs': 'vbscript',
  '.v': 'verilog',
  '.vh': 'verilog',
  '.vhd': 'vhdl',
  '.vim': 'vim',
  '.vue': 'vue',
  '.wasm': 'wasm',
  '.x86asm': 'x86asm',
  '.xq': 'xquery',
  '.xquery': 'xquery',
  '.yaml': 'yaml',
  '.yml': 'yaml',
};
