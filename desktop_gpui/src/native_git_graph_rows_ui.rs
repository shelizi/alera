//! Viewport-only central Git Graph row rendering. Scroll through GPUI's
//! uniform_list: non-visible commits must not build Divs or menu handlers.

use super::*;

pub(super) struct GitGraphViewCache {
    pub workspace_generation: u64,
    pub history_request: u64,
    pub visibility: GitMergedBranchVisibility,
    pub sparse_search: bool,
    pub prepared: Arc<PreparedGitGraph>,
}

impl Workbench {
    pub(super) fn render_git_graph_viewport_row(
        &mut self,
        index: usize,
        row: &PreparedGitGraphRow,
        lane_count: usize,
        compact_graph: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let oid = row.commit.oid.clone();
        let menu_subject = row.commit.subject.clone();
        let menu_oid = oid.clone();
        let is_merge = row.commit.parent_ids.len() > 1;
        let badge_limit = if compact_graph { 1 } else { 3 };
        let hidden_labels = row.labels.len().saturating_sub(badge_limit);
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
            .child(native_git_graph_ui::graph_lane_with_density(
                row.topology.clone(),
                lane_count,
                is_merge,
                row.is_head,
                compact_graph,
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(row.commit.subject.clone()),
            )
            .children(row.labels.iter().take(badge_limit).map(|label| {
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
                    .unwrap_or(label);
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
                        .child(row.commit.author.clone()),
                )
            })
            .child(
                div()
                    .w(px(66.0))
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(row.commit.short.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.preview_git_history(GitReadRequest::Commit(oid.clone()), cx);
            }))
            .context_menu(Self::kit_git_graph_commit_menu(
                cx.entity().downgrade(),
                menu_oid,
                menu_subject,
                row.commit.parents,
            ))
            .into_any_element()
    }
}
