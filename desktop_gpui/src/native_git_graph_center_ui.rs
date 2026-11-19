//! Read-only mixed-pane Git History tab backed by the bounded Native Git Graph.
//! A first-class mixed tab now owns only presentation and pane placement.
//! It never owns, saves, closes or duplicates Editor buffers or Host PTYs.

use super::*;

/// Use GPUI logical pixels, not OS physical pixels: the breakpoint must not
/// jump when an unchanged window is moved between displays with different DPI.
fn compact_center_graph_controls(has_secondary: bool, viewport_logical_width: f32) -> bool {
    has_secondary || viewport_logical_width < 720.0
}

fn center_graph_title(compact: bool, commits: usize) -> String {
    if compact {
        format!("Graph · {commits}")
    } else {
        "Git Graph".to_owned()
    }
}

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
        if self.git_graph_compare_center {
            self.close_center_branch_compare(cx);
        }
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
        let (short, remote) = name
            .strip_prefix("refs/remotes/")
            .map_or((name.as_str(), false), |short| (short, true));
        let live = self.git_history.as_ref().is_some_and(|history| {
            history
                .branches
                .iter()
                .any(|branch| branch.remote == remote && !branch.current && branch.name == short)
        });
        if !live {
            self.model
                .set_status("Git Graph comparison branch is stale; refresh Graph");
            cx.notify();
            return;
        }
        self.compare_git_center_branch_files(name, direction, cx);
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

    /// Open a bounded, case-insensitive picker over exact refs already
    /// enumerated by the read-only Git history loader.
    pub(super) fn open_git_center_branch_picker(
        &mut self,
        mode: GitGraphBranchPickerMode,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation || !self.git_history_tab_open {
            return;
        }
        self.git_branch_picker_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_branch_picker_mode = Some(mode);
        cx.notify();
    }

    pub(super) fn pick_git_center_searched_branch(
        &mut self,
        name: String,
        remote: bool,
        mode: GitGraphBranchPickerMode,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if generation != self.workspace_generation
            || self.git_branch_picker_mode != Some(mode)
            || !self.git_history_tab_open
        {
            return;
        }
        let valid = self.git_history.as_ref().is_some_and(|history| {
            history.branches.iter().any(|branch| {
                branch.name == name
                    && branch.remote == remote
                    && (mode != GitGraphBranchPickerMode::Compare || !branch.current)
            })
        });
        if !valid {
            self.model
                .set_status("Searched branch is stale; refresh Git Graph");
            cx.notify();
            return;
        }
        self.git_branch_picker_mode = None;
        self.git_branch_picker_input
            .update(cx, |input, cx| input.clear(cx));
        match mode {
            GitGraphBranchPickerMode::Perspective => {
                self.select_git_graph_center_perspective(
                    GitGraphCenterPerspective::Branch { name, remote },
                    generation,
                    cx,
                );
            }
            GitGraphBranchPickerMode::MergeTarget => {
                let target = if remote {
                    format!("refs/remotes/{name}")
                } else {
                    name
                };
                self.set_git_graph_merged_target(Some(target), generation, cx);
            }
            GitGraphBranchPickerMode::Compare => {
                self.compare_git_graph_center_branch(
                    if remote {
                        format!("refs/remotes/{name}")
                    } else {
                        name
                    },
                    CompareDirection::CurrentToSelected,
                    generation,
                    cx,
                );
            }
        }
    }

    pub(super) fn render_center_git_graph(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let filter_text = self.git_graph_search_input.read(cx).query().to_owned();
        let full_search = self.git_history_options.search_query.is_some();
        let filter_identity = format!("{full_search}:{filter_text}");
        if self.git_center_graph_filter != filter_identity {
            self.git_center_graph_filter = filter_identity;
            self.git_center_graph_scroll
                .scroll_to_item(0, gpui::ScrollStrategy::Top);
        }
        let query = GitGraphSearch::parse(&filter_text);
        let visibility = self.git_history_options.merged_visibility;
        let stale_cache = self.git_graph_render_cache.as_ref().is_none_or(|cache| {
            cache.workspace_generation != self.workspace_generation
                || cache.history_request != self.git_history_request
                || cache.visibility != visibility
                || cache.sparse_search != full_search
        });
        if stale_cache {
            self.git_graph_render_cache = self.git_history.as_ref().map(|history| {
                native_git_graph_rows_ui::GitGraphViewCache {
                    workspace_generation: self.workspace_generation,
                    history_request: self.git_history_request,
                    visibility,
                    sparse_search: full_search,
                    prepared: Arc::new(PreparedGitGraph::new(history, visibility, full_search)),
                }
            });
        }
        let prepared = self
            .git_graph_render_cache
            .as_ref()
            .map(|cache| Arc::clone(&cache.prepared));
        let graph_indices = Arc::new(if self.git_history_loading && full_search {
            Vec::new()
        } else {
            prepared
                .as_ref()
                .map(|graph| graph.matching_indices(&query, full_search))
                .unwrap_or_default()
        });
        let topology_error = prepared
            .as_ref()
            .and_then(|graph| graph.topology_error.clone());
        // Logical viewport width scales naturally with OS DPI; never use
        // physical screen pixels to decide compact Git Graph layout.
        let compact_graph = compact_center_graph_controls(
            self.mixed_panes.has_secondary(),
            f32::from(window.viewport_size().width),
        );
        let count = graph_indices.len();
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
                    || (!branch.remote && format!("refs/heads/{}", branch.name) == reference)
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
        let picker_data = self.git_branch_picker_mode.map(|mode| {
            let query = self.git_branch_picker_input.read(cx).query().to_owned();
            let compare_choices;
            let choices = if mode == GitGraphBranchPickerMode::Compare {
                compare_choices = branch_choices
                    .iter()
                    .filter(|branch| !branch.current)
                    .cloned()
                    .collect::<Vec<_>>();
                &compare_choices
            } else {
                &branch_choices
            };
            (mode, filter_git_branch_picker(choices, &query, false))
        });
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

        let graph_view = prepared.map(|graph| {
            let indices = Arc::clone(&graph_indices);
            let lane_count = graph.lane_count;
            gpui::uniform_list(
                "git-center-graph-virtual-commits",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    PreparedGitGraph::viewport_indices(&indices, range)
                        .iter()
                        .filter_map(|index| {
                            graph.rows.get(*index).map(|row| {
                                this.render_git_graph_viewport_row(
                                    *index,
                                    row,
                                    lane_count,
                                    compact_graph,
                                    cx,
                                )
                            })
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .track_scroll(&self.git_center_graph_scroll)
            .into_any_element()
        });
        let compare_panel = self
            .git_graph_compare_center
            .then(|| self.render_center_branch_compare(cx));
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
                            .child(center_graph_title(compact_graph, count)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(if compact_graph { String::new() } else { description }),
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
                    .when(compact_graph, |bar| bar.gap_1().px_2())
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(30.0))
                            .overflow_hidden()
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
                    .id("git-center-control-toolbar")
                    .h(px(34.0))
                    .flex_shrink_0()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    // Narrow split panes cannot display all Classic controls at
                    // full width. Keep every action in a single-line horizontal
                    // strip instead of squeezing them into wrapped/overlapping
                    // text. Icons retain their descriptive tooltip labels.
                    .when(compact_graph, |bar| bar.gap_1().px_2().overflow_x_scroll())
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("git-center-perspective-label")
                            .min_w_0()
                            .max_w(px(280.0))
                            .truncate()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .when(compact_graph, |label| label.hidden())
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
                    .child(
                        icon_button(
                            "git-center-find-branch",
                            UiIcon::Search,
                            "Find a local or remote branch in the full loaded inventory (read-only)",
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let generation = this.workspace_generation;
                            this.open_git_center_branch_picker(
                                GitGraphBranchPickerMode::Perspective, generation, cx,
                            );
                            window.focus(&this.git_branch_picker_input.read(cx).focus_handle(cx), cx);
                        })),
                    )
                    .when(merge_controls, |bar| {
                        bar.child(
                            div().text_xs().text_color(rgb(MUTED))
                                .when(compact_graph, |label| label.hidden())
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
                            "Compare entire committed tips in both directions (read-only)",
                            false,
                        )
                        .dropdown_menu(Self::kit_git_center_compare_menu(
                            cx.entity().downgrade(),
                            branch_choices,
                            selected_ref,
                            graph_generation,
                        )),
                    )
                    .child(div().flex_1().when(compact_graph, |spacer| spacer.hidden()))
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
                            .when(compact_graph, |label| label.hidden())
                            .child(format!("{count} / {MAX_HISTORY_PAGE} max")),
                    ),
            )
            .when_some(picker_data, |pane, (mode, matches)| {
                let available = matches.inventory_count;
                let matched = matches.match_count;
                let shown = matches.rows.len();
                pane.child(
                    div()
                        .id("git-center-branch-picker")
                        .h(px(194.0))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(PANEL))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div().flex_1().min_w_0()
                                        .child(self.git_branch_picker_input.clone()),
                                )
                                .child(
                                    icon_button(
                                        "git-center-branch-picker-close",
                                        UiIcon::Close,
                                        "Close branch search without changing Git Graph",
                                        false,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_branch_picker_mode = None;
                                        this.git_branch_picker_input.update(cx, |input, cx| input.clear(cx));
                                        cx.notify();
                                    })),
                                ),
                        )
                        .child(
                            div().text_xs().text_color(rgb(MUTED)).child(format!(
                                "{} · {matched} match(es) / {available} loaded refs · showing first {shown}{}",
                                match mode {
                                    GitGraphBranchPickerMode::Perspective => "Choose graph perspective",
                                    GitGraphBranchPickerMode::MergeTarget => "Choose merged-into target",
                                    GitGraphBranchPickerMode::Compare => "Compare local or remote branch with HEAD",
                                },
                                if available >= MAX_BRANCHES {
                                    " (inventory cap reached)"
                                } else {
                                    ""
                                },
                            )),
                        )
                        .child(
                            div().id("git-center-branch-picker-results")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .when(matched == 0, |list| {
                                    list.child(div().py_2().text_xs().text_color(rgb(MUTED))
                                        .child("No matching loaded branches; try another name"))
                                })
                                .children(matches.rows.into_iter().enumerate().map(|(i, branch)| {
                                    let name = branch.name;
                                    let remote = branch.remote;
                                    let label = format!(
                                        "{}{} {} · {}",
                                        if branch.favorite { "★ " } else { "" },
                                        if remote { "Remote:" } else { "Local:" },
                                        name,
                                        branch.tip
                                    );
                                    div()
                                        .id(("git-center-branch-picker-row", i))
                                        .min_w_0()
                                        .flex_shrink_0()
                                        .px_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.pick_git_center_searched_branch(
                                                name.clone(), remote, mode, graph_generation, cx,
                                            );
                                        }))
                                })),
                        ),
                )
            })
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
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // In a narrow split with an open branch comparison, the
                    // history graph should remain scrollable but must not
                    // consume half the pane and leave only 2–3 Patch lines.
                    // Full-width and non-comparing Graph layouts are unchanged.
                    .when(compact_graph && self.git_graph_compare_center, |pane| {
                        pane.h(px(120.0)).flex_shrink_0()
                            .border_b_1().border_color(rgb(BORDER))
                    })
                    .when(!compact_graph || !self.git_graph_compare_center, |pane| pane.flex_1())
                    .when_some(graph_view, |pane, viewport| pane.child(viewport))
                    .when(count == 0 && !loading, |pane| {
                        pane.child(div().px_3().py_2().text_xs().text_color(rgb(MUTED))
                            .child("No matching commits in this Git Graph view"))
                    }),
            )
            .when_some(compare_panel, |pane, comparison| pane.child(comparison))
            .into_any_element()
    }
}

#[cfg(test)]
mod compact_graph_controls_tests {
    use super::{center_graph_title, compact_center_graph_controls};

    #[test]
    fn compact_breakpoint_uses_logical_width_and_split_state() {
        assert!(compact_center_graph_controls(false, 0.0));
        assert!(compact_center_graph_controls(false, 719.9));
        assert!(!compact_center_graph_controls(false, 720.0));
        assert!(!compact_center_graph_controls(false, 1200.0));
        assert!(compact_center_graph_controls(true, 720.0));
        assert!(compact_center_graph_controls(true, 1920.0));
    }

    #[test]
    fn narrow_graph_title_keeps_commit_count_visible_without_long_label() {
        assert_eq!(center_graph_title(true, 512), "Graph · 512");
        assert_eq!(center_graph_title(true, 0), "Graph · 0");
        assert_eq!(center_graph_title(false, 512), "Git Graph");
    }
}
