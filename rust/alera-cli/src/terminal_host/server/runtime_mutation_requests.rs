use serde_json::Value;

use crate::managed_workspace::{
    ManagedWorkspaceRemoveRequest, ManagedWorkspaceSwitchBranchRequest,
};
use crate::terminal_host::host_error::HostResult;

use super::request_payloads::parse_payload;
use super::request_route_policy::{
    request_route_policy, RuntimeMutationPolicy, SerializedRuntimeMutation,
};
use super::requests::require_string_key;
use super::runtime_mutations::RuntimeMutationRequest;
use super::ServerActor;

impl ServerActor {
    pub(super) fn try_start_serialized_runtime_mutation(
        &mut self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<bool> {
        let RuntimeMutationPolicy::Serialized(kind) =
            request_route_policy(request_type).runtime_mutation
        else {
            return Ok(false);
        };

        self.require_auth(client_id)?;
        self.require_request_allowed(client_id, request_type)?;

        let request = match kind {
            SerializedRuntimeMutation::RemoveProject => RuntimeMutationRequest::RemoveProject {
                project_id: require_string_key(payload, "id")?,
            },
            SerializedRuntimeMutation::RemoveWorkspace => RuntimeMutationRequest::RemoveWorkspace {
                workspace_id: require_string_key(payload, "id")?,
                cascade_tabs: payload
                    .get("cascadeTabs")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            },
            SerializedRuntimeMutation::RemoveProjectWorkspaces => {
                RuntimeMutationRequest::RemoveProjectWorkspaces {
                    project_id: require_string_key(payload, "projectId")?,
                }
            }
            SerializedRuntimeMutation::RemoveManagedWorkspace => {
                RuntimeMutationRequest::RemoveManagedWorkspace {
                    request: parse_payload::<ManagedWorkspaceRemoveRequest>(payload)?,
                }
            }
            SerializedRuntimeMutation::SwitchWorkspaceBranch => {
                RuntimeMutationRequest::SwitchWorkspaceBranch {
                    request: parse_payload::<ManagedWorkspaceSwitchBranchRequest>(payload)?,
                }
            }
            SerializedRuntimeMutation::RemoveTab => {
                let tab_id = require_string_key(payload, "id")?;
                self.cancel_agent_title_job(&tab_id);
                RuntimeMutationRequest::RemoveTab { tab_id }
            }
            SerializedRuntimeMutation::RemoveWorkspaceTabs => {
                RuntimeMutationRequest::RemoveWorkspaceTabs {
                    workspace_id: require_string_key(payload, "workspaceId")?,
                }
            }
            SerializedRuntimeMutation::SleepWorkspace => RuntimeMutationRequest::SleepWorkspace {
                workspace_id: require_string_key(payload, "workspaceId")?,
            },
        };

        self.start_runtime_mutation(client_id, request_id, request);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_serialized_route_has_a_typed_mutation_kind() {
        let expected = [
            ("project.remove", SerializedRuntimeMutation::RemoveProject),
            (
                "workspace.remove",
                SerializedRuntimeMutation::RemoveWorkspace,
            ),
            (
                "workspace.removeForProject",
                SerializedRuntimeMutation::RemoveProjectWorkspaces,
            ),
            (
                "workspace.removeManaged",
                SerializedRuntimeMutation::RemoveManagedWorkspace,
            ),
            (
                "workspace.switchBranch",
                SerializedRuntimeMutation::SwitchWorkspaceBranch,
            ),
            ("tab.remove", SerializedRuntimeMutation::RemoveTab),
            (
                "tab.removeForWorkspace",
                SerializedRuntimeMutation::RemoveWorkspaceTabs,
            ),
            ("workspace.sleep", SerializedRuntimeMutation::SleepWorkspace),
        ];

        for (request_type, expected_kind) in expected {
            assert_eq!(
                request_route_policy(request_type).runtime_mutation,
                RuntimeMutationPolicy::Serialized(expected_kind),
                "{request_type}"
            );
        }
    }
}
