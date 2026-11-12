# Alera Rust + GPUI 原生桌面重寫：完整逐步執行、驗收及交付計畫

> 日期：2026-11-07（Asia/Taipei）。本規劃基準：`feat/gpui-desktop-20261107` 的程式 HEAD `570be90d64742f00700009add2d2a84262c31e9a`；**此文件後續提交會使 HEAD 改變**，執行者應先 `git rev-parse HEAD`。與 `docs/gpui-desktop-successor-handoff-20261107.md` 配套閱讀。
>
> **產品狀態：EXPERIMENTAL / UNSUPPORTED，不是 Flutter 正式桌面替代品。** 本文描述「已做的」、「未驗證的」與「計畫要做的」，不得將規劃當成已完成功能或將單元測試當成 GUI 驗收。
>
> **提交策略：** 在 `.worktrees/feat-gpui-desktop-20261107`（或個別新 worktree）持續開發；保留正式 `main`、既有 Flutter/mobile、Runtime Host、PTY 與使用者資料。**無新授權不得合併、推送正式遠端、打 tag、發布或覆蓋使用者安裝版本。**

> **2026-11-12 最新開發優先序：** 請先看 [GPUI 功能完整度追蹤表](gpui-feature-completeness-tracker-20261112.md) 的 NOW／NEXT。本文 G0–G6 保留作歷史與發版 Gate，不再以極端情境測試阻擋一般功能開發。

## A. 接手第一眼：確認基準、目前完成度與立即決策

### A.1 來源與責任

- Repo：`E:\Dropbox\work\alera\alera`。
- 專用 worktree：`E:\Dropbox\work\alera\alera\.worktrees\feat-gpui-desktop-20261107`。
- 功能實作位於 `desktop_gpui/`；`rust/alera-core` 與現有 `rust/alera-cli` 供重用；Flutter 正式桌面應保持可正常編譯。
- 既有交接：`docs/gpui-desktop-successor-handoff-20261107.md`；歷次增量：`docs/gpui-desktop-migration-20261107.md`；可執行命令：`desktop_gpui/README.md`。
- **本文件是逐步執行清單**。遇到此文件與目前實際程式衝突，先定位差異並更新文件，不能直接猜測已完成。

### A.2 此次已核對的客觀狀態

| 項目 | 目前可確定的事 | 目前**不能**宣稱 |
| --- | --- | --- |
| Git | `570be90d6` 已包含 P0 `EditorSession`、`editor_safety` 及唯讀 Terminal 世代防護；觀察時分支乾淨 | 後續接手時必定仍乾淨；每輪必須重新檢查 |
| Headless | 在 `570be90d6` 重跑 `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`，**83 passed / 0 failed / 0 ignored** | 83 個 model test 等於所有 OS GUI、候選窗與儲存競態已安全 |
| Windows 編譯 | 前次的 WIP 程式已通過 native `cargo check --all-targets`，較早 `155fbdcee` 的 native `cargo build` 已成功 | 此刻 `570be90d6` 已重新跑完整 EXE 建置、GUI 實際點擊／長檔案驗收 |
| Editor | UTF-8/Unicode、IME 文字模型、Undo/Redo、草稿、guarded Save、`EditorSession`、`uniform_list`（可讀所有 256 KiB 以內的行） | Windows 注音候選窗、滑鼠事件路由、5,000 行真實畫面、存檔競態已正式驗收 |
| Terminal | 現有 Host 的**唯讀** session list/read、VT 顏色／ANSI、過期 session/Workspace 回覆防護 | 已有互動式 PTY/貼上/IME/resize 或經過授權的 Live Session Lease |
| Git / Agent / MCP | 正式產品既有能力可供重用 | GPUI 版 Git Graph、Diff、Hooks、MCP/Agent parity 已完成 |
| CI/發行 | 存在 `.github/workflows/gpui-desktop.yml`（實驗性） | 遠端全部 job 綠燈、macOS/Linux GUI 合格、簽章與正式發布完成 |

### A.3 要求執行順序與工作邊界

**唯一最優先：先閉合 Gate G0（基線可重跑）與 G1（Windows 真實編輯＋資料不遺失），再進 P1 效能及其他 IDE 功能。**

```text
G0 乾淨基線 / test evidence
  → G1a  Windows 5,000 行 GUI / Unicode / 滑鼠與捲動
  → G1b  Windows 注音 IME 真實候選窗
  → G1c  Save / Draft / Crash / 外部修改 fail-closed
  → G1d  單編輯器與 Workspace / Tab 生命週期
  → G2   自動化 GUI + 漸進性能量測
  → G3   文字引擎 / Search / Diff / LSP / Workspace parity
  → G4   安全的 Host / Terminal 互動及既有 PTY ownership
  → G5   Git → Agent / MCP → OS、多視窗與拖放（各有自己的授權 Gate）
  → G6   多 OS 真正 E2E + 回退 / 打包 / 簽章 / 發行審查
```

**每一個 Gate** 都以失敗回復、程式碼提交、測試輸出和實機證據關閉。只要 G1 的資料安全仍有 FAIL/NOT RUN，就不應聲稱「桌面原生版可用」或開始做會讀寫使用者專案的新 Host/Git/MCP 行為。

## B. 共同操作規範：每張工作單先做這些

### B.1 證據與安全的 Git 流程

1. 進入**獨立 GPUI worktree**，先 `git branch --show-current`、`git rev-parse HEAD`、`git status --short`；有其他人 WIP 時**不要清除／reset／checkout/restore 他人檔案**。
2. 本張工作單寫明：目的／範圍／不做的事情／變更檔案／預期測試／失敗時回復手段；以失敗重現或 test fixture 起始。
3. 修改前檢查 `rust/alera-core`、既有 Host CLI/RPC 與 Flutter 已有功能，優先使用正式產品**相同的狀態／權限契約**；不另開一套 Host、CredentialStore、PTY owner 或 Git transaction。
4. 一個相互獨立的小功能／bug 一個 commit。不得為求測試綠燈移除失敗測試、擴大 ignore、取消安全檢查、改測試 fixture 偷換需求。
5. 本機測試命令輸出、當次 HEAD、OS/DPI/IME、實測時間、截圖/錄影、未測項目留到 `docs/gpui-test-evidence/` 或 PR artifact（如目錄尚不存在須安全建立）；**不要把金鑰／私有工作區檔案／Terminal transcript 上傳**。
6. `git diff --check` → 格式 → headless → Windows check/build → 適用的 GUI/Host E2E → `git add -- <explicit files>` → `git diff --cached --check` / `--name-only` → commit → `git status --short`。
7. 對一個 Gate 真正驗收前，列出 `PASS / FAIL / BLOCKED / NOT RUN`。所有 `NOT RUN` 都維持風險，**不能被「Rust test 已通過」取代**。

### B.2 Windows 現有機器基準命令

```powershell
cd 'E:\Dropbox\work\alera\alera\.worktrees\feat-gpui-desktop-20261107'
git branch --show-current
git rev-parse HEAD
git status --short

# Rust 1.98、GPUI 0.2.2；Windows native 需 SDK shader tools
$env:CARGO_BUILD_JOBS = '4'
$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-model-target'
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline

$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-target'
cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline
cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline
cargo fmt --manifest-path desktop_gpui/Cargo.toml --all -- --check
git diff --check
```

其他機器請將 `D:\alera-build` 換成合適的本機 target 位置。乾淨 CI 若 crates 未在 cache 裡，可在允許依賴下載的環境去掉 `--offline`；保留 `--locked` 以固定解析版本。已取得的 83/83 是**這次基線**，未來新增測試不能只回報原本的 83。

