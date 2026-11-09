//! Rust Host-owned Git favorites for GPUI Git Graph.
//! Never replace Host data with preview JSON, never block GPUI on CLI I/O.
use super::*;

impl Workbench {
    pub(super) fn refresh_shared_git_favorites(&mut self, cx: &mut Context<Self>) {
        if self.git_favorites_busy {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.git_history_error = Some("Git favorites need the existing Rust Host/CLI".into());
            cx.notify();
            return;
        };
        let store = self.session_store.clone();
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_favorites_busy = true;
        self.git_favorites_last_refresh = Instant::now();
        self.git_favorites_request = self.git_favorites_request.wrapping_add(1);
        let request = self.git_favorites_request;
        cx.spawn(async move |view, cx| {
            let query_root = root.clone();
            let result = cx
                .background_spawn(async move {
                    // Unlike an explicit saved [], an absent Native snapshot
                    // must not establish an empty Host tombstone and suppress
                    // a delayed Classic first-time Drift import.
                    match store {
                        Some(store) if store.has_legacy_git_favorites(&query_root)? => {
                            let legacy = store.load_git_favorites(&query_root)?;
                            bridge.git_favorites_seed_native(&query_root, &legacy)
                        }
                        _ => bridge.git_favorites_for_workspace(&query_root),
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_favorites_request != request
                    || view.model.root() != root.as_path()
                {
                    return;
                }
                view.git_favorites_busy = false;
                match result {
                    Ok(snapshot) => {
                        match serde_json::from_value::<Vec<String>>(snapshot["branches"].clone()) {
                            Ok(branches) => {
                                if view.git_history_options.favorites != branches {
                                    view.git_history_options.favorites = branches;
                                    if view.git_history_open {
                                        view.refresh_git_history(cx);
                                    }
                                }
                            }
                            Err(_) => view
                                .model
                                .set_status("Host returned malformed Git favorites; unchanged"),
                        }
                    }
                    Err(error) => view.model.set_status(format!(
                        "Git favorites not synchronized; Rust Host unavailable: {error}"
                    )),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn persist_shared_git_favorite(&mut self, branch: String, cx: &mut Context<Self>) {
        if self.git_favorites_busy {
            self.model
                .set_status("Git favorites are syncing; retry the star click");
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Cannot save Git favorites without Rust Host");
            cx.notify();
            return;
        };
        let expected = self.git_history_options.favorites.clone();
        let mut next = expected.clone();
        if let Some(index) = next.iter().position(|name| name == &branch) {
            next.remove(index);
        } else {
            next.push(branch);
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_favorites_busy = true;
        self.git_favorites_last_refresh = Instant::now();
        self.git_favorites_request = self.git_favorites_request.wrapping_add(1);
        let request = self.git_favorites_request;
        cx.spawn(async move |view, cx| {
            let query_root = root.clone();
            let result = cx
                .background_spawn(async move {
                    bridge.git_favorites_compare_and_set(&query_root, &expected, &next)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_favorites_request != request
                    || view.model.root() != root.as_path()
                {
                    return;
                }
                view.git_favorites_busy = false;
                match result {
                    Ok(snapshot) => {
                        match serde_json::from_value::<Vec<String>>(snapshot["branches"].clone()) {
                            Ok(branches) => {
                                view.git_history_options.favorites = branches;
                                view.model
                                    .set_status("Git favorites saved in Rust Runtime Host");
                                if view.git_history_open {
                                    view.refresh_git_history(cx);
                                }
                            }
                            Err(_) => {
                                view.model.set_status("Git favorites readback invalid");
                                view.refresh_shared_git_favorites(cx);
                            }
                        }
                    }
                    Err(error) => {
                        view.model.set_status(format!(
                            "Git favorites conflict or Host offline; refreshing: {error}"
                        ));
                        view.refresh_shared_git_favorites(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
