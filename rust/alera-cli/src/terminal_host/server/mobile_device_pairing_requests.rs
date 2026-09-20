use alera_core::runtime::RuntimeStore;
use serde_json::Value;

use crate::mobile_access::{
    cancel_mobile_pairing_offer, delete_mobile_device, list_mobile_devices, pair_mobile_device,
    rename_mobile_device, revoke_mobile_device, MobileDevicePairRequest,
};
use crate::terminal_host::host_error::HostResult;

use super::request_payloads::json_result;

pub(super) struct MobileDevicePairingRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> MobileDevicePairingRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn list_devices(&self, include_revoked: bool) -> HostResult<Value> {
        json_result(list_mobile_devices(self.runtime_store, include_revoked).await)
    }

    pub(super) async fn pair_device(&self, request: MobileDevicePairRequest) -> HostResult<Value> {
        json_result(pair_mobile_device(self.runtime_store, request).await)
    }

    pub(super) async fn revoke_device(&self, id: &str) -> HostResult<()> {
        json_result(revoke_mobile_device(self.runtime_store, id).await).map(|_| ())
    }

    pub(super) async fn delete_device(&self, id: &str) -> HostResult<()> {
        json_result(delete_mobile_device(self.runtime_store, id).await).map(|_| ())
    }

    pub(super) async fn rename_device(&self, id: &str, display_name: &str) -> HostResult<Value> {
        json_result(rename_mobile_device(self.runtime_store, id, display_name).await)
    }

    pub(super) async fn cancel_pairing(&self, id: &str) -> HostResult<()> {
        json_result(cancel_mobile_pairing_offer(self.runtime_store, id).await).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{MobileAccessSettings, RuntimeStore};

    use crate::mobile_access::{
        create_mobile_pairing_offer_for_settings, MobileDevicePairRequest,
        MobilePairingCreateRequest,
    };

    use super::MobileDevicePairingRequestHandler;

    async fn pairing_offer(
        store: &RuntimeStore,
        device_name: &str,
    ) -> crate::mobile_access::MobilePairingOfferPayload {
        let request = MobilePairingCreateRequest {
            endpoint: Some("ws://127.0.0.1:6768".into()),
            device_name: Some(device_name.into()),
            expires_minutes: Some(10),
        };
        create_mobile_pairing_offer_for_settings(
            store,
            &MobileAccessSettings::default(),
            &request,
            "ws://127.0.0.1:6768".into(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn device_and_pairing_persistence_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = MobileDevicePairingRequestHandler::new(&store);

        let offer = pairing_offer(&store, "Phone").await;
        let paired = handler
            .pair_device(MobileDevicePairRequest {
                pairing_id: offer.pairing_id,
                pairing_secret: offer.pairing_secret,
                device_name: None,
                public_key_b64: None,
            })
            .await
            .unwrap();
        let device_id = paired["deviceId"].as_str().unwrap().to_string();
        assert_eq!(paired["displayName"], "Phone");
        assert_eq!(
            handler
                .list_devices(false)
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let renamed = handler
            .rename_device(&device_id, "Renamed Phone")
            .await
            .unwrap();
        assert_eq!(renamed["displayName"], "Renamed Phone");

        handler.revoke_device(&device_id).await.unwrap();
        assert!(handler
            .list_devices(false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            handler
                .list_devices(true)
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );

        handler.delete_device(&device_id).await.unwrap();
        assert!(handler
            .list_devices(true)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());

        let cancellable = pairing_offer(&store, "Tablet").await;
        handler
            .cancel_pairing(&cancellable.pairing_id)
            .await
            .unwrap();
        assert!(store
            .find_mobile_pairing_offer(&cancellable.pairing_id)
            .await
            .unwrap()
            .is_none());
    }
}
