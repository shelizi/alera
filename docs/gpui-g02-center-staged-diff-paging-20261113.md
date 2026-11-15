# Alera GPUI G02 — 中央 Git Diff 的 Staged／Changes 正確分頁與 Hunk

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD `23cc37687`。
狀態：**核心功能修正與隔離 Git Repo 專項測試通過，Windows GUI 真實點擊／Hunk 操作待驗；`supported=false`。**

## 找到的正常使用缺口

- 原先中央 Git Diff 僅接受 `GitFileDiffPaging` 來源。一般 Changes／Untracked 會進分頁來源，但 `GitChangeArea::Staged` 走非分頁 `read_file_diff_for_area`，因此 staged 列即使能看到文字預覽，仍無法展開中央 Diff、逐個 Hunk 導覽或翻頁。
- 同一檔案可能同時存在 Staged 與 Changes 兩筆 Source Control 列。如果單純將 Staged 也送往舊 `read_file_diff_page`，舊 API 會先挑 Index→Worktree，可能把 **Working Tree Diff 錯顯成 Staged Diff**。

## 本輪修正

1. `git_inspect.rs` 增加 `read_file_diff_page_for_area`：以明確指定的 Staged／Changes／Untracked 讀取精確 Git 區域，先重驗目標文件狀態；Staged 對 HEAD→Index，Changes 對 Index→Worktree，Untracked 只讀安全且大小受限的工作樹文字。每頁仍使用 32 KiB 預覽上限及既有 Hunk parser，不改寫 Index／HEAD／工作檔。
2. `main.rs` 的 `GitFileDiffPaging` 保存 `area: Option<GitChangeArea>`，`preview_git_change_paged`、翻頁與背景讀取全程帶上該來源。通用歷史／其他一般預覽仍可使用原自動選擇 API；Source Control 點選 Staged、Changes 則使用精確區域 API。
3. `native_center_git_diff_ui.rs` 將 Staged 列接進分頁／中央 Diff。上方顯示選中區域（STAGED CHANGES、CHANGES、UNTRACKED 或 AUTO）及檔案路徑；原 Hunk 導覽邏輯沿用，可讀取新增的 Staged 虛擬列。
4. `native_action_dialog_ui.rs` 的 `GitDialogFileSource` 也加入 `area`，配合 Workspace generation、source ticket、relative path 三重來源驗證。即使相同路徑從 Staged 切成 Changes，舊中央／展開 Diff 視窗不能接收錯誤區域的資料。

## TDD／回歸

| 測試 | 結果 |
| --- | --- |
| RED | 新增真臨時 Git Repository 的「同一檔案 staged + unstaged」專項測試；修正前因缺少 `read_file_diff_page_for_area`，Rust 編譯 E0425。 |
| GREEN，Core | `g02_exact_staged_and_worktree_pages_never_mix_for_one_file` **1/1 PASS**。HEAD／Index／磁碟未改；Staged 包含 `+index line` 而不包含 worktree，Changes 包含 `+worktree line`，不誤選 staged。 |
| GREEN，Native | `g02_staged_patch_can_expand_into_exact_center_paged_source` 與 `g02_diff_source_refuses_same_file_with_different_staged_area` **2/2 PASS**。 |
| 全量測試 | Windows GPUI Library **563 PASS／0 FAIL／1 ignored**（隔離 Host 測試未跑）；Native Binary **40/40 PASS**，Windows EXE Build、fmt／Git diff check 均 PASS。 |

## 待 Windows 正常 GUI 驗收

- 使用獨立測試 Repo 同一檔案製作兩個不同 Hunk 的 Staged 修改、再另改 Working Tree。選 Source Control 的 Staged 列→中央 Diff→Next/Previous Hunk，應看到 HEAD→Index；改選 Changes 列→中央 Diff，應看到 Index→Worktree，不保留前一來源的舊 Hunk。
- 檢查 Staged 長 Diff 翻頁及展開詳情視窗，確定頁數和來源一致；點「Git Original」應取得對應 HEAD／Index 的唯讀內容，不變成可寫 Native Editor。
- 這些是真實 GUI 操作待驗，單元測試 PASS 不能代替；按照功能完整優先隊列，核心接線通過後下一項是 G03 Commit／Amend／History。

## 範圍與安全

- TDD 使用 `tempfile` Git Repo，沒有修改正式專案的 Index／HEAD 或使用者文件。保留已存在的 protected smoke fixture 與 target。
- `main` 未合併，沒有 Push、部署、Release；`supported=false` 維持。
- 預定追蹤狀態：**G02 INTEGRATED／待 GUI 驗收；下一項 NOW G03**。
