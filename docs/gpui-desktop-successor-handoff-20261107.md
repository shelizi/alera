# Alera GPUI 原生桌面版：接手交接與後續實作計畫

> **可直接照做的最新完整工作分解與驗收清單**：`docs/gpui-desktop-detailed-execution-plan-20261107.md`。本文件維持作為架構、風險登錄與歷史交接；請以新計畫的 G0–G6、83 項最新程式基線與分階段驗收規格安排後續實作。執行前一律重新核對 Git HEAD／工作樹；兩份文件都**不是**真實 GUI／注音／安全驗收已通過的證據。

> 更新基準：2026-11-07；本文件整理時最後一筆**功能程式**提交 `155fbdcee7dda00b038eccc9e3fb2e0a54d2e925`。接手時請先 `git log -1` 與 `git status`，以現場狀態為準。
>
> **狀態：EXPERIMENTAL / UNSUPPORTED。** 這是一個隔離的 Rust + GPUI 桌面重寫驗證分支，**不是**已可取代正式 Flutter 桌面版的產品。未取得驗收證據前，禁止合併到 `main`、宣稱支援、推送正式發布資產或替換目前使用者的安裝版本。

## 0. 給接手者的最短摘要（先讀）

1. **程式位置**：Repo 根目錄 `E:\Dropbox\work\alera\alera`；獨立 worktree `.worktrees/feat-gpui-desktop-20261107`；分支 `feat/gpui-desktop-20261107`。請在**該 worktree** 執行後續命令，不要在原本可能有未提交修改的 `main` 操作。
2. **現況**：可編譯的 Windows GPUI 原生視窗；Explorer／唯讀多 Tab 預覽／最近 Workspace；透過**既有** CLI 唯讀讀取 Runtime Host 與 Terminal 列表／VT 色彩；實驗性文字編輯器支援 IME API、Unicode 選取、Undo／Redo、顯式儲存與草稿復原。原生編輯器最近改為 `uniform_list` 虛擬化顯示，不再限前 600 行。**唯讀預覽**仍有 600 行限制。
3. **已取得的證據**：`cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`：**71/71 通過**；Windows `cargo check --all-targets`、`cargo build`、`cargo fmt --check`、Git 差異檢查成功。過往有隔離 Host／GUI 啟動存活測試；**最新虛擬列表功能未進行真實 GUI 點擊、IME 候選窗或大量捲動驗收**。
4. **下一個工作不是增加新面板**。請先做下述 **P0 驗收／資料安全**：跑真實 Windows UI，驗證第 601、5,000 行、滑鼠、捲動、繁中注音；找出並修正虛擬列表的座標／繪製問題；確認存檔與草稿不遺失資料。
5. **禁止繞過安全邊界**：Runtime Host／PTY 仍由既有 Rust Host 擁有；唯讀 CLI 每次必須帶 `terminal --no-start`；不可只因 CLI 支援 `terminal write` 就讓 GPUI 對任意現有 PTY 注入輸入。跨 Workspace／Tab/PTy 轉移遵守既有權限／Receipt／Lease 契約。
6. **提交方式**：一個可測試子任務一個 commit；提供受測平台、指令、輸出、尚未驗證項目。**不要為了「收尾」無限擴張需求**；先把 P0 用得動、資料不丟、測得過。

## 1. 權威入口、架構與當前範圍

### 1.1 閱讀順序

| 文件／程式 | 用途 |
| --- | --- |
| `docs/gpui-desktop-successor-handoff-20261107.md` | **本文件**：接手步驟、分工順序、驗收規格、風險 |
| `desktop_gpui/README.md` | 本地執行／建置／環境變數、已有功能、限制 |
| `docs/gpui-desktop-migration-20261107.md` | 既定 Phase 0–7 重寫願景及各次實際里程碑 |
| `desktop_gpui/src/main.rs` | GPUI Workbench 畫面／Editor 開關／Workspace／Terminal 輪詢 |
| `desktop_gpui/src/editor_ui.rs` | 編輯器 GUI、`EntityInputHandler`、`uniform_list`、`ShapedLine`、滑鼠／游標／IME |
| `desktop_gpui/src/editor.rs` | GPUI-free 編輯模型、`EditorLineIndex`、Unicode/Undo/Redo、顯式 Save、草稿 |
| `desktop_gpui/src/model.rs` | 唯讀 Explorer／預覽／多 Tab 狀態及其容量限制 |
| `desktop_gpui/src/session.rs` | GPUI 專用 Workspace／最近使用紀錄，**不是**產品 RuntimeStore |
| `desktop_gpui/src/runtime_bridge.rs`、`terminal.rs`、`vt_screen.rs` | 與既有 CLI 的唯讀 Host 邊界、Terminal 列表、VT 渲染 |
| `.github/workflows/gpui-desktop.yml` | GPUI 專用實驗性 CI；**不**發布資產 |
| `docs/terminal-tab-transfer-closure-20261030.md` | 原產品跨 Workspace PTY/Host 的失敗保守策略及各 Gate；非 GPUI 功能通行證 |
| `docs/t8e-p4-native-profile-handoff.md`、`docs/release-trust.md` | 原產品終端效能量測方法、正式簽章與更新信任模型 |

### 1.2 分層／所有權不能變

```text
GPUI 視圖（只負責呈現、焦點、事件與排版）
  ├─ Workbench / Explorer / virtual Editor / Terminal VT preview
  └─ feature controller / pure Rust model / bounded state
           ↓ 呼叫窄介面，不複製核心語義
      rust/alera-core + 現有 alera CLI
           ↓ 既有認證、版本與操作契約
      既有 Runtime Host / PTY / SQLite RuntimeStore / Git
```

- `desktop_gpui/` 是**獨立 Cargo workspace 與 lockfile**；不得讓 Flutter／mobile 或正式 CLI 因本實驗被迫依賴 GPUI。
- 不自己實作第二套 Host、PTY、Git 交易、MCP 授權、CredentialStore 或 SQLite 寫入。需要新能力時先定位 `alera-core`、既有 Host 協定與原產品測試。
- 所有使用者資料寫入（編輯器 Save、Workspace 切換、之後的 Git 操作）必須有明確操作／確認與失敗回復；只讀路徑絕不能隱性寫入。
- 編輯器狀態預設在 Windows `%APPDATA%\Alera\gpui-preview`（另參照 macOS Application Support、Linux XDG），測試必須使用絕對路徑 `ALERA_GPUI_STATE_DIR` 隔離，避免觸碰真實個人狀態。
- Host Session 清單目前是 **all workspaces**，不是針對目前專案授權／過濾的清單；GUI 不可將它誤標為只屬於當前 Workspace。

