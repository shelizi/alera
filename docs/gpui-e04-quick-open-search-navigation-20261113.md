# GPUI E04 — Quick Open 鍵盤結果選取與搜尋正常流程

日期：2026-11-13（Asia/Taipei）
分支：`integrate/gpui-central-diff-into-main-20261112`
狀態：**程式接線、Rust／Native 回歸及 Windows EXE 編譯 PASS；真實 GUI 操作與長檔名排版驗收仍待，`supported=false`。**

## 既有與本輪修正

- 原本 GPUI 有 `Ctrl+P` Quick Open、模糊比對、忽略目錄排除、Explorer 篩選、全文搜尋及 Workspace 相對路徑顯示。但 Quick Open `Enter` 固定開第一個結果，並沒有可用的 Up／Down 結果選取，對同名檔尤其不方便。
- 新增 `quick_open_move_selection`：有結果且未進行 IME 組字時，`Up`／`Down` 移動反白項目，索引界限封頂／保底；無匹配或注音候選階段不攔截鍵。
- `Workbench` 持有暫態選取索引及結果區 ScrollHandle；每次開啟 Quick Open、搜尋文字實際改變時重設索引到第一筆。變更索引會讓選定列捲入視窗，`Enter` 改為開啟**當前反白項目**，既有 `quick_open_target_is_active` 驗證仍保留，開檔失敗不會假裝完成。
- 搜尋結果繼續顯示 Workspace 相對路徑，沒有只剩同名 basename，也不會把 Host Agent 名稱混入檔案搜尋。未改動使用者原檔／搜尋索引語意。

## Windows 驗證

- 新增 `quick_open_ui::tests::e04_quick_open_arrows_select_bounded_matches_without_stealing_ime_keys` **1/1 PASS**；包含 Up/Down 上下界、查無結果與注音組字不攔截。
- `file_search::tests::` **3/3 PASS**：巢狀、Unicode、模糊、ignored directory、Ctrl+P 與 Explorer 篩選範圍區分。
- `project_search::tests::` **3/3 PASS**：Unicode 全文搜尋、片段與容量限制。
- Native Binary 回歸 **37/37 PASS**、Windows `cargo check --bin alera-gpui-desktop`、`cargo build --bin alera-gpui-desktop` **PASS**、`cargo fmt` 與 `git diff --check` PASS。

## 未驗收／後續

- 尚未用 Windows GUI 真正按方向鍵／Enter／注音候選實測；不能宣稱 GUI PASS，也不能從 Rust 測試推斷 GPUI 事件一定能傳遞到容器。
- 長檔名與同名檔在窄視窗的視覺排版、捲動位置仍需 GUI 操作對照 Classic；這次只確保選取邏輯、相對路徑語意與 Windows 編譯。
- E04 正常功能程式與基礎回歸已接線；繼續原 F2 主線 `E05`：跨 Worktree 內容搜尋與正確 Worktree 的結果開檔；遺留 GUI/Classic parity 統一追蹤，避免再次卡在細項。沒有合入 `main`、Push、部署或 Release；`supported=false`。
