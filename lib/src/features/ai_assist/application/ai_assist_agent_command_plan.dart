part of 'ai_assist_agent_runner.dart';

extension on CliAgentTaskRunner {
  Future<_AgentTaskCommandPlan> _planCommand(
    AgentTaskRunRequest request,
    Map<String, String> environment,
  ) async {
    final settings = request.settings;
    final useCustomCommand =
        request.useCustomCommand ||
        (request.agentType == null && settings.agentType == null);
    final agentType = useCustomCommand
        ? null
        : request.agentType ?? settings.agentType;
    if (request.accessPolicy == AgentTaskAccessPolicy.diffOnly) {
      requireDiffOnlyAiAssistAgentType(agentType);
    }
    if (useCustomCommand || agentType == null) {
      return _planCustomCommand(settings.customCommand, request.prompt);
    }
    final spec = aiAssistCapabilityFor(agentType);
    if (spec == null) {
      throw AiAssistException(
        '${agentDisplayName(agentType)} does not support AI Assist.',
      );
    }
    if (request.outputContract != AgentTaskOutputContract.plainText &&
        request.accessPolicy == AgentTaskAccessPolicy.repositoryReadOnly &&
        (!spec.supportsRepositoryRead || !spec.readOnlyGuarantee)) {
      throw AiAssistException(
        '${spec.label} cannot guarantee read-only repository access for this task.',
      );
    }
    final model = modelForAgentType(
      agentType,
      request.model ??
          modelIdForAgentType(settings, agentType) ??
          defaultModelIdForAgentType(agentType, settings),
      extraModels: discoveredModelsForAgentType(settings, agentType),
    );
    final thinking =
        request.reasoning ??
        settings.thinkingForModel(model.id) ??
        model.defaultThinkingLevel;
    if ((request.outputContract != AgentTaskOutputContract.plainText ||
            spec.promptDelivery == AiPromptDelivery.argv) &&
        utf8.encode(request.prompt).length > spec.maxPromptBytes) {
      throw AiAssistException(
        '${spec.label} cannot receive large prompts safely. Choose an agent that supports stdin prompts or reduce the staged diff.',
      );
    }
    Directory? promptDirectory;
    File? outputFile;
    final environmentOverrides = <String, String>{
      ...?spec.environmentOverridesForModel?.call(model.id),
    };
    var deliveredPrompt = '';
    if (spec.promptDelivery == AiPromptDelivery.argv) {
      deliveredPrompt = request.prompt;
    } else if (spec.promptDelivery == AiPromptDelivery.promptFile) {
      promptDirectory = await Directory.systemTemp.createTemp(
        'alera-ai-assist-',
      );
      try {
        final promptFile = File(
          '${promptDirectory.path}${Platform.pathSeparator}prompt.txt',
        );
        await promptFile.writeAsString(request.prompt, flush: true);
        deliveredPrompt = promptFile.path;
        if (spec.preparePromptFileEnvironment case final prepare?) {
          environmentOverrides.addAll(
            await prepare(
              promptDirectoryPath: promptDirectory.path,
              environment: environment,
              authenticationOnly:
                  request.accessPolicy == AgentTaskAccessPolicy.diffOnly,
            ),
          );
        }
      } catch (_) {
        try {
          await promptDirectory.delete(recursive: true);
        } catch (_) {}
        rethrow;
      }
    }
    var args = spec.buildArgs(
      prompt: deliveredPrompt,
      model: model.id,
      thinkingLevel: thinking,
      timeoutSeconds: settings.timeoutSeconds,
    );
    if (request.outputContract != AgentTaskOutputContract.plainText) {
      final schema = request.outputSchema?.trim();
      if (schema == null || schema.isEmpty) {
        throw const AiAssistException(
          'Structured generation requires an output schema.',
        );
      }
      promptDirectory ??= await Directory.systemTemp.createTemp(
        'alera-agent-task-',
      );
      switch (spec.nativeStructuredOutput) {
        case AiNativeStructuredOutput.none:
          break;
        case AiNativeStructuredOutput.codexSchemaFile:
          final schemaFile = File(p.join(promptDirectory.path, 'schema.json'));
          outputFile = File(p.join(promptDirectory.path, 'result.json'));
          await schemaFile.writeAsString(schema, flush: true);
          args = <String>[
            ...args,
            '--output-schema',
            schemaFile.path,
            '--output-last-message',
            outputFile.path,
          ];
        case AiNativeStructuredOutput.claudeJsonSchema:
          _setJsonOutputFormat(args);
          args = <String>[
            ...args,
            '--json-schema',
            schema,
            '--no-session-persistence',
          ];
        case AiNativeStructuredOutput.jsonSchemaArgument:
          _setJsonOutputFormat(args);
          args = <String>[...args, '--json-schema', schema];
      }
    }
    Map<String, String>? exactEnvironment;
    if (request.accessPolicy == AgentTaskAccessPolicy.diffOnly) {
      final authCredentialsStore =
          spec.diffOnlyAccess ==
              AiAssistDiffOnlyAccess.codexRestrictedFilesystem
          ? await codexAuthCredentialsStore(environment)
          : null;
      final execution = planDiffOnlyAiAssistExecution(
        spec: spec,
        arguments: args,
        environment: <String, String>{...environment, ...environmentOverrides},
        codexAuthCredentialsStore: authCredentialsStore,
      );
      args = execution.arguments;
      exactEnvironment = execution.environment;
    }
    return _AgentTaskCommandPlan(
      binary: spec.binary,
      args: args,
      stdinPayload: spec.promptDelivery == AiPromptDelivery.stdin
          ? request.prompt
          : null,
      label: spec.label,
      promptDirectory: promptDirectory,
      environmentOverrides: environmentOverrides,
      exactEnvironment: exactEnvironment,
      outputFile: outputFile,
    );
  }

