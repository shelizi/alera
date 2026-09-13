use super::{deferred_admission, ServerActor, ServerCommand, CHECKPOINT_DELAY};

impl ServerActor {
    pub(super) fn spawn_checkpoint_timer(&self, session_id: String, generation: u64) {
        let inbox = self.inbox.clone();
        let log_session_id = session_id.clone();
        if let Err(error) = self.deferred_admission.schedule_delayed(
            CHECKPOINT_DELAY,
            deferred_admission::DeferredRequestClass::Maintenance,
            "terminal.checkpoint",
            None,
            async move {
                let _ = inbox.send(ServerCommand::CheckpointTick {
                    session_id,
                    generation,
                });
            },
        ) {
            tracing::warn!(
                session_id = %log_session_id,
                generation,
                error = %error.wire_message(),
                "checkpoint timer was not admitted"
            );
        }
    }
}
