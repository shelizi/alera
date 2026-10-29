# Alera 功能優先續做：第八批四線隔離（2026-10-29）

## 最新：T2-D2 Host／Client／跨視窗 Watcher 三線已整合（2026-10-30）

以乾淨整合 `0b1f9f441` 為基準，重用三個沒有未提交變更的 D1 worktree，切出 D2 獨立分支平行修正後提交：

| 範圍 | 已落地的實際安全措施 | 整合提交 |
| --- | --- | --- |
| Rust Host | `terminate`／`createOrAttach`／`restart` 驗證移動後的 Session Workspace/Tab，防止舊窗關閉或錯誤 remint；帶收據 Tab 的 `tab.remove` 要求目前 `expectedWorkspaceId`；拒絕後不取消 Agent title | **`df69aece0`**（隔離 `f215ce93a`） |
| Dart Client | PTY terminate 帶來源檢視的預期身分；Workspace-scoped Tab Close 及相同身分的 timeout replay；Xterm 舊 Handle 同視窗重用時更新顯示 Workspace，不更動執行路徑／PTY | **`817fcf7e1`**（隔離 `faab3f691`） |
| Flutter Workbench | 其他視窗的 Tab watcher 檢測 Terminal 轉移收據，抑制舊版 provisional Layout 背景寫入，從 Host 重取雙側權威 Layout；epoch 阻止過期回覆回寫 UI | **`333afb30e`**（隔離 `2f0914e41`） |

此輪補齊了**部分**跨視窗移動、關閉與附加的安全界線，但 **`tab.transfer.capabilities.supported:false` 持續停用跨專案拖放**。下一輪宜補 Runtime Host 端全域 Layout 版本檢查、不同程序的 PTY lease／Close/Detach race，以及 crash/restart 後 PTY+Tab owner 不一致時的保守復原。**仍未執行 Flutter/Cargo tests、Analyze、Check、Build、實機驗證**；只有格式化及 Git staged whitespace check，無 push／release；原有 dirty `main` 不變。

## 最新：T2-D1 平行 Host／Recovery／Undo 三線已整合（2026-10-29）

本批基準 `97f21ab02e23cb7eba39791080dedd7f52bd10ae`；三線獨立 worktree／Git commits 並整合：

| 線路 | 新增的功能／風險處理 | 整合提交 |
| --- | --- | --- |
| Rust Host | `tab.transfer.status` 以精確 mutation receipt 與**當下活 PTY owner**雙重證明逾時轉移；擋進行中 Workspace Mutation／Shutdown；原移轉收據、原啟動路徑在 `tab.upsert` 保留，阻擋 Terminal 直接以 `tab.upsert` 跨 Workspace 移動；移動後通知 Agent Presence；保存原 Pane Group | **`43591a4be`**（隔離 `c1f8aa185`） |
| Dart recovery | Host timeout／disconnect 後不重送，向 `tab.transfer.status` 查原 Mutation＋活 Session；無法證實則顯示 `TerminalTransferOutcomeUnknownException`，不把未知當成功或失敗 | **`5a0f01bdd`**（隔離 `c340bc6fb`） |
| Flutter Undo | 正向轉移後保留準確 receipt ID，Sidebar 顯示可點 Undo；反向轉移前確認原 Workspace 和收據仍有效，盡量放回原 Pane Group，不關閉／重建 PTY | **`efce4bea6`**（隔離 `8ddc98a97`） |

**功能仍停用：Host capability `supported:false`，使用者跨專案 Drop 不接受。** 這批是生命週期安全防護及失聯確認／Undo 的原始碼成果，仍未驗證跨程序／視窗 Close、Detach、Output、重啟路徑、事件序列與相關回歸。依使用者指示只有格式化及 staged Git whitespace check，沒有 Flutter/Cargo Tests、Analyzer、Build、實機，**不可推送／發布為已支援功能**。原本 dirty `main` 未動，整合進 `refactor/tdd-integrate-20261029`。完整交接見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

## 最新：T2-B2 Host／Workbench／Terminal UX 平行三線已整合（2026-10-29）

