use tokio::net::TcpListener;

use alera_core::runtime::MobileAccessSettings;
use serde_json::json;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::mobile_gateway::spawn_mobile_gateway_accept_loop;
use crate::terminal_host::protocol::event;

use super::client_accept_loop::display_socket_address;
use super::ServerActor;

pub(super) enum MobileGatewayReplacement {
    Keep,
    Disabled,
    Bound {
        listener: TcpListener,
        bind_address: String,
    },
}

impl ServerActor {
    pub(super) async fn restart_mobile_gateway(&mut self) -> HostResult<()> {
        let settings = self
            .runtime_store
            .mobile_access_settings()
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let replacement = self.prepare_mobile_gateway_replacement(&settings).await?;
        self.replace_mobile_gateway(replacement).await;
        Ok(())
    }

    pub(super) async fn apply_mobile_gateway_settings(
        &mut self,
        current: MobileAccessSettings,
        next: MobileAccessSettings,
    ) -> HostResult<MobileAccessSettings> {
        let replacement = if self.can_keep_mobile_gateway(&current, &next) {
            MobileGatewayReplacement::Keep
        } else {
            let release_before_bind =
                self.should_release_mobile_gateway_before_bind(&current, &next);
            if release_before_bind {
                self.stop_mobile_gateway().await;
                self.dispose_mobile_clients().await;
            }
            match self.prepare_mobile_gateway_replacement(&next).await {
                Ok(replacement) => replacement,
                Err(error) => {
                    if release_before_bind {
                        if let Ok(restored) =
                            self.prepare_mobile_gateway_replacement(&current).await
                        {
                            self.replace_mobile_gateway(restored).await;
                        }
                    }
                    return Err(error);
                }
            }
        };
        let saved = self
            .runtime_store
            .set_mobile_access_settings(next)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        self.replace_mobile_gateway(replacement).await;
        self.restart_remote_relay().await;
        Ok(saved)
    }

    fn can_keep_mobile_gateway(
        &self,
        current: &MobileAccessSettings,
        next: &MobileAccessSettings,
    ) -> bool {
        self.mobile_gateway.is_some()
            && current.enabled
            && next.enabled
            && current.bind_host == next.bind_host
            && current.port == next.port
    }

    fn should_release_mobile_gateway_before_bind(
        &self,
        current: &MobileAccessSettings,
        next: &MobileAccessSettings,
    ) -> bool {
        self.mobile_gateway.is_some()
            && current.enabled
            && next.enabled
            && current.port == next.port
            && current.bind_host != next.bind_host
    }

    pub(super) async fn prepare_mobile_gateway_replacement(
        &self,
        settings: &MobileAccessSettings,
    ) -> HostResult<MobileGatewayReplacement> {
        if !settings.enabled {
            return Ok(MobileGatewayReplacement::Disabled);
        }
        let port: u16 = settings.port.try_into().map_err(|_| {
            HostError::state(format!(
                "mobile gateway port is outside the valid range: {}",
                settings.port
            ))
        })?;
        let bind_address = display_socket_address(&settings.bind_host, port);
        let listener = TcpListener::bind((settings.bind_host.as_str(), port))
            .await
            .map_err(|error| {
                HostError::state(format!(
                    "mobile gateway bind failed for {bind_address}: {error}"
                ))
            })?;
        let local_address = listener
            .local_addr()
            .map(|address| address.to_string())
            .unwrap_or(bind_address);
        Ok(MobileGatewayReplacement::Bound {
            listener,
            bind_address: local_address,
        })
    }

    async fn replace_mobile_gateway(&mut self, replacement: MobileGatewayReplacement) {
        match replacement {
            MobileGatewayReplacement::Keep => {}
            MobileGatewayReplacement::Disabled => {
                self.stop_mobile_gateway().await;
                self.dispose_mobile_clients().await;
                self.broadcast_authenticated(event(
                    "mobileGatewayChanged",
                    json!({ "enabled": false }),
                ));
            }
            MobileGatewayReplacement::Bound {
                listener,
                bind_address,
            } => {
                self.stop_mobile_gateway().await;
                self.dispose_mobile_clients().await;
                self.mobile_gateway = Some(spawn_mobile_gateway_accept_loop(
                    listener,
                    self.inbox.clone(),
                    self.next_client_id.clone(),
                ));
                self.cancel_shutdown_timer();
                self.broadcast_authenticated(event(
                    "mobileGatewayChanged",
                    json!({
                        "enabled": true,
                        "bindAddress": bind_address,
                    }),
                ));
            }
        }
    }

    async fn stop_mobile_gateway(&mut self) {
        if let Some(handle) = self.mobile_gateway.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}
