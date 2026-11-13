# Alera GPUI G03 — Commit、Amend 與 History 正常操作閉環

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD：`85bd6a540`。
狀態：**核心功能已補齊並通過 Windows Rust／Native／EXE 回歸；真正 GPUI GUI 點擊操作待驗；`supported=false`。**

## 既有功能盤點

- `GitActions::commit` 只提交明確 Stage 的 Git Index tree，已有合法訊息與 conflict 檢查；Native Source Control 的提交按鈕、手動訊息輸入、離線草稿及成功後刷新 Git Snapshot／已開啟的 History 已接線。
- `preview_amend_commit` 在背景擷取 HEAD／Index tree／原訊息，保留兩次使用者確認及 `apply_amend_preview` 的最新快照一致性驗證。已設定 upstream 的公開 HEAD 不允許直接 Amend；不自動 Push／Force Push。
- `GitHistorySnapshot` 與 `preview_commit` 皆唯讀，含首個 Commit 與一般父節點 Diff；正常提交後 History 如已開啟會重新載入。

## 本輪發現與修正

1. **首次 Commit 缺口**：剛執行 `git init`、尚未有 HEAD Commit 時，`GitActions::commit` 直接 peel HEAD 失敗 `UnbornBranch`，使新專案無法透過 GPUI 建立首筆版本紀錄。修正為允許**可驗證的本地 symbolic branch HEAD** 在 Index 有檔案時以零 parent 寫入初始 Commit。保留 Detached HEAD、空白訊息、空 Index、未 Stage、Index Conflict 等拒絕條件，不影響既有後續 Commit。
2. **Amend 預覽內容不夠清楚**：原本只警告替換 HEAD／訊息，卻不告知目前 Stage 的檔案內容是否會被一併放入新 Commit。`GitAmendPreview` 新增 `staged_changes`，由候選 Index Tree 與目前 HEAD Tree 比較，Native 第一次確認明確顯示「包含目前 Staged 內容」或「僅修改 Commit 訊息」；真正 Apply 仍重驗完整 Preview。
3. **提交成功輸入框未整理**：Commit／Amend 成功時，僅在輸入框仍是此次送出的訊息時清空；如果背景 Git 操作期間使用者已輸入下一筆訊息則保留，不會把新訊息一起抹掉。失敗的提交不清空。

## Windows TDD 與回歸結果

| 項目 | 結果與證據 |
| --- | --- |
| TDD RED | 先加入全新臨時 Git Repo 的第一筆 Commit→History→Preview 測試，原本因 `refs/heads/master` 不存在而失敗，符合現有功能缺口 |
| TDD GREEN 首次提交 | `g03_initial_commit_then_history_and_preview_in_new_repo` **PASS**：本地 Repo Identity、空 Index 拒絕、首次 Stage→Commit、零 parent、History 1 筆、首個 Commit Diff、未追蹤檔案與磁碟內容保持 |
| 一般 Commit→Amend | `g03_commit_amend_history_readback_preserves_unsaved_disk_changes` **PASS**：只提交 Staged 版本、不混入工作樹後續編輯，Amend 改訊息後 History 指向新 OID，舊檔、未追蹤檔與工作樹內容不變 |
| Amend 預覽邊界 | 既有 `amend_requires_exact_head_index_and_replaces_only_local_commit`、`amend_refuses_stale_index_and_no_op` 已擴充 `staged_changes` 斷言；訊息-only 與含 Staged 修改正確區分 |
| Native 輸入 UX | 新測試 `g03_commit_input_tests::commit_success_clears_only_the_unmodified_submitted_message` **PASS**：確認相同訊息清空、已輸入新訊息不清空、非提交動作不清空 |
| GPUI Library | **565 passed／0 failed／1 ignored**，忽略項為隔離 Host 測試，未冒充通過 |
| GPUI Native Binary | **41/41 PASS** |
| Windows EXE／格式 | `cargo build --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop`、`cargo fmt --check`、`git diff --check` 均 **PASS** |

## 尚待 Windows GUI 正常操作驗收

- 獨立、可拋棄的新 Git Repo：在 Source Control 選取 Stage→輸入訊息→第一筆 Commit；確認 Changes 變成乾淨、History 多一筆、Commit Preview 指向真正新增內容。
- 在同一隔離 Repo 建立第二筆 Staged 修改，確認 Commit 只含 Index；已開啟 History 自動刷新，點 Commit 預覽可見正確父子關係。
- 先做 Message-only Amend、再做帶 Staged 修改的 Amend Preview，確認文字會區分範圍，第二次明確確認才修改 HEAD；失敗或輸入框在背景執行期間修改時不抹去草稿。
- 這些 GPUI 真正鍵盤／滑鼠操作未在本輪取得證據；因此 **G03 核心 INTEGRATED／GUI 待驗**，不是完全 GUI DONE。

## 安全／後續

- 所有新增整合測試使用 `tempfile` 本地 Git Repo，沒有觸碰正式工作樹或使用者資料。原受保護 `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/` 保留。
- 只提交 GPUI 整合分支；`main` 不修改、不 Push、不部署、不 Release，`supported=false` 維持。
- 依功能完整優先既定隊列，完成 G03 核心接線後追蹤表 **NOW G04（Git Graph Ref／Commit／分支比較）**。