### B.3 GUI 測試只能操作隔離專案及程序

```powershell
$caseRoot = Join-Path $env:TEMP 'alera-gpui-acceptance'
$ws = Join-Path $caseRoot 'workspace-A'
New-Item -ItemType Directory -Force $ws | Out-Null
$env:ALERA_GPUI_STATE_DIR = Join-Path $caseRoot 'gpui-state'
$env:ALERA_RUNTIME_DIR = Join-Path $caseRoot 'runtime-no-production-host'
$env:ALERA_CLI_PATH = 'Z:\missing-gpui-test-cli.exe'

# 5,000 行，明確保留無 BOM UTF-8、繁中、emoji 與結合字元
$lines = 1..5000 | ForEach-Object { 'L{0:D4} 中文 👩‍💻 é' -f $_ }
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText((Join-Path $ws '5000-lines.txt'), ($lines -join [char]10), $utf8)
[System.IO.File]::WriteAllText((Join-Path $ws '601-lines.txt'), (($lines | Select-Object -First 601) -join [char]10), $utf8)
[System.IO.File]::WriteAllText((Join-Path $ws 'empty-and-crlf.txt'), ("甲" + [char]13 + [char]10 + [char]13 + [char]10 + "末行" + [char]13 + [char]10), $utf8)

$exe = 'D:\alera-build\alera-gpui-target\debug\alera-gpui-desktop.exe'
& $exe $ws
```

**注意**：這是實驗性 GPUI 視窗，不連接／接管正式 Host；只有專門 Host 測試才提供測試 CLI。若桌面有人使用其他軟體，改到獨立測試機或虛擬桌面，**不可注入全域滑鼠／鍵盤或依程序名稱大量 Kill**。結束時只停止本次啟動、PID 已確認的程序；清理前確認所有 fixture 路徑都屬於這次建立的 `$caseRoot`。

### B.4 證據紀錄的最小格式

對每張工作單保留一份 Markdown 結果：`ticket`、`start_head/end_head`、OS、Rust/GPUI 版本、Windows build exit code、單元測試總數、操作影片／截圖位置、未通過的 case、預期與實際差異、修正 commit、回復手段。對資料安全案例另外保存原始檔與保存後的 **SHA-256**；不要只相信 UI 顯示「Saved」。

## C. Gate G0：先建立可交付的乾淨基線

| 步驟 | 實作／執行動作 | 輸出與關門條件 |
| --- | --- | --- |
| G0-01 | 確認分支、commit、沒有他人 WIP；標記 `570be90d6` 已含 `EditorSession` 與 P0 library 測試 | `git status --short` 無非預期檔案；記錄精確 HEAD |
| G0-02 | 重跑 83 項 headless，查看 `editor_safety`、`editor_session` 的失敗分支是否真的有 assert | 全綠；失敗保留 log，不可以「偶發」就跳過 |
| G0-03 | 在**當次 HEAD** 重新跑 Windows native `check --all-targets`、`build` | EXE 存在且退出碼 0；計入新的完整 build 證據 |
| G0-04 | 核對 Workflow `model-tests`（Win/macOS/Linux）、`windows-native`、`cli-no-start` **有無真正執行** | 記下 Actions run URL、SHA、各 job 結果；只有 workflow 檔不算 PASS |
| G0-05 | 建立 fixture 清單（檔名、編碼、原始 SHA256、bytes、預期行數） | 全部在隔離路徑，沒有私人專案／密鑰／Host |
| G0-06 | 對 `docs/gpui-desktop-successor-handoff-20261107.md` 風險 R-01～R-12 重新標狀態 | PASS/FAIL/BLOCKED/NOT RUN，無無來源的綠燈 |

**已完成部分**：本計畫撰寫前在 `570be90d6` 重跑 83/83 headless PASS。Windows 原生**完整** build、遠端 Actions、視覺 E2E 仍由實際執行者在當時 HEAD 取證。G0 除這些之外不得以「已編譯過」就全數勾選。

## D. Gate G1a：Windows 5,000 行編輯器真正 GUI 驗收（工作單 GPUI-01）

**來源**：`desktop_gpui/src/editor_ui.rs` 的 `NativeEditor::render`、`render_row`、`EditorLineMetrics::paint`、`pointer_index_at`、`UniformListScrollHandle`、`line_geometry`；`desktop_gpui/src/editor.rs` 的 `EditorLineIndex`。`main.rs` 負責 Edit/Save/Close 入口。

**完成順序（每一小步可獨立修復與提交）**

1. **G1a-01 檔案準備**：建立 0 bytes／單行／空白行／CJK＋ZWJ emoji／combining marks／CRLF／尾端 LF／601 行／5,000 行／超長單行／大於 256 KiB 檔案。確認清單的行數、byte size 與 SHA256，確保測資沒有超出編輯器限制（超限案例另測拒絕）。
2. **G1a-02 首次開啟**：啟動隔離 Windows GPUI、點開 5,000 行檔案，按 `Edit`。驗證不 crash、不只有前 600 行、初始游標行可見；記錄畫面與右下角行列資訊。
3. **G1a-03 行導覽**：`Ctrl+Home` → 行 1；`Ctrl+End` → 行 5,000；再捲動到 600／601／2345 行。對照 fixture 預期文字；不可跳行、倒置或顯示錯誤檔案。
4. **G1a-04 精確滑鼠**：測首字、CJK 字中、emoji ZWJ 左右、Combining mark、空白行與長行；caret 必須停在合法 Unicode grapheme 邊界；不得隨時點都跳行尾。
5. **G1a-05 選取與複製**：Shift+click；由 700 向 599 行反向選取；滑鼠按住不放移到可視區上／下 24px、觸發捲動並繼續跨行選取；放開滑鼠後 2 秒內不得繼續捲。貼到隔離文字編輯器，逐字節核對選取文字。
6. **G1a-06 編輯與 Undo**：在第 601、5,000 行輸入 UTF-8，按 Enter／Backspace，Ctrl+Z／Ctrl+Y；驗證行索引重建後仍不錯位；重做不將已撤回文字復活到錯誤行。
7. **G1a-07 版面變更**：改視窗大小、捲動後改 DPI（若有測試螢幕）、切換其他預覽 Tab 再回編輯；實際 `ShapedLine`／命中座標／游標／選取仍一致。
8. **G1a-08 掃描渲染生命週期**：特別檢查 `line_geometry.clear()` 在 `render` 和 `uniform_list` processor callback 被觸發的時機，`EditorLineMetrics::paint` 回寫 state 是否造成重複 layout、stale bounds、無限 `notify` 或崩潰。若不合格，先用最小回歸重現，再調整 cache/paint ownership，不能靠延後計時掩蓋。
9. **G1a-09 基礎性能證明**：在 5000 行上下快速捲動、鍵入，記錄被 shape/layout 的可見行數、p50/p95 frame time、CPU/RSS、是否產生爆量 allocations；比對穩定性而非一開始就重寫 Rope。
10. **G1a-10 關門**：每個測試標 PASS/FAIL/BLOCKED，保留 GUI 真實截圖／影片、原始／編輯後 SHA、當次測試環境及 commit。失敗只修該問題及受影響模型，不先增加語法著色／MCP/Git。

**具體缺陷預防**：`uniform_list` 的固定列高 **19px**，不可將 soft-wrap/變高行當作已支援；`line_geometry` 必須只命中當前 viewport；`bounds_for_range` 若組字 caret 不在可見區不能虛構座標。唯讀預覽仍可能只有 600 行，這張工作單驗收的是**可編輯 native editor**。