### 1.3 確認完成的功能與精確限制

| 子系統 | 已實作／可驗證 | 仍欠缺／不可宣稱 |
| --- | --- | --- |
| Native shell | Windows GPUI 視窗、Dark Workbench；獨立 crate；過往真實 Win32 啟動／視窗 handle smoke | macOS／Linux 原生完整編譯與 GUI；正式分發包 |
| Explorer／Workspace | 最多 400 筆目錄列舉、24 個**唯讀**預覽 Tab、最近 12 Workspace、持久化導覽、切換拒絕失敗時保留原狀 | 大樹／搜尋／Git Worktree 全面整合、完整桌面 Workspace parity |
| 唯讀預覽 | 256 KiB／最多 600 行；支援開檔與 reload | 沒有移除它的 600 行上限；請勿與**原生編輯器**混淆 |
| Native editor | `EditorDocument` UTF-8、256 KiB 上限、Unicode grapheme 與 UTF-16 IME API、Shift 選取、Undo/Redo（64 steps／4 MiB）、顯式 Save／來源衝突檢測、未儲存草稿復原；新 `EditorLineIndex`／`uniform_list` 可索引 5,000 行測資 | 並非 Rope、大型程式碼編輯器、LSP/語法、實際注音驗收；目前單一獨立 Editor Entity，非完整每 Tab 文件生命週期；無完整 soft wrap／BiDi |
| 鼠標／IME 視圖 | 使用 GPUI `ShapedLine` 座標；可用模型測試點選、反向跨行選取、候選範圍／可見裁切；拖曳邊緣 40ms／19px／最多 250 tick | **最新版本**未取得 UI 滑鼠事件路由、拖曳手感、IME 候選窗實機通過證據；長行／DPI 多螢幕未驗證 |
| Editor Save／Draft | Save 前 canonical path 與磁碟內容比對；檔案衝突 fail-closed；draft 暫存在分離目錄且原文比對；組字 preedit 不寫草稿 | 真實 Windows 覆蓋／權限／異動競爭保護需補 OS 層驗證；草稿**明文且同步寫 UI thread**；目前沒有完整 Save As／scratch UX |
| Terminal | 已存在 Host session 的唯讀列表，8 個本地預覽 Tab，1.5s active poll／15s list，48 KiB text log，vt100 80×24／240 行回捲、ANSI 16/256/RGB 樣式 | **沒有可互動 PTY 鍵入、選取複製、IME、resize／attach lease**；不擁有 Host |
| Git／Agent／MCP | 少量 branch 顯示與既有 Rust Core／Host 資源可重用 | 完整 Git Graph、diff／merge、Agent profiles、跨專案 MCP／權限 UI、CLI/IDE parity 均未遷移 |
| CI／Release | GPUI 實驗性 headless 跨平台測試 job、Windows build、CLI `--no-start`／隔離 Host 回歸設定 | 遠端 CI **尚未證實執行綠燈**；沒有跨平台封裝、簽署、更新器與預設切換授權 |

## 2. 接手環境與可重現命令

以下命令在 **repo 根目錄**執行。若已 `cd` 進 feature worktree，該 worktree 即為下列命令的工作目錄。

```powershell
# 進入獨立 worktree（不要切換或清除原 main 的 dirty 狀態）
cd 'E:\Dropbox\work\alera\alera\.worktrees\feat-gpui-desktop-20261107'
git branch --show-current
git log -3 --oneline
git status --short

# Windows：Rust 1.98；GPUI 0.2.2；需 Windows SDK shader compiler fxc.exe
# 工作檔與 build target 可放到 Dropbox 以外，避免熱編譯大量同步檔案
$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-model-target'
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline
Remove-Item Env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-target'
cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline
cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline
cargo fmt --manifest-path desktop_gpui/Cargo.toml --all -- --check
git diff --check
```

```powershell
# 每次 GUI 驗收都開獨立工作區／狀態，不連正式 Host
$testRoot = Join-Path $env:TEMP 'alera-gpui-handoff-smoke'
New-Item -ItemType Directory -Force $testRoot | Out-Null
$env:ALERA_GPUI_STATE_DIR = (Join-Path $testRoot 'gpui-state')
$env:ALERA_RUNTIME_DIR = (Join-Path $testRoot 'runtime')
$env:ALERA_CLI_PATH = 'Z:\nonexistent-alera-cli.exe'
& 'D:\alera-build\alera-gpui-target\debug\alera-gpui-desktop.exe' $testRoot
# 實際開關檔、IME、drag 測試請在隔離窗口完成；測完關閉測試程式，
# 僅在確認路徑屬於本次建立的 temp fixture 後清理。
```

- 上述 `D:` build 路徑是**既有這台機器使用的範例**；其他接手者可改成自己非同步磁碟上的絕對路徑。`--offline` 僅適合 crates 已快取的環境；乾淨 CI 第一次可去除。
- **真正的隔離 Host E2E**：先用 `cargo build --manifest-path rust/Cargo.toml -p alera-cli --bin alera --locked` 建立匹配的測試 CLI，將 `ALERA_GPUI_TEST_CLI` 指向該 CLI，然後執行 `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test live_runtime_host --locked -- --ignored --nocapture`。該測試應由程式自行建立／關閉獨立 Host；可選 `ALERA_GPUI_TEST_APP` 指向新 GPUI EXE 做啟動存活檢查。**不要指到真實使用中的 Runtime Host**。
- 保持測試 evidence：`git rev-parse HEAD`、Windows build exit code、71/71 基線、視窗截圖／操作錄影、UI 回歸、環境版本、失敗重現步驟。**後續改動需重跑，不能沿用 71/71 當新 HEAD 的證據**。

