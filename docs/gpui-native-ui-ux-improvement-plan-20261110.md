# Alera GPUI Native UI/UX 實機複測與改善計畫（2026-11-10）

> 狀態：**待實作／待驗收（PLAN）**，不是功能完成宣告。
> 目標分支：`integrate/gpui-p0p1p2-20261108`。基準 HEAD：`05ab11257903d97ad945994594cb208e5f29497e`（2026-11-10 11:57 +08:00）。
> 產品標準：**Flutter Classic 的行為與操作路徑優先；Native 不需要像素完全一致，但不能為了實作方便而另造操作流程。**
> 規格來源：[`gpui-flutter-classic-ux-parity-20261108.md`](gpui-flutter-classic-ux-parity-20261108.md)；左側管理 [`gpui-left-project-worktree-parity-20261109.md`](gpui-left-project-worktree-parity-20261109.md)；既有總體 Gate [`gpui-desktop-detailed-execution-plan-20261107.md`](gpui-desktop-detailed-execution-plan-20261107.md)。

## 一、驗收基準及證據邊界

### 環境
- 作業系統：實際 Windows 桌面，顯示器實體解析度 3840 × 2160；實機畫面以 1600 × 900 JPEG 縮圖檢視。縮圖不能取代完整 DPI／字形驗證。
- 原始碼：既有 GPUI 整合 Worktree `.worktrees/integrate-gpui-p0p1p2-20261108`；未動 Classic `main`。
- 命令：在 `desktop_gpui` 執行 `cargo build --offline --locked --bin alera-gpui-desktop -j 2`，Cargo 回報成功；第二次 JSON compiler-artifact 顯示 Native 目標 `fresh=true`、產物 `D:/c/gpui/debug/alera-gpui-desktop.exe`。產物時間戳為 2026-11-10 11:56:41，**沒有內嵌 Commit 識別的獨立佐證**；後續要求啟動畫面／診斷資訊顯示編譯 HEAD，避免僅憑 Cargo fresh 判斷版本。
- 測試專案：獨立、可拋棄的 `.gpui-ux-retest-20261110`，含自己建立的 `.git`、`main`、`feature/ux-review`、README、繁中、長檔名、Modified 及 Untracked 檔案。
- 隔離設定：`ALERA_GPUI_STATE_DIR`、`ALERA_RUNTIME_DIR` 指向測試目錄；不啟動、不重設、不終止正式 Runtime Host／PTY；不操作實際工作樹的 Git 寫入指令。
- 證據口徑：以下「實測」只表示本輪 Windows GUI 觀察；「原始碼存在」或「舊版觀察」不可直接算作最新 GUI 驗收完成。

### 最新實測矩陣

| ID | 功能／情境 | 結果 | 實際發現與證據邊界 |
| --- | --- | --- | --- |
| QA-01 | 開啟最新 Native、Explorer 單擊預覽 | PASS | Windows GUI 可開啟；`README.md` 預覽顯示正確繁體中文。 |
| QA-02 | 預覽 `README.md` 後雙擊編輯 | **FAIL（UX）** | 出現兩個 `README.md` Tab，而不是把臨時預覽轉成／重用單一正式 Editor Tab。 |
| QA-03 | `Ctrl+P` 搜尋長檔名並開啟 | **PARTIAL** | Quick Open 篩選與長路徑列正常；開啟後出現兩個同名長檔名 Tab，需查是預覽＋Editor 並存還是重複文件實例。 |
| QA-04 | Editor 輸入、`Ctrl+S`、磁碟內容校驗 | PASS | 測試檔可新增 `LATEST_BUILD_UI_QA_20261110` 並保存，重新讀檔內容相符。 |
| QA-05 | 左側 Projects/Worktrees | **PARTIAL／改善** | 有常駐搜尋欄，隔離專案下清單較上一輪精簡；Host/CLI catalog 離線時顯示 `Project catalog unavailable: Alera CLI catalog query failed`，需改善離線提示，非直接定性為資料損壞。 |
| QA-06 | Tab 右鍵功能及釘選 | **PARTIAL** | 第一個 `README.md` 右鍵可見 `Pin Tab`，可觸發；其他 Tab 類型及跨窗格行為需逐一實測。 |
| QA-07 | 設定分類及本地化選項 | **PARTIAL／改善** | 最新版已呈現完整 Classic 分類及 System/EN/繁中/簡中/日文選項；選擇繁中後，多項設定內容仍顯示英文，需驗證真正應用語言與部分內容是否僅 catalog 預覽。 |
| QA-08 | 共用 Settings、雙列 Tab 開關 | BLOCKED | 測試時正式 Rust Host 未運作，欄位顯示 `Waiting for Rust Host settings`，`Two-row tabs` 顯示 `Not available in GPUI yet`；必須改以**隔離且相容**的 CLI/Host 重測 CAS、讀回與持久化，不能聲稱已失效或通過。 |
| QA-09 | 雙窗格、分割後標題／Tab | **PARTIAL（本輪未完整重測）** | 前一輪實機曾見窄視窗內容與頂部文字嚴重擁擠；本輪確認新版 Tab menu，但未取得可判定的完整分割＋復原證據。 |
| QA-10 | 右側 Git 檔案差異／Stage／Commit／Graph | **NOT RUN（最新 GUI 寫入路徑）** | 前輪舊版顯示狹窄側欄 patch 不易讀；新版尚未完成同一情境全流程測試。只能在獨立 Git fixture 動寫入。 |
| QA-11 | Terminal／Agent／IME 的 Host 實際操作 | BLOCKED | 隔離預覽使用 offline Host，不得以顯示 `Host offline` 取代 Terminal/Agent 或真實注音候選窗驗收。 |
| QA-12 | 重啟後版面、釘選、修改中的 Editor | NOT RUN | 需在可拋棄的狀態目錄進行二次啟動及身分持久化驗證。 |