**G1a 關門條件**：所有測試在真實 Win32 GPUI 視窗通過、成功編輯第 5,000 行、正確 Copy / Undo / Save、無重繪風暴且記錄測量結果。**headless 的 5,000 行索引測試不等於 G1a 完成。**

## E. Gate G1b：Windows 微軟注音／IME 真正驗收（GPUI-02）

**程式入口**：`editor_ui.rs` 的 `EntityInputHandler` 實作（`selected_text_range`、`marked_text_range`、`replace_text_in_range`、`replace_and_mark_text_in_range`、`unmark_text`、`bounds_for_range`、`character_index_for_point`），`editor.rs` 的 UTF-16 ↔ UTF-8、marked range、Undo composition transaction。

**實作順序**

1. **G1b-01 測試環境**：在 Windows 安裝／啟用**微軟注音**（記錄 OS、語系與輸入法版本）；除英文鍵盤以外，依次測注音與中英切換。使用 G1a 隔離 fixture，不接使用者日常檔案。
2. **G1b-02 聚焦／候選窗**：滑鼠點到空白行、長檔 601 行、5,000 行，輸入「中文測試」；確認 composition underline、候選列、按數字選字、Enter 提交。候選窗**實際貼近 caret**，不能固定顯示在視窗左上角。
3. **G1b-03 取消與更換選字**：組字中變更候選、按 Esc 取消、重新輸入、再提交；Undo 歷史只有「完整提交」的一步，取消不留下 dirty phantom edit 或誤寫 draft。
4. **G1b-04 多次 preedit**：在同一個 composition 中持續替換幾個候選，觸發 `replace_and_mark`、`unmark`；驗證一筆 Undo 還原提交前整段文字，不是退回單個注音符號。
5. **G1b-05 UTF 轉換邊界**：`a👩‍💻é中文字`、跨行反向 selection、Shift+Home/End、Ctrl+Z/Y、完整 emoji；UTF-16 surrogate 不可被當成 1 byte，OS range 與畫面選取一致。
6. **G1b-06 捲動／候選位置**：組字開始後上、下捲動，調整視窗大小及字型；不可引用已被 virtual list 回收的行幾何。caret 在畫面外時不得回報假 rectangle，回到畫面後候選位置應復原。
7. **G1b-07 DPI／多螢幕**：至少 100%、125%、150% 縮放各測一輪（若無測試硬體，標 **BLOCKED**，不得說 PASS）；試跨不同 DPI 螢幕移動視窗。記錄候選窗與 caret 的像素誤差、顯示器 ID。
8. **G1b-08 非正常操作**：組字中點 Save／切檔／切 Workspace／關閉視窗／程序崩潰；定義是完成組字還是阻擋。未提交 preedit 不可持久化為已保存文字，崩潰後 draft 必須保留最近 committed 狀態。
9. **G1b-09 修正設計**：若候選窗位置不正確，先量測真實 `ShapedLine` 的 caret bounds 和 `NativeInputCapture` 的輸入代理 bounds，確認 GPUI 0.2.2 平台 IME 管線；不要靠加固定 X/Y offset 修正單螢幕，導致高 DPI 壞掉。
10. **G1b-10 跨平台後續**：Windows 完成後，macOS 系統中文輸入法及 Linux IBus/Fcitx 是**各自獨立的產品驗收項**，不因 Windows 微軟注音通過自動視為支援。

**IME case matrix**

| ID | 操作 | 預期 |
| --- | --- | --- |
| IME-01 | 於第 1／601／5000 行輸入繁中並選字 | 字元與候選窗皆正確 |
| IME-02 | 多次改 preedit 後 Esc | 回復原內容、Undo 未增加 |
| IME-03 | 多次改 preedit 後 Enter／空白鍵提交 | 一筆 Undo 交易、Redo 正確 |
| IME-04 | ZWJ emoji＋組合字中插入／選取／貼上 | 不切開 grapheme、UTF-16 反向 selection 正確 |
| IME-05 | 組字中垂直／水平捲動與視窗變更 | candidate 跟隨當前 caret、不用過期幾何 |
| IME-06 | 組字未完成直接 crash/reopen | draft 僅含 committed 文字，來源沒被修改 |
| IME-07 | 1 分鐘快打、Ctrl+Z／Y 與中英切換 | 不吞字、不重複字、不卡 UI |
| IME-08 | 100%／125%／150% 與多螢幕 | caret／候選窗位置一致，記錄像素差 |

**G1b 關門**：IME-01～07 Windows 真實操作、錄影與逐案記錄；IME-08 應取得測試硬體證據才可關閉相應 DPI 風險。純 Rust composition 測試不能取代平台候選窗。

## F. Gate G1c：Windows Save／Draft／Crash 資料完整性（GPUI-03）

**目前基線**：`editor.rs::EditorDocument::{open,save,replace,unmark}`、`EditorDraftStore::{persist,restore,remove}`、`editor_session.rs::EditorSession::{open_file,save,close,persist_draft,can_switch_workspace}`；`editor_safety.rs` 已有不少負面測試，但它們是 headless。`Save` 前檢查來源再用同目錄暫存與 `rename`，**不是已證明的 OS 級原子 CAS／無競態**。

**執行順序**

1. **G1c-01 資料契約**：寫下每一狀態應保存的資料：disk 原文、buffer、draft 原文快照、dirty、Undo stack、目標 canonical path、當前 Workspace ID。使用測試檔案，對修改前、外部變更後、Save 後計算 SHA-256。
2. **G1c-02 一般 Save**：原文含 `CRLF`／LF／空白末行／繁中／emoji；修改並 Save；真實磁碟 byte-by-byte 與 UI 結果一致；再次開啟看到同一版本，成功清草稿，不擅自改換行／BOM。
3. **G1c-03 外部修改**：用另一個程序在「載入後、按 Save 前」改原檔；Save 必須提示 conflict、不覆蓋外部內容，buffer／草稿不消失，來源 hash 維持外部版本。
4. **G1c-04 路徑失效／錯 root**：將檔案移走、更名、刪掉；測 junction、symlink、路徑跳出 root、不同 workspace 同名檔，不能因名稱相同就覆寫另一個檔案。未能證明 canonical identity 時 fail-closed。
5. **G1c-05 Windows 寫入失敗**：只讀 ACL、鎖檔、temp 目錄無權限、磁碟容量不足（可用受控限額 fixture）、rename 目的已存在；UI 必須明確指出 Save 未成功或 Save 成功但 draft 清理失敗，絕不把兩者混成「成功且已無草稿」。
6. **G1c-06 check→replace 競態**：在檢查後、檔案取代前，用同步測試鉤子安排其他程序改內容／替換路徑；設計與驗證 Windows 安全取代策略（例如受 OS 允許的 open handle identity/locking/replace API）。即使不能完全阻止惡意競態，也要**列明不可保證的條件**、保留緩衝與兩方證據，不要誤宣稱真 CAS。
7. **G1c-07 Draft 寫入與還原**：修改但不 Save → 關閉測試進程（含真正 Kill 本次 PID）→ 重啟 → Edit → 確認 committed 文字復原；組字 preedit 不包含；外部磁碟版本改變後要顯示衝突並允許人工比較，不能隱性 Save。
8. **G1c-08 兩次 Discard 與 Cancel**：有 dirty 時 Close 不丟失；第一次 Discard 僅 arm，第二次 Confirm 才清；按 Cancel／改點 Save 後 confirmation 狀態必須消失。draft 清除失敗時保持編輯器與復原資料。
9. **G1c-09 明文風險處理**：目前 editor drafts 包含原始與未儲存明文，UI 必須告知。產品化前設計敏感工作區 opt-out／per-user ACL／系統保護與加密需求；即使先保留明文，**不得把這個目錄稱為安全儲存**或在不知情下同步到公開路徑。
10. **G1c-10 UI thread 持久化**：用 256 KiB cap 的極限編輯，量輸入延遲與同步寫 draft 所耗時間；如果有可量卡頓，再引入 debounce＋單 writer queue、generation、最後一次 flush barrier，確保 Crash／Close 的最後 committed 文字能復原。
11. **G1c-11 失敗矩陣跑完**：把「哪種故障 → 原檔 hash／buffer／draft／提示文字／回復操作」整理到測試報告，附 Windows 版本／權限環境與可重現腳本。