## 3. P0：第一批必做工作（先驗證再擴增，阻止資料遺失）

P0 的目標是「原生編輯器在單一 Windows 測試視窗**真的可用且不會誤儲存／丟失資料**」，而不是把全部 IDE 功能一次搬完。每個子項都應能獨立提交、獨立測試，若驗證不過先修正再往下走。

### P0-A：真實 Windows GUI 基線及虛擬列表（第一個交付件）

**代碼入口**：`desktop_gpui/src/editor_ui.rs` 的 `NativeEditor::render`、`render_row`、`EditorLineMetrics::paint`、`pointer_index_at`、`line_geometry`、`UniformListScrollHandle`；`editor.rs::EditorLineIndex`。

**待辦**

- [ ] 建立**不涉及任何使用者真實專案**的可重複 fixture：1 行空檔、中文＋emoji＋ZWJ＋combining、CRLF、尾端換行、連續空行、601 行、5,000 行（小於 256 KiB）、單行超長文字；另建立**超過 256 KiB** 的拒絕案例。
- [ ] 啟動隔離 GUI，實際點 **Edit**，證明視窗能繪製第 1／600／601／2,345／5,000 行，並能從檔尾 Ctrl+Home 回到第 1 行，再 Ctrl+End 返回。**記錄操作錄影或截圖＋行列狀態**。沒有視覺證據不能寫「GUI 已驗收」。
- [ ] 點第 601 行任意字元中間，游標應落在鄰近完整字素；Shift+click、拖曳向上／向下、反向跨行選取後 Copy，貼到隔離記事本核對**逐字節內容**，不可漏行、亂碼或切開 emoji。
- [ ] 驗證可見行繪製不造成頻繁 repaint、游標閃退、跳行、無限循環。特別查看 `uniform_list` 在 measure／prepaint／paint 期間的回呼順序與 `cx.processor`、`line_geometry.clear()`、`editor.update(cx, ...)` 的互動；**只有編譯成功不代表生命週期正確**。
- [ ] 測試垂直捲動（wheel／拖曳捲軸／Home-End）、縮放／視窗縮放後的可見範圍、游標跟隨、IME 候選範圍更新。捲動後不能使用上一 frame 的舊 `ShapedLine` 座標；不可將不可見行當作 mouse hit。
- [ ] 確認 `EditorLineIndex` 在輸入換行、刪多行、IME 提交、Undo／Redo、Recovery 草稿載入後更新正確；`line_range` 不可出現過期索引、panic、UTF-8 中間位元組範圍。
- [ ] 盤點空白行高度、選取換行的 `↵` 視覺標示、行內 `ShapedLine` 與實際 StyledText 字型／字寬相同。若出現量測差異先補測試與修錯，不要先加程式碼樣式功能。
- [ ] **效能驗收**：統計 frame 中建立／shape 的行數與總行數，確定 5,000 行只有可見行加合理 overscan 被 layout；保留 profiler 記錄。`EditorLineIndex::new` 目前仍在 render 重新掃描 ≤256 KiB 文字，若量測過高才改為增量快取。

**完成條件**：上述測資在真實 Windows GUI 跑完且有保留證據；沒有 crash／資料改寫／超界選取；Headless 回歸＋Windows native check/build 全綠，並註明 CPU/RSS／每 frame 行數。若 GUI 測試無法操作桌面，明確標 `BLOCKED: live GUI`，**不宣稱完成**。

### P0-B：繁中注音／鍵盤／IME 的實機驗收

**代碼入口**：`editor_ui.rs` 的 `EntityInputHandler` 實作（`selected_text_range`、`marked_text_range`、`replace_and_mark_text_in_range`、`unmark_text`、`bounds_for_range`、`character_index_for_point`）；`editor.rs` 的 `utf16_to_byte`、`byte_to_utf16`、`replace_and_mark`、`composition_start`。

**逐項測例**

| Case | 操作 | 通過條件 |
| --- | --- | --- |
| IME-01 | Windows 微軟注音：輸入「中文測試」並挑候選詞 | 不吞字、不重複提交，候選窗出現在**目前游標附近** |
| IME-02 | 組字中改候選、按 Esc 取消、Enter 提交 | 取消不汙染 Undo；確定提交**只形成 1 個 Undo 交易** |
| IME-03 | 在第 601 與第 5,000 行組字，邊組字邊捲動／改視窗大小 | 候選座標隨行移動；失去可見座標時採安全策略，不憑空亂定位 |
| IME-04 | emoji ZWJ、結合符號、全半形混合、跨行選取後貼上 | UTF-16 surrogate／UTF-8／grapheme 不錯位、不切半字 |
| IME-05 | 選取反向、Shift+方向鍵、Ctrl/Cmd+Z／Y、Home/End | TSF 回報的 `reversed` 與範圍對應真正 caret |
| IME-06 | 組字中切檔案／關閉／切 Workspace／程式異常結束 | 未提交 preedit 不寫草稿；不靜默覆蓋原檔；必要時明確阻擋操作 |
| IME-07 | 中英切換、快速貼上、輸入 1 分鐘後 Undo/Redo | 沒有卡 UI、文字亂序、候選窗脫離游標 |
| IME-08 | 多螢幕＋100%／125%／150% DPI，視窗跨螢幕 | 對應 DPI 的候選窗座標與選取範圍正確；未測平台標未驗 |

**不能跳過的區別**：Rust headless 模型驗證的 `replace_and_mark` 是資料層，不代表 Windows TSF 候選窗或 macOS 輸入法可用。`NativeInputCapture` 目前只有極小輸入代理範圍；如果 OS 依靠該 rectangle，須從真實 GUI 觀察後再設計正確的可見 caret anchor，避免只在純數字測試中假通過。

**完成條件**：至少 Windows 微軟注音 IME-01～07 逐例實測／記錄，候選窗座標經截圖核實；IME-08 的多 DPI 測試不能只靠單螢幕模擬。macOS／Linux 尚未驗證則標未測，不做跨平台支援宣稱。

### P0-C：存檔不遺失、草稿復原與衝突防護

**代碼入口**：`editor.rs::EditorDocument::save`／`apply_replace`／`EditorDraftStore::{persist,restore,remove}`、`editor_ui.rs::edit_result`、`main.rs::{open_editor,save_editor,close_editor,switch_workspace}`。

