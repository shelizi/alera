# Alera GPUI G05 — 指定分支 Pull／Push／Merge（不切換分支）

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD：`9a75c6a8b`。
狀態：**G05 核心正常流程已驗證、GUI 真實操作待驗；`supported=false` 不變。**

## 實際接線盤點

- 既有 `GitActions::pull_branch_without_switch` 對選定但未被任何 Worktree checkout 的本地分支執行 Fetch→fast-forward，使用 CAS `git update-ref`，不切換目前 Workspace 的 HEAD，也不覆蓋工作樹；檢查指定分支的真正 upstream 與 Worktree owner。
- 既有 `push_branch_without_switch` 只把選定分支推往配置的 upstream，不接受任意遠端推送方向、`force` 或任意 refspec；目前已 checkout 的分支則另有 `pull_ff_only`／`push_current_upstream`。
- Native Source Control 已有指定分支 Pull／Push 雙擊確認與本地 Merge 雙擊確認；Git Graph 分支右鍵已有 Pull、Push、Merge，後者會填入 Source Control 的來源並要求按 Confirm Merge。`merge_without_conflicts` 將選定分支合入目前分支，維持目前分支名稱，衝突會拒絕並保留原始內容。

## 本輪發現與修正

- `run_git_action` 原本對任何 Dirty Native Editor 拒絕執行全部非 Stage／Unstage 的 Git 操作。這使 **Fetch** 與對另一個未 checkout 分支的 **Pull／Push** 被不必要阻擋；這些操作只變動遠端或 Git Ref，不讀取未保存的 Editor Buffer，也不需要改動目前工作樹。
- 新增 `GitUiAction::is_worktree_independent`：Stage／Unstage 仍屬 index-only；Fetch、`PullSelected`、`PushSelected`、`PushUpstream` 屬工作樹獨立操作。若有 Dirty Editor 仍可執行上述操作；成功訊息提示其未儲存文字及工作檔案沒有被改動。
- **嚴格保留** `MergeWithoutConflicts`、`MergeWithChoices`、`SwitchBranch`、`PullFfOnly`、`StashSave` 等會影響目前已 checkout 版本的保護；沒把一般 Merge 當成安全唯讀操作。

## 真 Git Remote 整合驗證（全程隔離）

建立 `tempfile` 本地 Workspace、本地 Bare Remote 與另一份 Clone，對 `topic` 進行：

1. 建立未 checkout 的 topic 並配置 exact upstream；外部 Clone 提交並 Push 內容。
2. 本地目前分支另寫入未提交 `a.txt` 與 untracked `keep.txt`，對 topic 執行 `pull_branch_without_switch`，檢查 topic Ref 更新，**目前 HEAD、分支名稱、磁碟檔案、Index 不受更動**。
3. 在本地 Git Object Database 直接建立 topic 新 Commit、更新未 checkout topic Ref，再執行 `push_branch_without_switch`，確認 Bare Remote 的 topic OID 已正確更新，工作樹未改。
4. 在 Dirty 工作樹下試圖 Merge topic，確認被拒絕且目前 HEAD 不變；清理**隔離測試 Repo** 的故意 WIP 後執行 Merge topic，確認目前分支名稱不切換、HEAD 移至 topic 正確 OID，合併後檔案內容完整。

## TDD／Windows 驗證

| 驗證 | 結果 |
| --- | --- |
| RED Native | 新增 `g05_branch_remote_ui_tests`，先確定缺少 `is_worktree_independent` 時 E0599 編譯失敗 |
| GREEN Core | `g05_selected_pull_push_and_merge_preserve_active_branch_and_dirty_worktree` **1/1 PASS**：本地 Bare Remote、外部 Clone、真 Pull／Push、合併前 Dirty 拒絕與恢復後 Merge 全流程 |
| GREEN Native | `g05_branch_remote_ui_tests::selected_branch_remote_actions_do_not_require_saved_editor_buffers` **1/1 PASS**：Fetch／選定分支 Pull／Push／目前分支 Push 允許，Merge／Checkout／當前 Pull／Stash 仍保護 |
| 全量 Library | **567 PASS／0 FAIL／1 ignored**（隔離 Host 測試未執行） |
| Native Binary | **42/42 PASS** |
| Windows 原生 EXE／格式 | `cargo build --bin alera-gpui-desktop`、`cargo fmt --check`、`git diff --check` **PASS** |

## 待 Windows GPUI GUI 正常操作驗收

- 以隔離的測試 GUI Workspace 透過 Git Graph 右鍵選 topic Pull／Push，確認首次準備、第二次確認、狀態訊息與分支選單的操作一致，不使目前 UI 工作區跳轉。
- Native Editor 有 Dirty Buffer 時，Fetch／另一分支 Pull／Push 應能執行且保持 Buffer；Merge／當前 Pull／Switch 應顯示保護提示。
- 在隔離雙分支工作樹測試 Graph→Merge 預備與 Source Control→Confirm Merge，核對 HEAD 分支名稱不變、實際檔案反映合併結果；真正 GUI 實機尚未進行，**不宣稱 GUI DONE**。

## 安全邊界與下一項

- 所有新增 Git Remote／提交／檔案修改都只在 `tempfile` 隔離 Repo；原 GPUI 專案的 `main`、正式 Worktree／Index／使用者 WIP 未被操作。
- 保留原有 `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/`；沒有 Push、Merge 至正式 main、部署或 Release。
- **G05 INTEGRATED／待 GUI 驗收**；功能完整優先隊列的下一個 NOW 為 **G06：Worktree 建立／開啟／清理與簡單合併衝突操作**。