**失敗的最低安全狀態**

| 故障 | 原始檔 | 未存 buffer | Draft | UI |
| --- | --- | --- | --- | --- |
| 外部修改衝突 | 保持外部版本 | 保留使用者修改 | 保留 committed 修改 | 明確 conflict 與人工復原選項 |
| 來源被刪／改名或越界 | 不建立替代寫入 | 保留 | 保留 | Save blocked，顯示目標路徑 |
| 暫存／覆蓋／權限錯誤 | 保留可確認的最後正常版本 | 保留 | 保留或明確失敗提醒 | **不能顯示成功** |
| Save 已成功但 draft remove 失敗 | 使用者 Save 版本 | 保留可讀 | 可能殘留，需去重/明確提示 | 分開標示 saved＋cleanup failed |
| 未提交 IME preedit 時崩潰 | 不變 | 恢復最近 committed | 不包含未提交 preedit | 明確指出復原內容範圍 |
| Explicit Discard 確認 | 不變 | 刪除未存編輯狀態 | 成功後才清 | 一次 arm、第二次確認 |

**G1c 關門**：原檔及草稿皆有 hash／byte-level 與真實 OS 失敗情境證據，尚未能防止的 TOCTOU 競態不得模糊帶過。未通過前不要開啟 Git write／Agent write／PTY input 等更多可破壞動作。

## G. Gate G1d：Editor / Tab / Workspace 生命週期（GPUI-04）

**目前模型**：`EditorSession` 只管理**一份**可編輯 `EditorDocument`；`WorkbenchModel` 的其他 Tab 仍主要是唯讀 Preview。這是目前**刻意的限制**，不能靠 UI 改名假裝每個 Tab 都有獨立編輯 buffer。

1. **G1d-01 定義 State ownership**：制定以 canonical workspace ID、原始檔 identity、document ID 為鍵的 `EditorBuffer`／draft／focus／undo／selection 生命週期；不要用相對路徑或裸檔名當全域鍵。
2. **G1d-02 先驗單編輯器鎖定**：同一檔重複 Edit → 已編輯提示；有 dirty 時切其他檔 → 明確阻擋；Save／Discard 或 Cancel 後才能切。Save 實際寫入的路徑必須是 UI 所顯示的文件。
3. **G1d-03 Workspace 切換**：準備 A、B 兩個隔離 root、都含 `src/main.rs`，在 A 修改未存時切 B 必須 fail-closed；成功切換時放棄的是**本地唯讀 Terminal 預覽**，絕不能終止 Host PTY。
4. **G1d-04 已存檔切換**：在 A Save 後切 B，B 的唯讀 Tab 順序／Explorer 目錄從 B state 恢復；回 A 時讀 A 的 state，不能套用 B 的來源檔或 draft。
5. **G1d-05 多文件設計選擇**：經前述測試後才設計每 Tab 可編輯 entity；引入 `DocumentId/WorkspaceId` 與獨立 Undo、dirty、focus、復原檔映射，避免「同一個 NativeEditor Entity 換 path」造成狀態遺失。
6. **G1d-06 每 Tab 關閉策略**：Save / Don't Save / Cancel、視窗關閉、整個 Workspace 移除、rename/move、app restart 都須明確定義。被移動到其他 Worktree 的檔不能變成原文件可寫路徑。
7. **G1d-07 異步回覆世代**：Host list/read／檔案載入／未來 Git status 或 Agent callback 都捕獲 request epoch＋root identity；在切 Workspace 後拒絕老回覆；重複 Terminal handle 必須檢查 Host session identity 而不是直接重新附著。
8. **G1d-08 真實 GUI 重啟矩陣**：至少 2 Workspace × 3 文件，做兩輪開關程式與 Close/Discard/Cancel/Save；原檔 byte-level 比對且無錯 Workspace。

**G1d 關門**：即使多編輯 Tab 仍未實作，目前單編輯器模式也必須**不誤寫、不遺失、可明確拒絕**。完整每 Tab buffer 是下一階段功能，需獨立工作單和測試，不藉由本 Gate 放大 scope。

## H. Gate G2：自動化、效能與測試可重現性（G1 可靠後）

### H.1 GUI 自動化設計（不可用 fake model test 取代）

1. **G2-01 機制選型**：先評估 GPUI/OS 可用的 Window Handle、Accessibility tree 與 Win32 UI Automation API；選**實際可驅動視窗**的 harness，必要時配合小型測試專用 instrumentation，避免使用改過的 production code path。
2. **G2-02 隔離初始化**：runner 建立 temp workspace、temp `ALERA_GPUI_STATE_DIR`、temp `ALERA_RUNTIME_DIR`，CLI 指向明確不存在的測試路徑，確認**任何** production RuntimeStore/PTY 無被讀寫。
3. **G2-03 所有者追蹤**：runner 保留本次啟動 GPUI 的 PID、視窗 handle、起始時間、子程序 PID；逾時只停止本次 runner 擁有的程序，別用 `Stop-Process -Name alera` 造成使用者工作毀損。
4. **G2-04 最小 GUI E2E**：自動完成「啟動→選檔→Edit→第 601 行→輸入 UTF-8→Save→關閉→重啟→核對磁碟 hash／UI」；新增「有 dirty→切 Workspace 拒絕→Discard 二次確認」。
5. **G2-05 失敗取證**：保留 CLI stdout/stderr、GPUI crash stack、程序退出碼、測試用專案清單、關鍵畫面截圖、重現影片；確認遮蔽使用者機密。
6. **G2-06 拆分 CI 類型**：pure Rust headless：Win/macOS/Linux；Windows GUI：native Windows runner；有權限隔離的 Host E2E：獨立 job；macOS/Linux GUI 在有真實環境後分開新增。
7. **G2-07 檢核 workflow**：`.github/workflows/gpui-desktop.yml` 目前含 `model-tests`、`windows-native`、`cli-no-start`，先實際執行並核對每一個 run；若因環境 SDK / cache 失敗，要記錄具體原因，不直接把 job 標 skipped。
8. **G2-08 品質門檻**：PR 上每個必要 job 有 SHA 與通過/失敗證據；任何 fail 不被忽略。仍不加入正式 Flutter release workflow，不發布 GPUI artifact。

### H.2 真正性能量測與是否需要 Rope

