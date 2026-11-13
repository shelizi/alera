# Alera GPUI G06 — 獨立 Worktree 建立／安全清理／衝突選邊

日期：2026-11-13（Asia/Taipei）
分支：`integrate/gpui-central-diff-into-main-20261112`；G06 起始 HEAD `93fd2b1b9`。
狀態：**核心功能補強與真 Git Worktree 回歸已通過；Windows GUI 實機互動待驗，`supported=false`。**

## 既有能力盤點

- Project 區的 Worktree 操作已支援新分支或既有本地分支建立、依實際註冊工作樹選擇／切換，以及二次確認後移除乾淨 managed Worktree；Git Graph 的 Commit 右鍵也有指定 SHA 建立分支＋獨立 Worktree。
- `WorktreeManager` 針對主要／目前／locked／prunable／未註冊／非 `.worktrees` 受管目錄、dirty／untracked 的刪除目標已有保護，不用 `--force`，移除成功也保留原本的 Git 分支。
- Native Git Conflict Preview 能對已分歧分支產生唯讀 preview，選 Ours／Theirs 是**整個衝突檔案**而不是 Hunk，全部選完才允許二次確認、建立雙 Parent Merge Commit；不支援 binary、Symlink、大型或無法表示成文字的衝突檔案。

## 本輪日常功能缺口與修正

1. **Git Graph 從指定 Commit 建立獨立 Worktree**：這個動作只在另一個 managed 目錄 checkout 新分支，原 `run_git_action` 卻因目前 Native Editor 任一未存資料而拒絕。將 `GitUiAction::CreateWorktreeAtCommit` 加到「不影響目前 Workspace／Editor」分類，允許目前 Editor 有草稿時建立；建立成功只刷新 Worktree 清單，仍要使用者自行切換。
2. **清理另一個乾淨 Worktree**：`remove_worktree` 原本在二次確認後，仍用目前 Primary Editor 的 `switch_allowed()` 阻擋刪除；這既會阻止安全清理與目前 Editor 無關的 checkout，也漏考慮其他 Pane 一致性。現在改成以**欲移除目標是否為目前 Workspace**作 UI 判斷，底層 `WorktreeManager::preflight_remove` 再真正檢查目標 Git 狀態與受管身分；不需清空目前不相關的 Editor 草稿。
3. **保留需要切換 Workspace 的建立模式限制**：Project panel 的 Create Worktree 會自動 `switch_workspace` 到新目標，仍要求目前 Editor 儲存；Merge、Branch Switch、Stash／當前 Pull 的 Dirty Editor 防護不放寬。

## TDD 與隔離真 Worktree 證據

| 驗證 | 結果 |
| --- | --- |
| RED | `g06_create_commit_worktree_may_proceed_with_unsaved_editor_but_merge_cannot` 在尚未加上 UI 分類時確實 FAIL |
| GREEN Core | `g06_create_remove_separate_worktree_keeps_dirty_primary_files_and_index` **1/1 PASS**，真臨時 Git Repo 主工作樹有 tracked Dirty 與 Untracked WIP，仍從 Commit 建立新 Worktree。讀回新 checkout 內容、主 HEAD／分支／Index／WIP 均不動；未確認刪除拒絕，清潔目標二次確認後可刪除、分支保留。 |
| GREEN Native | `g06_create_commit_worktree_may_proceed_with_unsaved_editor_but_merge_cannot` **1/1 PASS**，獨立建立允許，Merge／Switch 仍拒絕 Dirty Editor。 |
| 衝突核心正向回歸 | 既有 `conflict_choice_commits_two_parents_without_losing_unrelated_files` **1/1 PASS**；整個檔案選邊後可建立兩 Parent Commit，非相關檔案保留。 |
| Windows 全量 Library | **568 PASS／0 FAIL／1 ignored**；隔離 Host 測試未執行、不計為通過。 |
| Native／EXE／格式 | Native **43/43 PASS**，Windows EXE Build、`cargo fmt --check`、`git diff --check` **PASS**。 |

## 待 GUI 正常操作驗收

- 在可丟棄的真 Git Project，保持目前 Editor Dirty，從 Git Graph 指定 Commit 建立 Worktree，核對目前 Editor／Tab 未被動切換，新 Worktree 出現在 Project 清單，可主動開啟。
- 對無 Dirty 的目標 Worktree，從其他仍有草稿的 Workspace 點 Remove→Confirm，驗證目標移除、分支保留；目標自身 Dirty／Untracked、Primary、Active Worktree 均必須拒絕。
- 對兩邊分歧且同一文字檔案有衝突的測試 Repo，從 Native Source Control 執行 Conflict Preview→選取整個檔案 Ours／Theirs→雙次確認 Merge，讀回雙 Parent Commit 並核對未衝突檔案保留。
- 以上 GUI 實際滑鼠／鍵盤流程目前尚未執行，不將 G06 標記為 GUI DONE；LLM 協助合併另列後續功能，不攔阻正常 Git 工作包推進。

## 安全與後續

- 新增 Git 改動及刪除只在 `tempfile` Repository。原整合分支以外的真 `main`／Git Index／用戶 WIP 沒有被重設。
- `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/` 保留；不 Push、合 main、部署或 Release，`supported=false`。
- G06 核心為 **INTEGRATED／待 GUI 驗收**；按既定功能完整優先隊列，下一個 NOW 移至 **S01 設定頁與 Classic/Rust Core 設定接線**。
