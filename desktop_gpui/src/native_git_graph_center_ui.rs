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

    pub(super) fn render_center_git_graph(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let query = GitGraphSearch::parse(self.git_graph_search_input.read(cx).query());
        let full_search = self.git_history_options.search_query.is_some();
        let commits = if self.git_history_loading && full_search {
            Vec::new()
        } else {
            self.git_history
                .as_ref()
                .map(|history| history.commits.clone())
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
                let is_merge = row.parents > 1;
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
                let hidden_labels = labels.len().saturating_sub(3);
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
                    .children(labels.into_iter().take(3).map(|label| {
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
                    .child(
                        div()
                            .w(px(116.0))
                            .truncate()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(row.author),
                    )
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
        let scope = self
            .git_history_options
            .selected_ref
            .clone()
            .unwrap_or_else(|| {
                if all_branches {
                    "All branches".into()
                } else {
                    "HEAD".into()
                }
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
                    .child(div().text_xs().text_color(rgb(MUTED)).child(description))
                    .child(div().flex_1())
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
                            .id("git-center-scope-head")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_xs()
                            .bg(rgb(if !all_branches { ACTIVE } else { PANEL }))
                            .text_color(rgb(TEXT))
                            .child("Current branch")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.git_history_options.all_branches = false;
                                this.git_history_options.favorites_only = false;
                                this.git_history_options.selected_ref = None;
                                this.save_mixed_pane_layout(cx);
                                this.refresh_git_history(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("git-center-scope-all")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_xs()
                            .bg(rgb(if all_branches { ACTIVE } else { PANEL }))
                            .text_color(rgb(TEXT))
                            .child("All branches")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.git_history_options.all_branches = true;
                                this.git_history_options.selected_ref = None;
                                this.save_mixed_pane_layout(cx);
                                this.refresh_git_history(cx);
                            })),
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