1. **G2-09 建立相同工作負載**：5k／10k／100k 行、UTF-8/CJK/emoji、超長單行、500 次小編輯、Undo/Redo、選取複製、視窗縮放、巨量 Terminal output，確保**測資大小上限不會被默默截斷**。
2. **G2-10 記錄基準**：同一設備/OS/字型/解析度、相同冷啟／暖啟情境，分開新程序量 GPUI 與正式 Flutter：cold start、p50/p95 input-to-paint、scroll frame time、RSS、CPU、Save latency；至少重複多次，報分佈而非單筆最高速。
3. **G2-11 隔離瓶頸**：`EditorLineIndex::new` 目前會掃文件建立行開始 byte offset；`render_row` ShapedLine 與 `EditorLineMetrics::paint` 會被 GUI 更新；`EditorDraftStore::persist` 仍在 UI thread，逐段量測時間。
4. **G2-12 先減少非必要成本**：重用行索引／字型 shape cache、只在內容 generation 改變時 invalidation、合併重繪、以節制頻率寫 draft；不能在未測到瓶頸前直接重寫全部文件模型。
5. **G2-13 Rope / Piece table 決策**：如果 256 KiB cap／整份 `String`／64 個 snapshot 成為顯著瓶頸，先比較既有 Alera Rust editor 的 Rope/持久化／增量 parser 設計，提出 ADR 記錄 API/記憶體/Unicode/IME/Undo 成本、遷移風險；有證據才換。
6. **G2-14 成效驗證**：改動前後測同一份 workload，對未達標或退步項目留異常原因，不可只挑最快的單一數字；檢查 Windows 和 macOS/Linux 差異。

**G2 關門**：有**真的跑過**的 GUI 測試 job、輸出 artifact、可重複量測和 bottleneck 證據。Rope、增量索引、cache 是測量後的選項，不是先前已定案的實作需求。不得以未實機測試的 benchmark 取代 G1。

## I. Gate G3：文字編輯器產品化、Explorer、搜尋、Diff、LSP

**依賴**：G1（可靠的單文件輸入／儲存）先完成；G2（可重跑 GUI／效能）可與設計工作並行。先保留「功能完整度矩陣」，不要把 UI 看起來像 VS Code 就當作功能 parity。

### I.1 文件模型與多文件 Tab

1. **G3-01** 整理現有 `EditorDocument`／`EditorSession`／`WorkbenchModel`，指定文件 identity、讀取 generation、draft key、dirty/recovery、Undo 所有者。
2. **G3-02** 建立每個可編輯 Tab 自己的 Document/Selection/Undo/IME state。關閉 Tab 必須走 Save/Discard/Cancel；不把 A Tab 的 dirty/scratch 顯示成 B Tab 的 Save 目標。
3. **G3-03** 完成 Untitled / Scratch 的新建、持久化及 Save As（有明確路徑／檔名／覆蓋確認）；取消選路徑不能建立檔案或丟失 buffer。
4. **G3-04** 引入檔案 watcher／外部變更提醒、重開檔保留 cursor、最近檔案；須用 Git Worktree 與 canonical path 區分同名檔。
5. **G3-05** 處理 line ending（LF/CRLF）、BOM/encoding（明確轉換）、二進位拒絕、超限檔拒絕或僅預覽；**不許截斷後 Save**。

### I.2 Explorer、搜尋及導航

1. **G3-06** 從正式 Alera 整理 Finder/Explorer 功能矩陣：Workspace 樹、跨 Worktree、搜尋／模糊搜尋、Ctrl+P、定位目前檔案、Tab favorite、最近檔案、拖入資料夾。
2. **G3-07** 將檔案系統掃描與搜尋搬到背景且可取消，限制檔案數量／結果數量／ignore 規則；GUI thread 不等待遠端磁碟或大型目錄。
3. **G3-08** 完成搜尋結果 → canonical workspace/file → 編輯器 Tab 的開啟路徑；重複結果不重複開；Workspace 切換時 discard stale results。
4. **G3-09** 測 nested repo、linked Worktree、bare Git、symbolic link、Windows junction、中文及長路徑、大小寫敏感／不敏感磁碟、目錄超限與權限拒絕。

### I.3 文字顯示、Diff 與語言服務

1. **G3-10** 為長行／水平捲動／變高行／軟換行／BiDi／字型 fallback 設立 geometry contract，確保 caret、drag、IME 的 bounds 與可見字串是同一版。
2. **G3-11** 依現有 Rust parser/LSP 資源加入語法 token、診斷、完成、跳定義、檔案搜尋；資料產生在 background，Editor 使用 version/generation 阻擋 stale response。
3. **G3-12** Diff 拆為唯讀資料模型與視圖：intraline、換行、加刪、文件滾動同步、幾千個 diff hunks；先重用既有 diff engine，不再複製一套不相容演算法。
4. **G3-13** Stage／unstage 會寫 Git index，與唯讀 Diff 分開設計安全操作。合併、解衝突、丟棄修改更要明確確認與回復。
5. **G3-14** 設定／快捷鍵／三語系至少覆蓋已提供的 UI 功能；不把缺少的功能寫進表單讓用戶按了沒反應。

**G3 關門**：常用編輯、搜尋、Diff、診斷流程有完整可操作 GUI，檔案 Save 仍 fail-closed；和正式 Flutter 一對一的 parity 表有測試，新增功能都有性能／回歸證據。

## J. Gate G4：Terminal 從唯讀預覽到**受權限保護**的互動（GPUI-06）

**當前界線**：`desktop_gpui/src/runtime_bridge.rs`、`terminal.rs` 只能 `terminal --no-start --json list/read`；現在雖能 VT100/ANSI 色彩呈現既有 Host 輸出，仍**不能輸入、paste、resize、終止或擁有**既有 PTY。`HOST_SESSION_LIST_SCOPE` 標註為所有 Workspaces，而不是目前工作區獨享。

### J.1 唯讀行為先驗收

1. **G4-01** 用專門測試 CLI 建立獨立 Runtime Host，不共用正式 `ALERA_RUNTIME_DIR`。確定 Host offline 時 `list/read` 不會啟 Host；沒有 `--no-start` 的舊 CLI 必須明確拒絕。
2. **G4-02** 驗證 ANSI 16/256/RGB、背景色、reverse/bold/italic、OSC52 不外洩到剪貼簿、DCS／alt-screen、表格、Unicode 寬字、回捲 240 行、48 KiB transcript cap。
3. **G4-03** 證明 `1.5s active poll`／`15s list refresh` 不產生多個併行 polling loop 或 CPU 空轉；Host 斷線時保留既有唯讀 transcript，但不顯示「仍在線」。
4. **G4-04** Session handle 被重用、Host epoch 改變、切 Workspace 後舊 list/read 回覆必須丟棄；測試新的 `TerminalDock::for_workspace`、`apply_workspace_read` 與現有 stale ticket。
5. **G4-05** 在 UI 清楚標「Read-only · existing Host · all workspaces」，不得錯標為某個 Workspace 專屬的 session 擁有權。

### J.2 設計互動操作安全契約（**完成前保持唯讀**）