### 驗證狀態原則
- **PASS**：操作及預期副作用可觀察，必要時含磁碟／Host 讀回。
- **FAIL**：實機重現且與 Classic 行為或操作直覺不同。
- **PARTIAL**：有可用操作但缺少其他條件，或只比對到部分介面。
- **BLOCKED**：需要未提供的相容隔離 Host、CLI 或真實 IME／環境。
- **NOT RUN**：尚未測試；不得因 Rust 單測、`cargo check` 或文件宣稱而變成 PASS。

## 二、優先改善工作包

### P0-01｜同一檔案的 Preview → Editor／Quick Open 不得產生意外的重複 Tab

**現況／重現**
1. 開啟隔離 Git 專案，Explorer 單擊 `README.md`。
2. 同檔案雙擊進入 Editor，出現兩個同名 Tab。
3. `Ctrl+P` 篩選 `long` 並選擇長檔名 Rust 檔案，也可看到兩個同名 Tab。

**設計決策**：同一 Pane 的單次開檔動作應重用現有 canonical file-backed Editor，並按 Classic 既有 Preview 提升語意轉換；臨時預覽不能錯當永久 Tab。不同 Pane 若明確要求同檔分割檢視，允許不同 view，但應遵守文件／Unsaved／Undo 權屬契約，不能默默建立兩個不一致的可編輯 buffer。先確認兩個 Tab 究竟分別屬於 Preview／Editor 何種 identity，再修最小根因；不可為了隱藏重複而任意關閉含未儲存內容的 Tab。

**預計檢查**：`desktop_gpui/src/main.rs`、`editor_tabs.rs`、`editor_session.rs`、`workbench_tabs.rs`、`workbench_panes.rs`、`quick_open_ui.rs`（最後一項依實際存在確認）。

**驗收 Gate**
- Explorer 預覽 → 同檔雙擊、`Ctrl+P` 重開、同檔再次開啟各循環 10 次；**同 Pane 不新增意外重複 Tab**。
- 臨時預覽點其他檔案會被安全替換；已固定／編輯／Dirty 的內容不被覆蓋。
- 各 Pane 明確開同檔後切換、移動、儲存、Undo、關閉、重啟仍保留正確身份與資料。
- 加入檔案 ID／Pane ID／Preview Promotion 的 Rust 回歸測試，並實際 Windows 滑鼠驗收。

### P0-02｜分割視窗與 Tab 釘選語意對齊 Classic

**現況**：本輪 Pin Tab 有可操作入口，但雙列設定與 Host 讀回未驗證；前輪窄視窗曾見長標題、兩個 Pane 的 Editor 狀態文字擁擠。

**改善**：每 Pane 一組獨立 Mixed Tab Strip（Terminal／Agent／Editor）；作用於該 Pane 的右鍵移動、關閉其他／右側、拖曳與釘選；一般 Tab 不得排到釘選 Tab 之前；兩列模式只有「設定啟用＋該 Pane 有釘選項」才出現。視窗狹窄時採省略、Tooltip、可捲動及合理最小寬度，不讓一 Pane 覆蓋另一 Pane 的按鈕或文字。

**預計檢查**：`workbench_tabs.rs`、`workbench_panes.rs`、`native_host_pins_ui.rs`、`host_tab_pins.rs`、`main.rs`、`workbench_layout_store.rs`、`native_shared_settings_ui.rs`。

**驗收 Gate**
- 1200×800 與 800×600（另跑系統 100%／150%／200% DPI）分別操作 Split／Unsplit，沒有重疊、失焦或消失的必要控制。
- 在左／右 Pane 各建立 Editor＋Terminal（需隔離 Host）並移動、關閉、重新開啟；操作只影響正確 Pane。
- Pin／Unpin、Close Others／Close Right、拖曳跨 Pane、重啟復原，均核對 Tab identity；含 Host 工作階段不因 Native Tab 關閉而被誤終止。
- Compatible Host/CLI 下驗證 Classic／GPUI 雙向 `tab.pin.cas` 與 `editor.twoRowTabs` 讀回；**不使用正式 Runtime Host**。

### P0-03｜設定畫面「列出分類」與「設定生效」必須分開驗收

**現況**：分類與語言切換器已存在，但離線時多個欄位為等待讀取或不可用；不能把分類齊全等同功能已完成。

**改善**：設定分成可用（可讀／可寫且讀回成功）、唯讀預覽（明示原因）、Host 離線（重試／連線提示）、尚未實作（禁止假按鈕）。Classic 與 GPUI 共用項的唯一真相應在 Rust Core／Host，Frontend 只維護 UI 個別狀態；CAS 版本衝突要告知並可重新載入，不以 Native JSON 偷代 Host 成功。语言切換要實際套用到選單／欄位／狀態，不只是下拉選擇器。

**預計檢查**：`native_settings_catalog_ui.rs`、`native_settings_ui.rs`、`native_settings.rs`、`native_shared_settings_ui.rs`、`settings_menu_catalog.rs`、`runtime_bridge.rs`、Rust Core `desktop_settings_contract`。

**驗收 Gate**
- 有相容隔離 Host 的四種語系及 Classic/GPUI 各改一次 → Host 讀回 → 雙窗同步 → 重啟復原，含競態 CAS 測試。
- Host 不存在時，顯示「尚未連線／不可修改」而非永遠旋轉；不可假成功；有可見重試途徑。
- 15 個分類逐一記錄「已接實際功能／僅預覽／缺少」，不可把全部列為完成。

### P1-01｜Editor 工具資訊分層，提升畫面有效內容比例

**問題**：`Native UTF-8 text editor (experimental)`、Undo/Redo 數字、草稿安全提示等持續佔用頂部多行，窄畫面特別凌亂。

**改善**：將檔名與關鍵行列／Dirty／儲存狀態縮成可辨識的單列工具資訊；詳情／非阻斷診斷改至狀態列、Tooltip 或明確的資訊面板。**草稿僅本機明文**屬實質風險，不得直接刪除提醒，應改成適當位置且仍能被使用者查到。Find/Replace 按需展開，不應擠壓第二窗格。

**驗收**：同一文字檔在 1200×800、800×600 比較 Classic/Native 編輯首屏可見行數；無不合理換行、控件重疊；Save／Undo／Redo、草稿提示仍可用。