- [ ] **修正或證明 Windows Save 安全**：目前原檔比對 → 寫同目錄 temp → `fs::rename`。這不是對惡意／高併發修改的 OS 級 compare-and-swap；必須驗證 Windows 目的檔已存在、唯讀 ACL、原子取代語義、跨磁碟、符號連結／junction、硬連結、路徑在 check 與 rename 間變更等。不要因「本機測試通過」宣稱完全無競態。
- [ ] 建立明確的衝突策略：使用者外部修改磁碟檔、另個 Process 同時存檔、檔案被刪／改名、只讀、磁碟滿、temp 無權限、過程 crash。**拒絕覆蓋，保留原檔與未儲存 buffer／草稿**；可採 Win32 安全取代 API／更嚴格開檔 identity + 鎖策略，但需寫跨平台設計與失敗回復測試。
- [ ] 檔案編碼與原貌：CRLF／LF、最後空行、Unicode、Windows 語系與檔案許可權不能無聲更改；二進位／非 UTF-8／超限檔案應明確拒絕或改唯讀，不可截斷後寫回。
- [ ] 草稿隔離：連續重新啟動仍能復原**已提交**文字；組字中非 committed 文字不落盤；外部修改原檔後復原必須產生衝突；成功 Save／明確 Discard 才清草稿；失敗時不可消失。
- [ ] **明文風險**：目前 draft 內含未儲存全文和原磁碟快照，屬本地**明文**。至少 UI 實際告知使用者；產品化前決定敏感專案 opt-out、使用系統憑證保護或只存必要資料與權限控管，不可直接把測試草稿當安全儲存。
- [ ] 不要在 GUI frame 同步大量 `fs::write`：現行 draft persist 在 UI thread 可能造成輸入延遲，後續應設 debounce／單 writer queue／最後有效版本序號，保證退出可 flush 或明確錯誤；**不能為了非同步而丟最後一筆**。
- [ ] 資料驗收必須做 **Kill Process／Restart／Recover／Save**：用測試資料，不觸碰真實專案。不同崩潰位置對應可驗證原檔與草稿狀態。

**完成條件**：每種失敗都有明確預期、定向單元／檔案系統回歸與至少一次真正 Windows 失敗情境；安全語義由現有原產品遵循或獨立審查，並有「不丟資料／不偷覆蓋」證據。

### P0-D：編輯器與 Tab／Workspace 的生命週期

- [ ] `main.rs` 目前使用一個獨立 `NativeEditor` entity；`WorkbenchModel` 的多 Tab 仍主要是**唯讀預覽**。先明訂「每個可編輯 Tab 有自己的 buffer／focus／undo／draft」及檔案識別規則，再搬到多 Tab；暫時不支援時 UI 應拒絕切換，不要讓顯示檔與 Save 目標不同。
- [ ] 測「未儲存 → 切別的檔／切 Workspace／關 Tab／關程式／重啟」，確認 Save／Cancel／Discard 選擇、dirty 提示和狀態持久化。兩個 Workspace 的相同相對路徑必須隔離，避免錯復原。
- [ ] 對同一檔重複 Edit、rename/move、Explorer refresh、關閉後重新開啟、不同 Worktree 的同名檔，使用 canonical workspace 路徑／identity 判斷，不透過裸檔名合併。
- [ ] Terminal read/list 的舊 async response 在 Workspace 切換後不能寫入新 Workspace；維持現有 generation／identity 檢查。
- [ ] 未來多視窗別直接搬同一 entity 或 PTY；每個 View 的焦點、生命週期、寫入權限和最後擁有者必須定義明確。

**完成條件**：在測試用 2 個 Workspace、至少 3 個檔案中，分別做 save／cancel／discard／restart／rename，零誤寫、零遺失、零跨專案污染。

### P0-E：CI 與長檔案回歸的最小自動化

- [ ] 將 601／5,000 行 Unicode row-index、Undo/Redo、IME 組字交易、mouse grapheme snapping、draft／衝突測試納入每次修改的 headless job。
- [ ] 另建立真正的 Windows GPUI UI 測試路徑：至少有啟動／開檔／第 601 行點選／Ctrl+End／輸入／Save／重啟（用全隔離 fixture）。若現有桌面被占用，使用獨立測試 runner／虛擬桌面，不要注入到使用者工作中的視窗。
- [ ] UI harness 應記錄 PID、process exit、視窗 handle、截圖、測試日誌、Windows/DPI/IME 版本；逾時要終止**自己建立的 PID**，不可 `Stop-Process -Name` 殺所有 Alera。
- [ ] GitHub Workflow 目前有 headless Win/macOS/Linux、Windows UI compile、CLI no-start／隔離 Host jobs；**先確認遠端真的執行並通過**，再視需求補真正的 UI runner，不要用 Workflow 存在當成功證據。

**P0 關門標準**：P0-A～D 的 Windows 真實操作證據＋71 項既有回歸與新增測試全過；P0-E 至少有自動化可重跑入口。若尚有未修復資料完整性問題，不進 P1；若只缺 macOS／Linux，明確標註 Windows-only experimental，**不進正式發行**。

## 4. P1：編輯器可用性、容量與效能（P0 穩定後）

