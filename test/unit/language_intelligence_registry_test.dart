import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('LanguageId', () {
    test('normalizes stable string ids and compares by value', () {
      expect(LanguageId(' Rust '), LanguageId('rust'));
      expect(LanguageId('typescript').value, 'typescript');
      expect(() => LanguageId('C#'), throwsArgumentError);
    });
  });

  group('LanguageExtensionRegistry', () {
    test('resolves language ids, aliases, and file extensions', () {
      final registry = LanguageExtensionRegistry();
      final typescript = LanguageExtensionDescriptor(
        id: LanguageId('typescript'),
        displayName: 'TypeScript',
        fileExtensions: const <String>['ts', '.tsx'],
        aliases: const <String>['ts'],
        capabilities: const <LanguageCapability>{
          LanguageCapability.syntax,
          LanguageCapability.definition,
        },
      );

      registry.registerLanguage(typescript);

      expect(registry.languageForId('typescript'), same(typescript));
      expect(registry.languageForId(' TS '), same(typescript));
      expect(
        registry.languageForPath(r'C:\repo\src\main.TS'),
        same(typescript),
      );
      expect(registry.languageForPath('/repo/src/view.tsx'), same(typescript));
      expect(registry.languageForPath('/repo/README.md'), isNull);
    });

    test('filters registered provider metadata by capability', () {
      final registry = LanguageExtensionRegistry();
      final rust = LanguageExtensionDescriptor(
        id: LanguageId('rust'),
        displayName: 'Rust',
        fileExtensions: const <String>['rs'],
        semanticProviderIds: const <String>['rust-semantic'],
        defaultSemanticProviderId: 'rust-semantic',
        capabilities: const <LanguageCapability>{
          LanguageCapability.syntax,
          LanguageCapability.definition,
          LanguageCapability.references,
        },
      );
      registry.registerLanguage(rust);

      registry.registerProvider(
        LanguageProviderDescriptor(
          id: 'rust-semantic',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{rust.id},
          capabilities: const <LanguageCapability>{
            LanguageCapability.definition,
            LanguageCapability.references,
          },
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
        ),
      );

      final definitionProviders = registry.providersFor(
        rust.id,
        LanguageCapability.definition,
      );
      expect(definitionProviders, hasLength(1));
      expect(
        registry.providersFor(rust.id, LanguageCapability.documentSymbols),
        isEmpty,
      );
      expect(definitionProviders.single.id, 'rust-semantic');
    });

    test('rejects ambiguous language and provider registrations', () {
      final registry = LanguageExtensionRegistry();
      registry.registerLanguage(
        LanguageExtensionDescriptor(
          id: LanguageId('javascript'),
          displayName: 'JavaScript',
          fileExtensions: const <String>['js'],
          aliases: const <String>['js'],
          capabilities: const <LanguageCapability>{LanguageCapability.syntax},
        ),
      );

      expect(
        () => registry.registerLanguage(
          LanguageExtensionDescriptor(
            id: LanguageId('other-js'),
            displayName: 'Other JavaScript',
            fileExtensions: const <String>['.js'],
            capabilities: const <LanguageCapability>{LanguageCapability.syntax},
          ),
        ),
        throwsStateError,
      );
      expect(
        () => registry.registerProvider(
          LanguageProviderDescriptor(
            id: 'unknown-language-provider',
            kind: LanguageProviderKind.semanticServer,
            languages: <LanguageId>{LanguageId('python')},
            capabilities: const <LanguageCapability>{
              LanguageCapability.definition,
            },
            processScope: LanguageProviderProcessScope.workspace,
            launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          ),
        ),
        throwsStateError,
      );
    });
  });

  test(
    'language settings are sparse and default every language to disabled',
    () {
      final rust = LanguageId('rust');
      final python = LanguageId('python');
      final settings = LanguageIntelligenceSettings().withLanguage(
        rust,
        const LanguageActivationSettings(
          enabled: true,
          structuralParserEnabled: true,
          semanticProviderId: 'rust-semantic',
          executablePath: r'C:\tools\rust-analyzer.exe',
          extraArgs: <String>['--stdio'],
        ),
      );

      expect(settings.forLanguage(rust).enabled, isTrue);
      expect(settings.forLanguage(rust).structuralParserEnabled, isTrue);
      expect(settings.forLanguage(python), LanguageActivationSettings.defaults);
      expect(
        LanguageIntelligenceSettings.defaults.forLanguage(rust).enabled,
        isFalse,
      );
      expect(
        LanguageIntelligenceSettings.defaults
            .forLanguage(rust, structuralParserDefaultEnabled: true)
            .structuralParserEnabled,
        isTrue,
      );
    },
  );

  test(
    'provider locations use scalar columns in a provider-neutral value type',
    () {
      final location = SourceLocation(
        workspaceId: 'workspace-1',
        path: 'lib/main.dart',
        range: const SourceRange(
          start: SourcePosition(line: 3, scalarColumn: 4),
          end: SourcePosition(line: 3, scalarColumn: 9),
        ),
      );

      expect(location.range.start.scalarColumn, 4);
      expect(location, equals(location.copyWith(path: 'lib/main.dart')));
    },
  );
}
