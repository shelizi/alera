use crate::terminal_host::host_error::{HostError, HostResult};

use super::mobile_gateway_surface::mobile_request_allowed;
use super::{ClientKind, ServerActor};

impl ServerActor {
    pub(super) fn require_request_allowed(
        &self,
        client_id: u64,
        request_type: &str,
    ) -> HostResult<()> {
        let Some(client) = self.clients.get(&client_id) else {
            return Err(HostError::unauthorized(
                "Terminal host client is not authenticated.",
            ));
        };
        if !client.authenticated {
            return Err(HostError::unauthorized(
                "Terminal host client is not authenticated.",
            ));
        }
        if client.kind == ClientKind::Local || mobile_request_allowed(request_type) {
            return Ok(());
        }
        Err(HostError::unauthorized(format!(
            "Mobile clients cannot call terminal host request: {request_type}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::terminal_host::client::ClientHandle;
    use crate::terminal_host::host_error::{HostError, OutcomeClass};
    use crate::terminal_host::server::actor_test_harness::{
        local_client, mobile_client, test_actor,
    };

    fn assert_unauthorized(error: HostError, expected_message: &str) {
        assert_eq!(error.outcome_class(), OutcomeClass::Unauthorized);
        assert_eq!(error.wire_message(), expected_message);
    }

    #[tokio::test]
    async fn authorization_failures_are_unauthorized_with_legacy_messages() {
        let dir = tempfile::tempdir().unwrap();
        let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        assert_unauthorized(
            actor
                .require_request_allowed(1, "workspace.list")
                .unwrap_err(),
            "Terminal host client is not authenticated.",
        );

        let dir = tempfile::tempdir().unwrap();
        let (handle, _receiver) = ClientHandle::test_channels();
        let mut client = local_client(handle);
        client.authenticated = false;
        let actor = test_actor(&dir, HashMap::from([(1, client)]), HashMap::new()).await;
        assert_unauthorized(
            actor
                .require_request_allowed(1, "workspace.list")
                .unwrap_err(),
            "Terminal host client is not authenticated.",
        );

        let dir = tempfile::tempdir().unwrap();
        let (handle, _receiver) = ClientHandle::test_channels();
        let actor = test_actor(
            &dir,
            HashMap::from([(1, mobile_client(handle, "phone"))]),
            HashMap::new(),
        )
        .await;
        assert_unauthorized(
            actor
                .require_request_allowed(1, "codex.thread.open")
                .unwrap_err(),
            "Mobile clients cannot call terminal host request: codex.thread.open",
        );
    }
}