1. **G4-06** 查既有 Host 的 session owner、Host generation/epoch、view lease、Workspace Tab ownership、transfer receipt、終止／重連語義；閱讀 `docs/terminal-tab-transfer-closure-20261030.md`，且**以目前實際 Host capabilities 為準**，不要把歷史 Gate 狀態當現況。
2. **G4-07** 撰寫 RFC/ADR：`attach(userIntent, hostId, sessionId, workspaceId, viewId, lease, epoch)`、`detach`、`renew/revoke lease`、`sendInput`、`resize`、`terminate`，每個請求要的身分、權限與預期失敗回覆。由既有 Host 作最終授權；GUI 僅呈現結果。
3. **G4-08** 加入「是否為使用者明確開啟互動模式」狀態；讀取資料不可自動取得寫入權限。複數 View 同時附著時要有合法 owner/lease 分歧規則。
4. **G4-09** 测 stale lease、owner 更換、Host restart、session 被刪／handle 重用、跨專案 view、並行輸入、亂序/遺失回覆與 token/epoch mismatch。任何未知狀態**不發送輸入、不殺 PTY、不重建替代 Shell**。
5. **G4-10** 只有 Host API／權限／拒絕測試全面通過後才在 GPUI 加最小鍵入與複製／貼上，接著做真實 Windows PTY 視窗測試；IME 文字**完成 composition 後**再轉 terminal input bytes，避免每個候選更新都輸出。
6. **G4-11** 量測 resize 與 80×24 模型同步、scrollback 保留、斷線重連、Session Lease 續期／撤銷、UI 關閉時 Host PTY 是否持續存在；不得因關閉本地 preview Tab 隱式 terminate。
7. **G4-12** 跨 Workspace／多視窗 Tab 移動只能使用既有 Host-approved transfer/receipt/journal，不能靠複製 transcript 或殺舊開新假裝無縫。

**隔離 Host E2E 指令**

```powershell
# 在該 worktree 根目錄；CLI 自己建置，不能指到正在使用的 Host
cargo build --manifest-path rust/Cargo.toml -p alera-cli --bin alera --locked
$env:ALERA_GPUI_TEST_CLI = (Resolve-Path 'rust/target/debug/alera.exe').Path
# 可選：先 build GPUI EXE 再設定本次測試 app 路徑
# $env:ALERA_GPUI_TEST_APP = (Resolve-Path 'desktop_gpui/target/debug/alera-gpui-desktop.exe').Path
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test live_runtime_host --locked -- --ignored --nocapture
```

`ALERA_GPUI_TEST_APP` 指到的 `desktop_gpui/target` 是**預設編譯路徑**，若使用 `CARGO_TARGET_DIR`，應改指向該實際 target 下的 EXE。這是 Host 協定與程序存活 smoke，不是 Terminal 真實鍵入驗收。

**G4 關門**：在互動 Lease 設計、Host 身分驗證及真實 PTY/E2E 未通過前，保持唯讀；交接文件將未支援互動功能明確標成 NOT IMPLEMENTED。

## K. Gate G5a：Git / Worktree / Diff 的讀寫分離（GPUI-07）

1. **G5a-01** 盤點正式 Flutter：Git Graph、branch favorites/current 置頂、Auto Fetch、worktree、status/diff/history、intraline、stage/unstage、merge、reset/discard、搜尋與右鍵選單。產生一張 **功能及風險**矩陣（唯讀／可逆修改／破壞性操作）。
2. **G5a-02** 先提供唯讀 `alera-core` 適配層：canonical repo/worktree ID → branch/status/history/diff/query → generation-tagged 非同步回覆；GPUI 不直接在 paint/render 中跑 blocking `git`。
3. **G5a-03** 確認 linked worktree、nested repo、bare repo、submodule、sparse checkout、中文/空白路徑與 renamed/deleted/ignored/untracked 能正確辨識；避免從父層錯誤判斷 repo。
4. **G5a-04** Diff 畫面先做唯讀正確性：新增／刪除／移動檔、CRLF、空白、emoji、幾千行 diff、過長行內 diff、二進位拒絕和可見部分虛擬化。
5. **G5a-05** Stage/unstage 與 branch checkout 要先檢查根目錄、目前工作樹與 dirty buffer，明確顯示變更的範圍與目的；不要在不同 workspace 看到同一檔名就誤操作。
6. **G5a-06** 合併／rebase／reset／discard 先顯示 dry-run 可確認資訊，要求使用者明確確認；Git index 或 working tree 操作失敗要保留原狀與可診斷錯誤。LLM 協助合併是可預覽方案，不代表可直接自動提交。
7. **G5a-07** 遠端 push/pull/fetch 要區分 branch、remote、auth 與企業 policy；Auto Fetch 頻率可調，不要讓 UI timer 重覆觸發費用高的操作。
8. **G5a-08** 每個 Git 寫入動作補 integration fixture（至少兩個 worktree、兩種 branch、dirty 改動），驗證錯 repo/錯 worktree **fail-closed**；破壞性功能的 UI 文案要清楚。

**G5a 關門**：唯讀階段可先交付；任何改變 Git working tree 的操作都必須另開明確權限／資料安全 Gate，不能因 status/diff 正確便自動開啟 merge/reset。

## L. Gate G5b：Agent / Hooks / MCP／跨 Workspace 權限（GPUI-08）

1. **G5b-01** 盤點原 Alera 的 Agent／Hooks／MCP 功能、不同 User/Project/Workspace scope、Session identity、Hook 的啟用與啟停；來源文件 `docs/alera-mcp-cross-workspace-grant-contract.md`、`docs/mcp-cross-project-navigation-20261104.md`。
2. **G5b-02** 先提供只讀 Session／Profile／Workspace/Worktree 查詢，讓 Agent 明確知道「目前哪個 Workspace」以及使用者授權可見哪些 Workspace；不因一個 MCP 工具可列出項目就有寫入權。
3. **G5b-03** 設計受控動作：`openWorkspace`、`openFile`、`revealRange`、`compareBranches`、`openGitGraph`、`querySettings`，將目標 workspace/repo ID 和 request generation 明確放入 API。
4. **G5b-04** 有副作用的 `edit/save/git/writeTerminal/changeSettings` 逐類明確授權、顯示影響、可取消、持久審計；獨立核對 host/session/lease，拒絕跨 Workspace 權限升級。
5. **G5b-05** stale tool call、Agent session 重啟、同名專案、兩個 worktree 同路徑相對檔、Host epoch 變更、不同窗口互相開檔；確認不因過期回覆對錯對象執行 mutation。
6. **G5b-06** 每個 Tool 用最小 privilege、bound timeout、輸出上限、授權期限；Hook 生命週期與 ProcessRunner 使用既有 Rust 平台工具，Windows 不應閃現 console，macOS/Linux 路徑與 PTY 行為需各自驗收。
7. **G5b-07** 資安測試：拒絕未授權跨 Workspace、重播 token、要求讀隱私檔、路徑跳出 Workspace、在未取得 Terminal Lease 時注入指令；測試成功及拒絕都要有可追蹤事件。

**G5b 關門**：跨 Workspace grant/deny、stale-response 與權限失效能驗證；若缺 Host 授權 API，先交付唯讀 UI、將寫入標 blocked，不仿造後端授權結果。

## M. Gate G5c：桌面 UI/UX、平台整合與多視窗

1. **G5c-01** 對照原產品 keyboard-first 操作：Tab 雙擊新增未命名檔、Ctrl+P、搜尋檔案、差異列表與行內換行、切分、拖曳 Tab、recent workspace、Branch Switch/Graph；排序為常用／次要／可延後。
2. **G5c-02** 一致的焦點與輸入路由（Editor vs Terminal vs Search vs Agent）。當 Tab/Window 關閉，相關 IME、游標、選取、draft 與 session lease 必須有關閉契約。
3. **G5c-03** File Drag&Drop：外部文件／資料夾拖入、內部 Tab 拖出新視窗、剪貼簿雙向；Windows Explorer／macOS Finder／Linux File Manager 分別測試，關閉窗口不丟 buffer 或活 PTY。
4. **G5c-04** 動態字型／字高／縮放及系統高 DPI；Windows 100/125/150%、macOS Retina、多螢幕混 DPI 的 caret 和候選窗不錯位。
5. **G5c-05** Accessibility／原生選單／捷徑／鍵盤導航／主題與繁簡日 locale；確認按鈕不是 inert，讀屏者能知道 Save conflict、Terminal read-only、Agent 權限。
6. **G5c-06** 單實例／多原生視窗、關閉／重新開啟與失敗回復；View 不能自行重建 Host-owned PTY 或從別的 Window 偷走 session lease。
7. **G5c-07** macOS Apple Silicon、Windows x64、Linux GTK/Vulkan 依賴的**真正 GUI**分開跑；只有 cargo check 或 cross-build 不算 UI/IME 通過。