| 子項 | 實作方向／既有可復用元件 | 驗收條件 |
| --- | --- | --- |
| P1-1 編輯 Buffer | 目前 `String`＋最多 256 KiB，Undo 全文 snapshot；先 profile 再評估重用現有 Rust Rope／文件抽象，避免發明第二套 text-engine | 10k／100k 行測資可操作；記憶體與輸入延遲有量測；新增 Undo/Redo、Unicode、外部檔案變更回歸 |
| P1-2 增量索引／可見行快取 | `EditorLineIndex::new` 每次 render 掃描整份文字；定位真正瓶頸後採增量 newline index、render state 或 generation invalidation | 編輯任意位置、Undo/Redo、IME、換行、重開後沒有 stale row；未改文字不重建整份 index；profile 有前後比較 |
| P1-3 捲動與大文字行 | `uniform_list` 假定固定高度，現在 19px；軟換行／BiDi／超長行／水平捲動要另做 layout 策略 | caret／selection／IME overlay 與真實繪製吻合；無水平溢出隱藏無法編輯字元；極長單行不卡 UI |
| P1-4 程式碼功能 | 先以既有 Rust Core／LSP 能力擬定 syntax tokens、語意服務、搜尋、診斷、跳行與檔案開啟 API | 與原 Flutter 常用工作流的功能矩陣對照；不因重寫造成 LSP／Git 語義分叉 |
| P1-5 Scratch／Save As | 原 `EditorDocument::scratch` 存在但 GUI 沒有完整 File-new／Save As；可沿用既有檔案對話框／草稿契約 | 未命名檔關閉／重啟可回復；Save As 明確選路徑／檔名／衝突；測試「取消對話框不寫檔」 |
| P1-6 多文件／檔案 watcher | 每個可編輯 Tab 獨立 buffer＋dirty 狀態；檔案系統觀察與外部更新對話框 | 多工作樹／多專案正確；外部修改不在背景偷覆蓋；關閉／重啟一致 |

**效能驗收方式**：同一台機器、同一測資及相同 warm-cache 條件，量 p50／p95 input-to-paint、frame time、RSS、CPU、UTF-8/IME typing、10k／100k line scroll、長行、diff、終端重輸出；同時量 Flutter 正式版。**不先假設 GPUI 一定比 Flutter 快**。參考 `docs/gpui-desktop-migration-20261107.md` 的 Performance comparison 與 `docs/t8e-p4-native-profile-handoff.md` 的不同模式**獨立新程序**測法。性能目標與容許差要先在該設備基準量完後寫入，不憑空填數字。

## 5. P2～P6：接下來的大階段（嚴格依依賴順序；不在 P0 一次做完）

這些是**尚未完工的需求與建議步驟**，不是已取得的功能。優先先讓編輯器可以用，再解封其他子系統；對應舊 `gpui-desktop-migration-20261107.md` 的既定 Phase 1～7，不另創一套與舊計畫衝突的 Release Gate。

### P2：Runtime／Terminal 從唯讀預覽到受控互動

**目前基線**

- `runtime_bridge.rs` 只使用既有 `alera` CLI 執行有認證的唯讀 `terminal --no-start --json list/read` 等；CLI 不存在或 Host 離線時必須 fail-closed，不能替使用者啟 Host。
- `terminal.rs` 管理 Session 唯讀預覽、游標、重連／stale reply、最多 8 個本地預覽 Tab；`vt_screen.rs` 負責 VT100 解析、ANSI 色彩、alternate screen、游標。畫面 80×24 不是 Host PTY 真正 resize 結果。
- 既有 Host 擁有 PTY／存活狀態與 Session 身分，GPUI 不應另生 PTY 或模仿 Flutter/Host 的 Tab 轉移交易。

**分段工作**

1. **P2-1 唯讀品質先達標**：實際測彩色輸出、全形、ANSI 24-bit、alternate screen、scrollback、長時間進出工作區、多 Session 自動輪詢、Host 重啟。確認同一 Host session 更換身分／重用 handle 時舊資料拒絕套用；視覺與終端樣板截圖對照。
2. **P2-2 設計明確的互動授權／Lease 契約**：制定使用者對「哪個 Session／哪個 Workspace／哪個 View／哪個 Host epoch」授予**可輸入**權限，以及 Lease 過期、取消、斷線、重新附著、同時多 View 的 owner 判定；須和既有 Host 端一致。未設計好時 UI 仍只讀。
3. **P2-3 取得同一 Host 可驗證 Lease 後才送 Input／Resize**：重查 Session 身分及 Host generation；組字先留在 GUI，commit 後才把正確位元組送給 Host；paste 大量內容須上限與用戶可理解的確認；任何未知 Lease／對不上 ID 的情況拒絕寫入。
4. **P2-4 多視窗／跨 Workspace Tab 與分割**：重用既有 Host-owned tab/session/receipt 轉移，別用「關舊 PTY，再建立新 PTY」冒充無縫轉移。對原 PTY、Transfer Receipt、Journal／Lock 未知狀態一律保留證據，禁止隱式 Kill、Unlock、Remint。
5. **P2-5 效能／電源**：固定更新與大輸出流測 CPU/RSS、每秒輸出、顯示滯後、遮蔽／離屏暫停；若要持久化 parser snapshot，參照現有 T8e hard/soft eviction 已有契約，**不要讓 GPUI 另開一套不相容序列化格式**。

**Terminal 安全 Gate（P2-2、P2-3 必須先於任何鍵盤輸入）**：

- [ ] 明確使用者授權 + Session ID／Workspace／Host epoch／View/Lease 驗證，含超時與撤銷。
- [ ] 重連／丟封包／回覆不明、CLI 失效、Session 被刪／重用、Host restart 全部 fail-closed；絕不寫錯 PTY。
- [ ] 實機 Windows PTY smoke，macOS/Linux 個別原生 PTY／IME E2E 另驗；至少一個真實 hostile/replay 測試。
- [ ] **不透過** `terminal write` 當作省事的 GUI 互動方案；沒有上述證據前保持 read-only。

**跨 Workspace 限制的閱讀方法**：`docs/terminal-tab-transfer-closure-20261030.md` 包含歷史 Gate A/B/C 與最新能力協商說明，不能從舊 `supported=false` 的歷史記錄推定現行正式產品永遠不支援；同樣也不能因現行正式 Host 有能力就宣稱 **GPUI** 已支援。應以當次 Host capability／合約與該平台實機驗收為準。

### P3：Git／Diff／檔案系統工作流

