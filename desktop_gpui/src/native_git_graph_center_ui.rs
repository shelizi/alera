//! Read-only mixed-pane Git History tab backed by the bounded Native Git Graph.
//! A first-class mixed tab now owns only presentation and pane placement.
//! It never owns, saves, closes or duplicates Editor buffers or Host PTYs.

use super::*;

impl Workbench {
    pub(super) fn open_center_git_graph(&mut self, cx: &mut Context<Self>) {
        self.git_history_tab_open = true;
        self.git_graph_center_open = true;
        // Classic's main-area Graph starts with all reachable branch tips.
        // Keep an explicitly selected reference or favorites projection intact.
        let expand_all = self.git_history_options.selected_ref.is_none()
            && !self.git_history_options.favorites_only
            && !self.git_history_options.all_branches;
        if expand_all {
            self.git_history_options.all_branches = true;
        }
        let first_open = !self.git_history_open;
        self.git_history_open = true;
        self.sync_mixed_tabs(cx);
        let identity = WorkbenchTabIdentity::GitHistory;
        self.mixed_panes.activate(&identity);
        self.mixed_tabs.select(&identity);
        self.save_mixed_pane_layout(cx);
        if first_open || expand_all || self.git_history.is_none() {
            self.refresh_git_history(cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn close_center_git_graph(&mut self, cx: &mut Context<Self>) {
        self.close_mixed_tab(&WorkbenchTabIdentity::GitHistory, cx);
    }

    /// Switch History Tab perspective without accepting arbitrary revspecs.
    /// Only a branch still present in the read-only snapshot may be selected.
    pub(super) fn select_git_graph_center_perspective(
        &mut self,
        choice: GitGraphCenterPerspective,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation || !self.git_history_tab_open {
            return;
        }
        let next = match choice {
            GitGraphCenterPerspective::Current => (false, false, None),
            GitGraphCenterPerspective::AllBranches => (true, false, None),
            GitGraphCenterPerspective::Favorites => (true, true, None),
            GitGraphCenterPerspective::Branch { name, remote } => {
                let verified = self.git_history.as_ref().is_some_and(|history| {
                    history
                        .branches
                        .iter()
                        .any(|branch| branch.name == name && branch.remote == remote)
                });
                if !verified {
                    self.model
                        .set_status("Git Graph branch changed; refresh perspective menu");
                    cx.notify();
                    return;
                }
                let reference = if remote {
                    format!("refs/remotes/{name}")
                } else {
                    name
                };
                (false, false, Some(reference))
            }
        };
        if (
            self.git_history_options.all_branches,
            self.git_history_options.favorites_only,
            self.git_history_options.selected_ref.clone(),
        ) == next
        {
            return;
        }
        self.git_history_options.all_branches = next.0;
        self.git_history_options.favorites_only = next.1;
        self.git_history_options.selected_ref = next.2;
        self.save_mixed_pane_layout(cx);
        self.refresh_git_history(cx);
    }

    /// Read-only comparison reuses the Source Control engine and verifies
    /// both the snapshot's local-branch identity and Workspace generation.
    pub(super) fn compare_git_graph_center_branch(
        &mut self,
        name: String,
        direction: CompareDirection,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation || !self.git_history_tab_open {
            return;
        }
        let live = self.git_history.as_ref().is_some_and(|history| {
            history
                .branches
                .iter()
                .any(|branch| !branch.remote && !branch.current && branch.name == name)
        });
        if !live {
            self.model
                .set_status("Git Graph comparison branch is stale; refresh Graph");
            cx.notify();
            return;
        }
        self.compare_git_branch_files(name, direction, cx);
    }

    /// Read-only merged branch display settings; switching cancels any stale
    /// graph load and persists only the existing workspace-local tab layout.
    pub(super) fn set_git_graph_merged_visibility(
        &mut self,
        visibility: GitMergedBranchVisibility,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation || !self.git_history_tab_open {
            return;
        }
        if self.git_history_options.merged_visibility == visibility {
            return;
        }
        self.git_history_options.merged_visibility = visibility;
        self.save_mixed_pane_layout(cx);
        self.refresh_git_history(cx);
    }

    /// Accept only a branch in the current snapshot, never arbitrary revspecs.
    pub(super) fn set_git_graph_merged_target(
        &mut self,
        target: Option<String>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation || !self.git_history_tab_open {
            return;
        }
        if let Some(reference) = target.as_deref() {
            let valid = self.git_history.as_ref().is_some_and(|history| {
                history.branches.iter().any(|branch| {
                    let key = if branch.remote {
                        format!("refs/remotes/{}", branch.name)
                    } else {
                        branch.name.clone()
                    };
                    key == reference
                })
            });
            if !valid {
                self.model
                    .set_status("Git Graph merge target changed; refresh and retry");
                cx.notify();
                return;
            }
        }
        if self.git_history_options.merged_target_ref == target {
            return;
        }
        self.git_history_options.merged_target_ref = target;
        self.save_mixed_pane_layout(cx);
        self.refresh_git_history(cx);
    }

    pub(super) fn render_center_git_graph(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let query = GitGraphSearch::parse(self.git_graph_search_input.read(cx).query());
        let full_search = self.git_history_options.search_query.is_some();
        let commits = if self.git_history_loading && full_search {
            Vec::new()
        } else {
            self.git_history
                .as_ref()
                .map(|history| history.display_commits(self.git_history_options.merged_visibility))
                .unwrap_or_default()
        };
        // Layout MUST be built before filtering. Sparse search results must
        // not gain invented connections across commits removed by the filter.
        let graph_layout = (!full_search).then(|| {
            layout_git_graph(
                &commits
                    .iter()
                    .map(|row| GraphCommit {
                        oid: row.oid.clone(),
                        parents: row.parent_ids.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let (rails, lane_count, topology_error) = match graph_layout {
            Some(Ok(layout)) => (Some(layout.rows), layout.max_lanes, None),
            Some(Err(error)) => (None, 1, Some(error.message().to_owned())),
            None => (None, 1, None),
        };
        let branch_name = self
            .git_history
            .as_ref()
            .map(|history| history.current.clone())
            .unwrap_or_default();
        let head_tip = self
            .git_history
            .as_ref()
            .and_then(|history| history.branches.iter().find(|branch| branch.current))
            .map(|branch| branch.tip.clone());
        let mut tags_by_oid: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(history) = &self.git_history {
            for tag in &history.tags {
                tags_by_oid
                    .entry(tag.oid.clone())
                    .or_default()
                    .push(format!("refs/tags/{}", tag.name));
            }
        }
        // In a split pane preserve the commit subject and SHA before author
        // and extra ref badges; full-width Graph keeps its detailed columns.
        let compact_graph = self.mixed_panes.has_secondary();
        let badge_limit = if compact_graph { 1 } else { 3 };
        let rows = commits
            .into_iter()
            .enumerate()
            .filter(|(_, row)| full_search || query.commit(row))
            .take(MAX_HISTORY_PAGE)
            .map(|(index, row)| {
                let oid = row.oid;
                let menu_oid = oid.clone();
                let menu_subject = row.subject.clone();
                let topology = rails.as_ref().and_then(|rows| rows.get(index)).cloned();
                let is_merge = row.parent_ids.len() > 1;
                let is_head = head_tip.as_ref().is_some_and(|tip| tip == &row.short);
                let mut labels = row.labels;
                if let Some(tags) = tags_by_oid.get(&oid) {
                    labels.extend(tags.iter().cloned());
                }
                labels.sort_by_key(|label| {
                    if label == &branch_name {
                        0
                    } else if label.starts_with("refs/tags/") {
                        1
                    } else if label.starts_with("refs/remotes/") {
                        3
                    } else {
                        2
                    }
                });
                let hidden_labels = labels.len().saturating_sub(badge_limit);
                let author = row.author;
                div()
                    .id(("git-center-graph-commit", index))
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(ACTIVE)))
                    .child(native_git_graph_ui::graph_lane(
                        topology, lane_count, is_merge, is_head,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(row.subject),
                    )
                    .children(labels.into_iter().take(badge_limit).map(|label| {
                        let color = if label.starts_with("refs/remotes/") {
                            native_git_graph_ui::REF_REMOTE_COLOR
                        } else if label.starts_with("refs/tags/") {
                            native_git_graph_ui::REF_TAG_COLOR
                        } else {
                            native_git_graph_ui::REF_LOCAL_COLOR
                        };
                        let display = label
                            .strip_prefix("refs/tags/")
                            .or_else(|| label.strip_prefix("refs/remotes/"))
                            .unwrap_or(&label);
                        let display = if display.chars().count() > 24 {
                            format!("{}…", display.chars().take(23).collect::<String>())
                        } else {
                            display.to_owned()
                        };
                        div()
                            .px_1()
                            .rounded_sm()
                            .bg(rgb(ELEVATED))
                            .text_xs()
                            .text_color(rgb(color))
                            .child(display)
                    }))
                    .when(hidden_labels > 0, |item| {
                        item.child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(format!("+{hidden_labels}")),
                        )
                    })
                    .when(!compact_graph, |entry| {
                        entry.child(
                            div()
                                .w(px(116.0))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(author),
                        )
                    })
                    .child(
                        div()
                            .w(px(66.0))
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(row.short),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preview_git_history(GitReadRequest::Commit(oid.clone()), cx);
                    }))
                    .context_menu(Self::kit_git_graph_commit_menu(
                        cx.entity().downgrade(),
                        menu_oid,
                        menu_subject,
                        row.parents,
                    ))
            })
            .collect::<Vec<_>>();

        let count = rows.len();
        let loading = self.git_history_loading;
        let more = !loading
            && self
                .git_history
                .as_ref()
                .is_some_and(|history| history.truncated && !history.scan_limit_reached)
            && self.git_history_options.max_commits.max(HISTORY_PAGE_STEP) < MAX_HISTORY_PAGE;
        let all_branches = self.git_history_options.all_branches;
        let favorite_scope = self.git_history_options.favorites_only;
        let selected_ref = self.git_history_options.selected_ref.clone();
        let branch_choices = self
            .git_history
            .as_ref()
            .map(|history| history.branches.clone())
            .unwrap_or_default();
        let perspective = if favorite_scope {
            GitGraphCenterPerspective::Favorites
        } else if all_branches {
            GitGraphCenterPerspective::AllBranches
        } else if let Some(reference) = selected_ref.as_deref() {
            let selected = branch_choices.iter().find(|branch| {
                let expected = if branch.remote {
                    format!("refs/remotes/{}", branch.name)
                } else {
                    branch.name.clone()
                };
                expected == reference
            });
            selected
                .map(|branch| GitGraphCenterPerspective::Branch {
                    name: branch.name.clone(),
                    remote: branch.remote,
                })
                .unwrap_or(GitGraphCenterPerspective::Current)
        } else {
            GitGraphCenterPerspective::Current
        };
        let scope = match &perspective {
            GitGraphCenterPerspective::Current => "HEAD".into(),
            GitGraphCenterPerspective::AllBranches => "All branches".into(),
            GitGraphCenterPerspective::Favorites => "Favorite branches".into(),
            GitGraphCenterPerspective::Branch { name, remote } => {
                if *remote {
                    format!("Remote: {name}")
                } else {
                    name.clone()
                }
            }
        };
        let merged_mode = self.git_history_options.merged_visibility;
        let merged_target = self.git_history_options.merged_target_ref.clone();
        let merge_controls = all_branches && !favorite_scope && selected_ref.is_none();
        let merged_target_display = merged_target
            .as_deref()
            .map(|reference| reference.strip_prefix("refs/remotes/").unwrap_or(reference))
            .unwrap_or("HEAD")
            .to_owned();
        let merged_target_missing = merged_mode != GitMergedBranchVisibility::ShowAll
            && merged_target.is_some()
            && self
                .git_history
                .as_ref()
                .is_some_and(|history| history.merged_target_oid.is_none());
        let merged_target_off_page = merged_mode == GitMergedBranchVisibility::HideMergedGraph
            && self.git_history.as_ref().is_some_and(|history| {
                history
                    .merged_target_oid
                    .as_deref()
                    .is_some_and(|oid| !history.commits.iter().any(|row| row.oid == oid))
            });
        let current_branch_for_merge = self
            .git_history
            .as_ref()
            .map(|history| history.current.clone())
            .unwrap_or_else(|| "HEAD".into());
        let graph_generation = self.workspace_generation;
        let description = if loading {
            "Loading Git history…".to_owned()
        } else if self.git_history.is_some() {
            // Favorite-sync/Host warnings must not mask a loaded local Git graph.
            format!("{count} commit(s) · {scope}")
        } else if let Some(error) = &self.git_history_error {
            format!("Git history unavailable: {error}")
        } else {
            "No Git history loaded".into()
        };

        div()
            .id("git-center-full-graph")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .h(px(44.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(icon(UiIcon::Graph, 16.0, TEXT))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child("Git Graph"),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(description),
                    )
                    .child(
                        icon_button(
                            "git-center-refresh",
                            UiIcon::Refresh,
                            "Refresh Git Graph (read-only)",
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.refresh_git_history(cx);
                        })),
                    )
                    .child(
                        icon_button(
                            "git-center-close",
                            UiIcon::Close,
                            "Close Git History tab without closing other workbench tabs",
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_center_git_graph(cx);
                        })),
                    ),
            )
            .child(
                div()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.git_graph_search_input.clone()),
                    )
                    .child(
                        icon_button(
                            "git-center-search-all",
                            UiIcon::Search,
                            "Search reachable history in the background (up to 50,000 commits)",
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_all_git_history(cx);
                        })),
                    )
                    .when(full_search, |panel| {
                        panel.child(
                            icon_button(
                                "git-center-return-paged",
                                UiIcon::History,
                                "Return to paged graph history",
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.exit_git_history_search(cx);
                            })),
                        )
                    })
                    .child(
                        icon_button(
                            "git-center-clear-query",
                            UiIcon::Close,
                            "Clear Git Graph filter",
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.git_graph_search_input
                                .update(cx, |input, cx| input.clear(cx));
                            this.exit_git_history_search(cx);
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .h(px(34.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("git-center-perspective-label")
                            .min_w_0()
                            .max_w(px(if compact_graph { 164.0 } else { 280.0 }))
                            .truncate()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(format!("Perspective: {scope}")),
                    )
                    .child(
                        icon_button(
                            "git-center-perspective-menu",
                            UiIcon::ChevronDown,
                            "Choose HEAD, all branches, favorites, local or remote branch",
                            false,
                        )
                        .dropdown_menu(
                            Self::kit_git_center_perspective_menu(
                                cx.entity().downgrade(),
                                branch_choices.clone(),
                                perspective,
                                graph_generation,
                            ),
                        ),
                    )
                    .when(merge_controls, |bar| {
                        bar.child(
                            div().text_xs().text_color(rgb(MUTED))
                                .child(if compact_graph {
                                    match merged_mode {
                                        GitMergedBranchVisibility::ShowAll => "Merged: All".to_owned(),
                                        GitMergedBranchVisibility::HideMergedNames => "Hide Names".to_owned(),
                                        GitMergedBranchVisibility::HideMergedGraph => "Hide Graph".to_owned(),
                                    }
                                } else {
                                    format!("Merged: {}", merged_mode.label())
                                }),
                        )
                        .child(
                            icon_button(
                                "git-center-merged-visibility-menu",
                                UiIcon::ChevronDown,
                                "Classic merged branch visibility: show all, hide merged names, or flatten merged graph",
                                false,
                            )
                            .dropdown_menu(Self::kit_git_center_merged_visibility_menu(
                                cx.entity().downgrade(), merged_mode, graph_generation,
                            )),
                        )
                        .when(merged_mode != GitMergedBranchVisibility::ShowAll, |bar| {
                            bar.when(!compact_graph, |bar| bar.child(
                                div().max_w(px(160.0)).truncate()
                                    .text_xs().text_color(rgb(if merged_target_missing { TEXT } else { MUTED }))
                                    .child(if merged_target_missing {
                                        "Merge target missing".to_owned()
                                    } else if merged_target_off_page {
                                        "Target beyond loaded page".to_owned()
                                    } else {
                                        format!("Into: {merged_target_display}")
                                    })
                            ))
                            .child(
                                icon_button(
                                    "git-center-merged-target-menu",
                                    UiIcon::ChevronDown,
                                    "Choose the target branch for merged-name and graph collapse",
                                    false,
                                )
                                .dropdown_menu(Self::kit_git_center_merged_target_menu(
                                    cx.entity().downgrade(),
                                    branch_choices.clone(),
                                    current_branch_for_merge,
                                    merged_target.clone(),
                                    graph_generation,
                                )),
                            )
                        })
                    })
                    .child(
                        icon_button(
                            "git-center-compare-menu",
                            UiIcon::History,
                            "Compare local branch changes with HEAD (read-only)",
                            false,
                        )
                        .dropdown_menu(Self::kit_git_center_compare_menu(
                            cx.entity().downgrade(),
                            branch_choices,
                            selected_ref,
                            graph_generation,
                        )),
                    )
                    .child(div().flex_1())
                    .when(more, |bar| {
                        bar.child(
                            icon_button(
                                "git-center-load-more",
                                UiIcon::Add,
                                "Load 96 more commits (maximum 512)",
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.load_more_git_history(cx);
                            })),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{count} / {MAX_HISTORY_PAGE} max")),
                    ),
            )
            .when(full_search || topology_error.is_some(), |pane| {
                pane.child(div().px_3().py_1().text_xs().text_color(rgb(MUTED)).child(
                    if full_search {
                        "Sparse search matches: parent rails are intentionally hidden".into()
                    } else {
                        format!(
                            "Graph topology unavailable: {}",
                            topology_error.unwrap_or_default()
                        )
                    },
                ))
            })
            .child(
                div()
                    .id("git-center-graph-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }
}
