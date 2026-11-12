//! Fast, offline, isolated tests of the **real** Alera CLI Hook CAS contract.
//! No Host credentials, network, live RuntimeStore or GUI are accessed.
#[cfg(test)]
#[path = "../../../rust/alera-cli/src/agent_hook_cas_contract.rs"]
mod contract;

#[cfg(test)]
mod tests {
    use super::contract::{
        native_agent_hook_cas_payload, native_agent_hook_cas_readback,
        native_agent_hooks_projection,
    };
    use serde_json::json;

    fn host(revision: i64) -> serde_json::Value {
        json!({
            "revision": revision,
            "agentStatusHooks": {"codex": false, "claude": true, "grok": false},
            "aiTextGeneration": {"apiKey":"SECRET_TOKEN", "prompt":"SENSITIVE_PROMPT"},
            "automation": {"privateKey":"PRIVATE_KEY"}
        })
    }

    #[test]
    fn only_hook_values_and_revision_survive_projection() {
        let projected = native_agent_hooks_projection(&host(7)).unwrap();
        assert_eq!(
            projected,
            json!({"kind":"agentHookSettings", "revision":7,
          "agentStatusHooks":{"codex":false,"claude":true,"grok":false}})
        );
        for forbidden in ["SECRET_TOKEN", "SENSITIVE_PROMPT", "PRIVATE_KEY"] {
            assert!(!projected.to_string().contains(forbidden));
        }
    }

    #[test]
    fn one_provider_write_preserves_every_other_switch_and_never_writes_secrets() {
        let request = native_agent_hook_cas_payload(&host(7), "codex", 7, false, true)
            .unwrap()
            .expect("changed value");
        assert_eq!(
            request,
            json!({"expectedRevision":7,
          "agentStatusHooks":{"codex":true,"claude":true,"grok":false}})
        );
        assert_eq!(request.as_object().unwrap().len(), 2);
    }

    #[test]
    fn classic_concurrent_revision_or_provider_change_cannot_be_overwritten() {
        // Classic updated the Host between the user's observation and the CLI's read.
        assert!(native_agent_hook_cas_payload(&host(8), "codex", 7, false, true).is_err());
        let mut modified = host(8);
        modified["agentStatusHooks"]["codex"] = json!(true);
        assert!(native_agent_hook_cas_payload(&modified, "codex", 8, false, true).is_err());
    }

    #[test]
    fn no_op_does_not_generate_an_update_or_advance_revision() {
        assert!(
            native_agent_hook_cas_payload(&host(7), "claude", 7, true, true)
                .unwrap()
                .is_none()
        );
        assert!(
            native_agent_hook_cas_payload(&host(7), "codex", 7, false, false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_version_agent_and_non_boolean_wire_data() {
        for malformed in [
            json!({"agentStatusHooks":{"codex":false}}),
            json!({"revision":-1,"agentStatusHooks":{"codex":false}}),
            json!({"revision":1,"agentStatusHooks":{"codex":"false"}}),
            json!({"revision":1,"agentStatusHooks":{"BAD\nID":true}}),
        ] {
            assert!(native_agent_hooks_projection(&malformed).is_err());
        }
        assert!(native_agent_hook_cas_payload(&host(7), "bad\nagent", 7, false, true).is_err());
        assert!(native_agent_hook_cas_payload(&host(7), "codex", -1, false, true).is_err());
    }

    #[test]
    fn response_must_confirm_revision_and_every_other_provider_unchanged() {
        let request = native_agent_hook_cas_payload(&host(7), "codex", 7, false, true)
            .unwrap()
            .unwrap();
        let good = json!({"revision":8,
          "agentStatusHooks":{"codex":true,"claude":true,"grok":false},
          "aiTextGeneration":{"secret":"DROP_ME"}});
        let projected = native_agent_hook_cas_readback(&good, &request).unwrap();
        assert_eq!(projected["revision"], 8);
        assert!(!projected.to_string().contains("DROP_ME"));
        for bad in [
            json!({"revision":7,"agentStatusHooks":{"codex":true,"claude":true,"grok":false}}),
            json!({"revision":9,"agentStatusHooks":{"codex":true,"claude":true,"grok":false}}),
            json!({"revision":8,"agentStatusHooks":{"codex":true,"claude":false,"grok":false}}),
            json!({"revision":8,"agentStatusHooks":{"codex":false,"claude":true,"grok":false}}),
            json!({"revision":8,"agentStatusHooks":{"codex":true}}),
        ] {
            assert!(native_agent_hook_cas_readback(&bad, &request).is_err());
        }
    }

    #[test]
    fn exhausted_revision_must_not_emit_a_write_the_native_ui_cannot_read_back() {
        // The Native parser rejects i64::MAX; a CAS at MAX-1 would advance
        // to that unrepresentable revision. Reject *before* contacting Host.
        assert!(native_agent_hook_cas_payload(
            &host(i64::MAX - 1),
            "codex",
            i64::MAX - 1,
            false,
            true
        )
        .is_err());
    }

    #[test]
    fn capacity_is_checked_after_filtering_only_known_current_provider_keys() {
        let mut current = host(7);
        let mut values = serde_json::Map::new();
        for i in 0..64 {
            values.insert(format!("p{i}"), json!(false));
        }
        current["agentStatusHooks"] = serde_json::Value::Object(values);
        assert!(native_agent_hook_cas_payload(&current, "new-agent", 7, false, true).is_err());
        assert!(
            native_agent_hook_cas_payload(&current, "p63", 7, false, true)
                .unwrap()
                .is_some()
        );
    }
}