### P1-02｜左側專案／Worktree 搜尋、離線回退及資訊密度

**現況**：新版已新增 Search projects / worktrees 且隔離專案清單更緊湊；但 Runtime catalog 不可用時在主要區域直接顯示技術性錯誤。

**改善**：保持 Classic 分組、釘選、最近項目與 Worktree 辨識；錯誤訊息區分 Host 離線、CLI 舊版、授權失敗、空列表，保留 local preview，提供可見但不遮擋清單的重新整理／診斷。沒有資料不等於 `Detached HEAD` 清單，截斷名稱需 Tooltip／完整路徑可取用。

**驗收**：0／1／20+ Worktree，長路徑、巢狀分組、搜尋、收合、跨 Workspace 切換；Host 離線→恢復後列表與當前選取一致。不得自動移除或修剪 Worktree。

### P1-03｜Git 差異預覽與 Classic 檔案操作路徑

**問題來源**：舊版實機的窄側欄 Raw Patch 不易閱讀；本輪最新版尚無完整 Stage／Commit GUI 實測。

**改善**：右側維持 Compact Changes／Staged／Untracked 樹與操作；點檔案時以**中央可讀 Diff View**（或可切換完整檢視）呈現，而非強迫在窄 sidebar 閱讀 Raw Patch。保持 Explorer 相同搜尋與檔案選取語意，明確標示 Git worktree root、操作對象、未追蹤檔案的安全刪除與提交預覽；不可用「Refresh」假裝操作成功。

**預計檢查**：`git_actions.rs`、`git_history.rs`、`git_inspect.rs`、`main.rs`，Classic `workspace_git_diff_panel_*.dart`。

**驗收**：**僅隔離 Git repo** 實測 Modified／Untracked 篩選、差異、Stage／Unstage、提交訊息預覽、取消、Refresh、Git Graph 進出。拒絕 root 身分不符時在按下變更按鈕**之前**提示，並保持檔案／Git index 不變。不得對使用者 Alera 工作樹提交。

### P1-04｜長檔名、Tab 與 Quick Open 呈現

**現況**：`Ctrl+P` 可找到 `src/a_very_long_filename_for_explorer_quickopen_tab_layout_validation.rs`；Tab 標題因寬度而截斷。

**改善**：長檔名自動省略，但懸浮可見完整檔名與相對路徑；不同檔名同名時以路徑辨識；選中 Tab 易辨識且 close/dirty/pin 標記不重疊。Quick Open 清單依路徑分層顯示，避免超長名稱蓋掉 keyboard focus 指示。

**驗收**：10、50、100 個 Tab＋極長檔名、單／雙 Pane、Quick Open 鍵盤上下選擇，無視覺重疊或誤關閉；實際 Windows 點擊與快捷鍵驗收。

### P1-05｜輸入法、焦點與多 DPI 的正常使用路徑

在 Explorer filter、Quick Open、Editor、兩側 Terminal 逐一驗收 Windows 注音的組字、選字、焦點切換與取消。當輸入法組字中，快捷鍵不得搶走候選窗或把未確認文字送入另一個 Pane。可另以小畫家等 OS 控制組驗證自動化滑鼠路由，**不能把桌面自動化注入失敗當成產品缺陷**。

### P2｜完整 Classic 進階對齊
- Git Graph 進階操作、跨分支比較／合併、Worktree 衝突、外部編輯器、OS 拖放、完整 Agent Profile 啟動／附加與 MCP 定向多窗。
- 重啟時兩窗格、釘選、dirty draft、搜尋條件與選取狀態的正確復原。
- macOS/Linux 的字形、IME、快捷鍵與原生視窗互動（另建平台 Gate）。

## 三、執行順序與分工邊界

| 順序 | 工作包 | 最小交付 | 拒絕條件 |
| --- | --- | --- | --- |
| 1 | P0-01 Preview/Editor 身分 | 先新增重現測試，再改最小 Tab/Session 路由並 GUI 重測 | 丟失 Dirty、創建兩個互相矛盾的 buffer |
| 2 | P0-02 Pane／Pin／Two-row | 每 Pane 佈局正確＋隔離 Host Pin CAS 實測 | 把 Host session 當可隨意關閉的 Native Tab |
| 3 | P0-03 共用設定 | Host 相容版本、讀寫 CAS／離線 UI／語言真正套用 | 假成功、把 Host 欄位偷偷寫入 Native 私有設定 |
| 4 | P1-01、04 編輯器與 Tab 視覺 | 精簡 chrome、長檔名適配、保存與 Undo 不中斷 | 刪除安全警告、按鈕不可見或不可點 |
| 5 | P1-02 專案／Worktree | 搜尋分組、狀態提示、離線回退 | 錯把 CLI 離線當刪除 Workspace |
| 6 | P1-03 Git | 可讀 Diff、完整安全操作 GUI smoke | 未隔離實際 Git 寫入、誤刪 Untracked |
| 7 | P1-05、P2 | 真 Windows IME、多 DPI、Host/Agent/Graph 進階覆蓋 | 只靠 model test 或螢幕截圖宣告完成 |

### 每張工作單的完成定義
1. 記錄實際 `HEAD`、初始 `git status --short` 與修改檔案白名單；保留其他人 WIP／原始 smoke fixture。
2. 先 RED（重現）、再 GREEN（專項 Rust 測試）、最後 Windows EXE Build＋GUI pointer/keyboard。不得以 Cargo 成功代替 GUI。
3. 測試至少涵蓋正常操作、Workspace 切換、重啟、取消、錯誤／離線與雙窗格；任何 Host 操作都使用單獨臨時 Runtime。
4. 記錄 `PASS / FAIL / BLOCKED / NOT RUN`、Windows／DPI／語言、程序 PID、測試 EXE 的 source HEAD、重現步驟、前後畫面與是否有磁碟／Host 讀回。
5. 不動 Flutter Classic `main`、正式 Runtime Host／PTY；無使用者明確要求不合併 `main`、不 push、不 tag、不 release；`supported=false` 直到正式 Gate 關閉。
6. 不修改安全限制來消除錯誤；跨 Workspace、Host Tab 世代、Git worktree root 身分失配都必須 fail closed。

