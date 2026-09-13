# Client disconnect observability

Client disposal is an actor-owned cleanup operation. The disconnect reason is internal telemetry only and does not change the terminal-host wire protocol.

## Disconnect reasons

| Reason | Meaning and real trigger points |
| --- | --- |
| `transport_write_failed` | An outbound frame could not be queued or written. Local TCP writes are reported at `client.rs:201`, `:217`, `:259`, `:294`; mobile WebSocket frame and pong writes at `mobile_gateway.rs:79`, `:95`, `:123`, `:159`, `:193`; relay writer failure at `relay_runtime_peer.rs:190`; server-side delivery failures at `server/client_delivery.rs:91`, `:103`, `:120`, `:134`, `:143`, `:152`, `:163`, `:174` and `server/output_delivery.rs:65`, `:168`. |
| `peer_closed` | The peer ended the inbound transport or the relay peer input channel closed. Local EOF/read failure is `client.rs:238`; WebSocket close/read failures are `mobile_gateway.rs:131`, `:139`; relay input closure is `relay_runtime_peer.rs:196`. The relay guard defaults to this reason at `relay_runtime_peer.rs:177` for an otherwise unclassified post-handshake exit. |
| `replaced` | An existing relay or mobile connection is intentionally retired by the owning transport lifecycle. Mobile gateway replacement/disable cleanup starts at `server/mobile_gateway_replacement.rs:45-47` and `:130-144`, then uses `client_delivery.rs:246-256`; relay client retirement starts at `server/remote_relay.rs:105-117` and uses `client_delivery.rs:186-188`. |
| `host_shutdown` | The actor is disposing clients while the host is shutting down. The shutdown path starts at `server/server_shutdown.rs:4-20`; the compatibility cleanup wrapper recognizes this state at `client_delivery.rs:186-188` and `:275-277`. Local handles are closed directly by shutdown, while relay clients may pass through `dispose_client`. |
| `unauthenticated` | An unauthenticated client reaches the existing no-response disposal path in `server/requests.rs:59`, `:64`, `:97`, or `:145`; the fallback classification is `client_delivery.rs:280-281`. |
| `protocol_violation` | An authenticated client reaches the same parser path because malformed input has no response target. The existing disposal call sites are `server/requests.rs:59`, `:64`, `:97`, and `:145`; the fallback classification is `client_delivery.rs:281-282`. Relay fragment expiry is explicitly classified at `relay_runtime_peer.rs:201`. |
| `device_revoked` | A connected mobile client is removed after its paired device is revoked. The trigger is `server/requests.rs:1088-1093`, routed through `client_delivery.rs:258-269`. |

Reason strings are stable snake-case values exposed in structured logs through `DisconnectReason::as_str()`.

## Cleanup-completion event

After `dispose_client` finishes its cleanup sequence, it emits one `tracing::info!` event with `event="cleanup-completion"`. The fields are:

| Field | Definition |
| --- | --- |
| `reason` | Stable `DisconnectReason` string for the disposal that completed. |
| `client_id` | Internal numeric client identifier. |
| `authenticated` | Whether the client had completed the hello handshake. |
| `waiters_removed` | Number of orchestration message waiters removed for this client. |
| `sessions_detached` | Number of sessions whose attachment set contained this client and was detached. |
| `admission_still_occupied` | Global active plus queued deferred-admission jobs after this client was disconnected. Delayed timers are not included because the existing admission snapshot does not expose them. |
| `receipts_left_pending` | Global pending idempotency receipt count after cleanup. Pending receipts are retained for seven days by design for reconnect replay, so this is not a leak indicator. The field is omitted when the receipt table/count is unavailable. |

The event is emitted after client removal, transfer cleanup, mobile presence notification, and idle-shutdown scheduling. It is diagnostic output only and is not sent to clients.
