//! View Options controls for the GPUI project sidebar.
use super::*;
use alera_gpui_desktop::runtime_catalog::RuntimeCatalog;
use alera_gpui_desktop::sidebar_manual_order::SIDEBAR_SORT_CHOICES;
use native_components::action_button;

#[derive(Default)]
pub(super) struct SidebarViewOptions {
    pub project_roots: HashSet<PathBuf>,
    tag_ids: HashSet<String>,
    primary_only: Option<bool>,
    active_only: bool,
}

impl SidebarViewOptions {
    pub(super) fn is_filtered(&self) -> bool {
        !self.project_roots.is_empty()
            || !self.tag_ids.is_empty()
            || self.primary_only.is_some()
            || self.active_only
    }

    pub(super) fn includes_worktree(
        &self,
        entry: &WorktreeEntry,
        catalog: Option<&RuntimeCatalog>,
        has_active_agent: bool,
    ) -> bool {
        if self
            .primary_only
            .is_some_and(|primary| primary != entry.is_primary)
            || (self.active_only && !has_active_agent)
        {
            return false;
        }
        self.tag_ids.is_empty()
            || catalog.is_some_and(|catalog| {
                catalog.workspaces.iter().any(|workspace| {
                    workspace.path == entry.path
                        && workspace.tag_ids.iter().any(|id| self.tag_ids.contains(id))
                })
            })
    }
}

impl Workbench {
    /// Both Native and Classic consume the same versioned Host record.
    /// A slow tree refresh must not overwrite a just-confirmed sort change.
    pub(super) fn accept_sidebar_order_prefs(&mut self, prefs: SidebarOrderPrefs) {
        if self
            .sidebar_order_prefs
            .as_ref()
            .is_none_or(|current| prefs.revision >= current.revision)
        {
            self.sidebar_order_prefs = Some(prefs);
        }
    }

