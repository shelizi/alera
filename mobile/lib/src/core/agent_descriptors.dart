/// Canonical mobile agent descriptor data.
///
/// These tables mirror `rust/alera-core/src/agent_descriptor/table.rs` for
/// canonical ids, display names, and aliases. The FRB bridge to `alera_native`
/// is not wired into `alera_mobile`, so this mirror is maintained manually.
const Map<String, String> canonicalAgentDisplayNames = <String, String>{
  'codex': 'Codex',
  'claude': 'Claude Code',
  'copilot': 'GitHub Copilot',
  'cursor': 'Cursor',
  'agy': 'Antigravity',
  'opencode': 'OpenCode',
  'opencode2': 'OpenCode 2',
  'pi': 'Pi',
  'amp': 'Amp',
  'grok': 'Grok Build',
  'devin': 'Devin',
  'fx': 'fx',
};

String canonicalAgentId(String agentType) {
  final normalized = agentType.trim().toLowerCase();
  return normalized == 'antigravity' ? 'agy' : normalized;
}
