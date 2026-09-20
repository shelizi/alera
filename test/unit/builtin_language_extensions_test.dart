import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_contribution.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'first-wave language paths resolve entirely through compiled extensions',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final cases = <String, String>{
        r'C:\repo\Program.cs': 'csharp',
        r'C:\repo\script.csx': 'csharp',
        '/repo/main.py': 'python',
        '/repo/stubs.pyi': 'python',
        '/repo/lib.rs': 'rust',
        '/repo/main.go': 'go',
        '/repo/index.php': 'php',
        '/repo/template.phtml': 'php',
        '/repo/app.ts': 'typescript',
        '/repo/component.tsx': 'typescript',
        '/repo/module.mts': 'typescript',
        '/repo/app.js': 'javascript',
        '/repo/component.jsx': 'javascript',
        '/repo/module.mjs': 'javascript',
      };

      for (final entry in cases.entries) {
        expect(
          registry.languageForPath(entry.key)?.id.value,
          entry.value,
          reason: entry.key,
        );
      }
      expect(registry.languageForId('C#')?.id.value, 'csharp');
      expect(registry.languageForId('golang')?.id.value, 'go');
      expect(registry.syntaxLanguageIdForPath(r'C:\repo\script.csx'), 'csharp');
      expect(registry.syntaxLanguageIdForPath('/repo/stubs.pyi'), 'python');
      expect(registry.syntaxLanguageIdForPath('/repo/template.phtml'), 'php');
      expect(
        registry.syntaxLanguageIdForPath('/repo/module.mts'),
        'typescript',
      );
      expect(
        registry.syntaxLanguageIdForPath('/repo/module.cts'),
        'typescript',
      );
      expect(registry.syntaxLanguageIdForPath('/repo/component.tsx'), 'tsx');
      expect(registry.syntaxLanguageIdForPath('/repo/component.jsx'), 'jsx');
    },
  );

  test(
    'each first-wave language declares structural and semantic providers',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final ids = <String>{
        'csharp',
        'python',
        'rust',
        'go',
        'php',
        'typescript',
        'javascript',
      };

      expect(
        registry.languages.map((language) => language.id.value).toSet(),
        ids,
      );
      for (final id in ids) {
        final language = registry.languageForId(id)!;
        expect(language.parserProviderId, isNotNull, reason: id);
        expect(language.defaultSemanticProviderId, isNotNull, reason: id);

        final parser = registry.provider(language.parserProviderId!);
        expect(parser?.kind, LanguageProviderKind.structuralParser, reason: id);
        expect(parser?.processScope, LanguageProviderProcessScope.document);
        expect(parser?.capabilities, contains(LanguageCapability.syntax));

        final semantic = registry.provider(language.defaultSemanticProviderId!);
        expect(semantic?.kind, LanguageProviderKind.semanticServer, reason: id);
        expect(semantic?.capabilities, contains(LanguageCapability.definition));
        expect(semantic?.capabilities, contains(LanguageCapability.references));
      }
    },
  );

  test('structural parser defaults preserve existing languages and opt in new grammars', () {
    final registry = createBuiltinLanguageExtensionRegistry();
    final expected = <String, bool>{
      'csharp': false,
      'python': true,
      'rust': true,
      'go': false,
      'php': false,
      'typescript': true,
      'javascript': true,
    };

    for (final entry in expected.entries) {
      expect(
        registry.languageForId(entry.key)?.structuralParserDefaultEnabled,
        entry.value,
        reason: entry.key,
      );
    }
  });

  test('semantic engines remain opt-in even when built-in providers exist', () {
    final registry = createBuiltinLanguageExtensionRegistry();
    for (final language in registry.languages) {
      final settings = LanguageIntelligenceSettings.defaults.forLanguage(
        language.id,
      );
      expect(settings.enabled, isFalse, reason: language.id.value);
      expect(settings.semanticProviderId, isNull, reason: language.id.value);
    }
  });

  test(
    'first-wave semantic launch metadata is explicit and provider-local',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final expected = <String, ({List<String> commands, List<String> args})>{
        'csharp.csharp-ls': (commands: <String>['csharp-ls'], args: <String>[]),
        'python.pyright': (
          commands: <String>['pyright-langserver'],
          args: <String>['--stdio'],
        ),
        'rust.rust-analyzer': (
          commands: <String>['rust-analyzer'],
          args: <String>[],
        ),
        'go.gopls': (commands: <String>['gopls'], args: <String>['serve']),
        'php.phpactor': (
          commands: <String>['phpactor'],
          args: <String>['language-server'],
        ),
        'typescript-javascript.typescript-language-server': (
          commands: <String>['typescript-language-server'],
          args: <String>['--stdio'],
        ),
      };

      for (final entry in expected.entries) {
        final provider = registry.provider(entry.key)!;
        expect(provider.executableCandidates, entry.value.commands);
        expect(provider.defaultArguments, entry.value.args);
        expect(
          provider.executableResolutionPolicy,
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
        );
      }
    },
  );

  test(
    'TypeScript and JavaScript share one workspace-family semantic server',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final typescript = registry.languageForId('typescript')!;
      final javascript = registry.languageForId('javascript')!;

      expect(
        typescript.defaultSemanticProviderId,
        javascript.defaultSemanticProviderId,
      );
      final provider = registry.provider(
        typescript.defaultSemanticProviderId!,
      )!;
      expect(
        provider.processScope,
        LanguageProviderProcessScope.sharedWorkspaceFamily,
      );
      expect(provider.languages, <LanguageId>{
        LanguageId('typescript'),
        LanguageId('javascript'),
      });
    },
  );

  test(
    'a new language can be contributed without changing built-in core code',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final mock = LanguageId('mocklang');
      registry.registerContribution(
        LanguageExtensionContribution(
          languages: <LanguageExtensionDescriptor>[
            LanguageExtensionDescriptor(
              id: mock,
              displayName: 'Mock Language',
              fileExtensions: const <String>['mock'],
              parserProviderId: 'mock.tree-sitter',
              semanticProviderIds: const <String>['mock.server'],
              defaultSemanticProviderId: 'mock.server',
              capabilities: const <LanguageCapability>{
                LanguageCapability.syntax,
                LanguageCapability.definition,
              },
            ),
          ],
          providers: <LanguageProviderDescriptor>[
            LanguageProviderDescriptor(
              id: 'mock.tree-sitter',
              kind: LanguageProviderKind.structuralParser,
              languages: <LanguageId>{mock},
              capabilities: const <LanguageCapability>{
                LanguageCapability.syntax,
              },
              processScope: LanguageProviderProcessScope.document,
              launchPolicy: LanguageProviderLaunchPolicy.explicit,
            ),
            LanguageProviderDescriptor(
              id: 'mock.server',
              kind: LanguageProviderKind.semanticServer,
              languages: <LanguageId>{mock},
              capabilities: const <LanguageCapability>{
                LanguageCapability.definition,
              },
              processScope: LanguageProviderProcessScope.workspace,
              launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
              executableResolutionPolicy:
                  LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
              executableCandidates: const <String>['mock-server'],
            ),
          ],
        ),
      );

      expect(registry.languageForPath('/repo/example.mock')?.id, mock);
      expect(registry.provider('mock.server')?.languages, contains(mock));
    },
  );

  test(
    'invalid contributions are transactional and leave no partial language',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final beforeLanguages = registry.languages.length;
      final invalid = LanguageId('invalidlang');

      expect(
        () => registry.registerContribution(
          LanguageExtensionContribution(
            languages: <LanguageExtensionDescriptor>[
              LanguageExtensionDescriptor(
                id: invalid,
                displayName: 'Invalid Language',
                fileExtensions: const <String>['invalid'],
                semanticProviderIds: const <String>['invalid.server'],
                defaultSemanticProviderId: 'invalid.server',
                capabilities: const <LanguageCapability>{
                  LanguageCapability.definition,
                },
              ),
            ],
            providers: <LanguageProviderDescriptor>[
              LanguageProviderDescriptor(
                id: 'invalid.server',
                kind: LanguageProviderKind.semanticServer,
                languages: <LanguageId>{LanguageId('not-registered')},
                capabilities: const <LanguageCapability>{
                  LanguageCapability.definition,
                },
                processScope: LanguageProviderProcessScope.workspace,
                launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
              ),
            ],
          ),
        ),
        throwsStateError,
      );
      expect(registry.languages.length, beforeLanguages);
      expect(registry.languageForId('invalidlang'), isNull);
      expect(registry.languageForPath('/repo/file.invalid'), isNull);
    },
  );
}
