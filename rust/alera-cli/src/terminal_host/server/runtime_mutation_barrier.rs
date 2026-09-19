use super::request_route_policy::{request_route_policy, RuntimeMutationPolicy};

pub(super) fn conflicts_with_runtime_mutation(request_type: &str) -> bool {
    request_route_policy(request_type).runtime_mutation == RuntimeMutationPolicy::Conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_runtime_store_writers_and_session_spawners_but_not_reads() {
        for writer in [
            "tab.upsert",
            "createOrAttach",
            "terminal.pulse.configure",
            "codex.turn.start",
            "codex.thread.resume",
            "codex.response",
            "orchestration.agentSpawn",
            "automation.upsert",
            "automation.approve",
            "automation.resume",
            "automation.restore",
            "automation.runNow",
            "automation.import",
        ] {
            assert!(
                conflicts_with_runtime_mutation(writer),
                "{writer} should be blocked"
            );
        }
        for read_or_serialized_mutation in [
            "tab.list",
            "terminal.read",
            "tab.remove",
            "codex.thread.list",
            "codex.thread.history",
            "codex.thread.snapshot",
            "codex.model.list",
            "codex.turn.interrupt",
        ] {
            assert!(
                !conflicts_with_runtime_mutation(read_or_serialized_mutation),
                "{read_or_serialized_mutation} should remain available"
            );
        }
    }
}
