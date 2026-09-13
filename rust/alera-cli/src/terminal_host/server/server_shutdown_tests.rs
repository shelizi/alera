use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::terminal_host::client::ClientHandle;

use super::actor_test_harness::{local_client, test_actor};
use super::{DisconnectReason, ServerActor};

#[derive(Default)]
struct CleanupRecordFields {
    event: Option<String>,
    reason: Option<String>,
    client_id: Option<u64>,
}

impl Visit for CleanupRecordFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "client_id" {
            self.client_id = format!("{value:?}").parse().ok();
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "event" => self.event = Some(value.to_string()),
            "reason" => self.reason = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "client_id" {
            self.client_id = Some(value);
        }
    }
}

struct CleanupRecordLayer {
    records: Arc<Mutex<Vec<(u64, String)>>>,
}

impl<S> Layer<S> for CleanupRecordLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut fields = CleanupRecordFields::default();
        event.record(&mut fields);
        if fields.event.as_deref() == Some("cleanup-completion") {
            if let (Some(client_id), Some(reason)) = (fields.client_id, fields.reason) {
                self.records.lock().unwrap().push((client_id, reason));
            }
        }
    }
}

fn cleanup_record_layer(records: Arc<Mutex<Vec<(u64, String)>>>) -> CleanupRecordLayer {
    CleanupRecordLayer { records }
}

async fn dispose_with_capture(actor: &mut ServerActor, records: Arc<Mutex<Vec<(u64, String)>>>) {
    let subscriber = tracing_subscriber::registry().with(cleanup_record_layer(records));
    let _default = tracing::subscriber::set_default(subscriber);
    actor.dispose().await;
}

#[tokio::test]
async fn dispose_emits_host_shutdown_cleanup_for_each_remaining_client() {
    let dir = tempfile::tempdir().unwrap();
    let (first_handle, _first_receiver) = ClientHandle::test_channels();
    let (second_handle, _second_receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(first_handle)),
            (2, local_client(second_handle)),
        ]),
        HashMap::new(),
    )
    .await;
    let records = Arc::new(Mutex::new(Vec::new()));

    dispose_with_capture(&mut actor, records.clone()).await;

    assert!(actor.disposed);
    assert!(actor.clients.is_empty());
    let mut cleanup_records = records.lock().unwrap().clone();
    cleanup_records.sort_unstable();
    assert_eq!(
        cleanup_records,
        vec![
            (1, "host_shutdown".to_string()),
            (2, "host_shutdown".to_string())
        ]
    );
}

#[tokio::test]
async fn disposing_again_or_disconnecting_after_shutdown_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let records = Arc::new(Mutex::new(Vec::new()));

    dispose_with_capture(&mut actor, records.clone()).await;
    actor.dispose().await;
    actor
        .dispose_client_with_reason(1, DisconnectReason::PeerClosed)
        .await;

    assert_eq!(records.lock().unwrap().len(), 1);
    assert!(actor.clients.is_empty());
}