**G5c 關門**：必需的桌面工作流可在目標 OS 真實操作，焦點／IME／剪貼簿／多視窗生命週期有反向、斷線、關閉場景的 E2E 證據。

## N. Gate G6：跨平台 CI、封裝、信任鏈、發布與回滾（GPUI-09）

**入場前置**：G1 資料完整性、G2 GUI 自動化與測量、已批准範圍的 G3～G5 核心使用情境全部有證據。若缺失使用者常用功能，需要產品負責人**書面選擇延期或不支援**，而不是在 UI 隱藏掉後假裝沒有需求。

1. **G6-01 平台 CI**：Windows/macOS/Linux 逐平台建置與啟動 GPUI；Windows 全 GUI/IME/PTY；macOS Apple Silicon 的系統 IME／視窗／剪貼簿／原生 picker；Linux 實際發行版 GTK/Vulkan／IME 各自跑。所有 skipped 明列原因。
2. **G6-02 完整 regression**：新 GPUI crate、受影響的 `alera-core`／CLI Host test，既有 Flutter desktop 回歸、mobile smoke，確保重新整理 Rust Core 沒改壞產品；新 UI 不能直接取代既有正式測試。
3. **G6-03 安全審查**：檔案路徑／Symlink、未保存草稿、敏感資料、本地 state ACL、Host authentication／Session Lease、Git/MCP mutations、Shell/Hook spawn、依賴漏洞與 Supply Chain；任何 critical/high 未解的發行阻塞要修正，不能默默 ignore。
4. **G6-04 效能比較**：與 Flutter 正式版在相同機器、資料、字型、視窗與暖機條件下量 input latency、CPU/RSS、啟動時間、Terminal 吞吐、長檔案滾動、Git 大庫、Crash recovery；預先定義可接受的回歸門檻，報實際分布與樣本數。
5. **G6-05 封裝**：用獨立 experimental 產品識別與 state directory 做 opt-in preview；Windows ZIP/安裝包、macOS .app/notarization、Linux deb/rpm/tar.gz，確保使用者能與現有 Flutter Alera 並存，不覆蓋正式設定或 Host。
6. **G6-06 信任鏈**：依 `docs/release-trust.md` 的 Windows Authenticode、macOS Developer ID + notarization、Linux repo/package signature、Ed25519 schema v3 manifest、artifact SHA256 與 updater/package-manager ownership；不自造不受信任的平行更新通道。
7. **G6-07 安裝／升級／移除**：全新安裝、升級、意外關閉、版本降回 Flutter、解除安裝、已有正式 Alera 併存；檔案、draft、Session 和 Host 不能被誤刪。設定 migration 必須可回滾或明確拒絕。
8. **G6-08 Release Candidate**：只在授權後於獨立渠道發布 RC，先限制測試人數；完整收集 crash reports、資料損毀、IME、PTY、Git/MCP 操作結果、性能告警。正式 updater 未受認證前不得自動切換用戶。
9. **G6-09 最終產品審查**：功能 parity 及每個「不支援」有批准；Windows/macOS/Linux 可用、有完整安全審查、CI 綠燈、User rollback 可操作、發行包可驗證，方可決定是否合併或切預設。
10. **G6-10 回退演練**：從實驗性 GPUI 切回 Flutter，打開既有 Workspace/Session，草稿與 Host 沒有破壞；失敗時保留可診斷收據、原檔、Host 資源，勿用全面清空 state 的方式強制恢復。

**G6 未通過時**：`main`、Flutter Desktop、官方 installer／auto-update／GitHub release/R2 packages 均維持原路徑。GPUI 仍為 opt-in unsupported 的開發分支；不合併、不 tag、不發布。

## O. 工單拆分與依賴圖：交給工程師可以逐筆認領

目前以**驗收順序而非日曆工期**管理；沒有根據實測不填虛假的天數。代碼可以 parallel 做，但同一功能分支的修改不可互相覆蓋。不同人並行時，各自建立獨立 worktree，再以測試與 review 合併。

| Ticket | 階段/優先 | 依賴 | 最小交付物與可以關單的證據 | 預設狀態（本文建立時） |
| --- | --- | --- | --- | --- |
| GPUI-00 | G0 / P0 | 無 | 當前 HEAD＋83/83＋Windows full build＋實際 CI job 狀態 | **PARTIAL**：83 PASS；其他待驗 |
| GPUI-01A | G1a / P0 | 00 | 專用 601/5,000 行測資 manifest 與 SHA，能從 Edit 開檔 | **NOT RUN（GUI）** |
| GPUI-01B | G1a / P0 | 01A | 第 601／5,000 行跳轉、點字、drag/選取與 Copy 的 E2E | **NOT RUN** |
| GPUI-01C | G1a / P0 | 01B | 虛擬行 geometry / scroll/paint lifecycle 無 panic／stale frame | **NOT RUN** |
| GPUI-02A | G1b / P0 | 01B | 微軟注音 01～04：組字、取消、提交、一筆 Undo | **NOT RUN（實機）** |
| GPUI-02B | G1b / P0 | 02A | IME 候選窗於捲動／縮放／不同 DPI 的座標 E2E | **NOT RUN** |
| GPUI-03A | G1c / P0 | 01A | CRLF/UTF8/外部修改/只讀/丟失檔案安全 Save E2E | **NOT RUN（實機）** |
| GPUI-03B | G1c / P0 | 03A | OS 競態替換與 Crash、draft flush、復原/明文警示 | **NOT RUN（實機）** |
| GPUI-04A | G1d / P0 | 03A | 單 EditorSession 2 Workspace × 3 File 切換／Close 生命週期 | **PARTIAL**：模型 83 測試包含部分情境 |
| GPUI-04B | G3 / P1 | 04A | 多可編輯 Tab／Scratch／Save As，正確 dirty/undo/save target | **NOT IMPLEMENTED** |
| GPUI-05A | G2 / P1 | 01C | 真正 Windows GUI 測試 harness／錄影／崩潰證據 | **NOT IMPLEMENTED** |
| GPUI-05B | G2 / P1 | 05A | p50/p95 frame/latency、CPU/RSS 與 Flutter 比較，Rope ADR | **NOT RUN** |
| GPUI-10 | G3 / P1 | 03B,04A | Explorer/Search/Worktree 檔案識別與安全背景查詢 | **NOT IMPLEMENTED** |
| GPUI-11 | G3 / P1 | 10 | Syntax/LSP/Diagnostics 與版本化回覆 | **NOT IMPLEMENTED** |
| GPUI-12 | G3 / P1 | 10 | 唯讀 Diff/inline changes + 大檔虛擬化 UI | **NOT IMPLEMENTED** |
| GPUI-20 | G4 / P2 | 00 | 隔離 Host GUI VT smoke＋穩定唯讀 Session API | **PARTIAL**：模型已測、最新 GUI 未測 |
| GPUI-21 | G4 / P2 | 20 | Host ownership/Session/Lease/epoch RFC、拒絕測試 | **NOT IMPLEMENTED** |
| GPUI-22 | G4 / P2 | 21 | 有授權才能輸入/貼上/resize，真實 PTY E2E | **NOT IMPLEMENTED** |
| GPUI-30 | G5a / P3 | 10 | Git 唯讀 status/history/branch/worktree/diff、stale filtering | **NOT IMPLEMENTED** |
| GPUI-31 | G5a / P3 | 30,03B | Stage/branch/merge 等寫入操作，dry-run/確認/回復 | **NOT IMPLEMENTED** |
| GPUI-40 | G5b / P4 | 10,21 | Agent/MCP 查詢當前 workspace 與 granted project | **NOT IMPLEMENTED** |
| GPUI-41 | G5b / P4 | 40,03B | 有權限控制的 open/file/Git/Terminal 操作＋審計 | **NOT IMPLEMENTED** |
| GPUI-50 | G5c / P5 | 04B,22 | 多視窗/拖曳/Clipboard/系統焦點/IME/DPI | **NOT IMPLEMENTED** |
| GPUI-60 | G6 / P6 | 必要 01～50 Gate | Win/macOS/Linux E2E＋簽章＋更新回退演練 | **NOT RUN** |