## 四、下一輪實際驗收清單

- [ ] QA-02：同檔預覽／開啟／Quick Open 在同 Pane 去重；Dirty 安全。
- [ ] QA-03：長檔名 Tab／Quick Open 視覺及點擊目標。
- [ ] QA-06：Editor、Terminal、Agent Tab 右鍵 Pin／Unpin，Close Others／Right，左右獨立 Pane。
- [ ] QA-08：隔離相容 Runtime Host 下 `editor.twoRowTabs`、`tab.pin.cas`、Classic/GPUI 同步與離線恢復。
- [ ] QA-09：1200×800、800×600 分割／復原及 100/150/200% DPI。
- [ ] QA-10：隔離 Git repo 中 Diff／Stage／Unstage／Commit／Graph／Refresh，無對 Alera repo 寫入。
- [ ] QA-11：隔離 Host Terminal/Agent 與 Windows 注音真實候選窗／焦點路由。
- [ ] QA-12：儲存、重啟、草稿恢復、外部檔案衝突、雙 Pane 持久化。
- [ ] 實際與 Classic 相同操作逐項比對，紀錄結果及未測項目。

## 五、狀態與維護

- **目前完成**：完成此份可執行改善方案；已取得新版 Windows GUI 的 Explorer／Editor／Quick Open／Settings／Pin Tab 初步證據，以及 `Ctrl+S` 寫入後讀檔驗證。
- **目前未完成**：下列未關閉的產品工作包、隔離 Host 交叉驗收、最新 Git 寫入路徑與正式發行 Gate。部分已通過的項目見第六節，不得把局部改善當成完整功能對齊。
- 每修一項，在此文件更新工作包狀態、實測時間、修正 Commit、對應測試／畫面；不要把「預計」改寫為「已完成」而無證據。

## 六、SWE-2 Flash Max 平行開發與整合紀錄（2026-11-10）

實作基準：`6ed0b5f80`；本輪產品驗證 HEAD：`48090ecd6`。四個開發工作樹相互隔離，僅將驗證通過的局部修正 Cherry-pick 至 `integrate/gpui-p0p1p2-20261108`。**這不是合併 Flutter Classic `main` 或正式 Release。**

| 工作線 | 執行方式與來源 Commit | 整合 Commit | 狀態／限制 |
| --- | --- | --- | --- |
| P0-01 Preview／Editor 去重 | SWE-2 Flash Max：`d3f4d9b5c`，修改 `main.rs`、`model.rs`、`session.rs` | `84516fd80` | 模型／儲存回歸及 Windows 單 Pane GUI 主要重現案例通過；雙 Pane Dirty／釘選仍待驗收 |
| P0-03 Settings Host 離線提示 | SWE-2 Flash Max：`e325ff9d9`，修改 `native_settings_catalog_ui.rs` | `1f0f78a31` | 區分 Loading／Unavailable／Ready，沒有新增獨立設定儲存；與真實相容 Host 同步及四語系仍未驗收 |
| P1-02 Projects Catalog 降級與重試 | 側欄 Flash Max 工作線未產生可用變更，由整合協調流程在獨立 Worktree 補寫 `207cf2a65`、視覺修正 `8f1b015af` | `b4ee2e6e6`、`48090ecd6` | 本地項目保留，精簡離線標籤／Details／Retry；窄窗版面與 Details 點擊已 GUI 驗證 |
| P1-03 Git Diff 擴展閱讀 | Flash Max 曾進行分析但未產生可驗證程式差異 | 無 | **NOT DONE**，不得宣稱 Git Diff 已對齊 Classic |

### 合併版本測試證據

- Windows Native `cargo build --offline --locked --bin alera-gpui-desktop -j 2` 成功，產物為 `D:/c/gpui/debug/alera-gpui-desktop.exe`，在 `48090ecd6` 重新編譯。
- `cargo test --quiet --offline --locked --no-default-features --lib -j 2`：**370 passed / 0 failed**（基線 368）。
- `cargo test --quiet --offline --locked --bin alera-gpui-desktop -j 2`：**8 passed / 0 failed**（設定 Host 狀態 2 項專項測試通過）。
- Windows GUI 使用獨立暫存資料與刻意不存在的 CLI：`README.md` 單擊預覽後雙擊編輯，**只留下 1 個同名 Tab**；`Ctrl+P` 選取長檔名檔案後總共 **2 個 Tab**；再次選取相同檔案仍維持 2 個，顯示重用既有 Editor。
- 同一個隔離 Workspace 重啟後，狀態列確認 `Restored 2 editable file tab(s) into GPUI panes`，GUI 沒有重複 Tab。
- Host 離線時設定頁實際顯示 `Rust Host settings unavailable`，而不是永遠 `Waiting for Rust Host settings`。窄側欄新版顯示 `Classic unavailable`、`Details`、刷新圖示；Details 會將 CLI 不可用的原因顯示於狀態列。
- 本輪測試環境使用的暫存 QA 資料夾已在 GUI 關閉後移除；原本 `.gpui-smoke-20261108/` 與 `desktop_gpui/tests/__pycache__/` 均未清理或寫入。正式 Runtime Host／PTY 與 Classic `main` 不變。

### 未關閉的驗收風險

- **P0-01** 尚需跨兩 Pane 的同檔檢視、Dirty／Undo／釘選、Save As 覆蓋原預覽及競態保護實機驗證，才能把整個工作包打勾。
- **P0-02／P0-03** 尚需相容隔離 Host／CLI 下實際 `tab.pin.cas`、`editor.twoRowTabs` 與 Classic/GPUI 雙向設定 CAS 同步；Host 離線時不可把提示改良等同於可保存設定。
- **P1-01／P1-03／P1-04** 的 Editor chrome 精簡、中央可閱讀 Git Diff、完整長檔名與高密度 Tab 視覺仍未完成。
- 真 Windows IME／DPI、Git Stage／Commit／Graph、Terminal／Agent 與多作業系統發行 Gate **未包含在本輪完成清單**；`supported=false` 維持。

