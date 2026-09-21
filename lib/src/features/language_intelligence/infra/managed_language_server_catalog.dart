enum ManagedLanguageServerInstallKind { npm, go, dotnetTool, rustupComponent }

final class ManagedLanguageServerPackage {
  const ManagedLanguageServerPackage({
    required this.name,
    required this.version,
  });

  final String name;
  final String version;
}

final class ManagedLanguageServerRecipe {
  const ManagedLanguageServerRecipe({
    required this.providerId,
    required this.version,
    required this.kind,
    required this.executableName,
    required this.source,
    required this.integrityPolicy,
    this.packages = const <ManagedLanguageServerPackage>[],
    this.goModule,
    this.dotnetPackage,
    this.rustupToolchain,
    this.rustupComponent,
  });

  final String providerId;
  final String version;
  final ManagedLanguageServerInstallKind kind;
  final String executableName;
  final String source;
  final String integrityPolicy;
  final List<ManagedLanguageServerPackage> packages;
  final String? goModule;
  final String? dotnetPackage;
  final String? rustupToolchain;
  final String? rustupComponent;
}

/// Curated language-server versions validated by Alera. The acquisition layer
/// never accepts package names or versions from workspace content.
const managedLanguageServerRecipes = <String, ManagedLanguageServerRecipe>{
  'csharp.csharp-ls': ManagedLanguageServerRecipe(
    providerId: 'csharp.csharp-ls',
    version: '0.28.0',
    kind: ManagedLanguageServerInstallKind.dotnetTool,
    executableName: 'csharp-ls',
    dotnetPackage: 'csharp-ls',
    source: 'https://www.nuget.org/packages/csharp-ls/0.28.0',
    integrityPolicy:
        'NuGet package integrity during pinned dotnet-tool restore plus '
        'Alera SHA-256 verification before every reuse.',
  ),
  'go.gopls': ManagedLanguageServerRecipe(
    providerId: 'go.gopls',
    version: 'v0.21.1',
    kind: ManagedLanguageServerInstallKind.go,
    executableName: 'gopls',
    goModule: 'golang.org/x/tools/gopls',
    source: 'https://pkg.go.dev/golang.org/x/tools/gopls@v0.21.1',
    integrityPolicy:
        'Go module checksum database during pinned module install plus '
        'Alera SHA-256 verification before every reuse.',
  ),
  'python.pyright': ManagedLanguageServerRecipe(
    providerId: 'python.pyright',
    version: '1.1.414',
    kind: ManagedLanguageServerInstallKind.npm,
    executableName: 'pyright-langserver',
    packages: <ManagedLanguageServerPackage>[
      ManagedLanguageServerPackage(name: 'pyright', version: '1.1.414'),
    ],
    source: 'https://www.npmjs.com/package/pyright/v/1.1.414',
    integrityPolicy:
        'npm package-lock SRI verification with lifecycle scripts disabled '
        'plus Alera SHA-256 verification before every reuse.',
  ),
  'rust.rust-analyzer': ManagedLanguageServerRecipe(
    providerId: 'rust.rust-analyzer',
    version: '1.97.1',
    kind: ManagedLanguageServerInstallKind.rustupComponent,
    executableName: 'rust-analyzer',
    rustupToolchain: '1.97.1',
    rustupComponent: 'rust-analyzer',
    source: 'https://static.rust-lang.org/dist/',
    integrityPolicy:
        'rustup signed/checksummed pinned toolchain manifests plus Alera '
        'SHA-256 verification before every reuse.',
  ),
  'php.intelephense': ManagedLanguageServerRecipe(
    providerId: 'php.intelephense',
    version: '1.18.5',
    kind: ManagedLanguageServerInstallKind.npm,
    executableName: 'intelephense',
    packages: <ManagedLanguageServerPackage>[
      ManagedLanguageServerPackage(name: 'intelephense', version: '1.18.5'),
    ],
    source: 'https://www.npmjs.com/package/intelephense/v/1.18.5',
    integrityPolicy:
        'npm package-lock SRI verification with lifecycle scripts disabled '
        'plus Alera SHA-256 verification before every reuse.',
  ),
  'typescript-javascript.typescript-language-server':
      ManagedLanguageServerRecipe(
        providerId: 'typescript-javascript.typescript-language-server',
        version: '6.0.0+typescript-5.9.3',
        kind: ManagedLanguageServerInstallKind.npm,
        executableName: 'typescript-language-server',
        packages: <ManagedLanguageServerPackage>[
          ManagedLanguageServerPackage(
            name: 'typescript-language-server',
            version: '6.0.0',
          ),
          ManagedLanguageServerPackage(name: 'typescript', version: '5.9.3'),
        ],
        source:
            'https://www.npmjs.com/package/typescript-language-server/v/6.0.0',
        integrityPolicy:
            'npm package-lock SRI verification with lifecycle scripts '
            'disabled plus Alera SHA-256 verification before every reuse.',
      ),
};
