enum ManagedLanguageServerInstallKind {
  npm,
  go,
  dotnetTool,
  rustupComponent,

  /// A prebuilt binary from an upstream release, pinned per platform by URL
  /// and SHA-256. Needs no toolchain on the user's machine.
  githubRelease,
}

/// Desktop targets Alera ships for; a release recipe without an asset for the
/// current one reports the server as unavailable instead of guessing.
enum ManagedLanguageServerPlatform {
  windowsX64,
  windowsArm64,
  macosX64,
  macosArm64,
  linuxX64,
  linuxArm64,
}

final class ManagedLanguageServerReleaseAsset {
  const ManagedLanguageServerReleaseAsset({
    required this.url,
    required this.sha256,
    required this.archiveMember,
  });

  final String url;

  /// Pinned here rather than fetched next to the asset: a checksum served by
  /// the same origin as the archive adds no protection against that origin.
  final String sha256;

  /// Path of the executable inside the archive; layouts differ per project.
  final String archiveMember;
}

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
    this.releaseAssets =
        const <
          ManagedLanguageServerPlatform,
          ManagedLanguageServerReleaseAsset
        >{},
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
  final Map<ManagedLanguageServerPlatform, ManagedLanguageServerReleaseAsset>
  releaseAssets;
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
  'python.pyrefly': ManagedLanguageServerRecipe(
    providerId: 'python.pyrefly',
    version: '1.3.1',
    kind: ManagedLanguageServerInstallKind.githubRelease,
    executableName: 'pyrefly',
    source: 'https://github.com/facebook/pyrefly/releases/tag/1.3.1',
    integrityPolicy:
        'Per-platform archive SHA-256 pinned in this catalog (checked against '
        'the published .sha256) plus Alera SHA-256 verification before '
        'every reuse.',
    releaseAssets:
        <ManagedLanguageServerPlatform, ManagedLanguageServerReleaseAsset>{
          ManagedLanguageServerPlatform.windowsX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-windows-x86_64.zip',
                sha256: 'd10bb909b9007d076fe7cb1f7164e26ab4287ca08e4df624fffaee7360f44baa',
                archiveMember: 'pyrefly.exe',
              ),
          ManagedLanguageServerPlatform.windowsArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-windows-arm64.zip',
                sha256: 'e92c6ee9078d7bbcdb3a22d6be44e0335ddce1e0f9ea9b0eea8fcdd9ce07010e',
                archiveMember: 'pyrefly.exe',
              ),
          ManagedLanguageServerPlatform.macosX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-macos-x86_64.tar.gz',
                sha256: '497d5743ff54f1b4a444f72a4147d7b85fa8cf20bf87123304b00b6173254545',
                archiveMember: 'pyrefly',
              ),
          ManagedLanguageServerPlatform.macosArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-macos-arm64.tar.gz',
                sha256: '3c7294e86efb53d6bc46619fd17002266731a79894a668b8e9db2ec189e746d2',
                archiveMember: 'pyrefly',
              ),
          ManagedLanguageServerPlatform.linuxX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-linux-x86_64.tar.gz',
                sha256: '4cf87ce9d0041e7edbc533fef0246b5992831f3f68efdbf4b6f5001912b4cf22',
                archiveMember: 'pyrefly',
              ),
          ManagedLanguageServerPlatform.linuxArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/facebook/pyrefly/releases/download/1.3.1/pyrefly-linux-arm64.tar.gz',
                sha256: '80ecd49f9c2d2efe6a601c67b5bb73607da2990dfa615f3435109e2a1af5e506',
                archiveMember: 'pyrefly',
              ),
        },
  ),
  'python.ty': ManagedLanguageServerRecipe(
    providerId: 'python.ty',
    version: '0.0.84',
    kind: ManagedLanguageServerInstallKind.githubRelease,
    executableName: 'ty',
    source: 'https://github.com/astral-sh/ty/releases/tag/0.0.84',
    integrityPolicy:
        'Per-platform archive SHA-256 pinned in this catalog (checked against '
        'the published .sha256) plus Alera SHA-256 verification before '
        'every reuse.',
    releaseAssets:
        <ManagedLanguageServerPlatform, ManagedLanguageServerReleaseAsset>{
          ManagedLanguageServerPlatform.windowsX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-x86_64-pc-windows-msvc.zip',
                sha256: 'e4b3c7cd30ff8b4ee4c2a62621f293341ad3c89fb3e7c6e4d2686150a4dbcf08',
                archiveMember: 'ty.exe',
              ),
          ManagedLanguageServerPlatform.windowsArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-aarch64-pc-windows-msvc.zip',
                sha256: '5310f17fc594e29e525ccd0eaa27b719b2af2a88ec7be38092af7036eaab5235',
                archiveMember: 'ty.exe',
              ),
          ManagedLanguageServerPlatform.macosX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-x86_64-apple-darwin.tar.gz',
                sha256: '3891e5509d306721cee4dd4e69b94535dbd96371af7ec3b734ee65f25b167ae4',
                archiveMember: 'ty-x86_64-apple-darwin/ty',
              ),
          ManagedLanguageServerPlatform.macosArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-aarch64-apple-darwin.tar.gz',
                sha256: 'c65c09f27bcef726c0b043dcee8d0f1e578bd1936799e5bc447235cbc3e19d91',
                archiveMember: 'ty-aarch64-apple-darwin/ty',
              ),
          ManagedLanguageServerPlatform.linuxX64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-x86_64-unknown-linux-gnu.tar.gz',
                sha256: '336bb36b7e917d844b8b16925d373b4614b326e452c881ff4ed6bc5904b65185',
                archiveMember: 'ty-x86_64-unknown-linux-gnu/ty',
              ),
          ManagedLanguageServerPlatform.linuxArm64:
              ManagedLanguageServerReleaseAsset(
                url: 'https://github.com/astral-sh/ty/releases/download/0.0.84/ty-aarch64-unknown-linux-gnu.tar.gz',
                sha256: 'd575243e0586742ae0e9186441358bd7e57e8160e319b3afb781430126762a39',
                archiveMember: 'ty-aarch64-unknown-linux-gnu/ty',
              ),
        },
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
