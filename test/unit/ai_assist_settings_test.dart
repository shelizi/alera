import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_registry.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('exposes every operation label and agent mapping', () {
    expect(
      AiAssistOperation.values.map((operation) => operation.label),
      <String>[
        'Commit Messages',
        'Pull Request Details',
        'Branch Names',
        'Reading Diffs',
        'Workspace Identity',
        'Agent Titles',
        'Speech Messages',
      ],
    );
    expect(AiAssistAgent.codex.agentType, AgentType.codex);
    expect(AiAssistAgent.claude.agentType, AgentType.claude);
    expect(AiAssistAgent.copilot.agentType, AgentType.copilot);
    expect(AiAssistAgent.cursor.agentType, AgentType.cursor);
    expect(AiAssistAgent.agy.agentType, AgentType.agy);
    expect(AiAssistAgent.opencode.agentType, AgentType.opencode);
    expect(AiAssistAgent.opencode2.agentType, AgentType.opencode2);
    expect(AiAssistAgent.pi.agentType, AgentType.pi);
    expect(AiAssistAgent.amp.agentType, AgentType.amp);
    expect(AiAssistAgent.grok.agentType, AgentType.grok);
    expect(AiAssistAgent.devin.agentType, AgentType.devin);
    expect(AiAssistAgent.fx.agentType, AgentType.fx);
    expect(AiAssistAgent.custom.agentType, isNull);
    expect(
      AiAssistAgent.values.map((agent) => agent.label),
      containsAll(<String>[
        'Codex',
        'Claude Code',
        'GitHub Copilot',
        'Cursor',
        'Antigravity',
        'OpenCode',
        'OpenCode 2',
        'Pi',
        'Amp',
        'Grok Build',
        'Devin',
        'fx',
        'Custom Command',
      ]),
    );
  });

  test('AI Assist registry is the complete selectable-agent source', () {
    final registered = aiAssistCapabilities.values
        .map((capability) => aiAssistAgentForType(capability.agentType)!)
        .toSet();
    final nonCustom = AiAssistAgent.values
        .where((agent) => agent != AiAssistAgent.custom)
        .toSet();

    expect(registered, nonCustom);
    expect(selectableAiAssistAgents, <AiAssistAgent>[
      ...aiAssistCapabilities.keys.map((type) => aiAssistAgentForType(type)!),
      AiAssistAgent.custom,
    ]);
    for (final spec in aiAssistCapabilities.values) {
      expect(aiAssistAgentForType(spec.agentType)?.agentType, spec.agentType);
    }
  });

  test(
    'AI Assist capabilities reuse the canonical agent descriptor identity',
    () {
      for (final entry in aiAssistCapabilities.entries) {
        final descriptor = agentDescriptorFor(entry.key);
        final capability = entry.value;

        expect(capability.agentType, entry.key);
        expect(capability.id, descriptor.id);
        expect(capability.label, descriptor.displayName);
        expect(capability.binary, descriptor.defaultCommand);
      }
    },
  );

  test('AI Assist capability lookup uses AgentType directly', () {
    expect(
      aiAssistCapabilityFor(AgentType.codex),
      same(aiAssistCapabilities[AgentType.codex]),
    );
    expect(aiAssistCapabilityFor(null), isNull);
  });

  test('AI Assist settings expose canonical AgentType identity', () {
    const settings = AiAssistSettings();
    const prompt = AiAssistPromptSettings(agent: AiAssistAgent.devin);

    expect(settings.agentType, AgentType.codex);
    expect(prompt.agentType, AgentType.devin);
  });

  test('AI Assist model helpers accept canonical AgentType', () {
    const settings = AiAssistSettings(
      selectedModelByAgent: <AiAssistAgent, String>{
        AiAssistAgent.devin: 'gpt-5-5-high',
      },
    );

    expect(modelIdForAgentType(settings, AgentType.devin), 'gpt-5-5-high');
    expect(modelsForAgentType(AgentType.devin, settings), isNotEmpty);
    expect(
      defaultModelIdForAgentType(AgentType.devin, settings),
      isA<String>(),
    );
  });

  test('parses Devin model-list output', () {
    final models = parseDevinModels('''
GPT-5.5 (gpt-5.5)
  gpt-5-5-low      GPT-5.5 Low Thinking  [272K context]
  gpt-5-5-high     GPT-5.5 High Thinking  [272K context]

Claude Sonnet 4.6 (claude-sonnet-4.6)
  claude-sonnet-4-6  Claude Sonnet 4.6  [200K context]
''');

    expect(models.map((model) => (model.id, model.label)), <(String, String)>[
      ('gpt-5-5-low', 'GPT-5.5 Low Thinking'),
      ('gpt-5-5-high', 'GPT-5.5 High Thinking'),
      ('claude-sonnet-4-6', 'Claude Sonnet 4.6'),
    ]);
  });

  test('registers Devin non-interactive AI Assist command contract', () {
    final spec = aiAssistCapabilities[AgentType.devin]!;

    expect(spec.binary, 'devin');
    expect(spec.modelsCommand, <String>['models', 'list']);
    expect(spec.modelCanInherit, isTrue);
    expect(
      spec.buildArgs(
        prompt: 'Summarize the change',
        model: 'gpt-5-5-high',
        thinkingLevel: null,
        timeoutSeconds: 120,
      ),
      <String>[
        '--print',
        '--permission-mode',
        'auto',
        '--respect-workspace-trust',
        'false',
        '--model',
        'gpt-5-5-high',
        'Summarize the change',
      ],
    );
  });

  test('round-trips settings through the generated mapper', () {
    const settings = AiAssistSettings(
      agent: .custom,
      customCommand: 'generate',
      selectedModelByAgent: <AiAssistAgent, String>{
        AiAssistAgent.codex: 'gpt-5',
      },
      selectedThinkingByOperation: <AiAssistOperation, Map<String, String>>{
        AiAssistOperation.commitMessage: <String, String>{'gpt-5': 'high'},
      },
      promptSettingsByOperation: <AiAssistOperation, AiAssistPromptSettings>{
        AiAssistOperation.commitMessage: AiAssistPromptSettings(
          agent: .claude,
          model: 'opus',
        ),
      },
    );

    expect(AiAssistSettings.fromJson(settings.toMap()), settings);
  });

  test('resolves prompt agent and model overrides independently', () {
    const settings = AiAssistSettings(
      agent: .codex,
      selectedModelByAgent: <AiAssistAgent, String>{
        AiAssistAgent.codex: 'gpt-global',
        AiAssistAgent.claude: 'sonnet',
        AiAssistAgent.opencode: 'provider/reading-model',
      },
      promptSettingsByOperation: <AiAssistOperation, AiAssistPromptSettings>{
        AiAssistOperation.commitMessage: AiAssistPromptSettings(agent: .claude),
        AiAssistOperation.pullRequestDetails: AiAssistPromptSettings(
          model: 'gpt-pull-request',
        ),
        AiAssistOperation.readingDiff: AiAssistPromptSettings(agent: .opencode),
      },
    );

    expect(settings.agentFor(.commitMessage), AiAssistAgent.claude);
    expect(settings.modelForOperation(.commitMessage), 'sonnet');
    expect(settings.agentFor(.pullRequestDetails), AiAssistAgent.codex);
    expect(settings.modelForOperation(.pullRequestDetails), 'gpt-pull-request');
    expect(settings.agentFor(.readingDiff), AiAssistAgent.opencode);
    expect(settings.modelForOperation(.readingDiff), 'provider/reading-model');
    expect(settings.modelForOperation(.workspaceIdentity), 'gpt-global');
  });

  test('keeps operation reasoning overrides isolated with global fallback', () {
    const settings = AiAssistSettings(
      selectedThinkingByModel: <String, String>{'gpt-5.5': 'low'},
      selectedThinkingByOperation: <AiAssistOperation, Map<String, String>>{
        AiAssistOperation.commitMessage: <String, String>{'gpt-5.5': 'high'},
      },
    );

    expect(settings.thinkingForOperation(.commitMessage, 'gpt-5.5'), 'high');
    expect(
      settings.thinkingForOperation(.pullRequestDetails, 'gpt-5.5'),
      'low',
    );
  });

  test('reports whether prompt settings inherit each global value', () {
    const inherited = AiAssistPromptSettings(model: '  ');
    const overridden = AiAssistPromptSettings(agent: .claude, model: 'sonnet');

    expect(inherited.inheritsAgent, isTrue);
    expect(inherited.inheritsModel, isTrue);
    expect(overridden.inheritsAgent, isFalse);
    expect(overridden.inheritsModel, isFalse);
  });
}