| 線路 | 實際新增功能／安全措施 | 整合提交 |
| --- | --- | --- |
| Rust Host | Local `tab.transfer`：驗證活 Session、Tab、Pulse／Automation／Orchestration 狀態，呼叫已有的雙布局 CAS，提交後重指派活 Session 顯示 Workspace 而不終止 PTY；Claude／Codex 原 Hook Workspace 可由 `terminalExecutionWorkspaceId` 驗證後關聯至新畫面；來源／目的雙側事件；保存原 Terminal 啟動路徑 | **`f3977fa99`**（隔離 `a9985034a`） |
| Flutter Workbench | Host capability probe 與 provider、Controller transfer 操作、來源／目的 watcher fence、成功後讀回雙側 Tabs/Layout 並**只套用不反寫**、選擇目的 Tab；Sidebar callback 有實際連通路徑但仍由能力旗標關閉 | **`8443c2f0e`**（隔離 `7d99dc6a5`） |
| Terminal UX | 跨 Workspace 移動後 Tab 顯示原 Execution Workspace 與原 Shell 啟動路徑提醒；若 PTY 重新建立，使用保留原本的 launch dir 與 Hook Workspace 身分，而非自動切換為目的 Git repo | **`683b99dc8`**（隔離 `359353a10`） |

**最重要的真實狀態：產品仍未開啟跨專案拖曳。** Rust Host 雖已有實驗性 `tab.transfer` RPC，`tab.transfer.capabilities` 仍明確回傳 `supported:false, livePtyTransfer:false, atomicLayouts:true`；新 UI 與 Workbench Controller 在此狀態下拒絕 Drop。嚴格未驗證項目包含 Session close/detach、跨程序 watcher 順序與本機 PTY handle、Agent Presence 即時移轉、意外 timeout/斷線的權威讀回、Undo、原啟動目錄不存在時的 Restart，以及 Windows/macOS 實機。下批應先處理這些 T2-D1 功能邊界並取得明確可用性再開旗標。詳 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

本輪僅 `dart format`、`rustfmt`、`git diff --cached --check`，**未執行測試、Analyzer、Cargo check、Build 或發布**；原 `main` dirty worktree 未操作、未 push。

## 最新：T2 三條平行實作基礎已整合（2026-10-29）

已從整合 `061216941` 切出三個獨立 worktree，完成 scoped source commits 並 cherry-pick 至 `refactor/tdd-integrate-20261029`：

| 工作線 | 交付內容 | 整合提交 |
| --- | --- | --- |
| Flutter T2-A | Terminal-only 跨 Workspace DragTarget（Workspace 側欄／Project 標題）、同 Host 檢查及多 Workspace 目的地選擇器；**缺 Host capability 時 callback=null，實際拖放拒絕** | `8c119f0a1`（隔離 `42c28ea1e`） |
| Dart T2-C0 | `WorkbenchTerminalTabTransferCoordinator`、Host Port：雙側 Layout snapshot、expected/new CAS payload、Tab／Session ID 保留驗證、同一 Tab in-flight 鎖定；**尚未組合/接 UI** | `97e02eedb`（隔離 `12cd69a41`） |
| Rust T2-B1 | `RuntimeStore::transfer_terminal_tab_storage`：SQLite transaction 更新 Terminal Tab 和雙側 Layout，核對狀態、Host、tab 集合、原布局與 mutation receipt，紀錄原 execution workspace；**尚未接 Actor live PTY owner** | `0fc27062b`（隔離 `800124981`） |

**實際支援狀態：仍不支援跨專案搬動執行中的 Terminal。** 新 Store 方法尚無 `tab.transfer` Host Request handler，且 live `Session.workspace_id` 會影響來源 Workspace 關閉、Restart、Agent hooks、PTY Pulse；不能單獨更新 DB 而誤殺程序。因此 UI transfer callback 刻意未接線。下一個阻擋項目 **T2-B2：統一 Runtime Actor 的 display owner 與 execution identity、live Session 轉移、事件廣播及失敗／重播語意**；完成後才能連結 Workbench UI、恢復與 Undo。詳細見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

本輪只做來源碼格式化及 Git staged whitespace check，**沒有 Flutter/Cargo 測試、靜態分析、Build 或實機驗證**；不等於可發布。原本 dirty 的 `main` 未修改、未 merge、未 push、未 release。

## 最新後續待辦：T2 Terminal Tab 跨專案移轉（2026-10-29）