  void _setJsonOutputFormat(List<String> args) {
    final formatIndex = args.indexOf('--output-format');
    if (formatIndex >= 0 && formatIndex + 1 < args.length) {
      args[formatIndex + 1] = 'json';
      return;
    }
    args.addAll(const <String>['--output-format', 'json']);
  }

  _AgentTaskCommandPlan _planCustomCommand(String template, String prompt) {
    final tokens = _tokenizeCommandTemplate(template);
    if (tokens.isEmpty) {
      throw const AiAssistException('Custom command is empty.');
    }
    final usesPlaceholder = tokens.any((token) => token.contains('{prompt}'));
    final substituted = tokens
        .map((token) => token.replaceAll('{prompt}', prompt))
        .toList(growable: false);
    return _AgentTaskCommandPlan(
      binary: substituted.first,
      args: substituted.skip(1).toList(growable: false),
      stdinPayload: usesPlaceholder ? null : prompt,
      label: substituted.first,
    );
  }

  List<String> _tokenizeCommandTemplate(String template) {
    final matches = RegExp(r'''"([^"]*)"|'([^']*)'|(\S+)''')
        .allMatches(template);
    return matches
        .map((match) => match.group(1) ?? match.group(2) ?? match.group(3)!)
        .toList(growable: false);
  }
}

Future<void> _deleteTemporaryDirectory(Directory directory) async {
  for (var attempt = 0; attempt < 3; attempt += 1) {
    try {
      if (await directory.exists()) {
        await directory.delete(recursive: true);
      }
      return;
    } catch (_) {
      if (attempt < 2) {
        await Future.pause(const Duration(milliseconds: 100));
      }
    }
  }
}

class const _AgentTaskCommandPlan({
  required final String binary,
  required final List<String> args,
  required final String? stdinPayload,
  required final String label,
  final Map<String, String> environmentOverrides = const <String, String>{},
  final Map<String, String>? exactEnvironment,
  final Directory? promptDirectory,
  final File? outputFile,
}) {
  Future<void> dispose() async {
    final directory = promptDirectory;
    if (directory == null) {
      return;
    }
    await _deleteTemporaryDirectory(directory);
  }
}