## 七、第二輪平行作業：Git Diff 擴展閱讀與 Dirty 跨 Pane 防護（2026-11-10）

本輪以 `8a370d56f` 為基線，獨立建立 `feat/gpui-git-expanded-diff-flashmax-20261110b` 與 `feat/gpui-dirty-pane-guard-flashmax-20261110b` 兩個 Worktree，同時啟動 Devin `swe-2-flash-max`。兩個 Agent 完成原始碼探索，但在受限執行期限內**未交付可整合的修改**；下列實際產品修正與測試由整合協調流程在隔離分支完成，不能誤記為 Agent 產生的程式碼。

| 工作包 | 來源提交 | 整合提交 | 本輪實際結果 |
| --- | --- | --- | --- |
| P1-03 Git Diff 安全回歸 | `cb282bb42` | `dc85e20e6` | 新增 staged patch 唯讀、HEAD／Index／檔案不變、超大 untracked 限制與越界拒絕測試；3/3 專項測試通過 |
| P1-03 Git Diff 擴展預覽 | `b1276512e` | `537684336` | 在側欄 Patch 區增加 `Expand diff`，以 GPUI Kit Dialog 顯示寬 760px、最高 430px 的可捲動等寬字體唯讀快照；不可在載入中或空資料時開啟；1/1 Binary 專項測試通過 |
| P0-01 Dirty Editor 路徑別名安全 | `465dd573c` | `57da7edba` | 修正已滿 24 個 Tab 的第二 Pane 遇到 `sub/../file` 路徑別名時誤報容量滿的缺陷；需比對 canonical workspace root，保留原 Instance、Dirty、Undo 與磁碟內容；新增跨 Workspace 拒絕測試 |

### RED／GREEN 實證與最後整合測試

- 對 Dirty Editor 的 `full_secondary_pane_reopens_moved_dirty_file_by_canonical_alias_without_duplication` 測試先在未修正程式**失敗**：`Editor tab limit reached`。完成 canonical file/root 比對後，同一測試**通過**；第二個不同 Workspace root 的拒絕測試也通過。
- 整合 Commit `537684336`：Native Windows Build 成功，Library **372/372**、Binary **9/9**。
- 最終產品程式碼 Commit `57da7edba`：`cargo build --offline --locked --bin alera-gpui-desktop -j 2` 成功，Library **374/374**、Binary **9/9**，均為 0 failed。編譯僅回報 Windows linker stdout 訊息。
- 本次建置 EXE 為 `D:/c/gpui/debug/alera-gpui-desktop.exe`（2026-11-10 16:48 本機），對應產品原始碼 `57da7edba`；後續僅文件提交不需要重新編譯。
- **GUI BLOCKED / NOT RUN**：本次工具工作區政策阻擋 `Start-Process`，訊息為「workspace scope 禁止通过子进程写入 Workspace 外部路径」。未嘗試放寬政策；不能宣稱新的 Expand Dialog 經 Windows 滑鼠實測。前一輪的單 Pane Tab GUI 驗收仍保留，不可拿來代替新功能的本輪實測。
- 本輪建立的隔離 Git QA 資料夾 `.worktrees/gpui-git-expanded-diff-flashmax-20261110b/.gpui-git-diff-qa-20261110/` 尚保留（untracked），因安全政策拒絕遞迴刪除巢狀 `.git`；另有 `%TEMP%/alera-gpui-gitdiff-qa-20261110b` 暫存資料。兩者均為測試資料，不包含在任何整合提交，**需由具授權的維護者確認後清理**。
- Classic `main`、正式 Runtime Host／PTY、Git Stage／Unstage／Commit 的生產路徑、既有 smoke 資料均未修改；未 push、未 Release。

### 下一階段 Gate（尚未通過）

- P1-03：使用本輪新 Build 在真 Windows GUI 開啟 Git Diff 放大視窗，檢查長 Patch、橫向捲動、關閉、Workspace 切換時的唯讀快照；更完整的中央 Diff、雙欄高亮仍未實作。
- P0-01：實際雙 Pane 開檔／Dirty／Undo／Save As、Pinned Tab 與關閉操作須測過，不能以 Library Test 單獨認定完整驗收。
- P0-02／P0-03：隔離相容 Host 下 `tab.pin.cas`、`editor.twoRowTabs` 及 Classic／GPUI 雙向同步尚未完成。
- P1-05：Windows 注音真候選窗、800×600 及多 DPI 和其他操作一致性測試仍待執行。**`supported=false` 維持。**

## 八、第三輪續作：Host 斷線設定 Fail-closed 與跨 Pane Pin 測試（2026-11-10）

延續上一輪的「共用設定與釘選／雙列 Tab」未完成項目。由驗證基線 `a3e02f622` 建立兩條獨立 Worktree：`fix/gpui-settings-stale-host-20261110` 與 `test/gpui-pinned-tab-flashmax-20261110`。後者使用 Devin `swe-2-flash-max` 進行釘選 Tab 測試分析，但因非互動式工具確認遭拒，**未產生可用程式差異**，已停止該 Agent；實際修正與測試均由整合協調流程完成，不將結果誤記為 Agent 實作。

| 內容 | 來源 Commit | GPUI 整合 Commit | 實際完成與界線 |
| --- | --- | --- | --- |
| P0-03 Host 離線／快取設定保護 | `16ce682b9` | `32f6c01bc` | 當 Rust Host 刷新失敗，即使曾成功載入過 snapshot，也不將舊值顯示為可編輯；刷新時先顯示 Loading，斷線時顯示 Unavailable，拒絕使用舊快取送出 CAS；App Language 選項同步停用，設定錯誤提示新增 Retry |
| P0-02 跨 Pane 釘選保護回歸 | `8eeda2c15` | `9ecbfaa24` | 新增兩項 Rust 測試：各 Pane Close Others／Close Right 保留釘選、跨 Pane 不誤關且順序正確；釘選 Editor 移動到第二 Pane、儲存重啟與關閉後過期 Pin 清理 |