**狀態：已納入規劃，尚未實作。** 依使用者要求將跨 Project/Workspace 拖放既有 Terminal Tab 列為 **P2 功能待辦 T2**。目標是直接移動**同一個** Tab／Rust 持久 PTY Session（保留 Process、scrollback、原 CWD、Shell 歷史及畫面），而非新開 Terminal、關閉舊 PTY 或自動 `cd`。目的地有多個 Workspace 時必須供使用者選擇，不偷偷建立 Workspace；成功後更新來源／目的 Tab 與 Pane Layout，可反向移轉以復原。

既有設計源自 `feat/cross-project-terminal-tabs-20261029` 的 `ec150a493`，本次只將規劃文件 `docs/terminal-tab-cross-workspace-plan-20261029.md` 收編至本整合工作樹並補上最新 T1/W1/C1 相依與分期。**UI 拖曳預覽與 Rust Host 契約設計可平行**，但 Host/Store atomic transfer、PTY ownership 與 Workbench Tab/Layout 接線須分清擁有者再整合。詳細階段、風險、復原及未來驗收條件見該文件。這次**只有文件變動，沒有功能程式碼、測試或發布**。

## 最新：合入 macOS Claude Code CLI 貼上多出 V 的修正（2026-10-29）

已將本地最新修正分支 `fix/macos-claude-kitty-paste-v-20261029`（分支 HEAD `3d232acff`，同時是觀察到的 `main` HEAD）以 **`git merge --no-ff`** 合併至獨立整合分支 `refactor/tdd-integrate-20261029`，產生 merge commit **`f49ee344944f85036c2798f33ba3b5e01e290e35`**，無衝突。分支僅比共同祖先 `3501b0555` 多兩個 commit：`4ee872d7f` 將 `third_party/xterm` gitlink 更新為 **`6b365fc3a6eecae1865e9e8da3b7b5de8fb160aa`**（xterm2 的 `fix(mac): prevent Claude Code Kitty V key-up after paste`），`3d232acff` 將 `pubspec.yaml` 版號改成 **`0.91.15+308`**；舊的 10/27 `fix/macos-paste-extra-v-20261027` 不是本次合併目標。

已核對遠端 `shelizi/xterm2` 的 `alera` 分支確實發布上述 xterm commit，並在**整合工作樹**透過 `git submodule update --init -- third_party/xterm` 成功 checkout 相同 SHA。因整合樹原先有沒有 `.git` 的舊 xterm 依賴檔案副本，標準 init 初輪拒絕覆蓋；已先將副本**保留**至 `.worktrees/_xterm-copy-before-mac-hotfix-20261029`，再重新初始化，沒有刪除該副本。原始 `main` 的 dirty 檔案／子模組 working tree 未 checkout、未清理、未提交、未推送。

**本輪只確認 Git 合併、遠端 gitlink 可取得、整合子模組 checkout 與 Git 工作樹狀態；依使用者之前的要求沒有執行 Flutter/Cargo 測試、Analyzer、macOS 編譯或實機 Claude Code 貼上驗證。因此不能宣稱此修正已在 Mac 實機驗證或正式發布。**

> 使用者決策：暫停新增測試、靜態分析、效能量測與純粹為降低 500 行違規的拆檔；這一批改為四條互不衝突的功能實作。**本批尚未執行 Flutter/Cargo 測試、Analyzer、Guard、跨平台編譯或發行驗證，不得宣稱本批編譯／測試通過。**程式碼已格式化、做過變更檢視及 Git staged diff whitespace check。
>
> 起點：第七批已驗證、乾淨的整合 HEAD `54d6280c1815f5311b2907e86deded483648bd01`。實作分支均以此點切出。第七批交接時記錄的 `main` HEAD 為 `3501b0555`，但**第八批收尾實際查到 `main` 已由其他並行作業更新至 `3d232acff9822154c68049ee63c57ed5966b84e0`**，仍有既存未提交資料（包括 `third_party/xterm`）；本批**沒有**在 `main` 上提交、清理、merge、push 或 release。切勿以第七批的 `main` SHA 當作新現況。

## 第八批已落地功能與隔離提交