    fn refresh_sidebar_sort_options(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_order_busy {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.sidebar_sort_error = Some("Shared sidebar sort unavailable".into());
            cx.notify();
            return;
        };
        self.sidebar_order_busy = true;
        self.sidebar_sort_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.sidebar_order_snapshot() })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.sidebar_order_busy = false;
                match result {
                    Ok(prefs) => view.accept_sidebar_order_prefs(prefs),
                    Err(_) => {
                        view.sidebar_sort_error = Some("Shared sidebar sort unavailable".into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_sidebar_sort_mode(
        &mut self,
        kind: &'static str,
        mode: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_order_busy || self.sidebar_order_prefs.is_none() {
            return;
        }
        if self
            .sidebar_order_prefs
            .as_ref()
            .is_some_and(|prefs| match kind {
                "project" => prefs.project_sort == mode,
                "workspace" => prefs.workspace_sort == mode,
                _ => true,
            })
        {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.sidebar_sort_error = Some("Shared sidebar sort unavailable".into());
            cx.notify();
            return;
        };
        self.sidebar_order_busy = true;
        self.sidebar_sort_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.set_sidebar_sort(kind, mode) })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.sidebar_order_busy = false;
                match result {
                    Ok(prefs) => {
                        view.accept_sidebar_order_prefs(prefs);
                        view.model.set_status("Shared sidebar sort saved");
                    }
                    Err(error) => {
                        view.sidebar_sort_error = Some("Sidebar sort could not be saved".into());
                        view.model
                            .set_status(format!("Sidebar sort not saved: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn render_sidebar_sort_row(
        &self,
        kind: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let locale = self.settings_menu_locale;
        let selected = self.sidebar_order_prefs.as_ref().and_then(|prefs| {
            let mode = if kind == "project" {
                &prefs.project_sort
            } else {
                &prefs.workspace_sort
            };
            SIDEBAR_SORT_CHOICES
                .iter()
                .find(|(id, _)| *id == mode)
                .map(|(id, _)| *id)
        });
        let value = SIDEBAR_SORT_CHOICES
            .iter()
            .find(|(id, _)| Some(*id) == selected)
            .map(|(_, label)| tr(locale, label))
            .unwrap_or_else(|| tr(locale, "Unavailable"));
        let owner = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(tr(locale, label)),
            )
            .child(
                Button::new(if kind == "project" {
                    "sidebar-project-sort"
                } else {
                    "sidebar-workspace-sort"
                })
                .compact()
                .ghost()
                .h(px(26.0))
                .px_2()
                .disabled(self.sidebar_order_busy || selected.is_none())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .child(value)
                        .child(icon(UiIcon::ChevronDown, 14.0, MUTED)),
                )
                .dropdown_menu(move |mut menu, _, _| {
                    for (id, label) in SIDEBAR_SORT_CHOICES {
                        let owner = owner.clone();
                        let label = tr(locale, label);
                        let item = PopupMenuItem::new(label).checked(Some(id) == selected);
                        menu = menu.item(item.on_click(move |_, _, cx| {
                            let _ = owner
                                .update(cx, |view, cx| view.set_sidebar_sort_mode(kind, id, cx));
                        }));
                    }
                    menu.min_w(px(160.0))
                }),
            )
            .into_any_element()
    }

    pub(super) fn open_sidebar_view_options(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_sidebar_sort_options(cx);
        let owner = cx.entity().downgrade();
        let locale = self.settings_menu_locale;
        window.open_dialog(cx, move |dialog, _, cx| {
            let content = owner
                .upgrade()
                .map(|view| view.update(cx, |view, cx| view.render_sidebar_view_options(cx)));
            dialog
                .title(tr(locale, "View Options"))
                .w(px(460.0))
                .centered()
                .overlay_closable(true)
                .child(
                    div()
                        .id("sidebar-view-options-scroll")
                        .max_h(px(460.0))
                        .overflow_y_scroll()
                        .children(content),
                )
        });
    }

    fn render_sidebar_view_options(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let prefs = &self.sidebar_view_options;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            // Match Classic's grouped project layout: compact, independent
            // dropdown rows with the selected value at the trailing edge.
            .child(self.render_sidebar_sort_row("project", "Sort Projects By", cx))
            .child(self.render_sidebar_sort_row("workspace", "Then Workspaces By", cx))
            .when(self.sidebar_order_busy, |body| {
                body.child(div().text_xs().text_color(rgb(MUTED)).child(tr(
                    self.settings_menu_locale,
                    "Loading sidebar sort settings",
                )))
            })
            .when_some(self.sidebar_sort_error.as_ref(), |body, error| {
                body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(tr(self.settings_menu_locale, error)),
                        )
                        .child(
                            action_button(
                                "sidebar-sort-retry",
                                tr(self.settings_menu_locale, "Retry"),
                                false,
                                false,
                            )
                            .on_click(
                                cx.listener(|view, _, _, cx| view.refresh_sidebar_sort_options(cx)),
                            ),
                        ),
                )
            })
            .child(div().h(px(1.0)).bg(rgb(BORDER)))
            .child(tr(self.settings_menu_locale, "Show Workspaces"))
            .child(
                div().flex().gap_1().children(
                    [
                        (None, "All"),
                        (Some(true), "Default"),
                        (Some(false), "Non-Default"),
                    ]
                    .into_iter()
                    .map(|(kind, label)| {
                        action_button(label, label, prefs.primary_only == kind, false).on_click(
                            cx.listener(move |view, _, _, cx| {
                                view.sidebar_view_options.primary_only = kind;
                                cx.notify();
                            }),
                        )
                    }),
                ),
            );
        body = body.child(
            action_button(
                "sidebar-active-only",
                "Active Workspaces Only",
                prefs.active_only,
                false,
            )
            .on_click(cx.listener(|view, _, _, cx| {
                view.sidebar_view_options.active_only = !view.sidebar_view_options.active_only;
                cx.notify();
            })),
        );
        if let Some(catalog) = &self.runtime_catalog {
            body = body
                .child("Projects")
                .children(catalog.projects.iter().enumerate().map(|(index, project)| {
                    let root = project.root.clone();
                    action_button(
                        ("sidebar-filter-project", index),
                        project.name.clone(),
                        prefs.project_roots.contains(&root),
                        false,
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let roots = &mut view.sidebar_view_options.project_roots;
                        if !roots.remove(&root) {
                            roots.insert(root.clone());
                        }
                        cx.notify();
                    }))
                }));
            body = body
                .child("Tags")
                .children(catalog.tags.iter().enumerate().map(|(index, tag)| {
                    let id = tag.id.clone();
                    action_button(
                        ("sidebar-filter-tag", index),
                        tag.name.clone(),
                        prefs.tag_ids.contains(&id),
                        false,
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let tags = &mut view.sidebar_view_options.tag_ids;
                        if !tags.remove(&id) {
                            tags.insert(id.clone());
                        }
                        cx.notify();
                    }))
                }));
        }
        body.child(
            action_button("sidebar-clear-filters", "Clear Filters", false, false).on_click(
                cx.listener(|view, _, _, cx| {
                    view.sidebar_view_options = SidebarViewOptions::default();
                    cx.notify();
                }),
            ),
        )
        .into_any_element()
    }
}
