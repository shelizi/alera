import 'dart:io';

const _agentTypeSourcePath =
    'lib/src/features/agent_status/domain/agent_status.dart';
const _baselinePath = 'tool/quality/agent_extension_touchpoints.txt';
const _scanRoot = 'lib/src';
const _explicitManagedHookOwnershipToken = '_globalManagedHookAgentIds';
const _targetTouchpointCount = 5;
const _normalizerRoot = 'lib/src/features/agent_status/infra/normalizers';
const _managedHookRoot = 'lib/src/features/agent_status/infra/managed_hooks';
const _sharedHookPolicyPaths = <String>[
  'lib/src/features/agent_status/infra/agent_hook_event_normalizer.dart',
  'lib/src/features/agent_status/infra/managed_agent_hook_installer.dart',
  'lib/src/features/agent_status/infra/managed_agent_hook_descriptors.dart',
  'lib/src/features/agent_status/infra/managed_agent_hook_scripts.dart',
  'lib/src/features/agent_status/infra/normalizers/agent_hook_status_helpers.dart',
  'lib/src/features/agent_status/infra/normalizers/agent_hook_tool_preview.dart',
  'lib/src/features/agent_status/infra/normalizers/agent_hook_tool_snapshot.dart',
  'lib/src/features/agent_status/application/agent_hook_lifecycle_guard.dart',
  'lib/src/features/agent_status/application/agent_status_identity_resolver.dart',
];

void main() {
  final violations = <String>[];
  final agentMembers = _agentTypeMembers(violations);
  _validatePerAgentHookAdapters(agentMembers, violations);
  final actual = _findTouchpoints(agentMembers, violations);
  final baseline = _readBaseline(violations);

  final additions = actual.difference(baseline).toList()..sort();
  final stale = baseline.difference(actual).toList()..sort();

  if (additions.isNotEmpty) {
    violations.add(
      'New agent-extension touchpoint(s) are not allowed without first '
      'removing equivalent coupling:\n${additions.map((path) => '  + $path').join('\n')}',
    );
  }
  if (stale.isNotEmpty) {
    violations.add(
      'Agent-extension touchpoint baseline contains stale entries; ratchet the '
      'baseline down:\n${stale.map((path) => '  - $path').join('\n')}',
    );
  }

  if (violations.isNotEmpty) {
    stderr.writeln('Agent extension guard failed:');
    for (final violation in violations) {
      stderr.writeln(' - $violation');
    }
    exitCode = 1;
    return;
  }

  stdout.writeln(
    'Agent extension guard passed: ${actual.length} manual touchpoint file(s) '
    '(target <= $_targetTouchpointCount; ratchet prevents growth).',
  );
}

void _validatePerAgentHookAdapters(
  Set<String> agentMembers,
  List<String> violations,
) {
  for (final member in agentMembers) {
    final normalizerPath =
        '$_normalizerRoot/${member}_agent_hook_normalizer.dart';
    if (!File(normalizerPath).existsSync()) {
      violations.add(
        'AgentType.$member must own a dedicated hook adapter file: '
        '$normalizerPath',
      );
    }
    final managedHookPath =
        '$_managedHookRoot/${member}_managed_agent_hook.dart';
    if (!File(managedHookPath).existsSync()) {
      violations.add(
        'AgentType.$member must own a dedicated managed-hook adapter file: '
        '$managedHookPath',
      );
    }
  }

  final explicitAgentMember = RegExp(r'\bAgentType\.[A-Za-z_][A-Za-z0-9_]*\b');
  for (final path in _sharedHookPolicyPaths) {
    final file = File(path);
    if (!file.existsSync()) {
      violations.add('Missing shared hook policy file: $path');
      continue;
    }
    final matches =
        explicitAgentMember
            .allMatches(file.readAsStringSync())
            .map((match) => match.group(0)!)
            .where((member) => member != 'AgentType.values')
            .toSet()
            .toList()
          ..sort();
    if (matches.isNotEmpty) {
      violations.add(
        '$path contains agent-specific branching (${matches.join(', ')}). '
        'Move that policy into the matching per-agent hook adapter.',
      );
    }
  }
}

Set<String> _agentTypeMembers(List<String> violations) {
  final file = File(_agentTypeSourcePath);
  if (!file.existsSync()) {
    violations.add('Missing AgentType source: $_agentTypeSourcePath');
    return <String>{};
  }
  final source = file.readAsStringSync();
  final body = RegExp(
    r'enum\s+AgentType\s*\([^)]*\)\s*\{(.*?);',
    dotAll: true,
  ).firstMatch(source)?.group(1);
  if (body == null) {
    violations.add('Could not parse AgentType enum from $_agentTypeSourcePath');
    return <String>{};
  }
  final members = RegExp(
    r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(',
    multiLine: true,
  ).allMatches(body).map((match) => match.group(1)!).toSet();
  if (members.isEmpty) {
    violations.add('AgentType enum has no parsed members.');
  }
  return members;
}

Set<String> _findTouchpoints(
  Set<String> agentMembers,
  List<String> violations,
) {
  final root = Directory(_scanRoot);
  if (!root.existsSync()) {
    violations.add('Missing agent extension scan root: $_scanRoot');
    return <String>{};
  }
  if (agentMembers.isEmpty) {
    return <String>{};
  }

  final escapedMembers = agentMembers.map(RegExp.escape).join('|');
  final explicitAgentMember = RegExp('\\bAgentType\\.(?:$escapedMembers)\\b');
  final touchpoints = <String>{};
  for (final entity in root.listSync(recursive: true, followLinks: false)) {
    if (entity is! File || !entity.path.endsWith('.dart')) {
      continue;
    }
    final path = _displayPath(entity);
    if (path.endsWith('.g.dart') ||
        path.endsWith('.mapper.dart') ||
        path.endsWith('agent_descriptor_snapshot.dart')) {
      continue;
    }
    final source = entity.readAsStringSync();
    if (explicitAgentMember.hasMatch(source) ||
        source.contains(_explicitManagedHookOwnershipToken)) {
      touchpoints.add(path);
    }
  }
  return touchpoints;
}

Set<String> _readBaseline(List<String> violations) {
  final file = File(_baselinePath);
  if (!file.existsSync()) {
    violations.add('Missing agent extension baseline: $_baselinePath');
    return <String>{};
  }
  return file
      .readAsLinesSync()
      .map((line) => line.trim())
      .where((line) => line.isNotEmpty && !line.startsWith('#'))
      .toSet();
}

String _displayPath(File file) {
  final root = Directory.current.absolute.path.replaceAll('\\', '/');
  final absolute = file.absolute.path.replaceAll('\\', '/');
  if (absolute.startsWith('$root/')) {
    return absolute.substring(root.length + 1);
  }
  return absolute;
}