| 線路 | 實際使用者功能 | 位置與提交 | 嚴格範圍／尚待 |
| --- | --- | --- | --- |
| R1 Rust Runtime | localhost 連線除了原有同時 32 Socket、64 個已 admission commands、32 MiB 已 admission requests，另外新增**最多 64 MiB 未 admission/部分讀取中的請求內容額度**。讀取前按 4 KiB 區塊向共用 semaphore 嘗試拿配額；配額不足時直接拒絕該連線，而非一面持有其他部分 request、一面等待而互鎖。已完成的請求成功進入 Actor 才釋放未 admission 配額；取消、socket 關閉由 RAII 自動釋放。 | `rust/alera-cli/src/terminal_host/client_line_reader.rs`、`client.rs`；隔離 `e82f9c579` → 整合 **`48a45b5f3`** | 限制的是**按字節向上取整計費的 localhost pending payload**，不是 Rust 配置器的真實 heap bytes，也不含 BufReader、String/Vec 額外容量、其他 actor/輸出佇列；不包含 Mobile、Relay、Hook。配額壅塞下將關閉新讀取的 socket，呼叫端需重新連接。**本批未驗證 Cargo 編譯、控制指令行為**。 |
| C1 Mobile 跨端一致性 | Mobile 更新共享 Workbench 偏好值時若收到 request timeout，不直接重送可能已 commit 的 mutation，而是重新讀取 Runtime 的權威 view prefs 及 revision；只有提交的每個欄位都已反映才當作成功，集合欄位忽略排列順序；不相符或讀取失敗仍保留原本 TimeoutException。 | `mobile/lib/src/features/runtime/infra/mobile_runtime_workspace_sidebar_client.dart`；隔離 `f8205773f` → 整合 **`4437ee71e`** | 只涵蓋 **`workbenchViewPrefs.update`** 且仍能查詢的 timeout，不自動重送、不對所有 Create/Delete/Terminal write 推廣冪等；真正斷線或 server 舊版沒有讀回能力時仍須重連並重新整理。**尚未 Mobile 測試／編譯。** |
| T1 Terminal 生命週期 | 當本機有兩個 Tab／檢視共用相同 durable PTY session ID，明確關閉其中一個只釋放這個視圖，不終止其他仍存活的本機檢視；關閉 Workspace 也改經過相同判斷，最後一個本機檢視才保留原有的 terminate 行為。 | `lib/src/features/workbench/presentation/terminal_runtime_session_owner.dart`；隔離 `f3a927a31` → 整合 **`45941055a`** | 只根據**同一 Desktop runtime registry 的已建立 handle**判斷，跨程序/遠端 Mobile 看不見，無法保證全域多客戶端 reference count。顯式刪除 Tab 的外層 overlays/observer 清理仍為既有行為；**尚未 UI/PTY 整合驗證**。 |
| W1 Workbench | 背景 `loadLayoutForWorkspace` 等待 listTabs 與 layout resolver 時，如果較新的工作區選擇、分割或焦點變更已經建立 layout，舊結果直接捨棄，避免覆蓋新的版面配置；workspace 已刪除／controller 已 dispose 仍不套用。 | `lib/src/features/workbench/application/workbench_tab_layout_owner.dart`；隔離 `868cdd733` → 整合 **`a7ee48fbd`** | 精準處理「原本沒有 layout、背景補載」與較新 layout 的競態。尚不等於所有 tab/layout watcher 都有 transaction generation，也未驗證 UI／跨端回放行為。 |

## 本批整合狀態與後續

- 四條線的 commit 已 cherry-pick 至 `refactor/tdd-integrate-20261029`，分別為 R1 `48a45b5f3`、C1 `4437ee71e`、T1 `45941055a`、W1 `a7ee48fbd`。以上是**已完成的來源碼修改／Git 提交**，**不是**「已測試、可發布」的聲明。
- 第八批交接 HEAD `abc7c3f1f017c70a2bac340d614f1c176469f831`；整合樹與四條隔離工作樹 `git status --porcelain` 均乾淨。若其他代理還在更新 `main`，後續要合併必須先做新差異盤點，不能假設仍是 `3501b0555`。
- 這批無意擴大 Rust 化：手機 UI／Workbench 焦點狀態保持 Dart，只有 localhost protocol memory admission 由 Rust 控制。無 FRB API/schema 變動，未重新產碼。
- 下一輪功能優先：Mobile/Relay/Hook 的跨 transport quota 和 request lifecycle，跨端操作冪等／lost reply 結果確認，跨 process terminal owner 引用計數，Workspace Tabs watcher 的更完整 generation／版本化，Explorer worktree 隔離和 Git Diff 取消一致性。
- 如要合併 `main`、對外推送或發布，應另行處理本批未驗證的編譯與回歸風險；不可把第七批的 142 Flutter／71 Rust green 視為本批驗證。
