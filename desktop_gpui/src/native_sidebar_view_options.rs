//! View Options controls for the GPUI project sidebar.
use super::*;
use alera_gpui_desktop::runtime_catalog::RuntimeCatalog;
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
    pub(super) fn open_sidebar_view_options(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let content = owner
                .upgrade()
                .map(|view| view.update(cx, |view, cx| view.render_sidebar_view_options(cx)));
            dialog
                .title("View Options")
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
            .child("Show Workspaces")
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