### 驗證結果

- 獨立設定分支 `native_settings_catalog_ui::tests` 2/2 通過；Native Binary 9/9 通過。
- 獨立釘選測試分支 Rust Library **376/376** 通過（相較上一輪 374，新增兩項）。
- 最終整合產品程式碼 HEAD `9ecbfaa24`，`cargo build --offline --locked --bin alera-gpui-desktop -j 2` 通過，完整 Library **376/376**、Binary **9/9**，全部 0 failed。
- Windows EXE 路徑 `D:/c/gpui/debug/alera-gpui-desktop.exe`，建置時間 2026-11-10 16:56（台灣本機）。後續文件提交不會改變該產品程式碼。
- **真 Windows GUI／隔離 Host 驗收仍未執行**：未再繞過前輪工作區對 Native 子程序啟動的安全限制；僅能宣告 Build／Rust 測試通過。沒有驗證 Classic／GPUI 雙向 CAS、`tab.pin.cas`、`editor.twoRowTabs` 的隔離 Host 認證同步、實際兩列 Tab 視覺與 800×600／多 DPI。
- Classic `main`、正式 Runtime Host／PTY、既有 smoke fixture 不變；未 push／merge `main`／release，`supported=false` 保持。

### 下一步

1. 在已授權且隔離的 Windows GUI／Runtime Host 環境，測試 Settings：先連線→斷線→控制項停用→按 Retry→Host 恢復→重新可編輯與 CAS 讀回。
2. 在 Classic／GPUI 分別切換 `editor.twoRowTabs` 與 Pin，確認左右兩 Pane 獨立釘選列、釘選不被 Close Others／Right 誤關、重啟持久化；完成後才可結案 P0-02。
3. 補完 Git Diff 放大視窗 GUI 點擊、捲動、Workspace 切換，以及 Editor Dirty／Undo／IME 實機驗收。

## 九、第四輪續作：Git Diff 可讀性與截斷警示（2026-11-10）

以第三輪最後的 `b6a77c888` 為基線，建立互不干擾的 `fix/gpui-git-diff-bounds-20261110` 與 `feat/gpui-git-diff-readable-ui-20261110` Worktree。本輪安排 SWE-2 Flash Max 在 UI 工作樹平行調查；它只完成程式閱讀，未產生可整合的修改，因非互動權限要求而停止；兩項修正均由整合協調流程在獨立工作樹實作，並經完整驗證後 Cherry-pick。

| 工作包 | 原始提交 | 整合提交 | 完成內容 |
| --- | --- | --- | --- |
| P1-03 已追蹤 Patch 截斷警示 | `e26a7c4a7` | `a06ca076e` | 修正 32 KiB 預覽剛好抵達上限時，ASCII Patch 可能被無提示截斷；統一以明確 `… diff preview truncated` 提醒，限制 UTF-8 邊界，維持 HEAD／Index／原檔唯讀 |
| P1-03 展開 Diff 可讀性 | `5ee383cf9` | `1399a18fc` | Kit 唯讀視窗依顯示寬高自適應（小視窗與 4K）；顯示已載入快照中的檔案／Hunk／新增／刪除統計，前 400 行使用不同顏色，剩餘內容仍以純文字保留，並明確告知上色上限 |

### TDD 與整合驗證

- `oversized_tracked_ascii_patch_explicitly_reports_truncation_without_writes` 測試修正前 **RED：0/1 failed**，錯誤為沒有截斷警告；修正後 **GREEN：1/1 passed**。另測試大段繁體中文及 Emoji 截斷，確認合法 UTF-8、明確提醒、輸出長度受限且原檔不變。
- `git_inspect::tests` **5/5** 全部通過；完整獨立工作樹 Rust Library **378/378** 通過。
- `native_action_dialog_ui::git_preview_tests` **3/3** 通過，驗證 800×600／3840×2160 寬高邊界、Unified Diff 行數統計不把 `+++`／`---` 算成變更、上下文／新增／刪除／Hunk 色彩不同。
- 最終整合產品程式碼 `1399a18fc`：Windows `cargo build --offline --locked --bin alera-gpui-desktop -j 2` 成功，`cargo test --quiet --offline --locked --no-default-features --lib -j 2` **378/378 passed**、`cargo test --quiet --offline --locked --bin alera-gpui-desktop -j 2` **11/11 passed**，全部 0 failed；只有資訊性 Windows linker stdout 警告。
- Windows 建置產物 `D:/c/gpui/debug/alera-gpui-desktop.exe`，本機時間 2026-11-10 17:05；文件更新不影響已編譯產品。
- **GUI NOT RUN：** 此輪沒有取得上輪被工作區安全政策限制的 Windows GUI 執行權限。以上「響應式 800×600」為尺寸純函式測試，不是實際滑鼠／顯示縮放驗收。仍需測試真實視窗內換頁、捲動、Ctrl+C、對話框關閉及長行橫向閱讀。
- 此版本仍**不是中央完整雙欄 Diff**，上限仍為 32 KiB 的唯讀 Patch 快照；不能用統計及視覺改善宣稱超過上限的內容已全部讀取。Host／PTY、Classic `main`、原有 smoke 暫存資料不變，未 Push／Release，`supported=false` 維持。

### 尚待解決的 Gate

1. 取得授權的隔離 Windows GUI 執行環境，實測 Git Diff 小尺寸與高 DPI 的換行／橫捲、完整關閉，以及切換 Workspace 後只顯示原快照；確認不修改 Git。
2. 規劃安全、完整的中央檔案差異（可針對單檔分頁擷取／分塊載入，嚴格限制記憶體與 Git 路徑），不可移除容量限制而只把整段 Patch 放進畫面。
3. 回到 P0-01／02／03：實機雙 Pane Dirty／Undo／釘選與 `tab.pin.cas`／`editor.twoRowTabs` 的 Classic／GPUI 共用 Host 同步；在驗證前 `supported=false`。

