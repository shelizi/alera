import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_contribution.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/settings/domain/settings_ownership.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:alera/src/features/workbench/domain/terminal_search.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('canonical AgentType descriptor mapping is a total lookup', () {
    for (final agent in AgentType.values) {
      expect(agentDescriptorFor(agent).id, canonicalAgentKey(agent.key));
    }
  });

  test('source position/range/location equality and copy semantics', () {
    const start = SourcePosition(line: 2, scalarColumn: 4);
    const sameStart = SourcePosition(line: 2, scalarColumn: 4);
    const end = SourcePosition(line: 3, scalarColumn: 1);
    expect(start, sameStart);
    expect(start.hashCode, sameStart.hashCode);
    expect(start, isNot(end));

    const range = SourceRange(start: start, end: end);
    const sameRange = SourceRange(start: sameStart, end: end);
    expect(range, sameRange);
    expect(range.hashCode, sameRange.hashCode);

    const location = SourceLocation(
      workspaceId: 'workspace-1',
      path: 'lib/main.dart',
      range: range,
    );
    const sameLocation = SourceLocation(
      workspaceId: 'workspace-1',
      path: 'lib/main.dart',
      range: sameRange,
    );
    expect(location, sameLocation);
    expect(location.hashCode, sameLocation.hashCode);
    expect(location.copyWith(path: 'lib/other.dart').path, 'lib/other.dart');
    expect(location.copyWith().workspaceId, 'workspace-1');
    expect(location.copyWith(workspaceId: 'workspace-2'), isNot(location));
  });

  test('language contribution requires a language entry', () {
    expect(
      () => LanguageExtensionContribution(
        languages: <LanguageExtensionDescriptor>[],
        providers: <LanguageProviderDescriptor>[],
      ),
      throwsArgumentError,
    );
  });

  test('language metadata rejects invalid optional and default values', () {
    final rust = LanguageId('rust');
    expect(
      () => LanguageExtensionDescriptor(
        id: rust,
        displayName: 'Rust',
        semanticProviderIds: const <String>['rust-analyzer'],
        defaultSemanticProviderId: 'missing',
      ),
      throwsArgumentError,
    );
    expect(
      () => LanguageExtensionDescriptor(
        id: rust,
        displayName: 'Rust',
        fileExtensions: const <String>['.'],
      ),
      throwsArgumentError,
    );
    expect(
      () => LanguageExtensionDescriptor(id: rust, displayName: '   '),
      throwsArgumentError,
    );
  });

  test(
    'language provider rejects empty capabilities and unsafe executables',
    () {
      final rust = LanguageId('rust');
      LanguageProviderDescriptor create({
        Iterable<LanguageId>? languages,
        Iterable<LanguageCapability>? capabilities,
        Iterable<String> executableCandidates = const <String>[],
      }) => LanguageProviderDescriptor(
        id: 'rust-analyzer',
        kind: LanguageProviderKind.semanticServer,
        languages: languages ?? <LanguageId>[rust],
        capabilities: capabilities ?? const <LanguageCapability>[.hover],
        processScope: LanguageProviderProcessScope.workspace,
        launchPolicy: LanguageProviderLaunchPolicy.explicit,
        executableCandidates: executableCandidates,
      );

      expect(() => create(languages: <LanguageId>[]), throwsArgumentError);
      expect(
        () => create(capabilities: <LanguageCapability>[]),
        throwsArgumentError,
      );
      expect(
        () => create(executableCandidates: <String>['/usr/bin/rust-analyzer']),
        throwsArgumentError,
      );
    },
  );

  test('language settings JSON factories decode defaults', () {
    final activation = LanguageActivationSettings.fromJson(
      const <String, Object?>{},
    );
    expect(activation.enabled, false);
    final settings = LanguageIntelligenceSettings.fromJson(
      const <String, Object?>{},
    );
    expect(settings.languages, isEmpty);
    expect(settings.forLanguage(LanguageId('rust')).enabled, isFalse);
  });

  test('runtime ownership preserves populated and trimmed optional fields', () {
    const settings = AiAssistSettings(
      selectedModelByAgent: <String, String>{'codex': 'gpt'},
      selectedThinkingByOperation: <AiAssistOperation, Map<String, String>>{
        AiAssistOperation.commitMessage: <String, String>{'gpt': 'high'},
      },
      promptSettingsByOperation: <AiAssistOperation, AiAssistPromptSettings>{
        AiAssistOperation.commitMessage: AiAssistPromptSettings(
          agent: 'claude',
          model: '  opus  ',
        ),
      },
    );
    final map = runtimeAiAssistSettings(settings);
    expect(map['selectedModelByAgent'], <String, String>{'codex': 'gpt'});
    expect(map['selectedThinkingByOperation'], <String, Object?>{
      'commitMessage': <String, String>{'gpt': 'high'},
    });
    expect(
      (map['promptSettingsByOperation']! as Map)['commitMessage'],
      <String, Object?>{'agent': 'claude', 'model': 'opus'},
    );
  });

  test(
    'text action explicitly selects agent and terminal lines are indexed',
    () {
      const action = TextAction(
        id: 'explain',
        name: 'Explain',
        prompt: 'Explain selection',
        agentOverride: 'claude',
      );
      expect(
        action.effectiveAgentType(const AiAssistSettings()),
        AgentType.claude,
      );
      const terminalLine = TerminalSearchLine(
        id: 'line-1',
        index: 1,
        text: 'hi',
      );
      expect(terminalLine.id, 'line-1');
      expect(terminalLine.index, 1);
      expect(terminalLine.text, 'hi');
    },
  );
}
