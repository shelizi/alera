# GPUI E05 — 跨 Git Worktree 內容搜尋與正確結果開檔

日期：2026-11-13（Asia/Taipei）
分支：`integrate/gpui-central-diff-into-main-20261112`
狀態：**主要正常功能接線、Windows Rust／Native 回歸已驗；實際 Windows GUI 點擊待驗；`supported=false`。**

## 原本缺口

GPUI 原有 `ProjectSearch::new(self.model.root()).search(query)` 只能在當前 Workspace 搜尋。結果僅包含 Workspace-relative path、行列與片段，沒有 checkout 身分。按下結果時又用當前 Workspace 根目錄組合路徑，因此即使另外掃描多個 Worktree，`src/same.txt` 可能誤開為目前工作樹的同名檔。

## 實際改動

1. 增加 `desktop_gpui/src/worktree_content_search.rs`，定義 `WorktreeContentScope::{Current,AllWorktrees}`、`WorktreeContentMatch`（明確含 `root`／`branch`／`ContentMatch`）和 `WorktreeContentResults`。
2. 維持預設「**Current worktree**」；內容搜尋面板新增「**All Git worktrees**」，使用 Git 現場登錄的 Worktree，而非掃描任意資料夾。Worktree 列表與讀檔全部在 GPUI background worker，最多 24 個 Worktree、每 Worktree 100 筆、合計最多 1000 筆；上限或無法讀取時顯示 partial/skipped，不假稱掃描完整。
3. 結果畫面顯示 `branch · root` 及 Workspace-relative filename、行、Unicode 欄與片段，保留每一筆原始所屬工作樹。
4. 點擊結果先檢查搜尋結果仍屬目前結果列表，再以 `checked_path()` 驗證來源檔案仍在該 Worktree 下。不是目前 Worktree 時先經既有 `switch_workspace`（Dirty Editor／Git／Host 忙碌保護都保留），確認真正切換成功後才 `model.open_path`，最後 `NativeEditor.navigate_to_content_hit`；失敗絕不默默改開目前工作樹的同名檔。
5. Git linked Worktree 可能在主目錄 `.worktrees/` 之下，因此**實體路徑包含於主目錄不代表它屬於主 Worktree**。明確使用 `worktree_hit_requires_switch` 比較註冊的 checkout root，而不是用 `starts_with` 推定歸屬。

## 實測證據

- `e05_scopes_results_to_exact_worktree_with_duplicate_relative_paths`：建立真 Git repo 與真 linked Worktree，兩邊各有 `src/same.txt`，但內容不同；Current 僅見目前 Worktree，All 能找到兩者；從 linked 端搜尋亦可讀到兩者。
- `e05_same_relative_file_opens_owner_after_explicit_worktree_selection`：實際 `WorkbenchModel` 在主 Worktree 開同名檔會讀到 primary 內容，選 linked root 後開搜尋結果則讀到 linked 的另一份內容；驗證必須以 Worktree 身分而非相對路徑選擇。
- `e05_deleted_or_escaped_hit_is_never_opened_from_another_workspace`：目標過期或相對路徑跳出根目錄時拒絕。
- E05 真實 Git Worktree TDD **3/3 PASS**；`project_search::tests::` **3/3 PASS**；GPUI Native Binary **37/37 PASS**；`cargo check --bin alera-gpui-desktop` PASS。Windows EXE 最終 `cargo build --offline --locked --bin alera-gpui-desktop` **PASS**。
- 只在 tempfile repo 寫入測試內容，沒有碰正式使用者檔案、Provider、Host、PTY 或受保護 smoke fixtures。

## 後續

- Windows 實際 GUI 點「All Git worktrees」→搜尋→選另一 Worktree 同名檔→導覽行列，包含目前 Workspace 有未儲存內容時的安全阻擋，仍未實測。不能聲稱 GUI PASS。
- 目前跨 Worktree 切換後使用既有行為重設內容搜尋面板；連續跨 Worktree 檢視同一批結果的便利性仍可於後續 UI/UX 階段改善。
- E05 一般功能先整合，開發隊列移到 **E06：雙 Editor Pane／Dirty 草稿持久化與重開復原**；不再以個別 GUI 驗收卡住功能實作。仍 `supported=false`，不合 `main`、不 Push／部署／發版。