## 十、第五輪續作：大型 Git 檔案差異的安全分頁（2026-11-10）

以 `73c5e7a27` 為基準，建立獨立 Worktree `feat/gpui-git-diff-paged-20261110`，在不變更 Classic 與正式 Runtime Host 的前提下，改善上輪「超過 32 KiB 只能看到截斷快照」的限制。**此功能是單檔唯讀 Patch 分頁，並非已完成中央雙欄完整 Diff。**

| 項目 | 來源 Commit | 整合 Commit | 完成內容 |
| --- | --- | --- | --- |
| P1-03 Git 單檔 Patch 分頁 | `9b57c2bcf` | `062b5aca4` | `git_inspect.rs` 新增 `read_file_diff_page`：單頁保留最多 32 KiB 的 Git Patch，依 Git Patch 記錄行游標讀取下一頁，支援已追蹤檔案的 Staged／Unstaged 狀態，不把完整大型差異一次存在記憶體 |
| P1-03 GPUI 分頁操作 | 同上 | 同上 | Git 側欄選取變更檔案後顯示 `Previous`／`Next` 及目前頁數；最多 32 頁並限制掃描 100,000 筆 Patch 記錄；既有 Expand Dialog 可放大目前選取的單頁快照 |
| P1-03 容量／編碼邊界 | 同上 | 同上 | 巨大單行只顯示受限片段並標記 clipped；Untracked 檔案恰好 32 KiB 加上標頭後，依然截斷到安全範圍且明確標記，不會無聲超出頁面上限 |

### 實測證據與重要限制

- **Git 9/9 專項案例通過**，包含大型繁中／Emoji 已追蹤 Patch 分頁後仍可讀到最後一行、已暫存 2,000 行的 Patch 分頁且 HEAD／Index／原檔不變、上頁重讀、超長單行片段提示、越界路徑與最大行游標拒絕。
- Untracked 檔案剛好 32 KiB（另外有顯示標頭）時，修正前 RED 測試失敗：沒有截斷提示；修正後 GREEN。檔案內容保持不變。
- 整合產品程式碼 `062b5aca4`：Windows Native `cargo build --offline --locked --bin alera-gpui-desktop -j 2` 成功，Rust Library **382/382** 與 Native Binary **11/11** 通過，均 0 failed。EXE：`D:/c/gpui/debug/alera-gpui-desktop.exe`，本機建置時間 2026-11-10 17:15。
- 每頁均**重新讀取目前 Git 工作樹**，不會跨頁拼接成宣稱一致的單一不可變快照；檔案在翻頁間修改時應重新點選檔案從第一頁閱讀。頁數與掃描上限屬防護機制，**不能宣稱超過上限的差異已完整顯示**。
- `Git History`、分支 Compare、MCP 比較與 Workspace 切換會清除檔案分頁狀態，並使用既有的 request ticket／workspace generation 守衛拒絕過期回應；分頁不附加任何 Stage／Commit／寫入功能。
- **Windows GUI NOT RUN**：前輪 Native 啟動曾因 Coding MCP 工作區安全限制被拒。本輪完成 Windows Build 和 Rust 測試，但沒有獲得獨立 GUI／隔離 Host 的核准驗收；因此不能宣稱實體滑鼠切頁、狹窄側欄排版、放大視窗切頁及大視窗橫捲已通過。
- Classic `main`、正式 Host／PTY、既有 Smoke 資料、Git 寫入行為不變；未 Push、未合併 `main`、未部署或發行，`supported=false` 維持。

### 下一階段

1. 在有正式授權的隔離 Windows 桌面實測 Previous／Next／Expand 的互動，確認窄側欄、翻頁後重新開啟 Expand、已暫存與未暫存狀態切換及 Workspace 改變不會造成舊頁混入。
2. 評估真正可視化中央 Diff（選取檔案後中央編輯區顯示左右差異、來源行號與 Hunk 導航），使用有界緩衝區或分頁讀取，並建立重繪／極長檔案效能測試。
3. 繼續驗收 `editor.twoRowTabs`、`tab.pin.cas` Classic／GPUI 雙向 Host 同步、雙 Pane Dirty／Undo、Windows 注音與高 DPI；通過前不可將 `supported` 設為 true。

## 十一、第六輪續作：放大 Diff 視窗內直接翻頁（2026-11-10）

由上一輪最後驗證的 `9fe39bfad` 建立獨立 `feat/gpui-git-dialog-paging-20261110` Worktree。本輪先處理放大 Git Diff 視窗只有靜態單頁的操作落差；未修改 Patch 分頁讀取引擎、Classic UI 或正式 Host。

| 工作包 | 原始提交 | 整合提交 | 實作 |
| --- | --- | --- | --- |
| P1-03 Kit 放大 Diff 的 Previous／Next | `5d5e2b6a4` | `29c24887b` | 保留原本 32 KiB 單頁上限，Kit 放大視窗直接呈現目前檔案頁面及可用的 Previous／Next，背景讀取時先停用控制。讀取完成後只替換目前頁面；更換頁面也使用不同滾動區身分 |
| P1-03 放大視窗來源驗證 | 同上 | 同上 | 以 `workspace_generation`、檔案相對路徑及新的 `source_ticket` 鎖定對話框；同檔案重新選取也換新票證。舊視窗在切換檔案、Workspace 或 Git History／Compare 後只顯示來源已變更提示，不再對新來源送出翻頁請求；按鈕 callback 再做一次條件查核 |
| P1-03 比較來源分離 | 同上 | 同上 | Git History／分支 Compare 仍在 Kit Dialog 顯示開啟當下的唯讀快照，不誤套檔案分頁選擇；檔案分頁標記為 `Read-only current page` 與 `live Git`，避免被視為跨頁不可變快照 |

### 驗證與尚未通過的 Gate