### O.1 可並行但不能互相封關的工作

- **可並行**：P0 GUI 測資製作、純 Rust 新增 fail-closed model tests、研究 Windows Save replace API、整理 Flutter parity matrix、建立 Benchmark schema；各自在不同 worktree，避免共用 cargo target lock 實際互相干擾。
- **不能宣稱互相替代**：G1a GUI 行數測試 ≠ G1b 注音；Save 模型測試 ≠ G1c 實際 OS replace；Terminal list/read ≠ Terminal input Lease；同平台 compile ≠ 三平台 GUI。
- **未有安全 Gate 不實作寫入**：未達 `G4-06～09` 的授權契約，不做 Terminal write；未達 `G5a-05～08`，不做 destructive Git；未達 `G5b-03～07`，不放開 MCP mutation。
- **每張單停損條件**：測試失敗連續反覆、bug 涉及既有 Host 身分／檔案損毀／未知 write outcome，就停止擴充 scope，回到最小重現、保留資料、指定 owner review。不得用「忽略失敗、試著重啟」作為產品行為。

## P. Gate 驗收總表：功能／驗證證據分開標

| Gate | 入口條件 | 必要證據／出口 | 現在狀態 |
| --- | --- | --- | --- |
| G0：基線 | 乾淨 worktree，可取得 Rust toolchain | 當次 HEAD 的 headless lib suite、Windows full build、CI run 來源 | **PARTIAL**。HEAD `4b0e0e2c2` 的 headless 是 84 passed，check 與 build exit 0。遠端 Actions 仍是 **NOT RUN**。不要沿用較早的 83 passed。 |
| G1a：長檔案 | G0 可重現 | 601/5,000 行真實 GUI、選取、修改、Undo/Save、frame 指標 | **BLOCKED（GUI）**。隔離程序啟動兩次且 fixture 位元組未變，但沒有 Edit、Ctrl+Home、Ctrl+End、點擊、拖曳、複製、Undo 或 Save。Headless 可見行命中不是 GUI 驗收。 |
| G1b：繁中 IME | G1a 文字幾何正常 | IME-01～08 的 TSF/候選窗、DPI 實際證據 | **NOT RUN（GUI）** |
| G1c：Save/Draft | G1a 最小 Save 可操作 | Windows 替換競態／ACL／外部變更／Crash、hash 對照 | **NOT RUN（OS E2E）** |
| G1d：Session | G1c 明確失敗策略 | 雙 Workspace／三檔／關閉重啟／old reply 零誤寫 | **PARTIAL（headless）** |
| G2：自動化/效能 | G1 運作正確 | 真實 GUI CI、frame/RSS/p95 可重現比較 | **NOT RUN** |
| G3：完整 Editor | G1 + G2 | 多 Tab、Scratch、LSP/Diff/Search，parity matrix 驗收 | **NOT IMPLEMENTED** |
| G4：Host Terminal | Host ownership 安全設計 | 首先 Read-only VT GUI，再 grant/lease 寫入 E2E | **READ-ONLY ONLY** |
| G5：Git/Agent/OS | G1、安全 API | 功能 parity＋讀寫權限分層＋多平台 UI E2E | **NOT IMPLEMENTED** |
| G6：產品化 | G0～G5 必要需求全部有證據 | Win/macOS/Linux 真實 E2E、簽章/更新/回退、產品審查 | **BLOCKED** |

### P.1 一張 Ticket 的交付回報範本

```text
Ticket / Owner / Reviewer:
開始 HEAD / 工作目錄 / 分支:
本輪 scope（做）／non-goals（不做）:
變更檔案（含是否觸碰原 Host／Flutter）:
純模型測試：X passed / Y failed / Z ignored:
Windows cargo check/build + exit code:
macOS/Linux 實測：PASS/FAIL/BLOCKED/NOT RUN:
GUI 行為 case-ID + 原始截圖/錄影路徑:
IME/DPI 版本與觀察結果:
Save/Draft/Crash 原始與處理後 SHA256:
Runtime Host/PTY/Git/MCP 身分與權限邊界:
效能 p50/p95、RSS、CPU、樣本數（適用時）:
未通過／未測／安全風險與復原策略:
已提交 commit + Git status 是否乾淨:
是否 merge/push/release（未明確授權時固定 NO）:
下一張允許開始的 Ticket / 明確阻塞:
```

### P.2 真實資料與安全性紅線

- 絕不修改使用者正在使用的工作樹／SQLite RuntimeStore／PTY，包含「只是測試一下」的輸入、Kill、Journal 清除與過期 token 延長。
- 不允許假定 production Host 離線就安全，認證或 epoch 不確定時保持 fail-closed；保留已有的 Receipt、Journal／草稿。
- 不將草稿明文、Host transcripts、實際程式碼、敏感專案路徑上傳到公開測試 artifact。
- 任何 destructive Git/MCP 操作需明確授權、確認與審計；不由 LLM 或 GUI 單方面認定操作合法。
- 確認 `git diff --cached --name-only` 只含本張單，沒有混入其他人的 dirty 修改；提交後立即檢查 Git clean。
- **每次交接最新狀態必須重寫到真實檔案**，不要讓上個階段的「已通過」誤導下一位接手者。

## Q. 接手者今天應先執行的最小閉環

1. 重新確認 HEAD；不覆蓋其他人的修改。
2. 重跑 83 個 headless test（基準），再跑當次 HEAD Windows native full build；記錄 CLI 輸出。
3. 建立 `$env:TEMP\alera-gpui-acceptance` 的 5,000 行 fixture 與隔離 state，產生 SHA。
4. **真的啟動 GPUI 視窗** → Edit 5000-lines → Ctrl+Home／Ctrl+End → 點第 601／5,000 行 → Shift+click／drag → Copy → 修改／Undo／Redo → Save → 重啟核對原檔。
5. 任一案例 FAIL 就先完成最小修正與回歸；若 GUI 測試設備不可用就標 BLOCKED（不加入新功能冒充進度）。
6. G1a 通過後開始 G1b Windows 注音與 G1c Windows Save/Crash；有安全缺口則先鎖定其修復。
7. 每輪更新**本文件的 Ticket/Gate 表**，與 `docs/gpui-desktop-successor-handoff-20261107.md` 的風險登錄表同步；測試未通過前不合併或發布。

**最終決策**：現在應把專案視為「已有可編譯且模型驗證的 Windows GPUI 編輯器原型，尚欠實際操作及資料完整性驗收」，而不是正式可用的桌面版。**先完成 G0/G1，不要先進 G3～G6。**
