# Alera MCP — cross-project navigation (TDD)

## Scope

The built-in MCP agent may navigate *visible, active and nonarchived* Alera
workspaces in other projects without obtaining another project's Agent or PTY
control credentials. This is a UI operation, not an Agent-message or shell API.

## Tool contracts

The following MCP tools support project-scoped navigation:

| Tool | Parameters | Effect |
| --- | --- | --- |
| `alera.list_projects` | optional `query`, `limit` | Discover project IDs |
| `alera.list_workspaces` | optional `projectId`, `limit` | Discover Workspace IDs |
| `alera.list_tabs` | optional `workspaceId`, `projectId`, `limit` | Read only metadata for current or explicitly selected Workspace |
| `alera.focus_workspace` | required `workspaceId`, optional `projectId`, `windowId` | Select existing Workspace without creating an initial Terminal |
| `alera.select_tab` | required `workspaceId`, `tabId`; optional `projectId`, `windowId` | Select an existing Tab after target validation |
| `alera.open_file` | required `relativePath`; optional `workspaceId`, `projectId`, `openMode`, `windowId` | Open and select file-backed Editor Tab |
| `alera.reveal_file` | required `relativePath`; optional `workspaceId`, `projectId`, `windowId` | Reveal target file in Explorer without opening it |

`openMode`: `reuse` is the default. `new` creates an independent permanent
Editor Tab, even if the target file is already open.

For any target Workspace other than the calling Agent's own, **both**
`workspaceId` and `projectId` must be provided; the Host checks that they
belong together, that the project exists, and that the Workspace is active and
not archived. `alera.list_tabs` enforces the same explicit scope.

Example:

```json
{
  "name": "alera.open_file",
  "arguments": {
    "projectId": "project-b",
    "workspaceId": "workspace-b",
    "relativePath": "lib/main.dart",
    "openMode": "new"
  }
}
```

## Safety invariants

- The caller's Host-minted per-PTY token and live source Workspace still
  authorize every UI command. Target IDs do not impersonate a target Agent.
- Cross-project UI navigation is an allowlist. `alera.focus_agent`, settings
  changes, Agent output and Agent messaging remain constrained to the caller's
  original scope; no target PTY input is forwarded by navigation tools.
- Host validates the target's durable Project, Workspace and (for
  `select_tab`) Tab ownership before delivering an event to a local App.
  Unknown/removed/archived/mismatched targets fail closed.
- The App independently checks its live visible Workspace/Project collection.
  `select_tab` and file opening confirm the selected Tab after a rendered frame.
  A status of `accepted` is *not* `applied`; only the registered local App
  can acknowledge the command, and a `select_tab` acknowledgment must match
  the requested Tab ID.
- File paths remain workspace-relative and must exist within the resolved
  target Workspace, including symlink boundary checks.
- An explicit `windowId` is required when multiple Workbench windows are
  registered; no arbitrary window is selected silently.

## Red → green coverage

- `mcp::tests::tool_arguments_fail_closed_on_unknown_or_unbounded_values`:
  RED on the missing `alera.focus_workspace`, GREEN after registering the
  strict contracts.
- `WorkspaceTabService opening a requested new editor tab...`: RED on missing
  `forceNew`, GREEN on new independent Tab + unchanged default reuse.
- `mcp_cross_project_navigation_tests`: Host target, App ACK, foreign Tab,
  missing project, archived Workspace and Agent scope.
- `MCP cross-project navigation ACK follows visible Workspace and Tab`:
  Flutter Shell navigation with an actual Runtime Host event and a fake local
  App endpoint, checking both visible selection and rejected targets.

## Release status

This implementation belongs to the built-in MCP PR and does not by itself
change the previously documented MCP native-host/multi-platform release gates.
Run current PR checks and required native Host/PTY acceptance before merging
to `main` or shipping.