- 獨立 Worktree：`cargo check --offline --locked --bin alera-gpui-desktop -j 2` 成功；增加一項來源識別測試，證明同檔案翻頁保留原視窗身分，而變更 Workspace generation、切換檔案、清空 paging 或重新選取相同檔名後均拒絕操作。Native Kit Dialog 專項 **4/4** 通過。
- 完整獨立分支 Windows Native Build 通過，Library **382/382**、Binary **12/12** 通過；Rustfmt scoped check 及 `git diff --check` 通過。
- 整合產品程式碼 `29c24887b` 重新執行 Windows Native Build、Library **382/382**、Binary **12/12**，全數 0 failed；本機 EXE `D:/c/gpui/debug/alera-gpui-desktop.exe`，2026-11-10 約 17:27。
- **Windows GUI NOT RUN**：沒有取得新授權可啟動隔離 Windows GPUI 桌面程式，故對話框真正的滑鼠 Previous／Next、非同步刷新、捲動位置、800×600／高 DPI、來源失效顯示仍需實機驗證。Rust 編譯／測試不可代替 UI 視覺驗收。
- 32 KiB／頁、32 頁及 100,000 Patch 記錄掃描限制均保留。原始資料每頁以目前 Git 工作樹重新讀取，不應被誤認為同一不可變 Git 快照。中央雙欄完整 Diff 尚未實作，仍是 `P1-03 PARTIAL`。
- 未推送、未部署、未合併 Classic `main`、未更動正式 Runtime Host／PTY 或已存在的 smoke/cache 資料，`supported=false` 不變。

### 後續優先項目

1. 在經核准的隔離 Windows GUI 中開啟大於 32 KiB 的已暫存及未暫存 Patch：在放大 Kit Dialog 直接翻頁、返回上一頁、切換選取檔案／Workspace、關閉視窗，核對內容及 Git Index 完全不變。
2. 下一輪可改進中心區可讀性，例如含行號／Hunk 跳轉的中央唯讀檔案 Diff，應保留嚴格的每頁資源限制並另做長行效能測試。
3. 回到 Classic／GPUI 共同設定與 Tab Pin CAS：隔離 Host 的 `editor.twoRowTabs` 與 `tab.pin.cas` 雙向持久化、雙 Pane Dirty／Undo、Windows 注音候選窗與多 DPI 實機驗收。

## 十二、第七輪續作：Git Diff 雙側行號與 Hunk 快速定位（2026-11-10）

本輪由驗證基線 `67701ed73` 建立獨立 Worktree `feat/gpui-git-hunk-line-ux-20261110`，只修改 Native GPUI 的 `desktop_gpui/src/native_action_dialog_ui.rs`。既有 32 KiB／頁、32 頁、100,000 筆 Git Patch 記錄安全限制與 Git Index/HEAD 唯讀行為未變。

| 工作包 | 來源提交 | GPUI 整合提交 | 實作內容 |
| --- | --- | --- | --- |
| P1-03 Git Unified Diff 雙側行號 | `800d327d0` | `44f91e448` | 放大 Kit Dialog 對每頁前 400 行，依可見 `@@ -old[,count] +new[,count] @@` 標頭追蹤原檔／修改後行號；新增行僅遞增新檔、刪除行僅遞增原檔、Context 同時遞增；`+++`／`---`／`No newline` 標記不計算。跨檔案、合併式 `@@@` 或頁首沒有 `@@` 時，行號留白，不猜測不存在的前頁座標 |
| P1-03 Kit Hunk 快速定位 | 同上 | 同上 | 前 400 個已獨立渲染的行建立真正可捲動的 ScrollHandle 目標，最多 12 個 `-old / +new` 按鈕，點擊跳至對應 Hunk 標頭，不按估算像素定位；超出 400 行的內容仍以原始純文字顯示，不額外生成大量 UI 元件 |

### Rust 與 Windows 整合驗證

- 新增 4 項 Native Unit 回歸：多段 Hunk old/new 行號、續頁／`@@@`／無效標頭不造假、400 行／12 個按鈕跳轉索引上限、多檔 Patch 不沿用上一檔座標。Git Dialog 專項 **8/8** 通過。
- 獨立 Worktree `cargo check --offline --locked --bin alera-gpui-desktop -j 2`、Windows Native `cargo build` 通過，Library **382/382**、Binary **16/16**，均 0 failed。
- 整合產品程式碼 `44f91e448`：重新執行 `cargo build --offline --locked --bin alera-gpui-desktop -j 2` 成功，Rust Library **382/382**、Binary **16/16** 皆通過；只出現既有 Windows linker stdout 資訊警告。EXE：`D:/c/gpui/debug/alera-gpui-desktop.exe`，2026-11-10 約 17:37。
- **GUI NOT RUN**：目前未獲工作區政策核准啟動隔離 Windows GPUI 圖形程式；`ScrollHandle` 的真實滑鼠點擊、橫向捲動、Hunk 對齊、長行／多 DPI／800×600 表現及動態分頁刷新，仍不能由編譯或單元測試代替。此輪只能確認原生編譯與行號／Hunk 定位邏輯。
- **限制：**行號與 Hunk 快捷鍵只涵蓋當前頁前 400 行，最多 12 個按鈕；更後面的文字照常可讀，但尚無該區塊的快速定位和行號。對續頁缺少 Header 的 Diff 不會偽造行號。中央雙欄完整 Diff 尚未完成，`P1-03 PARTIAL`、`supported=false` 維持。
- 沒有修改 Classic `main`、正式 Runtime Host／PTY、既有 smoke/cache 資料；未 Push、未部署、未發行。

### 下一輪優先項目

1. 在授權隔離 Windows GUI 環境，驗證放大 Kit Diff 的前 400 行雙側行號、Hunk 定位、Previous/Next 頁切換、狹窄視窗與高 DPI 的實際表現。
2. 評估獨立中央唯讀 Diff Pane 的完整行號／Hunk 虛擬化，保留有界每頁資料及事件取消防護，並讓未顯示的遠端頁可以安全導覽。
3. 驗證 Classic／GPUI 共用設定和 `tab.pin.cas`／`editor.twoRowTabs` Host 同步、跨 Pane Dirty／Undo、Windows 注音輸入。全部 Gate 通過前不調整 `supported=false`。
