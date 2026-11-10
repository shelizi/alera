//! Host-owned Tab pin state for the mixed GPUI strip.
//! Never write a Native fallback when Runtime Host is absent or stale.
use super::*;

impl Workbench {
    pub(super) fn refresh_host_tab_pins(&mut self, cx: &mut Context<Self>) {
        if self.host_pins_loading || self.host_pins_busy {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            return;
        };
        let root = self.model.root().to_path_buf();
        let attached_workspace_ids: Vec<String> = self
            .mixed_tabs
            .entries()
            .iter()
            .filter_map(|tab| match &tab.identity {
                WorkbenchTabIdentity::Host(target) => Some(target.workspace_id().to_owned()),
                WorkbenchTabIdentity::Editor { .. } => None,
            })
            .collect();
        let generation = self.workspace_generation;
        self.host_pins_loading = true;
        self.host_pins_request = self.host_pins_request.wrapping_add(1);
        let ticket = self.host_pins_request;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    // Resolve every owner through the authenticated Host
                    // catalog. The active root may not itself be registered
                    // when the user has manually attached another Workspace.
                    let registered = bridge.list_registered_workspaces()?;
                    let root_workspace = match select_registered_host_workspace(&root, &registered)
                    {
                        Ok(id) => Some(id),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => return Err(error),
                    };
                    let workspaces = host_pin_query_workspaces(
                        root_workspace.as_deref(),
                        &attached_workspace_ids,
                        &registered,
                    );
                    if workspaces.is_empty() || workspaces.len() > 24 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "No registered Host Tab owners, or too many attached Workspaces",
                        ));
                    }
                    let mut pins = Vec::new();
                    for workspace_id in &workspaces {
                        pins.extend(bridge.host_tab_pin_list(workspace_id)?);
                    }
                    Ok::<_, std::io::Error>(pins)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.host_pins_request != ticket {
                    return;
                }
                view.host_pins_loading = false;
                match result {
                    Ok(pins) => {
                        view.host_pins_error = None;
                        let map: HashMap<_, _> = pins
                            .into_iter()
                            .map(|pin| ((pin.workspace_id.clone(), pin.id.clone()), pin))
                            .collect();
                        let ids: Vec<_> = view
                            .mixed_tabs
                            .entries()
                            .iter()
                            .map(|tab| tab.identity.clone())
                            .collect();
                        for id in ids {
                            let WorkbenchTabIdentity::Host(target) = &id else {
                                continue;
                            };
                            let key =
                                (target.workspace_id().to_owned(), target.tab_id().to_owned());
                            let pinned = map
                                .get(&key)
                                .filter(|pin| pin.matches_target(target))
                                .is_some_and(|pin| pin.pinned);
                            view.mixed_tabs.set_pinned(&id, pinned);
                        }
                        view.host_pin_snapshots = map;
                    }
                    Err(error) => {
                        // An old/offline Host cannot authorize a new CAS.
                        view.host_pin_snapshots.clear();
                        view.host_pins_error = Some(error.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn toggle_host_tab_pinned(
        &mut self,
        target: TerminalTabContextTarget,
        cx: &mut Context<Self>,
    ) {
        if !self.terminals.context_target_is_live(&target) {
            self.terminals.status = "Host Tab changed or is offline; refresh before pinning".into();
            cx.notify();
            return;
        }
        if self.host_pins_busy || self.host_pins_loading {
            self.terminals.status = "Host pin snapshot/update pending; retry after refresh".into();
            cx.notify();
            return;
        }
        let key = (target.workspace_id().to_owned(), target.tab_id().to_owned());
        let Some(observed) = self
            .host_pin_snapshots
            .get(&key)
            .filter(|pin| pin.matches_target(&target))
            .cloned()
        else {
            self.terminals.status = match self.host_pins_error.as_deref() {
                Some(error) => {
                    format!("Host pin sharing unavailable ({error}); compatible Host/CLI required")
                }
                None => "Host pin metadata unavailable; refresh before pinning".into(),
            };
            self.refresh_host_tab_pins(cx);
            cx.notify();
            return;
        };
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Host CLI unavailable; pin not changed".into();
            cx.notify();
            return;
        };
        let next = !observed.pinned;
        let generation = self.workspace_generation;
        self.host_pins_busy = true;
        self.host_pins_request = self.host_pins_request.wrapping_add(1);
        let ticket = self.host_pins_request;
        let target_for_rpc = target.clone();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    // Agent metadata may advance updatedAt between 15s
                    // refreshes. Fetch the latest revision, but reject if
                    // Classic has changed the actual pin in the meantime.
                    let fresh = bridge.host_tab_pin_list(&observed.workspace_id)?;
                    let current = fresh
                        .into_iter()
                        .find(|pin| pin.matches_target(&target_for_rpc))
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "Original Host Tab was closed or replaced",
                            )
                        })?;
                    if current.pinned != observed.pinned {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "Tab pin changed in Classic; refresh the current Tab",
                        ));
                    }
                    bridge.host_tab_pin_compare_and_set(&current, next)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.host_pins_request != ticket {
                    return;
                }
                view.host_pins_busy = false;
                let id = WorkbenchTabIdentity::Host(target.clone());
                match result {
                    Ok(readback)
                        if view.terminals.context_target_is_live(&target)
                            && view.mixed_tabs.contains(&id)
                            && readback.matches_target(&target) =>
                    {
                        view.host_pin_snapshots.insert(key, readback);
                        view.mixed_tabs.set_pinned(&id, next);
                        view.terminals.status = if next {
                            "Host Tab pinned"
                        } else {
                            "Host Tab unpinned"
                        }
                        .into();
                    }
                    Ok(_) => {
                        view.terminals.status =
                            "Host saved the pin, but its original preview is gone; refresh".into();
                        view.refresh_host_tab_pins(cx);
                    }
                    Err(error) => {
                        view.terminals.status =
                            format!("Host pin rejected: {error}. Refreshed current Tab state");
                        view.refresh_host_tab_pins(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
