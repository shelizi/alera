import 'package:path/path.dart' as p;

import '../../../shared/infra/files/path_identity.dart';

// Each list replaces the server's own default, so the defaults are repeated.
const List<String> _pyrightDefaultExcludes = <String>[
  '**/node_modules',
  '**/__pycache__',
  '**/.*',
];

const List<String> _intelephenseDefaultExcludes = <String>[
  '**/.git/**',
  '**/.svn/**',
  '**/.hg/**',
  '**/CVS/**',
  '**/.DS_Store/**',
  '**/node_modules/**',
  '**/bower_components/**',
  '**/vendor/**/{Tests,tests}/**',
  '**/.history/**',
  '**/vendor/**/vendor/**',
];

/// `workspace/configuration` answers, keyed by the section a server asks for,
/// that keep [nestedCheckouts] out of that server's analysis.
///
/// Only servers that were observed indexing a nested checkout are listed.
/// rust-analyzer, gopls, pyrefly and typescript-language-server load projects
/// from the root or from opened files and never picked one up; csharp-ls
/// does, but offers no exclusion setting.
Map<String, Object?> nestedCheckoutExclusionSections({
  required String providerId,
  required String workspaceRoot,
  required List<String> nestedCheckouts,
  p.Context? pathContext,
}) {
  if (nestedCheckouts.isEmpty) return const <String, Object?>{};
  final context = pathContext ?? p.context;
  List<String> relative() => <String>[
    for (final path in nestedCheckouts)
      if (relativePathWithin(
            root: workspaceRoot,
            path: path,
            pathContext: context,
          )
          case final relative?)
        context.split(relative).join('/'),
  ];

  return switch (providerId) {
    'dart.analysis-server' => <String, Object?>{
      'dart': <String, Object?>{'analysisExcludedFolders': nestedCheckouts},
    },
    'python.pyright' => <String, Object?>{
      'python': <String, Object?>{
        'analysis': <String, Object?>{
          'exclude': <String>[..._pyrightDefaultExcludes, ...nestedCheckouts],
        },
      },
    },
    // ty keeps its built-in excludes and adds these to them.
    'python.ty' => <String, Object?>{
      'ty': <String, Object?>{
        'configuration': <String, Object?>{
          'src': <String, Object?>{'exclude': relative()},
        },
      },
    },
    'php.intelephense' => <String, Object?>{
      'intelephense': <String, Object?>{
        'files': <String, Object?>{
          'exclude': <String>[
            ..._intelephenseDefaultExcludes,
            for (final path in relative()) '**/$path/**',
          ],
        },
      },
    },
    _ => const <String, Object?>{},
  };
}
