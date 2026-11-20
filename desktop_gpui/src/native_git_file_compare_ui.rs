//! Classic-equivalent, read-only file revision and branch comparison dialogs.
//! GPUI Kit owns dialog behavior; Git2 comparison runs only on a worker.
use super::*;
use alera_gpui_desktop::git_file_revision_compare::{
    compare_file_with_git_reference, list_local_git_branches,
};
use alera_gpui_desktop::native_i18n::tr;

impl Workbench {
    pub(super) fn open_git_file_revision_comparison(
        &mut self,
        relative: PathBuf,
        branch: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_file_compare_ticket = self.git_file_compare_ticket.wrapping_add(1);
        let ticket = self.git_file_compare_ticket;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        let label = branch.clone().unwrap_or_else(|| "HEAD".to_owned());
        self.git_file_compare_result = None;
        self.model.set_status(format!(
            "Comparing {} with {label} (read-only)…",
            relative.display()
        ));
        let owner = cx.entity().downgrade();
        let dialog_path = relative.display().to_string();
        window.open_dialog(cx, move |dialog, _, cx| {
            let (heading, content, loading) = owner
                .upgrade()
                .map(|entity| {
                    entity.update(cx, |view, _| {
                        if view.workspace_generation != generation
                            || view.git_file_compare_ticket != ticket
                        {
                            return (
                                "Source changed",
                                "Workspace or comparison changed. Close and select again."
                                    .to_owned(),
                                false,
                            );
                        }
                        match &view.git_file_compare_result {
                            Some((current, Ok(diff))) if *current == ticket => (
                                "Read-only Git file comparison",
                                format!(
                                    "File: {}\nReference: {}\nCommit: {}\n\n{}",
                                    diff.relative.display(),
                                    diff.reference,
                                    diff.revision_oid,
                                    diff.patch
                                ),
                                false,
                            ),
                            Some((current, Err(error))) if *current == ticket => (
                                "Git comparison unavailable",
                                format!(
                                    "Unable to compare {} with {}: {}",
                                    dialog_path, label, error
                                ),
                                false,
                            ),
                            _ => (
                                "Loading Git file comparison…",
                                "Reading committed revision and working file…".to_owned(),
                                true,
                            ),
                        }
                    })
                })
                .unwrap_or((
                    "Git comparison unavailable",
                    "Workspace closed".to_owned(),
                    false,
                ));
            dialog
                .title(heading)
                .w(px(920.))
                .centered()
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| true)
                .child(
                    div()
                        .id("git-file-revision-compare-dialog")
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_xs().text_color(rgb(MUTED)).child(if loading {
                            "Loading · read-only"
                        } else {
                            "File revision vs current worktree · read-only"
                        }))
                        .child(
                            div()
                                .id("git-file-compare-patch-scroll")
                                .h(px(420.))
                                .overflow_y_scroll()
                                .bg(rgb(BG))
                                .p_3()
                                .font_family("JetBrains Mono")
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(content),
                        ),
                )
        });
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    compare_file_with_git_reference(&root, &relative, branch.as_deref())
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_file_compare_ticket != ticket
                {
                    return;
                }
                view.model.set_status(match &result {
                    Ok(diff) => format!(
                        "Compared {} with {} without editing files",
                        diff.relative.display(),
                        diff.reference
                    ),
                    Err(error) => format!("File revision comparison refused: {error}"),
                });
                view.git_file_compare_result = Some((ticket, result));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Local branch selection is an inert, searchable list of names returned
    /// by Git2. A typed query never becomes a revspec or shell argument.
    pub(super) fn open_git_file_branch_picker(
        &mut self,
        relative: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branches = match list_local_git_branches(self.model.root()) {
            Ok(branches) => branches,
            Err(error) => {
                self.model
                    .set_status(format!("Branch search refused: {error}"));
                cx.notify();
                return;
            }
        };
        if branches.is_empty() {
            self.model
                .set_status("No local branches available for file comparison");
            cx.notify();
            return;
        }
        let input = cx.new(|cx| QuickOpenInput::with_placeholder("Search local branches…", cx));
        cx.observe(&input, |_: &mut Workbench, _, cx| cx.notify())
            .detach();
        let owner = cx.entity().downgrade();
        let generation = self.workspace_generation;
        let first_focus = std::cell::Cell::new(true);
        let locale = self.settings_menu_locale;
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                window.focus(&input.read(cx).focus_handle(cx), cx);
            }
            let query = input.read(cx).query().trim().to_lowercase();
            let matching = branches
                .iter()
                .filter(|branch| branch.to_lowercase().contains(&query))
                .count();
            let rows = branches
                .iter()
                .filter(|branch| branch.to_lowercase().contains(&query))
                .take(60)
                .enumerate()
                .map(|(i, branch)| {
                    let selected_branch = branch.clone();
                    let selected_path = relative.clone();
                    let selected_owner = owner.clone();
                    div()
                        .id(("git-file-compare-branch", i))
                        .flex()
                        .items_center()
                        .px_3()
                        .h(px(30.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(ACTIVE)))
                        .text_sm()
                        .child(selected_branch.clone())
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = selected_owner.update(cx, |view, cx| {
                                if view.workspace_generation == generation {
                                    view.open_git_file_revision_comparison(
                                        selected_path.clone(),
                                        Some(selected_branch.clone()),
                                        window,
                                        cx,
                                    );
                                }
                            });
                        })
                })
                .collect::<Vec<_>>();
            dialog
                .title(tr(locale, "Compare With Branch"))
                .w(px(540.))
                .centered()
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| false)
                .child(
                    div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().child(input.clone()))
                        .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                            "{matching} matching local branch(es); showing up to 60"
                        )))
                        .child(
                            div()
                                .id("git-file-branch-results")
                                .h(px(300.0))
                                .overflow_y_scroll()
                                .children(rows),
                        ),
                )
        });
    }
}
