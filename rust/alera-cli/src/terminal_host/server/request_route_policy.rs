#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeMutationPolicy {
    Available,
    Conflicts,
    Serialized(SerializedRuntimeMutation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SerializedRuntimeMutation {
    RemoveProject,
    RemoveWorkspace,
    RemoveProjectWorkspaces,
    RemoveManagedWorkspace,
    SwitchWorkspaceBranch,
    RemoveTab,
    RemoveWorkspaceTabs,
    SleepWorkspace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PostResponseAction {
    None,
    Restart,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeferredReadRoute {
    ProjectConfigEffective,
    ProjectBranchesList,
    WorkspaceRepositoryWebUrl,
    HostDirectoryList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RequestRoutePolicy {
    pub(super) mobile_allowed: bool,
    pub(super) runtime_mutation: RuntimeMutationPolicy,
    pub(super) post_response: PostResponseAction,
    pub(super) deferred_read: Option<DeferredReadRoute>,
}

const LOCAL: RequestRoutePolicy = RequestRoutePolicy {
    mobile_allowed: false,
    runtime_mutation: RuntimeMutationPolicy::Available,
    post_response: PostResponseAction::None,
    deferred_read: None,
};

const MOBILE: RequestRoutePolicy = RequestRoutePolicy {
    mobile_allowed: true,
    ..LOCAL
};

const LOCAL_CONFLICT: RequestRoutePolicy = RequestRoutePolicy {
    runtime_mutation: RuntimeMutationPolicy::Conflicts,
    ..LOCAL
};

const MOBILE_CONFLICT: RequestRoutePolicy = RequestRoutePolicy {
    mobile_allowed: true,
    ..LOCAL_CONFLICT
};

/// Cross-cutting system policy for one terminal-host request name.
///
/// Domain dispatch remains in the request modules. This registry owns only
/// policy that otherwise drifts across authorization, mutation admission, and
/// post-response lifecycle handling.
pub(super) fn request_route_policy(request_type: &str) -> RequestRoutePolicy {
    if request_type.starts_with("codex.") {
        let runtime_mutation = if matches!(
            request_type,
            "codex.thread.list"
                | "codex.threads.list"
                | "codex.session.list"
                | "codex.thread.history"
                | "codex.thread.turns.list"
                | "codex.session.history"
                | "codex.thread.snapshot"
                | "codex.thread.items.list"
                | "codex.model.list"
                | "codex.collaborationModes.list"
                | "codex.skills.list"
                | "codex.apps.list"
                | "codex.turn.interrupt"
        ) {
            RuntimeMutationPolicy::Available
        } else {
            RuntimeMutationPolicy::Conflicts
        };
        return RequestRoutePolicy {
            runtime_mutation,
            ..LOCAL
        };
    }

    match request_type {
        "host.restart" => RequestRoutePolicy {
            mobile_allowed: true,
            post_response: PostResponseAction::Restart,
            ..LOCAL
        },
        "host.shutdown" => RequestRoutePolicy {
            post_response: PostResponseAction::Shutdown,
            ..LOCAL
        },

        "automation.approve"
        | "automation.import"
        | "automation.restore"
        | "automation.resume"
        | "automation.runNow"
        | "automation.upsert"
        | "project.register"
        | "project.rename"
        | "projectConfig.remove"
        | "projectConfig.upsert"
        | "tab.rename"
        | "terminal.attach"
        | "terminal.create"
        | "terminal.restart"
        | "terminate"
        | "workbenchViewPrefs.update"
        | "workspace.createManaged"
        | "workspace.rename"
        | "workspace.setPinned"
        | "workspaceRelation.link"
        | "workspaceRelation.unlink"
        | "workspaceSection.create"
        | "workspaceSection.remove"
        | "workspaceSection.setForWorkspace"
        | "workspaceTag.create"
        | "workspaceTag.remove"
        | "workspaceTag.setForWorkspace"
        | "write" => MOBILE_CONFLICT,

        "project.remove" => serialized_mobile(SerializedRuntimeMutation::RemoveProject),
        "tab.remove" => serialized_mobile(SerializedRuntimeMutation::RemoveTab),
        "workspace.removeManaged" => {
            serialized_mobile(SerializedRuntimeMutation::RemoveManagedWorkspace)
        }
        "workspace.sleep" => serialized_mobile(SerializedRuntimeMutation::SleepWorkspace),

        "projectConfig.effective" => mobile_deferred(DeferredReadRoute::ProjectConfigEffective),
        "project.branches.list" => mobile_deferred(DeferredReadRoute::ProjectBranchesList),
        "workspace.repositoryWebUrl" => {
            mobile_deferred(DeferredReadRoute::WorkspaceRepositoryWebUrl)
        }
        "hostDirectory.list" => mobile_deferred(DeferredReadRoute::HostDirectoryList),

        "createOrAttach"
        | "layout.remove"
        | "layout.upsert"
        | "linkedReview.remove"
        | "linkedReview.upsert"
        | "orchestration.agentSpawn"
        | "orchestration.dispatch"
        | "orchestration.dispatchAccept"
        | "orchestration.run"
        | "orchestration.taskRecover"
        | "orchestration.terminalPrune"
        | "project.upsert"
        | "tab.upsert"
        | "terminal.pulse.configure"
        | "workspace.runSetup"
        | "workspace.upsert"
        | "workspaceActivity.remove"
        | "workspaceActivity.upsertAll"
        | "workspaceTag.assign"
        | "workspaceTag.unassign"
        | "workspaceTag.upsert" => LOCAL_CONFLICT,

        "tab.removeForWorkspace" => {
            serialized_local(SerializedRuntimeMutation::RemoveWorkspaceTabs)
        }
        "workspace.remove" => serialized_local(SerializedRuntimeMutation::RemoveWorkspace),
        "workspace.removeForProject" => {
            serialized_local(SerializedRuntimeMutation::RemoveProjectWorkspaces)
        }
        "workspace.switchBranch" => {
            serialized_local(SerializedRuntimeMutation::SwitchWorkspaceBranch)
        }

        "agentPresence.list"
        | "agentProfile.launch"
        | "agentProfile.launchIdempotent"
        | "agentProfile.list"
        | "agentQuota.consumeCodexResetCredit"
        | "agentQuota.fetchClaudeTui"
        | "agentQuota.snapshot"
        | "agentSkill.install"
        | "agentUsage.snapshot"
        | "aiText.agentTitle.generate"
        | "aiText.cancel"
        | "aiText.speechMessage.generate"
        | "aiText.workspaceIdentity.generate"
        | "automation.cancel"
        | "automation.complete"
        | "automation.context"
        | "automation.export"
        | "automation.extend"
        | "automation.heartbeat"
        | "automation.list"
        | "automation.pause"
        | "automation.policy"
        | "automation.purge"
        | "automation.runs"
        | "automation.runShow"
        | "automation.show"
        | "automation.tags"
        | "automation.templates"
        | "automation.trash"
        | "automation.wait"
        | "cliRegistration.install"
        | "cliRegistration.status"
        | "configuration.apply"
        | "configuration.published"
        | "configuration.snapshot"
        | "configuration.transfer.cancel"
        | "configuration.transfer.chunk"
        | "configuration.transfer.commit"
        | "configuration.transfer.read"
        | "configuration.transfer.start"
        | "detach"
        | "hostDirectory.roots"
        | "layout.find"
        | "linkedReview.find"
        | "mobile.aiDictation.cancel"
        | "mobile.aiDictation.capabilities"
        | "mobile.aiDictation.transcribe"
        | "mobile.cloudEnrollment.create"
        | "mobile.cloudSubscriptions.refresh"
        | "mobile.promptAttachment.read"
        | "mobile.promptFile.cancel"
        | "mobile.promptFile.chunk"
        | "mobile.promptFile.complete"
        | "mobile.promptFile.start"
        | "mobile.promptImage.cancel"
        | "mobile.promptImage.chunk"
        | "mobile.promptImage.complete"
        | "mobile.promptImage.start"
        | "mobile.relayAuthorization.renew"
        | "mobile.runtimeSettings.get"
        | "mobile.runtimeSettings.update"
        | "mobile.status.get"
        | "mobile.workspaceFile.read"
        | "mobile.workspaceQuickOpen.search"
        | "mobile.workspaceQuickOpen.start"
        | "mobile.workspaceQuickOpen.stop"
        | "project.clone.cancel"
        | "project.clone.list"
        | "project.clone.start"
        | "project.list"
        | "project.remove.preview"
        | "resize"
        | "setOutputPaused"
        | "status.get"
        | "tab.find"
        | "tab.list"
        | "terminal.driver.list"
        | "workbenchViewPrefs.get"
        | "workspace.find"
        | "workspace.list"
        | "workspace.listAll"
        | "workspace.storageImpact"
        | "workspaceCascade.preview"
        | "workspaceRelation.list"
        | "workspaceSection.list"
        | "workspaceSidebar.snapshot"
        | "workspaceTag.list" => MOBILE,

        _ => LOCAL,
    }
}

const fn serialized_local(kind: SerializedRuntimeMutation) -> RequestRoutePolicy {
    RequestRoutePolicy {
        runtime_mutation: RuntimeMutationPolicy::Serialized(kind),
        ..LOCAL
    }
}

const fn serialized_mobile(kind: SerializedRuntimeMutation) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        ..serialized_local(kind)
    }
}

const fn mobile_deferred(route: DeferredReadRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        deferred_read: Some(route),
        ..LOCAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_mobile_and_runtime_mutation_policy() {
        assert_eq!(
            request_route_policy("workspace.createManaged"),
            MOBILE_CONFLICT
        );
        assert_eq!(
            request_route_policy("workspace.removeManaged").runtime_mutation,
            RuntimeMutationPolicy::Serialized(SerializedRuntimeMutation::RemoveManagedWorkspace)
        );
        assert_eq!(request_route_policy("workspace.upsert"), LOCAL_CONFLICT);
        assert_eq!(
            request_route_policy("workspace.remove").runtime_mutation,
            RuntimeMutationPolicy::Serialized(SerializedRuntimeMutation::RemoveWorkspace)
        );
        assert_eq!(request_route_policy("workspace.list"), MOBILE);
        assert_eq!(request_route_policy("unknown.request"), LOCAL);
    }

    #[test]
    fn codex_routes_default_to_conflicting_except_declared_non_writers() {
        assert_eq!(
            request_route_policy("codex.turn.start").runtime_mutation,
            RuntimeMutationPolicy::Conflicts
        );
        for request in [
            "codex.thread.list",
            "codex.thread.history",
            "codex.thread.snapshot",
            "codex.model.list",
            "codex.turn.interrupt",
        ] {
            assert_eq!(
                request_route_policy(request).runtime_mutation,
                RuntimeMutationPolicy::Available,
                "{request}"
            );
        }
    }

    #[test]
    fn host_lifecycle_actions_are_declared_as_route_policy() {
        let restart = request_route_policy("host.restart");
        assert!(restart.mobile_allowed);
        assert_eq!(restart.post_response, PostResponseAction::Restart);

        let shutdown = request_route_policy("host.shutdown");
        assert!(!shutdown.mobile_allowed);
        assert_eq!(shutdown.post_response, PostResponseAction::Shutdown);
    }

    #[test]
    fn deferred_read_routes_are_typed_and_mobile_safe() {
        let expected = [
            (
                "projectConfig.effective",
                DeferredReadRoute::ProjectConfigEffective,
            ),
            (
                "project.branches.list",
                DeferredReadRoute::ProjectBranchesList,
            ),
            (
                "workspace.repositoryWebUrl",
                DeferredReadRoute::WorkspaceRepositoryWebUrl,
            ),
            ("hostDirectory.list", DeferredReadRoute::HostDirectoryList),
        ];

        for (request_type, route) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(policy.deferred_read, Some(route), "{request_type}");
        }
    }
}