1. 盤點現有 Flutter UX 行為：Explorer 搜尋／模糊開檔、跨 worktree、差異檢視、行內 diff、Git Graph、stage/unstage、分支切換、merge/conflict、Auto Fetch、收藏分支、右鍵選單與 locale。建立 parity checklist，標必需／可延後。
2. 先以**唯讀** Rust Core Git status、branch、worktree list、diff data 建立可測資料投影，不在 GPUI view 呼叫裸 `git` 或在 UI thread 操作大型 repo；避免並行重複查詢造成 502/503 或 UI 卡頓。
3. 引入寫入操作時明確區分 current branch 與非目前分支的 push/pull，顯示目標 repo/worktree、需要 dirty 檔案保護／確認；merge／reset／discard 禁止無提示執行。
4. 使用原 Core／既有 Host 的 Git 操作契約做 undo/rollback／衝突流程，不另實作不同的 merge conflict 語義；LLM 協助 merge 應要求 review 後確定提交。
5. 驗收 linked worktree／submodule／bare repo／nested repo／路徑空白、中文檔名、Windows symlink、Git LFS、deleted/renamed、二進位 diff、大型變更集；每個操作可重試、不誤寫另一 workspace。

**交付定義**：先讓唯讀 Explorer/Git/差異正確且順暢，再逐個開啟修改操作；任何 destructive 指令有二次確認、身份明確、回滾可理解。

### P4：Agents／Hooks／MCP／跨專案操作

1. 重用既有 Agent/Session/Hook 能力（Claude／Codex／其他 profile），先做唯讀列示與 session context，不在 GPUI 再建立第二份 Session 資料庫。
2. MCP 工作區管理必須可查「目前 Workspace／可用 Workspace／開檔與跳行／目標 Tab／使用的編輯器」，並可指定跨專案目標，但沒有 grant 的絕不越權。參照 `docs/alera-mcp-cross-workspace-grant-contract.md` 和 `docs/mcp-cross-project-navigation-20261104.md`。
3. Agent 動作先有「計畫 → 用戶可見確認 → 明確授權執行 → 結果可追蹤」；跨 Workspace、Terminal Input、原生 Host／PTY、外部程式操作不能只因模型要求就升權。
4. Hook 生命週期、Windows shell 路徑與無 Console 閃現要走既有 `alera-core::child_process`／ProcessRunner；跨 OS 實際啟動與終止測試必須分開。
5. 對 stale request、Agent 重啟、Session identity 變更、權限撤回、不同 root same filename 建立拒絕與事件紀錄測試。

**交付定義**：能力清單、授權流程、不可重播 mutation、失敗回復與跨專案 UI E2E 有證據；不可把 read-only 管理頁當完整 Agent/MCP 支援。

### P5：桌面 OS 互動與使用者體驗

- 多視窗／拆分／拖曳 Tab／Workspace（Windows、macOS、Linux 各自測），保持 Editor buffer、Terminal Lease、focus、IME 和 Clipboard 所有權。
- 檔案／資料夾拖入、拖出 OS、雙向複製貼上；Windows Explorer 與 macOS Finder 實測。關閉與異常退出不能遺失草稿。
- 可調主題／字體／行高、系統快捷鍵／Accessibility tree、高 DPI 與多螢幕座標、可鍵盤操作焦點、平台 Menu／Tray／Deep Link／單實例。
- File watcher、快捷搜尋、搜尋／Tab 顯示密度、Diff 行內換行等與既有 Alera 使用習慣維持一致。先做使用者最常用路徑，不為了「功能齊全」一次堆滿側欄。
- macOS Apple Silicon 與 Windows x64 原生編譯／啟動，Linux GUI（Vulkan/GTK 系統依賴）各做實機驗證；cross-compile 或 `cargo check` 不等於 GUI 已正常。

**交付定義**：主要 keyboard-first 流程、焦點、IME、Drag/Drop、多視窗 state 不混亂，沒有未告知的系統權限或背景資源占用。

### P6：可觀測性、升級、封裝及發行（最後，不要提前）

1. **量化相容性**：逐項列出正式 Flutter 桌面功能矩陣，標 GPUI 已完成/缺少/刻意不支援；除非產品決策授權，不要偷偷刪掉既有工作流。
2. **實際多平台 CI**：Headless 模型 + 真實 Windows/macOS/Linux build/launch/UI smoke；E2E 用測試專用 Host／PTY／工作樹；把 skipped/ignored 與失敗標明。
3. **效能與安全**：和正式版一對一比較 warm/cold startup、CPU/RSS、輸入延遲、捲動、Terminal 巨量輸出、Git 大庫、Crash/restart、資料保存；必要時作 profile 與安全審查。
4. **簽章與更新器**：保持原產品 `docs/release-trust.md` 的 Windows Authenticode、macOS Developer ID/Notarization、Linux package signing、Ed25519 schema v3 下載驗證與 Package Manager ownership 契約；GPUI 僅在明確批准後加入正式分發矩陣。
5. **灰度／回退策略**：先 opt-in internal/preview、分開狀態資料庫、顯式版本與診斷；支持恢復 Flutter、不碰使用者 Host／草稿；正式預設切換需產品負責人明確簽核。

**注意**：`.github/workflows/gpui-desktop.yml` 現在是 **experimental**，不得改成在 PR 自動發布 GPUI 版本，亦不得擅自修改 Flutter 正式 release workflow。

## 6. 推薦拆分的交付順序（每張工作單要能結案）

| 工作單 | 優先 | 前置 | 主要修改位置 | 交付證據／DoD |
| --- | --- | --- | --- | --- |
| GPUI-01：Windows 長檔案 GUI 運作 | P0 | 現有 `155fbdcee` | `editor_ui.rs`／測試 fixture | 第 601、5,000 行、點擊、scroll、Edit/Save 視窗錄影＋Crash-free＋測試 |
| GPUI-02：微軟注音及 IME 座標 | P0 | 01 基本繪製可用 | `editor_ui.rs`／`editor.rs` | IME-01～07 截圖／測試紀錄；組字 Undo 及草稿行為一致 |
| GPUI-03：Save／Draft／Crash 資料安全 | P0 | 01 可編輯 | `editor.rs`／`main.rs` | 外部磁碟修改／只讀／異常退出 E2E，不覆蓋、不遺失 |
| GPUI-04：Editor 多 Tab／Workspace 生命週期 | P0/P1 | 03 資料策略定案 | `main.rs`／`model.rs`／`session.rs` | 至少 3 文件、2 Workspace，Save/Cancel/Discard/Restart 零混用 |
| GPUI-05：長檔案效能／Rope 決策 | P1 | 01～03 完成 | `editor.rs`／index／view + Benchmark | 基線對照、100k line 試驗與增量索引／buffer 選型理由 |
| GPUI-06：互動 PTY 權限設計 | P2 | Host 合約審查 | `runtime_bridge.rs`／`terminal.rs`／Host | Lease／owner／epoch／撤權設計審查＋拒絕測試；未通過保持唯讀 |
| GPUI-07：Git 唯讀 status/diff/worktree | P3 | Runtime API 穩定 | `alera-core` adapter + GPUI UI | 大 repo、linked worktrees 的效能及一致性測試 |
| GPUI-08：Agents／MCP 可見操作及 grant | P4 | 明確權限 API | 既有 MCP/Agent contracts + view | 跨專案 grant/deny／stale-reply／無誤授權測試 |
| GPUI-09：平台整合與發行準備 | P5/P6 | 01～08 必要功能已驗收 | Native package/CI | Windows/macOS/Linux 真實 smoke、簽署／更新／回退 Gate |

