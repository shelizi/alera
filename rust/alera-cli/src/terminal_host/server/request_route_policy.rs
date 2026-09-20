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
pub(super) enum DeferredWriteRoute {
    ProjectRegister,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CoalescedReadRoute {
    WorkspaceSidebarSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CliRegistrationOperation {
    Status,
    Install,
}

impl CliRegistrationOperation {
    pub(super) const fn request_type(self) -> &'static str {
        match self {
            Self::Status => "cliRegistration.status",
            Self::Install => "cliRegistration.install",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AgentQuotaOperation {
    QuotaSnapshot,
    UsageSnapshot,
    FetchClaudeTui,
    ConsumeCodexResetCredit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiTextOperation {
    AgentTitle,
    WorkspaceIdentity,
    SpeechMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiDictationOperation {
    LocalTranscribe,
    MobileTranscribe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceDeferredOperation {
    CreateManaged,
    RunSetup,
    StorageImpact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeferredJobRoute {
    CliRegistration(CliRegistrationOperation),
    AgentQuota(AgentQuotaOperation),
    AiText(AiTextOperation),
    AiDictation(AiDictationOperation),
    Workspace(WorkspaceDeferredOperation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MobilePromptImageOperation {
    Start,
    Chunk,
    Complete,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MobilePromptFileOperation {
    Start,
    Chunk,
    Complete,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MobileWorkspaceFileOperation {
    QuickOpenStart,
    QuickOpenSearch,
    WorkspaceFileRead,
    PromptAttachmentRead,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MobileDeferredRoute {
    PromptImage(MobilePromptImageOperation),
    PromptFile(MobilePromptFileOperation),
    WorkspaceFile(MobileWorkspaceFileOperation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RequestRoutePolicy {
    pub(super) mobile_allowed: bool,
    pub(super) runtime_mutation: RuntimeMutationPolicy,
    pub(super) post_response: PostResponseAction,
    pub(super) deferred_read: Option<DeferredReadRoute>,
    pub(super) deferred_write: Option<DeferredWriteRoute>,
    pub(super) coalesced_read: Option<CoalescedReadRoute>,
    pub(super) deferred_job: Option<DeferredJobRoute>,
    pub(super) mobile_deferred: Option<MobileDeferredRoute>,
}

const LOCAL: RequestRoutePolicy = RequestRoutePolicy {
    mobile_allowed: false,
    runtime_mutation: RuntimeMutationPolicy::Available,
    post_response: PostResponseAction::None,
    deferred_read: None,
    deferred_write: None,
    coalesced_read: None,
    deferred_job: None,
    mobile_deferred: None,
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
        | "project.rename"
        | "projectConfig.remove"
        | "projectConfig.upsert"
        | "tab.rename"
        | "terminal.attach"
        | "terminal.create"
        | "terminal.restart"
        | "terminate"
        | "workbenchViewPrefs.update"
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

        "project.register" => {
            mobile_conflicting_deferred_write(DeferredWriteRoute::ProjectRegister)
        }

        "workspaceSidebar.snapshot" => {
            mobile_coalesced_read(CoalescedReadRoute::WorkspaceSidebarSnapshot)
        }

        "mobile.promptImage.start" => mobile_deferred_operation(MobileDeferredRoute::PromptImage(
            MobilePromptImageOperation::Start,
        )),
        "mobile.promptImage.chunk" => mobile_deferred_operation(MobileDeferredRoute::PromptImage(
            MobilePromptImageOperation::Chunk,
        )),
        "mobile.promptImage.complete" => mobile_deferred_operation(
            MobileDeferredRoute::PromptImage(MobilePromptImageOperation::Complete),
        ),
        "mobile.promptImage.cancel" => mobile_deferred_operation(MobileDeferredRoute::PromptImage(
            MobilePromptImageOperation::Cancel,
        )),
        "mobile.promptFile.start" => mobile_deferred_operation(MobileDeferredRoute::PromptFile(
            MobilePromptFileOperation::Start,
        )),
        "mobile.promptFile.chunk" => mobile_deferred_operation(MobileDeferredRoute::PromptFile(
            MobilePromptFileOperation::Chunk,
        )),
        "mobile.promptFile.complete" => mobile_deferred_operation(MobileDeferredRoute::PromptFile(
            MobilePromptFileOperation::Complete,
        )),
        "mobile.promptFile.cancel" => mobile_deferred_operation(MobileDeferredRoute::PromptFile(
            MobilePromptFileOperation::Cancel,
        )),
        "mobile.workspaceQuickOpen.start" => mobile_deferred_operation(
            MobileDeferredRoute::WorkspaceFile(MobileWorkspaceFileOperation::QuickOpenStart),
        ),
        "mobile.workspaceQuickOpen.search" => mobile_deferred_operation(
            MobileDeferredRoute::WorkspaceFile(MobileWorkspaceFileOperation::QuickOpenSearch),
        ),
        "mobile.workspaceFile.read" => mobile_deferred_operation(
            MobileDeferredRoute::WorkspaceFile(MobileWorkspaceFileOperation::WorkspaceFileRead),
        ),
        "mobile.promptAttachment.read" => mobile_deferred_operation(
            MobileDeferredRoute::WorkspaceFile(MobileWorkspaceFileOperation::PromptAttachmentRead),
        ),

        "cliRegistration.status" => mobile_deferred_job(DeferredJobRoute::CliRegistration(
            CliRegistrationOperation::Status,
        )),
        "cliRegistration.install" => mobile_deferred_job(DeferredJobRoute::CliRegistration(
            CliRegistrationOperation::Install,
        )),
        "agentQuota.snapshot" => mobile_deferred_job(DeferredJobRoute::AgentQuota(
            AgentQuotaOperation::QuotaSnapshot,
        )),
        "agentUsage.snapshot" => mobile_deferred_job(DeferredJobRoute::AgentQuota(
            AgentQuotaOperation::UsageSnapshot,
        )),
        "agentQuota.fetchClaudeTui" => mobile_deferred_job(DeferredJobRoute::AgentQuota(
            AgentQuotaOperation::FetchClaudeTui,
        )),
        "agentQuota.consumeCodexResetCredit" => mobile_deferred_job(DeferredJobRoute::AgentQuota(
            AgentQuotaOperation::ConsumeCodexResetCredit,
        )),
        "aiText.agentTitle.generate" => {
            mobile_deferred_job(DeferredJobRoute::AiText(AiTextOperation::AgentTitle))
        }
        "aiText.workspaceIdentity.generate" => {
            mobile_deferred_job(DeferredJobRoute::AiText(AiTextOperation::WorkspaceIdentity))
        }
        "aiText.speechMessage.generate" => {
            mobile_deferred_job(DeferredJobRoute::AiText(AiTextOperation::SpeechMessage))
        }
        "aiDictation.transcribe" => local_deferred_job(DeferredJobRoute::AiDictation(
            AiDictationOperation::LocalTranscribe,
        )),
        "mobile.aiDictation.transcribe" => mobile_deferred_job(DeferredJobRoute::AiDictation(
            AiDictationOperation::MobileTranscribe,
        )),
        "workspace.createManaged" => mobile_conflicting_deferred_job(DeferredJobRoute::Workspace(
            WorkspaceDeferredOperation::CreateManaged,
        )),
        "workspace.runSetup" => local_conflicting_deferred_job(DeferredJobRoute::Workspace(
            WorkspaceDeferredOperation::RunSetup,
        )),
        "workspace.storageImpact" => mobile_deferred_job(DeferredJobRoute::Workspace(
            WorkspaceDeferredOperation::StorageImpact,
        )),

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
        | "agentSkill.install"
        | "aiText.cancel"
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
        | "mobile.cloudEnrollment.create"
        | "mobile.cloudSubscriptions.refresh"
        | "mobile.relayAuthorization.renew"
        | "mobile.runtimeSettings.get"
        | "mobile.runtimeSettings.update"
        | "mobile.status.get"
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
        | "workspaceCascade.preview"
        | "workspaceRelation.list"
        | "workspaceSection.list"
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

const fn mobile_conflicting_deferred_write(route: DeferredWriteRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        runtime_mutation: RuntimeMutationPolicy::Conflicts,
        deferred_write: Some(route),
        ..LOCAL
    }
}

const fn mobile_coalesced_read(route: CoalescedReadRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        coalesced_read: Some(route),
        ..LOCAL
    }
}

const fn mobile_deferred_job(route: DeferredJobRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        deferred_job: Some(route),
        ..LOCAL
    }
}

const fn local_deferred_job(route: DeferredJobRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        deferred_job: Some(route),
        ..LOCAL
    }
}

const fn mobile_conflicting_deferred_job(route: DeferredJobRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        runtime_mutation: RuntimeMutationPolicy::Conflicts,
        deferred_job: Some(route),
        ..LOCAL
    }
}

const fn local_conflicting_deferred_job(route: DeferredJobRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        runtime_mutation: RuntimeMutationPolicy::Conflicts,
        deferred_job: Some(route),
        ..LOCAL
    }
}

const fn mobile_deferred_operation(route: MobileDeferredRoute) -> RequestRoutePolicy {
    RequestRoutePolicy {
        mobile_allowed: true,
        mobile_deferred: Some(route),
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

    #[test]
    fn project_registration_is_a_typed_deferred_write() {
        let policy = request_route_policy("project.register");
        assert!(policy.mobile_allowed);
        assert_eq!(policy.runtime_mutation, RuntimeMutationPolicy::Conflicts);
        assert_eq!(
            policy.deferred_write,
            Some(DeferredWriteRoute::ProjectRegister)
        );
    }

    #[test]
    fn workspace_sidebar_snapshot_is_a_typed_coalesced_read() {
        let policy = request_route_policy("workspaceSidebar.snapshot");
        assert!(policy.mobile_allowed);
        assert_eq!(
            policy.coalesced_read,
            Some(CoalescedReadRoute::WorkspaceSidebarSnapshot)
        );
    }

    #[test]
    fn prompt_image_routes_are_typed_mobile_deferred_operations() {
        let expected = [
            (
                "mobile.promptImage.start",
                MobilePromptImageOperation::Start,
            ),
            (
                "mobile.promptImage.chunk",
                MobilePromptImageOperation::Chunk,
            ),
            (
                "mobile.promptImage.complete",
                MobilePromptImageOperation::Complete,
            ),
            (
                "mobile.promptImage.cancel",
                MobilePromptImageOperation::Cancel,
            ),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.mobile_deferred,
                Some(MobileDeferredRoute::PromptImage(operation)),
                "{request_type}"
            );
        }
    }

    #[test]
    fn prompt_file_routes_are_typed_mobile_deferred_operations() {
        let expected = [
            ("mobile.promptFile.start", MobilePromptFileOperation::Start),
            ("mobile.promptFile.chunk", MobilePromptFileOperation::Chunk),
            (
                "mobile.promptFile.complete",
                MobilePromptFileOperation::Complete,
            ),
            (
                "mobile.promptFile.cancel",
                MobilePromptFileOperation::Cancel,
            ),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.mobile_deferred,
                Some(MobileDeferredRoute::PromptFile(operation)),
                "{request_type}"
            );
        }
    }

    #[test]
    fn workspace_file_routes_are_typed_mobile_deferred_operations() {
        let expected = [
            (
                "mobile.workspaceQuickOpen.start",
                MobileWorkspaceFileOperation::QuickOpenStart,
            ),
            (
                "mobile.workspaceQuickOpen.search",
                MobileWorkspaceFileOperation::QuickOpenSearch,
            ),
            (
                "mobile.workspaceFile.read",
                MobileWorkspaceFileOperation::WorkspaceFileRead,
            ),
            (
                "mobile.promptAttachment.read",
                MobileWorkspaceFileOperation::PromptAttachmentRead,
            ),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.mobile_deferred,
                Some(MobileDeferredRoute::WorkspaceFile(operation)),
                "{request_type}"
            );
        }
    }

    #[test]
    fn cli_registration_routes_are_typed_deferred_jobs() {
        let expected = [
            ("cliRegistration.status", CliRegistrationOperation::Status),
            ("cliRegistration.install", CliRegistrationOperation::Install),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.deferred_job,
                Some(DeferredJobRoute::CliRegistration(operation)),
                "{request_type}"
            );
            assert_eq!(operation.request_type(), request_type);
        }
    }

    #[test]
    fn agent_quota_routes_are_typed_deferred_jobs() {
        let expected = [
            ("agentQuota.snapshot", AgentQuotaOperation::QuotaSnapshot),
            ("agentUsage.snapshot", AgentQuotaOperation::UsageSnapshot),
            (
                "agentQuota.fetchClaudeTui",
                AgentQuotaOperation::FetchClaudeTui,
            ),
            (
                "agentQuota.consumeCodexResetCredit",
                AgentQuotaOperation::ConsumeCodexResetCredit,
            ),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.deferred_job,
                Some(DeferredJobRoute::AgentQuota(operation)),
                "{request_type}"
            );
        }
    }

    #[test]
    fn ai_text_routes_are_typed_deferred_jobs() {
        let expected = [
            ("aiText.agentTitle.generate", AiTextOperation::AgentTitle),
            (
                "aiText.workspaceIdentity.generate",
                AiTextOperation::WorkspaceIdentity,
            ),
            (
                "aiText.speechMessage.generate",
                AiTextOperation::SpeechMessage,
            ),
        ];
        for (request_type, operation) in expected {
            let policy = request_route_policy(request_type);
            assert!(policy.mobile_allowed, "{request_type}");
            assert_eq!(
                policy.deferred_job,
                Some(DeferredJobRoute::AiText(operation)),
                "{request_type}"
            );
        }
    }

    #[test]
    fn ai_dictation_routes_are_typed_deferred_jobs() {
        let local = request_route_policy("aiDictation.transcribe");
        assert!(!local.mobile_allowed);
        assert_eq!(
            local.deferred_job,
            Some(DeferredJobRoute::AiDictation(
                AiDictationOperation::LocalTranscribe
            ))
        );

        let mobile = request_route_policy("mobile.aiDictation.transcribe");
        assert!(mobile.mobile_allowed);
        assert_eq!(
            mobile.deferred_job,
            Some(DeferredJobRoute::AiDictation(
                AiDictationOperation::MobileTranscribe
            ))
        );
    }

    #[test]
    fn workspace_deferred_routes_preserve_distinct_policy() {
        let create = request_route_policy("workspace.createManaged");
        assert!(create.mobile_allowed);
        assert_eq!(create.runtime_mutation, RuntimeMutationPolicy::Conflicts);
        assert_eq!(
            create.deferred_job,
            Some(DeferredJobRoute::Workspace(
                WorkspaceDeferredOperation::CreateManaged
            ))
        );

        let setup = request_route_policy("workspace.runSetup");
        assert!(!setup.mobile_allowed);
        assert_eq!(setup.runtime_mutation, RuntimeMutationPolicy::Conflicts);
        assert_eq!(
            setup.deferred_job,
            Some(DeferredJobRoute::Workspace(
                WorkspaceDeferredOperation::RunSetup
            ))
        );

        let impact = request_route_policy("workspace.storageImpact");
        assert!(impact.mobile_allowed);
        assert_eq!(impact.runtime_mutation, RuntimeMutationPolicy::Available);
        assert_eq!(
            impact.deferred_job,
            Some(DeferredJobRoute::Workspace(
                WorkspaceDeferredOperation::StorageImpact
            ))
        );
    }
}
