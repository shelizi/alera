# GPUI SWE Flash E01 — preview/open draft handoff（2026-11-13）

- Worktree: .worktrees/gpui-sweflash-e01-open-20261113
- Branch: feat/gpui-sweflash-e01-open-20261113
- Base: b805a033eca3a6254a4b836226e9047ac06d8957
- State: **DRAFT / NOT INTEGRATED / E01 NOT DONE**
- Source: Devin SWE-2 Flash High subagent, isolated worktree, stopped deliberately after safety boundary review.

## 本輪實際變更

- desktop_gpui/src/editor_tabs.rs: 新增 e01_preview_promote_and_quick_open_reopen_keep_one_dirty_buffer，覆蓋 Explorer 預覽 → 正式 Editor → Ctrl+P 重開同檔與 Dirty/Undo 單 buffer。
- 同檔新增 e01_same_basename_files_in_different_worktrees_stay_distinct，覆蓋跨 worktree 同名檔 root+path 身分不混淆。
- 此輪**沒有修改產品邏輯**，並非已修好一個新缺口。Agent 盤點發現現有預覽與 editor model 已有部分閉環，加入回歸測試以保護日常功能。

## 證據與限制

- 整合者執行 git diff --check：PASS。
- 整合者執行 desktop_gpui cargo fmt --all -- --check：PASS。
- Agent 輸出曾聲稱局部 tests pass，但未留可獨立核對的完整測試命令/正式日誌；**不得以此宣稱 E01 測試 PASS 或 GUI DONE**。
- 未進行獨立 Windows GUI 開檔/磁碟讀回；暫時只屬初步測試草稿。
- 兩個新 worktree 曾被子代理建置準備加入 rust/target → D:\\c\\t Junction；整合者停止子代理，僅 unlink 新 worktree 的 junction，**未清理外部共用 D: target**。未再允許子代理執行外部建置準備，後續改由整合者在明確隔離環境安排測試。

## 後續 Gate

1. 在核准的獨立 target/cache 下，實際跑新增兩個定向 Rust tests 及關聯回歸；發現 RED 才修正常用功能。
2. 使用隔離 Windows GUI 驗證單擊 preview → 雙擊開啟 → Ctrl+P 重開，確認只有一個可編輯 Tab 且 Dirty/Undo 不遺失；另一個 worktree 同名檔不混淆。
3. E01 只能在 T07 日常完整驗收結束後整合至 GPUI integration 分支，不能自行合 main/Push/Release；supported=false。