建議接手者**先認領 GPUI-01**，把 Windows 實際 UI 證據拿到；不能通過則只修這張單的錯誤，不為了「更完整」同時開 P2～P6。可以平行做的是「純模型測試、資料準備、文件審查」等互不共用 worktree 的工作；若並行實作，請另建個別 feature worktree，最後在測試通過後依依賴順序整合。

## 7. 待追風險／已知限制登錄表（請按證據關閉）

下表是**目前需要驗證或修正的風險**，不是「每一列都已觀察到故障」；發現真實 bug 請另外開 issue 記錄重現步驟與 commit。

| 風險 ID | 嚴重度 | 事實／風險 | 解除條件 |
| --- | --- | --- | --- |
| R-01 | P0 | `uniform_list` 近期已移除 600 行限制，**只有純 Rust 行索引測試和 Windows compile/build**，沒有真正 UI 捲到第 5,000 行的證據 | GPUI-01 實機錄影／互動案例全過 |
| R-02 | P0 | 每次 `render`／列表 range callback 會更新 `line_geometry`；`EditorLineMetrics::paint` 又回寫 entity state；需檢查 layout/prepaint/paint 時序與 stale geometry | GUI 循環／memory/frame 計數；確認無舊座標、重繪風暴或 panic |
| R-03 | P0 | 微軟注音 TSF 的 candidate placement 在實機尚未驗證；`NativeInputCapture` 高度與實際 caret 錨點可能不同 | IME-01～08 視覺驗收，包含捲動、DPI、composition |
| R-04 | P0 | `save` 前比對原文後再 rename 的 **TOCTOU／Windows replace** 行為尚未達正式級別保證 | Windows atomic replace + race/permission/crash 測試，無 silent overwrite |
| R-05 | P0 | Draft 含**明文未儲存全文＋原檔快照**；同步 IO 會阻塞 UI；寫入失敗需使用者可見 | 安全產品策略、同步/非同步 durability 驗證、失敗提醒 |
| R-06 | P0 | 現在多 Tab 為唯讀 Preview，Native editor 尚非每 Tab buffer；切換、重開、Save target 有潛在錯配風險 | 2 Workspace × 多文件 lifecycle E2E |
| R-07 | P1 | 256 KiB cap、`String` 全文編輯與 snapshot Undo（64／4 MiB）；`EditorLineIndex` 每次 render 重掃 | 可重複的 10k／100k 行 profile、明確選擇 Rope/增量索引的證據 |
| R-08 | P1 | 每一行固定 19px，長行／CJK 字型 fallback／BiDi／soft wrap 沒有完整布局合約 | 多字型／長行／水平捲動與候選窗驗收 |
| R-09 | P2 | Terminal list/read 可以看到**所有 Workspace** Session；GUI 沒有輸入 Lease，不能因有 session handle 就任意寫入 | Session owner/Host epoch/Lease 授權合約與 E2E |
| R-10 | P2 | 1.5 秒的 CLI subprocess 輪詢不是長期低功耗的最終架構 | 實測 CPU/可用性、設計持久認證 client 和 stale response/permission 防護 |
| R-11 | P5/P6 | GPUI 的 macOS/Linux 原生 GUI、DPI、平台封裝與正式 CI 目前未驗證 | 各 OS 實機 build/start/UI 與包簽／更新回退驗收 |
| R-12 | P6 | 既有 Flutter desktop 功能遠多於 GPUI prototype，尚無可批准的 parity matrix | 核心用例一對一驗收、效能／安全回歸、產品負責人簽核 |

**禁止用來「關閉風險」的替代證據**：cargo compile ≠ 視覺測試；Rust model test ≠ TSF 候選窗測試；Host process 存活 ≠ PTY 輸入安全；歷史另一個 commit 的測試結果 ≠ 新 HEAD 通過；在 Windows 成功 ≠ macOS/Linux 支援。

## 8. 每次提交與總體驗收 Checkpoints（完成定義）

### 8.1 每張工作單的最低 DoD

- [ ] 需求與本張單範圍固定，不順手改其他子系統；如果需要改 Host 協定，獨立提設計和測試。
- [ ] 修改前能重現問題或有可量的 baseline；核心邏輯先寫 regression，再實作修正。
- [ ] `cargo fmt`、`cargo test --no-default-features --lib --locked`、Windows `cargo check --all-targets --locked`、`cargo build --locked`、`git diff --check` 成功。對應上層 CLI/Host 修改則另跑相應 Rust suite。
- [ ] 如果操作牽涉滑鼠／IME／focus／原生視窗，**必須有真實 GUI 操作證據**；不能單靠編譯、模型測試或暫存 screenshot。
- [ ] 驗證關閉、失敗、重試、跨 Workspace、外部修改、重啟及取消；若功能尚不存在標示不支援，不替使用者猜成功。
- [ ] 更新本交接文件和 `desktop_gpui/README.md`；注明 HEAD、測試指令、pass/fail/skipped、尚待驗證風險。
- [ ] Git 只 stage 相關檔；提交前查 `git diff --cached --check`、`git diff --cached --name-only`，提交後查 `git status --short`；**不要合併／push／release** 除非後續明確授權。

### 8.2 P0 驗收矩陣（每一格都要明確 PASS / FAIL / NOT RUN）

