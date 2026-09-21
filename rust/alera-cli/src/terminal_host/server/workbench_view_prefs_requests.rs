use alera_core::runtime::{RuntimeStore, SharedWorkbenchPrefsWriter, SharedWorkbenchViewPrefs};
use serde::Deserialize;
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateViewPrefsRequest {
    prefs: SharedWorkbenchViewPrefs,
    expected_revision: Option<i64>,
}

pub(super) struct WorkbenchViewPrefsRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkbenchViewPrefsRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn get(&self) -> HostResult<Value> {
        serde_json::to_value(
            self.runtime_store
                .shared_workbench_view_prefs()
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn update(
        &self,
        payload: &Value,
        writer: SharedWorkbenchPrefsWriter,
    ) -> HostResult<Value> {
        let mut compatible = payload.clone();
        let current = self
            .runtime_store
            .shared_workbench_view_prefs()
            .await
            .map_err(state_error)?;
        let current_json = serde_json::to_value(current.prefs).map_err(state_error)?;
        if let Some(prefs) = compatible.get_mut("prefs").and_then(Value::as_object_mut) {
            for key in [
                "sectionSort",
                "collapsedSectionIds",
                "othersSectionCollapsed",
            ] {
                if !prefs.contains_key(key) {
                    prefs.insert(key.to_string(), current_json[key].clone());
                }
            }
        }
        let request: UpdateViewPrefsRequest =
            serde_json::from_value(compatible).map_err(format_error)?;
        let value = self
            .runtime_store
            .update_shared_workbench_view_prefs(request.prefs, request.expected_revision, writer)
            .await
            .map_err(state_error)?;
        serde_json::to_value(value).map_err(state_error)
    }
}

fn format_error(error: impl std::fmt::Display) -> HostError {
    HostError::format(error.to_string())
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, SharedWorkbenchPrefsWriter};
    use serde_json::json;

    use super::WorkbenchViewPrefsRequestHandler;

    #[tokio::test]
    async fn legacy_pref_updates_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = WorkbenchViewPrefsRequestHandler::new(&store);
        let prefs = json!({
            "groupBy": "section",
            "sectionSort": "recent",
            "collapsedSectionIds": ["section"],
            "othersSectionCollapsed": true,
            "projectSort": "name",
            "workspaceSort": "name",
            "workspaceKindFilter": "all"
        });
        let saved = handler
            .update(
                &json!({"prefs": prefs}),
                SharedWorkbenchPrefsWriter::Desktop,
            )
            .await
            .unwrap();
        let mut legacy = prefs.clone();
        for field in [
            "sectionSort",
            "collapsedSectionIds",
            "othersSectionCollapsed",
        ] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["groupBy"] = json!("project");

        let updated = handler
            .update(
                &json!({"prefs": legacy, "expectedRevision": saved["revision"]}),
                SharedWorkbenchPrefsWriter::Mobile,
            )
            .await
            .unwrap();

        assert_eq!(updated["prefs"]["groupBy"], "project");
        assert_eq!(updated["prefs"]["sectionSort"], "recent");
        assert_eq!(updated["prefs"]["collapsedSectionIds"], json!(["section"]));
        assert_eq!(updated["prefs"]["othersSectionCollapsed"], true);
        assert_eq!(handler.get().await.unwrap(), updated);
    }
}