| 類別 | 代表案例 | 最新證據（GUI 與 headless 分開） |
| --- | --- | --- |
| 虛擬列表 | 0／1／600／601／5,000 行、空白行、最後換行、編輯、Undo、長行 | **NOT RUN**（真實視窗、滑鼠、捲動）。Headless library 已覆蓋空檔、601 行、5,000 行、連續空白行、結尾換行與 CRLF 的行界，且不切在 UTF-8 或 grapheme 中間。 |
| 輸入 | Unicode、emoji ZWJ、按鍵、貼上、跨行選取、反向拖曳 | **NOT RUN**（真實按鍵、貼上與滑鼠拖曳）。Headless library 已覆蓋 CJK、emoji ZWJ、組合字元，以及反向與跨行複製停在 grapheme 邊界。可見行範圍更換後，指標只使用目前列幾何；打在不在該範圍的列上不移動 caret、選取或文件位元組。 |
| 繁中 IME | 微軟注音候選窗、取消、提交、選取、DPI／捲動、異常退出 | **NOT RUN**（候選窗、DPI、捲動、TSF）。Headless library：取消不改 undo；候選更新加提交是一步；未提交 preedit 不寫入 draft。 |
| 資料安全 | Save 外部衝突、只讀、失敗清理、crash 後 draft、跨工作樹 | **NOT RUN**（最新 GUI 操作）。Headless library：外部修改、唯讀、路徑消失或改名、解析到工作區外、取代失敗都留下原檔與未存緩衝或 draft；成功存檔與明確丟棄才清 draft；兩個工作區根的相同相對路徑不互通；恢復儲存的狀態字串寫明是本機明文，不是安全儲存。 |
| Terminal | 唯讀畫面 VT ANSI、Host absent no-start、session stale/restart | **NOT RUN**（最新 GUI）。Headless library：list/read 使用 `--no-start`，沒有 write、input 或 resize；較舊的 workspace generation 與被換掉的 Host session 不會套用。本輪沒有重跑隔離 Host E2E。 |
| macOS／Linux | 正式編譯、視窗、IME、檔案系統、PTY | **NOT RUN** |
| CI | 實驗性 GitHub Actions 各 job 真正執行 | **NOT RUN**（本機 cargo 不是遠端 Actions） |
| 發行預設 | 把 GPUI 當成正式桌面產品 | **NOT RUN**。此前端仍是 experimental、unsupported。 |

本輪只把 P0 編輯器安全與單一工作階段生命週期放進 `desktop_gpui` 函式庫（`EditorSession`）。GPUI 視窗只呈現、取得焦點並轉送事件，不另開 Runtime Host、PTY、Git 寫入、憑證庫或 SQLite。覆蓋指令是 `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked`。真實滑鼠、注音候選窗、多 DPI、macOS／Linux GUI、遠端 CI，以及把 GPUI 設成預設產品，都仍是 **NOT RUN**。存檔仍是比對後 `rename`，不能當成惡意 TOCTOU 已證明安全。

### 8.3 正式功能預設切換之前不可略過的條件

1. **資料完整性**：Editor / Draft / Git / Terminal 變更不會靜默覆寫或誤丟資料；OS/Host identity 權威清楚，危險操作可取消／重試。
2. **使用者工作流 parity**：Editor、Explorer、Terminal、Git、Agent/MCP、Workspace、OS Integration 的「必需功能」對照現有 Flutter，全套可用或有產品負責人**書面批准**不支援的項目。
3. **跨平台證據**：Windows/macOS/Linux 的實機 GUI、IME、PTY、打包及 E2E、失敗注入；不能用同一 OS 的 cargo check 當替代。
4. **安全／效能／資源**：簽章與更新可信鏈、來源保存、Host 權限／Lease、CPU/RSS/latency 與 Flutter 比較有重複實驗數據。
5. **CI／發布**：無紅燈被忽略、沒有直接放寬測試或無理由更新 snapshot；正式 updater/manifest/簽章/installer 回退皆檢查後，才討論切換預設入口。
6. **產品決策**：目前 GPUI `supported=false`（**產品定位**，不是在說 Host 的 `tab.transfer.capabilities.supported` 一定為 false）；需明確人工 review／批准後才可 merge/release。

本節是**GPUI 產品化的檢核表**，不改寫既有跨 Workspace T2 的 Gate A/B/C 協定及最新版本狀態。

## 9. 接手回報格式（每輪貼在 PR／Issue 或本文件下方）

```text
工作單：GPUI-XX
Repo/Worktree：
Branch/開始 HEAD -> 結束 HEAD：
修改的檔案：
這輪明確完成（用戶可見行為）：
安全／權限影響與回復策略：
驗證平台、Windows/macOS/Linux 版本、DPI、IME：
  - cargo fmt --check：PASS/FAIL（完整命令）
  - cargo test：X passed / Y failed / Z ignored
  - cargo check/build：PASS/FAIL
  - isolated Host E2E：PASS/FAIL/NOT RUN
  - 真實 GUI 操作／截圖／錄影：PASS/FAIL/NOT RUN（附路徑／case）
殘留 bug、風險 ID 與對應下一步：
是否動到 main / 原 Host / 使用者狀態：否（若是，必須說明）
是否 push / merge / release：否（只有明確授權才變更）
Git worktree 是否 clean：
```

## 10. 實際接手的第一小步

**從 GPUI-01 開始**：請先跑 `git status --short` → 71 個 headless 基準測試 → Windows cargo check/build → 使用 `ALERA_GPUI_STATE_DIR` 的隔離資料建立 5,000 行 fixture → 開 GPUI 編輯器 → 實際跳第 5,000 行、滑鼠定位、修改／儲存／重啟。**如果真正 GUI 操作有問題，只修這條路徑的問題到通過，先不要加其他功能。**

通過後接 GPUI-02（注音候選窗）與 GPUI-03（存檔／Crash 安全），再決定要不要進 GPUI-04～09。每張單結案後更新本文件中的矩陣與風險狀態；不要把尚未執行的測試寫成 PASS。

**交接狀態**：文件撰寫完成；程式基準 `155fbdcee`，待接手者重新確認目前 HEAD。此文件**不包含、也不構成**推送、合併或發版授權。
