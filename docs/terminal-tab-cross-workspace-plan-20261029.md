# T2 — 跨專案／跨 Workspace Terminal Tab 移動：功能規劃與並行重構邊界

> **2026-10-31 更新**：下方 `feature-gated OFF`、Windows-only 和「未執行測試」為歷史設計與開發記錄。當前實作以連線 Host 的三項能力回覆判定 T2 可用，**不依 Host 作業系統限制**；原 PTY、收據、View/Layouts 驗證與未知結果時的 fail-closed 規則仍有效。跨平台原生驗收需分開記錄，不能僅因 Host 協定宣告就假稱實機已通過。

**Priority：P2；Status：T2 一次集中收尾的 Store/Journal 安全修改已整合；仍有 A/B/C 三項不可假定通過的有限 Release Gate；feature-gated OFF（2026-10-30）。** 詳 `docs/terminal-tab-transfer-closure-20261030.md`。**跨專案拖放仍未開放；Flutter/Cargo 測試、靜態分析、編譯及實機回歸仍未執行。**

本文件原是 `feat/cross-project-terminal-tabs-20261029` 分支上的設計稿（commit `ec150a493`），現**只收編此規劃檔**至 `refactor/tdd-integrate-20261029`；未合併該舊分支的產品程式。原設計所記載的並行分工為 2026-10-29 第三批的歷史狀態；最新整合已包含第八批功能修正及 macOS Claude Code 貼上 hotfix。

## 最新：T2 集中收尾與最終驗收清單（2026-10-30；Capability 仍 OFF）

以 `docs/terminal-tab-transfer-closure-20261030.md` 為**唯一最新收尾與驗收入口**。D17 乾淨起點 `7cac43d53`，Host/Store Source `d479429e7` → integration **`09c8aba3a`**，Journal Source `be6d1922a` → integration **`d12ca2e69`**。普通 Store Tab Delete/Workspace Sleep/Cascade/Project Delete 已對已移轉 Receipt/Host Exit Marker 加同筆 SQLite transaction 下的 failclosed Guard；原 Host Actor 已驗證活 PTY 世代與唯一 View、最新收據後，特別移動 Tab Close 才會以同筆 CAS 更新 Tab＋Target Layout＋持久稽核；Journal Lock v2 增加 App Process Epoch、正常釋放仍需同 PID/Hash/Nonce/Process Epoch。**沒有**實作 Host Crash 後冒用原 PTY、自動 stale Lock 解鎖、跨 OS Process durable Last-view，未跑 Test/Build/E2E，Gate OFF。**不再新增新編號階段**，僅依 A/B/C Release Gate 收斂。

## 歷史：T2-D17 三線／三筆安全補強已整合（2026-10-30；Capability 仍 OFF）

**Git**：D16 clean `46845b80ed8fa5eb0ada82bf869d4dd2c4473781` 起。三條來源分支：`feat/t2-d17-store-upsert-20261030`、`feat/t2-d17-journal-correlation-20261030`、`feat/t2-d17-mobile-current-host-20261030`；Source→Integration：`c373fbe75`→**`2cd2b67f3`**、`85ca84604`→**`1a0543ead`**、`4c45ddb0c`→**`52dbe008a`**。更新本文件前 Head `52dbe008a`；Docs commit 後應重新查 `git log -1`／`git status`。

| 工作線 | D17 實際防護／仍不具備的保證 |
| --- | --- |
| Store 單筆 SQLite Upsert 交易 | `RuntimeStore.upsert_workspace_tab` 已不再直接用 `INSERT ... ON CONFLICT UPDATE` 覆寫已移轉 Tab。先在同一筆 SQLite transaction 讀原 Row；只要有 Transfer 或原 Host Exit marker（**包含顯式 null/損毀**），拒絕一般 upsert 更改 Workspace owner、Tab kind、原 Receipt／Exit、`terminalSessionId`、原執行 Workspace／Working Directory。對新 Tab／從未轉移 Tab 不接受 client 直接偽造 Transfer/Exit marker。此功能涵蓋**透過普通 Store Upsert API** 的 callers；特別保護 Host RPC 以外的內部寫入路徑，**不是**對所有 direct SQLite/Remove 的完全覆蓋。 |
| Journal 本地證據交叉比對 | `inspectManualRecoveryDossier` 新增 `readablePendingArchiveCrossReferences` 與 `archivesWithoutReadableMatchingPending`，列出同 Tab Pending Mutation 對應的同 mutation / 不同 mutation Local Archive；歷史 Archive 可能是前次正常完成，不可視為惡意/失敗。受損／無法讀取檔案仍在原始盤點，絕不猜測對應 Host/Tab。比對無原子性、沒 Host 簽證，`requiresHostVerification=true` 和 `permitsAutomaticUnlock=false` 不變。 |
| Mobile 視圖身分與最新 Host Client 一致 | Controller 原來只判斷原 View Lease ID、Ready Attached Session 和 D16 Attachment Revision；現在 `attachedViewLeaseId` 每次同步再讀 `terminalClientProvider(hostId)`，必須仍是原綁定 Host Client instance，才能授權準備 moved Tab Close。即使新 Socket 更新 Provider 但 Controller Listener 尚未切入 Recovery，舊 Ready 的 View ID 亦不可被沿用。並不保證跨程序、多個 Phone／OS Window、Host restart 全域 Last-view。 |

**D18 尚待**：持久跨 Window/Process/Phone/Host-restart 的 View/PTY Owner；Crash 於 SQLite commit 與 Session reparent 的 definite 交易結論；使用者意圖/原 Host Live Lease/退出證據及雙 Layout CAS 綁定的**合法 exited Tab＋Audit 原子退休**；Store 原始 `remove_workspace_tab`／直接 SQL 寫入需協調合法最終授權後加守衛；Journal OS-level stale Lock owner incarnation、Host-auth outcome 與人工安全解鎖 UX；實際 Windows/macOS/Phone E2E 與 Cargo/Flutter tests/Analyze/Check/Build。**`tab.transfer.capabilities.supported=false` 保留；本輪只進行 Rust/Dart format、Git whitespace checks、隔離 Git commit/cherry-pick，未執行或新增測試／編譯／Analyzer／E2E／Push／Release，也不改 dirty main。**

**接手入口**：`docs/refactor/feature-progress-20261029.md` 最上方 D17、`docs/refactor/tdd-roadmap-20261029.md` 最上方 D17，D18 必須在合法授權與未驗證風險解決前保持功能 OFF。

## 歷史：T2-D16 三線／四筆安全補強已整合（2026-10-30；Capability 仍 OFF）

**Git**：從 D15 乾淨整合 `c4047798ad5ecbe6f9953c5c418351cc563ea5dd` 起，source 分支 `feat/t2-d16-store-retirement-preview-20261030`、`feat/t2-d16-journal-manual-dossier-20261030`、`feat/t2-d16-mobile-view-close-20261030`。四筆 Source→Integration：`cbe304c8b`→**`88f09d941`**、`fa5130e5a`→**`5837f89b8`**、`6f29de5e3`→**`dd6eeb3e3`**、`429557d4e`→**`e3f771ed1`**。文件提交前 HEAD `e3f771ed1`，Docs commit 後接手先查 Git HEAD/status。

| 工作線 | D16 實作與不能推論之事 |
| --- | --- |
| Store Receipt／原 PTY Exit | `record_transferred_terminal_exit` 對原 Exit **key presence** 判斷，連 `null` 也當作需保留的原 Host 證據，不能覆寫；`transfer_terminal_tab_storage` 把先前 Exit Marker、格式錯/身份錯誤的 Transfer Receipt 和不同原 PTY Instance 一律擋下。相同 mutation replay 也需用**同一 SQLite transaction** 驗證來源/目標持久 Layout 和兩側全部 Tab registry，不能僅有收據就宣稱 success。 |
| Host 退出 PTY 一筆快照預檢 | Store `inspect_transferred_terminal_retirement_evidence` 共享 D13/D14 的兩 Layout＋Tab registry 單筆交易檢查，另核對原 Host Observed Exit 的 mutation/Session/Workspace/PTY Token/Code/合法時間；Host `tab.transfer.status` 在 `pty_exit_observed` 時附上 **`originalExitAndLayoutsConsistent`**，對原 Host generation mismatch 仍如實標示。回傳保持 **`applied:false,recoveryRequired:true`**；即使 true，也**不是**可刪 Tab、可清 Journal、已無其他 live viewer 或能復活原 PTY 的權威批准。 |
| Journal 人工復原 Dossier | Desktop `inspectManualRecoveryDossier()` 結合 Pending／所有 Lock 與 Resolved／孤兒檔的唯讀盤點，帶 UTC 觀察時間與本地 evidence flags。任一根目錄不安全就 fail closed；兩份不同時間讀取不構成 atomic snapshot。`requiresHostVerification=true`、`permitsAutomaticUnlock=false` 固定不變；不得以 PID、日期、Archive 格式判定已安全解除鎖。 |
| Mobile Close 的附著版本 | `TerminalSessionController._attachmentRevision` 每次重新附著 increment，`observedAttachedView` 是同步只讀 View ID＋當次 revision。轉移 Tab Close 捕捉 click-time 原 Controller＋revision，`await workspaceClientProvider` 完成後確認 Provider 仍同 instance 且 View Revision 未改；新 Socket 使用**同 UUID**也不能自動繼承舊點擊的 Close 意圖。普通 fallback 再次使用 `hasTransferEvidence` 防 Exit-only 遺失。 |

**D17 未完成**：跨原生視窗／跨 App／多 Phone／Host restart 的真正 durable last-view owner lease；Host Crash SQLite Commit／Session reparent 的 definite outcome/補償；需使用者授權＋原 Host Process Generation + 真實 original Exit＋雙 Layout CAS 的**同交易安全退休與持久 Audit**；Journal stale Lock 適當的 process incarnation/liveness 與 Host-signed manual recovery/UX；完整 Win/Mac/Phone E2E 和 Cargo/Flutter test/analyze/check/build。D16 **不提供**可破壞性清理/解鎖的 RPC，Gate `tab.transfer.capabilities.supported=false` 保留；只做 Rust/Dart format、Git whitespace checks、Git 隔離 commit/cherry-pick，**沒有測試、編譯、分析、Push、Release，也不碰 dirty main**。接手讀 `docs/refactor/feature-progress-20261029.md` 最上方 D16。

## 歷史：T2-D15 三線／六筆安全補強已整合（2026-10-30；Capability 仍 OFF）

**Git**：從 D14 乾淨整合 `ac5d4383c51b8cda5da946e6f1da954622e66256` 起，三條來源分支 `feat/t2-d15-corrupt-receipt-20261030`、`feat/t2-d15-resolution-inventory-20261030`、`feat/t2-d15-mobile-receipt-20261030`。程式碼最後一筆整合 **`6b255be72`**；文件提交後 HEAD 會再向前，接手先看 `git log -1`／`git status`。

| 工作線 | D15 具體內容／不得越界聲稱 | Source → Integration |
| --- | --- | --- |
| H：不可信 Transferred Receipt／Exit | 引入 Host `has_transfer_recovery_evidence`／`invalid_transfer_receipt_marker`。只要有 `lastTerminalTabTransfer` **或** `lastTerminalTabExit` Key，即使 null、畸形、缺 ID/from/to/PTY Token、Exit 不符原 Mutation／Session／Workspace／PTY Token／Code，均視為未解決證據；`terminate`、Attach/Restart、新 Transfer、`tab.remove` Actor preflight 與 background worker、自然 Exit、Workspace/Project 隱式清理均拒絕 fallback legacy 處理。Host status 對損毀收據回 `applied:false,recoveryRequired:true,reason:malformed_transfer_receipt`，不能因 JSON 壞掉而刪已移轉 Terminal；**未新增真正的退休／回復交易** | `2e1aa4995` → **`12ff53b74`**；`8fc786f22` → **`c28d49b45`** |
| H2：`tab.upsert` Host-owned Evidence | 新增 Host Upsert guard，已轉移 Row 不能經 stale Client 修改 Workspace/Kind；沒有 transfer 的 row/client 新建不能偽造 Transfer 或 Exit Marker。既有 `preserve_host_owned_tab_payload` 納入 `lastTerminalTabExit`、`terminalSessionId`，舊視窗 Rename/Title 更新不會丟失退出證據或原 Session ID。這只涵蓋 Host RPC 封裝的 Upsert，未提供跨不同 Host Process 寫入 SQLite 的 durable CAS | `812de0e13` → **`0499ec5d2`** |
| M：Mobile 不回退到 unscoped Close | Mobile `WorkspaceTabSummary.hasTransferEvidence` 同時識別 Transfer/Exit Marker Key；`hasMalformedTransferEvidence` 檢查原 Receipt 欄位及 Exit 關聯，不把損毀 JSON、只剩 Exit 的 Tab 當從未移轉。手機轉移 Tab Close 仍需同步取得已附著原 View Lease + 最後看到的 Receipt，直接 Host `tab.remove`；普通 `removeObservedTab`／`terminateObservedTerminal` 明確拒絕 transferred Evidence，避免先殺原 PTY 或默默刪掉不可恢復的收據 | `09cfaf15b` → **`2a3d33934`**；`c1e183f0e` → **`6b255be72`** |
| J：Resolved Audit 目錄完整盤點 | 新 `inspectResolutionArchiveInventory()` 唯讀列出所有 resolved_v1 封存，找出孤兒檔、損壞、未知副檔名/型別、SHA256 名稱與 `[tabId,clientMutationId]` 不符；`requiresHostVerification` 永遠 true，因本地 `callerConfirmedHostAndBothLayouts` 記錄不是 Host 簽證。**不會因封存存在就清 Pending／刪 Lock**，不提供自動解鎖 | `3c9c11615` → **`24bfe5751`** |

**整合中的 Git ref 問題**：Mobile cherry-pick 曾已完成 index 但 ref 更新失敗；核對 HEAD、staged diff、`CHERRY_PICK_HEAD` 後，`git cherry-pick --continue` 產生 **`2a3d33934`**，並非重複 cherry-pick。最後二筆 orphan Exit safety follow-up 亦已整合。

**D16 遺留**：真正跨 Windows/macOS Native Window/多 App/多 Phone/Host Restart 的 durable Live/Last-view lease；Host crash 在 SQLite commit 和 in-memory PTY owner reparent 之間的決定性補償；受 Original Receipt/PTY Generation/Exit 與使用者意圖綁定的 exited transferred Tab+雙側 Layout 同交易安全退休；Journal stale Lock 的安全 Manual Repair、跨程序 owner incarnation/liveness 和 Host 權威判定 UX；Mobile 多裝置與 Win/Mac/Phone E2E。**Gate `tab.transfer.capabilities.supported=false` 保留**，本輪只做 Rust/Dart format 與 Git whitespace checks；沒有新增/執行 Cargo/Flutter Tests、Analyzer、Build、Check、E2E、Push 或 Release，原始 dirty `main` 不動。接續 D16 優先讀 `docs/refactor/feature-progress-20261029.md` 的 D15。

## 歷史：T2-D14 三線/四筆補強已整合（2026-10-30；Capability 仍 OFF）

**Git**：D13 baseline `cefc1d80c3a75ebae59217fbc59844c3153135ab`，三條乾淨來源工作樹切 `feat/t2-d14-host-epoch-20261030`、`feat/t2-d14-journal-root-20261030`、`feat/t2-d14-mobile-handoff-20261030`。四筆程式提交依序 cherry-pick 到 `refactor/tdd-integrate-20261029`，**文件提交前整合最後 HEAD `1d9c4b220`**。Docs commit 後 HEAD 會變，接續需重查 Git 狀態，不碰 dirty `main`。

| 工作線 | 已完成程式碼／精確的安全意義 | Source → Integration |
| --- | --- | --- |
| H：Host 原程序世代辨識 | 不發明新 process ID；重用既有每 Host process 隨機 UUID + PTY instance counter 的 `ptyInstanceToken`。當 persisted transfer receipt 來自不同 Host generation，`tab.transfer.status` 明確回 `applied:false,recoveryRequired:true,reason:host_generation_mismatch`；若同一原 Receipt 有真正 Host 觀察到的 `lastTerminalTabExit`，則回 `pty_exit_observed,exitCode,hostGenerationMismatch:true`。**不能因此斷言原 Host 已死亡**（可能另一程序仍存活）、SQLite 交易失敗、允許自動刪 Tab／Journal 或重新啟動替代 PTY | `b170bc619` → **`edfe3abdc`** |
| H2：同一 SQLite 快照權威 Tab | D13 `verify_terminal_transfer_layout_and_receipt` 從 bool 改為 `Option<WorkspaceTabRecord>`，在那筆單一 SQLite transaction 已驗證兩側 Layout、完整 WorkspaceTab registry、原 Session/Receipt/PTY Token，且**只回傳那筆交易讀到的同一 Tab**；Host status 給 Client 的 `tab` 不再是驗證前另一次讀取、可能已變動的舊副本。這仍不是 OS Host process/PTY/多 App 并行的真正跨層原子性 | `c7bc2e67c` → **`1d9c4b220`** |
| J：Journal 根目錄與審計檔證據 | Pending/Resolved Archive 根目錄必須是真 Directory、不能是 symlink；鎖檔、Pending JSON、封存證據在 Read/Write/Delete 前拒絕 symlink/非一般 File；Inspector 用 `FileSystemEntity.type(followLinks:false)` 不跟隨偽裝的檔案。正常擁有自己的 Lock 在清理前還需 original Tab Hash、Nonce、PID、Version 全對。**不保證敵對外部寫入者下的零 TOCTOU**、不清 stale Lock、也不把本地 Archive 當 Host 授權 | `a2e840af7` → **`d212d87c6`** |
| M：Mobile Named View Socket Handoff | `_attachAfterVerifiedHandoff` 先以同一原 UUID 正常嘗試新的 Host Client Attach（舊 Client 已斷開則成功）。只有 Host 明確回 `view lease is already registered on another Host client` 才向**目前原舊 Socket** 送具名 `detach` 並等待確認，清原本 _client/_sessionId，重試一次。舊 Socket 不可達、detach 答覆不確定或仍被別的 Client 持有時拒絕搶 Lease，不合成新 View ID；Legacy 未轉移路徑沿用舊法 | `058216b57` → **`28336851d`** |

**D15 仍未完成**：真正 per-native Window/Device/App/Host restart durable Live/Last-view registry、跨多程序 Host owner 確認；Host Crash 於 SQLite commit/live Session reparent 間的持久復原/決定性結果；受權威證據與 User Intent 綁定的 exited moved Terminal Tab + 兩側 Layout 原子合法退休；Journal stale Lock OS Process incarnation/live proof、Host-authorized manual unlock/UX/保留歸檔；Mobile 多 Phone、舊 socket 連線切換、native Windows/macOS/Phone E2E，以及 Cargo/Flutter tests/analyzer/check/build。**D14 是防護與診斷，不代表功能已可開放**；`tab.transfer.capabilities.supported=false` 不變，格式化／Git whitespace checks 外所有測試、編譯、推送、發布仍禁止。

**接續入口**：`docs/refactor/feature-progress-20261029.md` 最上方 D14，`docs/refactor/tdd-roadmap-20261029.md` 最上方 D14；下輪從目前真實 HEAD 續 D15。

## 歷史：T2-D13 四部分安全補強已整合（2026-10-30；Capability 仍 OFF）

**Git**：從 D12 clean `13842cc37` 出發，隔離分支 `feat/t2-d13-layout-attestation-20261030`、`feat/t2-d13-journal-failclosed-20261030`、`feat/t2-d13-mobile-capability-20261030` 並行分檔；後者另有 Workbench Detached Window 獨立 commit。五筆 source commits 各自 cherry-pick 且不衝突，更新 docs 前最後整合 **`f601c1a86`**。後續 Docs Commit 會推進 HEAD，接手須重新查 status/log，不可直接沿用。

| 工作線 | D13 已做／限制 | Source → Integration |
| --- | --- | --- |
| H：Host 讀回雙 Layout + 原 PTY | `RuntimeStore.verify_terminal_transfer_layout_and_receipt` 在一個 SQLite read transaction 比對 **Tab 目前 owner、Session、最新 receipt ID/from/to/PTY Instance、兩側 Layout split tree/pane/Tab ID 的合法唯一性，且雙側所有 Tab 成員完全等於各自 Persisted WorkspaceTab 名冊**。Host `tab.transfer.status` 在既有原 live PTY token 證實後才呼叫；失敗回 `applied:false,recoveryRequired:true,reason:persisted_layout_inconsistent`，不清 Pending。這仍不是 Host Crash 補償／合法已死 PTY 退休 | `1f9bbf429` → **`ad91df026`** |
| J：Journal 不忽略未知檔 | `loadPending()` 遇任何 `.lock`、損毀或非 File `.json`、非 SHA256 名稱與不認識的目錄項目均 fail closed，沒有把 broken symlink/資料夾 lock 當正常空 Journal；唯讀 `inspectRecoveryInventory()` 加入各種未知副檔名、隱藏檔，使操作員能看見被拒來源。可能被非 Journal 垃圾檔阻擋，但不可擅自刪除；仍不證明 PID 的 Owner liveness 或 Host 結論 | `987090545` → **`c6a6117d9`**；`122bfb48d` → **`f601c1a86`** |
| M：Mobile 具名 View 協定版本宣告 | Host `mobile.hello` 加 `terminalNamedViewLeaseV1`，手機在具名 Attach/Detach/Restart 和 moved Tab Close 前必須從 Host authenticated capability 讀到此值，不能只靠新 Client 方法或舊 Host 對額外 JSON 欄位的寬鬆忽略而宣告具備 View proof。Legacy 未移動 PTY 保留舊 attach；**宣告支援協定 ≠ `tab.transfer` 已開放** | `59cb60152` → **`682a460ac`** |
| W：Shared/Child Detached Window Close Guard | 桌面同步 Close 點擊捕獲 Receipt/Lease 時，若 `detachedTabWindows.isDetached` 或 `isLaunching`，已轉移 Terminal 必須先合回主視窗，不把已暫時 unmounted 的獨立原生視窗誤算為只有一個正在掛載的 PTY View；同時保留 D11 `mounted surface == 1` 保守條件。只覆蓋已由 **本機當前 App** 記錄的 detached windows，不提供跨原生程序/Device/Host Restart Durable Last-view | `ad7a714ff` → **`524cb4c49`** |

**D14 未完成**：跨 Native Window、Mobile Device、不同 App Process 和 Host restart 真正持久 per-View/Last-view Lease；SQLite commit/live PTY reparent 的 Crash 補償與可驗證 definite outcome；Host 觀察到 Exit 後合法 retired Tab+雙 Layout 原子清理；Journal Stale Lock 安全 owner-liveness 檢查、Host 可稽核解除、人工 UX；Mobile 舊 Socket 仍維持 attached View 時的安全重連流程；完整 Cargo/Flutter Tests、Analyze、Check、Build、Windows/macOS/Phone 多視窗/E2E。**嚴格 `tab.transfer.capabilities.supported=false`，只有 Rust/Dart format + Git whitespace checks；未新增/執行測試／編譯／分析／實機驗證／Push／Release，原 dirty main 不動。**

**接手入口**：`docs/refactor/feature-progress-20261029.md` 最上方 D13，及本文件最上方 D13；先核實最新整合 HEAD，再分離工作樹續 D14。

## 歷史：T2-D12 三線程式碼已整合（2026-10-30；Capability 仍 OFF）

**Git**：D11 乾淨整合起點 `fff99a3c6`。三個之前已乾淨的 Worktree 分別切 `feat/t2-d12-exit-evidence-20261030`／`feat/t2-d12-journal-inventory-20261030`／`feat/t2-d12-mobile-view-20261030`，各一筆提交再不重疊 cherry-pick，D12 程式整合最後 **`62ce1d3b3`**。文件提交會讓 HEAD 再變，接手先重新查 `git log -1`、`git status`。

| 工作線 | 實作／精確安全範圍 | Source → Integration |
| --- | --- | --- |
| H：真實原 PTY Exit Durable Evidence | 在 `rust/alera-core/src/runtime/store_tab_transfer.rs` 新增 `record_transferred_terminal_exit`，只接受 Host Actor **真正收到** 原 PTY Exit 的 `Session`、Workspace、Tab、receipt ID、`ptyInstanceToken` 全對上的值，SQLite 用原始 `payloadJson` CAS 寫 `lastTerminalTabExit`，含 Exit Code／UTC。Host status 在缺失或不再執行原 Session 時可回 `applied:false,recoveryRequired:true,reason:pty_exit_observed,exitCode`，以**已觀測正常退出**區分未知 Crash／missing PTY。這不是權威 Host Crash 成敗決定，也不允許重鑄／刪除 moved Tab | `60b1eb3d8` → **`f259d98df`** |
| J：Journal Inventory 唯讀 | `FileWorkbenchTerminalTransferJournal.inspectRecoveryInventory()` 列出整個 pending 目錄，依 SHA256 檔名整合 `.json`／`.lock`，顯示有效 Tab／Client Mutation、錯誤 Hash／無法解析的 JSON、匹配本地 Archive、Lock metadata／PID／UTC，亦列出不合法檔名。檢查不持原子鎖，所有資訊僅是**當下可疑證據**，不會解鎖、復原、重試或宣告結果。原 `loadPending` 碰到 Lock 必須失敗的防護不變 | `0f50c913f` → **`535db1096`** |
| M：手機有名 View 及 moved Close | 新版手機每個 `TerminalSessionController` 生成 24-byte crypto-random `viewLeaseId`，原 ID 沿用 Attach／Detach／Reconnect／Restart；Mobile Runtime Client 新窄介面不強迫 Legacy Fake 編譯升級，Host `restart_terminal` 會驗證 View 不屬另一 Client，重建 Session 後具名註冊。對搬移 Tab，`TabsController` 不再先 `terminate` 後 `tab.remove`；在任何 await 前向**已存在、成功附著**的手機 Controller 取得原 Token，經 `MobileObservedViewLeaseTabRemoval` 用原 Tab/Workspace/receipt/token 直接執行受 Host Actor 序列化的 `tab.remove`。缺憑證／未開啟 UI／不支援新 Port 仍拒絕，從不臨時新建視圖授權；未移轉 Tab 舊路徑維持 | `29a74ed32` → **`62ce1d3b3`** |

**D13 仍未做**：不同 Native Window（包含未掛載 Surface）、不同 App/Device、多 Host Restart 的真正 durable Last-view Lease；Host crash 在 SQLite commit 和 in-memory Session owner 切換間的可靠補償與 definitive failure；已正常退出 moved PTY 的合法 Tab retire（目前保持收據）；跨程序可獨立驗證 liveness 的 Journal stale Lock 修復及 UX；真正手機多裝置和重連舊 socket 同時存活、Host watcher／Window 語意；完整 Cargo/Flutter Compile、Analyze、Tests、Win/Mac/Phone E2E。**正常退出 Marker ≠ 可刪 Tab 的憑證；PID ≠ 已確認持鎖者仍活著／已停止。**

**嚴格開關與工作限制**：`tab.transfer.capabilities.supported=false` 保留。D12 僅執行 Rust/Dart 格式化和 Git whitespace check，沒有新增／執行 Tests、Analyzer、Cargo check、Build、實機 E2E、Push、Release，也沒有修改／reset／clean 原本 dirty `main`。下一輪由 `docs/refactor/feature-progress-20261029.md` 最上方 D12 接手 D13。

**D12 最後安全追補**：Store 不覆寫 non-null malformed `lastTerminalTabExit`，保留原始有問題的持久資料以供確認，`3773a5512` → **`04aff070f`**；Journal Inventory 不跟隨 .json/.lock 符號連結、會標示具備這種檔名的非 File 異常物件，`e11d3d570` → **`37bbe6b03`**。兩者均不讓 Client 自動清退未知 Receipt 或 Lock。

## 歷史：T2-D11 三線程式碼已整合（2026-10-30；Capability 仍 OFF）

**Git**：從 D10 乾淨整合 `42b0deee2`，三個已乾淨 D10 工作樹各自切 `feat/t2-d11-host-lease-20261030`、`feat/t2-d11-journal-20261030`、`feat/t2-d11-workbench-close-20261030`。Host Exit、Dart Journal、Dart WorkbenchClose/Repo 分檔修改、各自提交再 cherry-pick 無衝突；程式碼整合 HEAD **`2ea227339`**（文件 Commit 後務必重新確認）。

| 工作線 | D11 源碼實作／保守範圍 | Source → Integration |
| --- | --- | --- |
| H：Host PTY 出口證據保留 | 通用 `remove_terminal_session_tab` 被自然 Exit 或顯式 `terminate` 呼叫時，若 Store Tab 的 `lastTerminalTabTransfer` 存在，直接返回 `Ok(false)`，**不刪 Tab／Transfer Receipt**；既有流程 checkpoint exited Session／顯式終止 PTY，非轉移 Tab 行為不變。這保住過去會被自然退出抹掉的復原證據，但**已退出 moved Tab 仍無合法最終處置**；下一批要有明確的持久化 process-generation/retirement transaction，不可逕行全域清理 | `831bdb1e0` → **`673566e07`** |
| J：可辨識但不可自動移除的 Journal Lock | per-Tab `.lock` 建立成功後持久寫入 `tabIdHash`、UUID `nonce`、PID、建立 UTC；`remember/forget` 只刪自身 nonce/PID 匹配的 Lock。Crash 留空 Lock、毀損內容或跨程序身份變更都 fail closed。原有 `inspectRecovery` 補 `lockMetadataValid`、`lockOwnerProcessId`、`lockCreatedAtUtc`，**不能**把 PID／UTC 當成程序已結束證明，也**沒有**新增 Unlock 按鈕、Host 簽章或失敗定論 | `62fd828ef` → **`95ab5e5b6`** |
| C：真實 Workbench Tab Close 帶原視圖 Lease | UI 透過新可選 `TerminalRuntimeAttachedViewLeaseSource`，只讀已存在 `XtermTerminalRuntime.peekSession` 的活 PTY `viewLeaseId`。 `closeWorkspaceTabs` 進入任何 `await` 前，同步擷取原 Tab／Workspace／Transfer Receipt／view ID；搬移過的 Terminal 必須走 `WorkbenchViewBoundObservedTabCloseStore` 與新可選 Repository Port，`tab.remove` RPC 綁 `expectedViewLeaseId`，Timeout/Disconnect 重播**保持原值**，禁止 fallback unscoped。另加 **mounted Surface count == 1** 條件：0／多個共享 Handle 視圖不授權 Close；無法識別被卸載的其他視窗，這不是完整跨 Window Last-view 協議 | `a4eef3eba` → **`3adc4f465`**；`abc61e686` → **`2ea227339`** |

**下一批 D12 未完成**：物理 OS Window/Device 的 View 生命週期辨識，不是只看 Xterm Handle 的已掛載 Surface；跨 App/Host restart 的 durable Lease、Client token 延續驗證、Mobile 具名 Attach/Detach/Close／legacy fallback、Host SQLite Receipt 與 live PTY owner crash window 的真正可恢復 process-generation 與確定失敗證據。Terminated/Exited moved PTY 目前**保留 Metadata**，因未證實合法退休方法，這不是正常長期清除策略。Journal 鎖的 PID 不可當 liveness；需要另一個獨立的確認所有持有者退出、Host Receipt + 兩 Layout 再讀回與安全解鎖/稽核 UI。還要補足 Terminal watcher 退場清理政策，完成 Flutter/Cargo Tests、Analyzer、Build、Win/Mac 多 App/E2E。

**開放條件仍未達成**：`tab.transfer.capabilities.supported=false`。本輪只使用 `rustfmt`、`dart format` 與 Git whitespace checks；未新增或執行 Cargo/Flutter Tests、Analyzer、Build、Windows/macOS E2E，未 Push/Release，不碰 dirty `main`。接手先讀 `docs/refactor/feature-progress-20261029.md` 最上方 D11，核對最新 Git HEAD 後分線續做 D12。

## 歷史：T2-D10 三條隔離安全工作線已整合（2026-10-30；Capability 仍 OFF）

從 D9 整合乾淨 HEAD `dfc9c2fe7` 重新切三個 D10 隔離分支：`feat/t2-d10-view-registry-20261030`、`feat/t2-d10-view-close-20261030`、`feat/t2-d10-client-leases-20261030`。按 Host Session／Host Transfer Close／Dart Terminal Client 及 Journal 分檔工作；三條獨立提交並無衝突 cherry-pick，D10 程式整合最後 HEAD `6c5571280`，其後仍會因 Docs Commit 前進。

| D10 工作線 | 具體改動、保證與侷限 | Source → Integration |
| --- | --- | --- |
| V：per-View Session Registry | `Session.view_leases` 實際追蹤 `client_id -> Set<viewLeaseId>`，同一 Socket 下多個 Desktop/Phone 視圖各有 Token；`createOrAttach` 可收 `viewLeaseId`、會拒絕跨 Host Client 重用同 Token，`detach` 若帶 Token 只放掉那個視圖，仍有其他視圖就保留 Client 附著與輸出。另以 `unscoped_clients` 記錄同 Socket 的舊版無 Token View，防止與新版混用時被誤當成只有一個視圖；`terminal.attach` mobile 原樣轉傳可選 View ID。Moved Terminal Attach/Detach 缺 Lease 直接拒絕；未搬移的 legacy Terminal 維持兼容。**不是持久化 Lease** | `6ea692df8` → **`dae1db212`** |
| G：Host 全視圖身份檢查＋Recovery 權威分類 | `tab.remove` 追加可選 `expectedViewLeaseId`（**搬移的 Tab 必須提供且 Host 判定唯一 live View**），即使 Runtime Store RPC 另一 Socket，也用原 Token 代替單一 Host Client 當 Last-view 憑證；顯式 `terminate` 同樣要求 requester Host Client 附著的唯一 View Token。若仍有舊版 unscoped View，`tab.transfer` 拒絕搬移，避免隱形視窗被殺。 `tab.transfer.status` 在有另一筆收據或持久 owner/Session 不一致時分別回 `recoveryRequired:true,reason:receipt_conflict`／`persisted_owner_inconsistent`；**這是需要人工核對的未決，不是確定失敗** | `e6262d93a` → **`c22cf91f3`** |
| C：Dart View Lease 傳遞＋本機 Journal 中斷診斷 | `TerminalHostViewLeaseClient` 是狹義可選 Host Capability，不強迫所有 Fake 實作；Production `SocketTerminalHostClient` 支援 attach/detach/terminate 帶 Lease，`TerminalHostPtySession` 每 Instance 隨機 UUID，重連沿用原 ID。Dispose 單一 UI view 不再 detach 共用 Host Client 的其他 Views。補齊 `TerminalHostClient.terminate` observed transfer receipt 介面與兩個 Fake（只格式化，不執行測試）。`FileWorkbenchTerminalTransferJournal.inspectRecovery(tabId)` 可唯讀回應 Lock 是否存在、Pending 是否可讀、是否有匹配 caller-confirmed Local Archive；**不能據此直接刪 Lock 或清 Journal** | `6aa4a890f` → **`6c5571280`** |

**D11 阻擋與相容**：Workbench 真正進入 transferred `tab.remove` 時，尚未將視圖 Session 的 Lease Token 綁定給其 `expectedViewLeaseId`，因此 **搬移 Tab Close 目前仍會拒絕**；必須把同一 Token 交由目前 Tab 的真正 UI owner，避免舊窗口代拿最新 Host token 造成 ABA。Mobile 客戶端舊版沒有 view ID，搬移後 Attach/Terminate/Close 安全拒絕，待新版本完整接線；Host Session 記憶體 Lease 遇 Host Restart 消失，原 PTY 不能自動重鑄，仍需持久跨程序身份、可稽核 PTY generation renewal／definitive failure 或補償、正常退出後已轉移 Tab 的合法清理。Journal 的 `.lock` 只允許**唯讀檢查**，manual stale-lock repair 必須先有跨程序沒有持鎖者的證據及 Host receipt/雙 Layout 驗證，否則不能解鎖；稽核保留政策與 UI 尚未做。仍欠 Flutter/Cargo、Analyzer、Build、Windows/macOS 多程序實測。

**嚴格不變**：Host `tab.transfer.capabilities.supported=false`，不啟用真正拖放；只執行 Rust/Dart 格式化與 Git whitespace check，不新增／執行 Flutter/Cargo Tests、Analyzer、Build 或實機 E2E，不 push、release、merge/reset/clean dirty `main`。下一輪讀 `docs/refactor/feature-progress-20261029.md` 最上方 D10 與本文件 D10，重新查 HEAD，再從 D11 未完成項目繼續。

**D10 UI 補正**：原始收據衝突與持久 owner 不一致同屬 `recoveryRequired`，不能誤寫「Host 一定已保存原移轉收據」。RuntimePort 註解與 Sidebar 未決警示已修改，獨立來源 `e38e0c9c7` → 整合 **`c7def499b`**。仍保留原 Mutation、Journal 與操作阻擋；沒有宣告確定成功／失敗。

## 歷史：T2-D9 三條隔離安全工作線已整合（2026-10-30；Capability 仍 OFF）

**Git**：從整合乾淨基準 `632f1429b`，重用三個 D8 工作樹新切 `feat/t2-d9-owner-close-20261030`／`feat/t2-d9-pty-remint-20261030`／`feat/t2-d9-audited-journal-20261030`。各線 owner 原封不動，只寫 Host Close／Host Spawn + Transfer／Dart Journal 的分離檔案；獨立提交後無衝突 cherry-pick，追加 Host Receipt Cascade 修正，原始碼整合最後 HEAD `75161c9bf`（此文件提交後還會更動，務必重新核對）。

| 工作線 | 本輪精確實作 | 來源 → 整合 |
| --- | --- | --- |
| D9-H Host Close／Cascade | Queued `PrepareRuntimeMutation` 帶真正 Client ID；對 transferred `tab.remove` 先用原收據向 Host `tab.transfer.status` 查證原 instance、目前 Session/Tab owner，再限定要求者已 attached、沒有其他 attached Client；失敗時保留原 Tab。Workspace／Project／Managed／Sleep／Workspace Tabs Delete 若會移除**任何持久化 transferred Tab**，即使原 PTY 已不在也拒絕；若在 SQLite owner 已更新但 Session 仍停在舊 Workspace 的 partial commit，舊 Workspace Cleanup 也拒絕。這不等於多 Window 共用單 Client 的獨立 View Lease | `c9468fd77` → **`3f1cf30d7`**；`3753d60d7` → **`75161c9bf`** |
| D9-R 防止 original PTY Remint | 已移轉的 Tab 在 Attach／CreateOrAttach 必須有仍在執行且 `ptyInstanceToken` 匹配的原 PTY。重啟後缺失/新生同 Session ID 不許自動生成 Shell；`spawnOnCreate` 和 `tab.upsert` spawn-error 不再刪 moved Tab／殺原程序；直接 `terminal.restart` 也保守拒絕，直到新增獨立的可稽核 PTY generation 更換協議 | `7775ff028` → **`f10fb0f52`** |
| D9-J 同 App Support 路徑跨程序鎖與清除稽核 | `FileWorkbenchTerminalTransferJournal` 的 `remember`／`forget` 取得 `<tab-hash>.lock`，由 OS exclusive File.create 跨程序協調；Interrupted Lock 被 `loadPending` 當作需要人工調查而非自動清。僅在 Controller 已確認 Host 原始收據與雙 Layout 權威 readback 後，`forget` 先寫入完整 Plan + `callerConfirmedHostAndBothLayouts` + UTC 時間的 `terminal_transfer_resolved_v1` archive，核對後才刪 pending。若無 pending 又無符合的 archive 不能假裝已清。此 audit **不是 Host 授權簽章**，也沒有同跨裝置 lock | `2d9244d25` → **`f536c68b6`** |

**D10 尚未解決的功能核心**：目前採用**保守拒絕而非自動復原**。真正的 View ID／Last-view Lease 要同時覆蓋多 App、多 Window 共用同個 Host client、Mobile 及 Host restart；需要合法、安全的 transferred PTY 正常退出／顯式終止後 Tab 關閉協議，否則已轉移而 dead 的 Tab 將持續不可批次刪除。SQLite Commit 與 Live PTY owner 之間 Host Crash 不會因 Instance Token 而自動補償或復活 PTY；應定義已持久化但 PTY 消失的 definitive success/failure／人工介入證據、PTY generation renewal，而非隨意解除 Journal。跨 App `.lock` 後的 crash-stale 手工修復、稽核檔保留／清理政策、Host 明確拒絕、Mobile 舊客戶端相容與 Windows/macOS E2E 必須另外設計、驗證。

**開放條件仍未達成**：`tab.transfer.capabilities.supported=false`。只做 Rust/Dart Format 和 Git scoped whitespace checks；**沒有新增／執行 Flutter/Cargo 測試、Analyzer、Cargo check、Build、Win/Mac 多程序實機、Push、Release**，沒有修改 dirty `main`。實際啟用前必須依使用者既有政策另行恢復驗證，不能從這些尚未編譯的修改推論功能已可用。接手以 `docs/refactor/feature-progress-20261029.md` 最上方 D9 入口為準，再往 D10。

## 歷史：T2-D8 三條隔離安全工作線已整合（2026-10-30；Capability 仍 OFF）

從乾淨整合 `379ec62cadf04686255dbb094101a1b2e9c345d7` 重用既有 Host／Rust transfer／Desktop Client 工作樹，分別切 `feat/t2-d8-cascade-20261030`、`feat/t2-d8-pty-identity-20261030`、`feat/t2-d8-journal-concurrency-20261030`。Host Cascade 專責 `session_termination.rs`、`runtime_mutation_queue.rs`；PTY Identity 專責 `terminal_tab_transfer.rs`、`store_tab_transfer.rs`；Desktop 專責 `workbench_controller_tabs.dart`、`workbench_terminal_tab_transfer_coordinator.dart`。各線完整範圍無同檔寫入／衝突：

| 線路 | 確實做了什麼／不保證什麼 | 隔離 → 整合 Commit |
| --- | --- | --- |
| D8-H Host cascade | `RemoveWorkspace`、`RemoveProject`、`RemoveProjectWorkspaces` 以及現在會進 Actor preflight 的 `RemoveWorkspaceTabs`、`SleepWorkspace`，只要涉及**仍在執行且有 Host 客戶端附著**的 transferred Terminal，就拒絕背景刪除；`RemoveManagedWorkspace` 也先擋再做不可逆的 Close Sessions。沒有自動 Detach 其他視圖；全數已 Detach 的 Session 不在此防護，亦非持久 Lease | `6fa141bfc` → **`4bd4a8f96`** |
| D8-I Store + Host 原始 PTY 證據 | 新增 `ptyInstanceToken = random process epoch + Session.instance_id`，在 transfer 的**同一 SQLite Transaction** 存進 `lastTerminalTabTransfer`，維持先前所有 CAS／來源布局保護。Host `tab.transfer.status` 對原 Receipt、來源目的、活 PTY 核對後，還要求 Token 一致；Host 重啟／Shell remint（相同 Session ID）或舊版沒有 Token 的收據回 `applied:false,recoveryRequired:true,reason:pty_instance_unverified`。直接重送同一 `tab.transfer` 若 Token 不符也拒絕，**不能透過相同 ID 欺騙成功** | `28612d716` → **`463725075`** |
| D8-J Desktop 未決查證排他 | `confirmUnknownTerminalTransfer` 進入時先以原 Journal Mutation ID 占用同 Tab 的 transfer guard，直到確證收據、雙 Layout 權威重讀、Journal 比對刪除及 UI fence 釋放。重複點 `Check status` 或再拖曳同 Tab 被拒絕；Host 不可核實時仍保留原 Journal。這是單 Desktop coordinator 期間的排他，不是不同 App 程序間的全域鎖 | `b5b654d24` → **`0ffb9b3a3`** |

**仍阻擋啟用的 D9 工作**：單 Host Actor 規則不等於跨程序／跨機器最後視圖 Lease，也沒把每筆 `tab.remove` 與發出要求的 Client ID、原視圖 Epoch 透過同一原子 close 契約收攏；對沒有附著觀看者的已轉移 PTY 做 Workspace／Project 清理的政策、跨 Host 關閉與 Mobile 舊版要再明定。Host Crash 時 SQLite 已持久化但原 PTY 消失，新的 Token **只能拒絕新程序冒充**，不能保證修復、補償、確定失敗，也不能直接把 Journal 清掉；需有可稽核的最終狀態與安全恢復／人工介入方案，還有正式 Flutter/Cargo 測試、靜態分析、編譯與 Windows/macOS 雙程序實機驗證。

**本批嚴格限制**：只有 Rust／Dart 格式化與 Git diff、staged whitespace check，**沒有**新增或執行測試、Analyze、Cargo check、Build、Win/Mac E2E、Push、Release；原始 dirty `main` 未變。保留 `tab.transfer.capabilities.supported=false`，不可把此文件當成已正式支援或編譯成功的證明。下一次接手先讀 `docs/refactor/feature-progress-20261029.md` **最上方 D8 入口**，重新核對 HEAD，再做 D9。

## 歷史：T2-D7 三線隔離已整合（2026-10-30；Capability 仍 OFF）

從**乾淨最新整合基準 `f9b36ac29`**，重用三個原本乾淨的 D6 工作樹，切 `feat/t2-d7-host-20261030`／`feat/t2-d7-client-20261030`／`feat/t2-d7-contract-20261030`，無新建大型 checkout，且按檔案擁有者分離 Rust Host、Dart Client/UI、兩份契約文件。各線均獨立提交，cherry-pick 至 `refactor/tdd-integrate-20261029` 無衝突：

| 線路 | 已實作且有範圍限制的行為 | 獨立提交 → 整合提交 |
| --- | --- | --- |
| D7-H Rust Host | `RuntimeMutationQueue` 記錄正在執行的 Tab Remove、辨認尚在排隊的同 Tab Remove。 `verify_terminal_attachment_owner`／`verify_terminal_termination_owner` 於讀 SQLite 前拒絕在移除空窗中新的 Attach／Terminate；收據已成功寫入 Store 但活 PTY 缺失／owner 不符時 `tab.transfer.status` 回 `recoveryRequired:true` 與明確 `pty_missing`／`pty_owner_mismatch`，不冒充 `applied:true` | `a6f810b04` → **`291a1c4c2`** |
| D7-C Dart Client／Workbench UX | Desktop Runtime Port 對 `recoveryRequired` 丟出具有 `hostRecoveryRequired` 標記的 Unknown Outcome；Coordinator 不吞掉此明確狀態，也不清除同 Tab 原 Mutation／App Support Journal；側邊欄顯示 Host 已存收據但無法核實執行中 PTY 的訊息 | `2cfe61566` → **`c32d3662f`** |
| D7-O Terminal／Tab 契約 | 明訂 `tab.remove` 排隊至 Session 關閉的 Actor fence 與轉移回覆三態：確定有原收據＋活 PTY／無對應收據／原收據存在但缺活 PTY；完整說明此保護不等於跨程序持久租約 | `6a7306e9a` → **`375c89568`** |

**尚未完成的 D8 安全需求，不可誤宣稱 D7 已解決**：

1. **跨程序 Last-view Lease**：Actor 的 `Session.clients` 和本輪 `RemoveTab` fence 是單一 Host 執行期狀態，不是獨立程序／Host restart 後的可信 Lease；`RemoveWorkspace`／`RemoveProject`／`tab.removeForWorkspace` 也需獨立檢視並設拒絕策略，保證其他視圖的 PTY 不被關閉。
2. **Crash partial commit 正式復原**：目前 `recoveryRequired:true` 只是顯示「持久收據與活 PTY 不一致」；沒有重掛真正原 PTY、沒有證明確定失敗、沒有補償／原子回滾，也不能因此自動重新建立 Shell 或解除未知狀態。
3. **Journal／UX 完成態**：正確處理 Host 明確拒絕、Store/PTY 不一致、Host 重新上線仍無 PTY、保留或人工安全解除 Journal 的權威證據。保護 A→B→A 收據、同 Tab 跨程序排他與 Mobile 舊版拒絕，不能靜默放行第二次轉移。
4. **恢復正式驗證後才談啟用**：Flutter/Cargo targeted tests、Analyzer、Build、Windows/macOS 雙視窗／雙程序 Close/Detach/Attach/Output、Lost-reply、Restart、Watcher、Undo、Mobile 相容 E2E；本輪沒有執行。

本 D7 僅執行 Rust/Dart 格式化與 Git whitespace check，**沒有新增測試、Cargo check、Flutter analyze/test、Build、實機、Push 或 Release**。原始 `main` HEAD `3d232acff` 仍 dirty 且完全未修改；Host `tab.transfer.capabilities.supported=false` 保持不變。接手以 `docs/refactor/feature-progress-20261029.md` 最上方 D7 入口為準，勿從舊 D1／B2「待辦」重做。

## 歷史：T2-D6 四條平行安全工作線已整合（2026-10-30；Capability 仍 OFF）

從**乾淨整合基準 `6bdb0cbb837de2a0c2e6d9c54435d54ed7e9e12d`**，重用四個確認乾淨的既有 Worktree 分別建立獨立 D6 分支（避免 Windows/Dropbox 大型重新檢出），各線獨立格式化、Git staged whitespace check、提交，然後依序 cherry-pick 回 `refactor/tdd-integrate-20261029`，沒有 Git 衝突。

| 線路 | 已修改、實際保證 | 獨立提交 → 整合提交 |
| --- | --- | --- |
| D6-H：Host `tab.remove` 強制轉移收據 | **所有**已轉移 Terminal 的序列化移除請求都要攜帶與持久化 `lastTerminalTabTransfer.id` 相符的 `expectedTransferMutationId`，不再只因 Workspace ID 相同便允許舊 Desktop 視窗刪除（防 A→B→A）。未移轉的 legacy Tab 保持原契約 | `19a9c3dd0` → **`8a9e78d83`** |
| D6-S：Rust Store 精確 Pane 剪枝 | 來源 Pane 消失時，重算 `WorkbenchLayoutNode.pruneGroups`：只允許移除空群組並提升原本的存活子樹；保留其他 Split 的 axis/ratio、剩餘 Pane 與 activeGroupId，不准暗中改變布局；最後一個來源 Pane 則必須變成空的預設 Pane | `7ea2bf150` → **`74f8062a9`** |
| D6-J：Desktop 未決 Journal 原子建立 | `FileWorkbenchTerminalTransferJournal.remember` 由 `exists + temp.rename` 改為檔案系統 `File.create(exclusive:true)`，競爭的另一個 App 程序不得覆寫相同 Tab 的原 Mutation ID。中斷時保留損壞或空 Journal 供安全拒絕，不當成未送出操作 | `b16c0b58` → **`dd71d3401`** |
| D6-C：Desktop Close 使用「視圖所見」收據 | `WorkbenchTabCloseCoordinator` 從使用者原始 Tab Snapshot 傳收據；`WorkspaceTabService`／Runtime repository 使用可選 observed close port 將相同 Workspace／Receipt 傳給 Host `tab.remove`，含 timeout/disconnect 的同一操作重送。Fake/舊 repository 不支援搬移後 Close 時安全拒絕，不會在 Close 前偷讀最新收據 | `b63c637d1` → **`c23dd4189`** |

**仍未完成且不可啟用**：這次的 Journal 排他建立只防**同機同路徑**的原操作覆寫，**不是跨程序／跨機器 PTY lease**；Host Crash 後 SQLite 已提交、live Session 已消失的復原／拒絕結果的 Journal 安全清理 UX、Attach/Detach/Close 跨程序最後視圖協調，以及完整的 Windows/macOS 多程序實測尚未解決。Rust Store/Client 仍**沒有** Flutter/Cargo 編譯、測試／靜態分析或實機通過證據，不能將 `tab.transfer.capabilities.supported` 設為 true。原始 dirty `main` 未更動、未 push、未 release。以下 D5+ 文件為歷史實作範圍，遇到衝突以 D6 本節為準。

## 歷史：T2-D5+ 追加三條平行安全工作線（2026-10-30；Capability 仍 OFF）

從最新乾淨整合 `7b7b3b877` 重用三個乾淨隔離 worktree，各切新 D5 branch。實作期間另一組 D5 Store／Workbench／Agent watcher 修正已合入 `0b95fbf7b`；確認**變更檔案互不重疊**後，將本組三個提交 cherry-pick 到該最新整合分支，無衝突。

| 工作線 | 本輪確實修改 | 原始提交 → 整合提交 |
| --- | --- | --- |
| D5+-H：Rust Host 多視圖阻擋與 Tab Remove 收據 | **已轉移的 Terminal** 在 `terminate` 發現其他已附著 Host client 時，拒絕殺死 PTY；序列化的 `tab.remove` 新增 Host Actor 執行前的多視圖 preflight，防止背景 SQLite 移除後才發現其他使用者。若 `tab.remove` 帶 `expectedTransferMutationId`，會核對目前持久化最後收據，阻擋 A→B→A 的舊手機畫面。未移轉 Terminal 保留舊契約 | `bcd7f7cb0` → **`32f365b79`** |
| D5+-J：Desktop 不明轉移持久 Journal | `FileWorkbenchTerminalTransferJournal` 使用 App Support Directory，以 Tab ID 雜湊獨立存放操作計畫；**RPC 送出前**持久化原 `clientMutationId` 與雙側布局，App 重啟先載入。收據符合並且雙側 Layout 已權威回讀後才清除；損壞／無法讀取則 fail closed，不自動重送或假設成功 | `1e85f25a1` → **`db7eca96b`** |
| D5+-M：Mobile 觀察歸屬的安全關閉 | Mobile 新增相容現有介面的 observed-terminal close，從使用者畫面當時的 `WorkspaceTabSummary` 傳 `expectedWorkspaceId`、`expectedTabId`、`expectedTransferMutationId`；Agent Tab 關閉前重新取得指定 Workspace 的對應 Tab，確認仍相同；**轉移後的終止拒絕不再被吞掉並繼續刪除**。舊 fake/client 不支援時對移轉 Tab fail closed | `bef757b7d` → **`ea881429b`** |

**界線／下一步**：Host 的 attached-client preflight **不是跨程序持久租約**，兩個 Client Close/Attach 與 queued mutation 仍須有更完整的 owner/lease protocol 與驗證；本機 Journal 是**原操作記錄**，不是跨程序分散式鎖，Host Crash 若只持久化 Tab/Layout 但 live PTY 不見，不能僅據 DB 宣稱轉移成功；Host 明確拒絕操作後留下的未決 Journal 需要可核實的安全清理 UX。Transferred Terminal 的 Desktop legacy `tab.remove` 若未帶收據，仍只用 Workspace scope，尚有 A→B→A ABA 的狹義缺口。Mobile 最新客戶端已帶收據，舊 Mobile 對已移轉 Tab 仍被拒絕。**必須在恢復目標測試、Cargo/Flutter 編譯、Windows/macOS 多程序實機驗證後，才能考慮把 `tab.transfer.capabilities.supported` 改為 true**。本批只格式化和 Git staged whitespace check；原始 dirty `main` 未改，未 push／release。

## 最新：T2-D5 三線平行安全補強（2026-10-30；仍未啟用）

D5 從乾淨 D3 基準 `1464828459c66b66c281a1a0664232af6aafcb21` 出發，**重用三個確認乾淨的 B2 工作樹**並切新分支；D4 同時在其他工作樹推進並合入整合分支 `7b7b3b877`，三條 D5 只修改互不重疊的單一檔案，接著依序 cherry-pick 至**已含 D4** 的整合分支。

| 工作線 | 實際修正 | 獨立提交 → 整合提交 |
| --- | --- | --- |
| D5-S Rust Store | `transfer_terminal_tab_storage` 在原子雙 Layout CAS／Tab membership 檢查之外，限制**來源僅移除被轉移的 Terminal**、目的僅把該 Tab 加入單一 Pane；其餘 Pane 的 Tab 順序與 Active Tab 不得偷偷改寫，目的 Split Tree 及來源未修剪 Pane 時的幾何結構不得改動。來源刪除空 Pane 時的**精確 root prune 比對仍待補** | `416dca6bf` → **`5b4d15db5`** |
| D5-C Workbench readback | `WorkbenchTerminalTransferReconciliation` 驗證來源／目的各自完整 Tab 清單、Workspace 身分、唯一 Pane membership、Active Tab 與 Split Tree，再**二次讀回**兩側 Tabs＋Layouts 核對是否發生併發變動；未通過時**不套用舊 UI 狀態，也不反寫 DB** | `6c5e3505a` → **`a7a673591`** |
| D5-L Agent Session lifecycle | `WorkbenchRetiredResourceCleaner` 在 watcher 驅動的 Tab/Workspace 退役時只釋放本地畫面／編輯器，**不因來源 Tab 消失便停止仍存活的 Codex transcript watcher**。只有外部明確確認 PTY 已結束時，才可指定 `confirmedSessionExit:true` 清除追蹤；explicit close 仍由原本獨立清理流程處理 | `c0f1fa5c0` → **`9d420bf8d`** |

**限制與後續**：二次讀回不是跨 Workspace 原子 Snapshot，也尚未有跨程序／跨視窗 PTY 最後擁有者 Lease；Host Crash 後 Store 已更新但 Session 不在記憶體、跨 App 重啟保留未知 Mutation Journal、Mobile 端相容、Agent watcher 確認退出後的最終清理與 Win/Mac 實機驗證仍需處理。Host `tab.transfer.capabilities.supported=false` **不變**。本輪只有 `rustfmt`、`dart format` 和 staged Git whitespace check；**沒有測試、Analyze、Build、push、release，沒有動原始 dirty `main`**。

## 最新：T2-D4 三線安全修正（2026-10-30；仍未啟用）

確認基準為乾淨整合 `1464828459c66b66c281a1a0664232af6aafcb21`，沿用 D3 乾淨獨立工作樹切換到 D4 新分支，沒有重新 checkout 全專案。啟用旗標 `tab.transfer.capabilities.supported` **仍為 false**。

| 工作線 | 實際程式碼交付 | 獨立提交 → 整合提交 |
| --- | --- | --- |
| D4-H Rust Host 終止 ABA 防護 | `verify_terminal_termination_owner` 在已轉移過的 Terminal 上額外核對視圖送出的 `expectedTransferMutationId` 與 Host 持久化 `lastTerminalTabTransfer.id`。A→B→A 即使 Workspace/Tab ID 再次相同，仍拒絕舊收據終止；從未轉移的舊 Tab 無收據，維持 legacy 行為 | `2179039f1` → **`e74cda072`** |
| D4-C Dart 未知結果二次確認 | Coordinator 保存精確原操作 `WorkbenchTerminalTabTransferPlan`，收到 Host 權威收據＋活 PTY 確認後，**仍維持 unknown guard，直到來源及目的 Tabs/Layout 以 `persist:false` 回讀完成**才解除。Controller 新增 `confirmUnknownTerminalTransfer`，失敗或斷線保持原 mutation 未解決；錯誤內容不符的轉移回覆也視為未知、不能宣稱確定失敗 | `4c827a88f` → **`ad35694ac`** |
| D4-U Dart 終端視圖與提示 | Terminal Host client 終止請求帶入**本視圖觀察到的**最後轉移收據，不會關閉前偷查最新 ID；Terminal `sync` 與新建 PTY handle 分別更新本地觀察值。Sidebar 對失聯或逾時回覆顯示「結果未知」及「Check status」操作，查證成功才重讀兩側 Layout，不會自動重試或顯示假成功 | `e5dcf1f87` → **`469b64c54`** |

**仍待啟用前處理**：Host Crash/Restart 時「Tab 已提交但活 PTY 已失效」要有可解決的 recovery 狀態；跨**不同程序**共享 Session 的最後一個視圖、Detach/Close 與輸出游標順序尚未有真正端到端的 ownership lease；斷線後持續跨 App 重啟的 unknown mutation journal 尚未實作。Transferred Terminal 的 Mobile 舊版 terminate 未帶最後收據會**安全拒絕**，需另設 Mobile 相容 UX。關鍵路徑的 Flutter/Cargo 編譯、實際多視窗 Win/Mac 測試與操作契約驗證**仍未執行**。本批只有格式化與 Git staged whitespace check；不推送、不發布，不修改原始 dirty `main`。

## 最新：T2-D3 三線平行修正（2026-10-30；尚未啟用）

以 D2 整合 HEAD `ca42a1be76e967580921f3ece25299765c5eba35` 為基準，重用乾淨既有 D2 worktree、各切新 D3 分支，避免 Windows Dropbox 同步資料夾大量重建 checkout。

| D3 工作線 | 確實修補 | 獨立 → 整合 commit |
| --- | --- | --- |
| D3-H：Rust Store 的全 Host Layout membership guard | `layout.upsert` 與 transfer row 讀取置於**同一 SQLite transaction**，對每個已轉移過的 Tab 檢查建議布局：在目前所屬 Workspace 必須包含它，在其他 Workspace 不得再引用；舊視窗使用 legacy `layout.upsert` 也無法繞過 Flutter watcher 保護 | `01109e5c1` → **`4c6fc975d`** |
| D3-C：Client ambiguous transfer 收據 | 逾時或斷線後查不到 Host 權威 Receipt，就在 Coordinator 以同一 Tab 儲存**原 Mutation ID 與原操作計畫**；不准新 Drop 直接 mint 不同 ID。新增 `WorkbenchTerminalTabTransferRecoveryPort.confirmCommitted`，只有 Host 原收據＋活 Session 佐證成功才釋放本機未知狀態 | `69a81acd7` → **`76e093931`** |
| D3-O：Terminal 原執行資料夾與 Restart | Dart 在**明確 Restart** 前檢查預期工作目錄，拒絕不存在的原啟動路徑；Rust Host 的 `createOrAttach` 在活 PTY 存在時仍准 Attach，只有需要建新程序時才檢查移動後的原路徑；`terminal.restart` 在退出與清除 Agent 前比對原路徑（canonical）；`spawnOnCreate` 以原路徑重建，若失敗則保存已移轉 Tab 而不是刪除 | `e9e22859d` → **`0809bbe3f`** |

**嚴格界線**：D3-H 只守護**有持久化轉移收據的 Terminal membership**，不等於全系統每一筆 Layout 都有 Revision/CAS；D3-C 的未知操作在證實完成之前維持阻擋，不是無條件重試；D3-O 儲存的是**原啟動目錄**，不是偵測 Shell 在執行中 `cd` 之後的目前 CWD。功能旗標仍為 `tab.transfer.capabilities.supported=false`，未做跨程序多視窗 PTY Lease/Close/Detach 可靠性整合，也未跑 Flutter/Cargo 測試、Analyze、Build、Windows/macOS 實機。**不可把這三條原始碼提交等同功能已正式支援**。

**下批 T2-D4 可平行分工**：一條 Host/PTY 唯一 owner 負責跨進程終止/Detach 最後使用者的規則與 Crash 重新掛接；另一條處理 Watcher/Client 重新連接後 Layout/Receipt 的權威重新同步；第三條完成 `docs/tab-layout-operation-contract.md`／`docs/terminal-operation-contract.md` 及高風險情境的狀態 UX。先確認目前整合 HEAD 與 clean worktree，避免在同一檔案平行編輯。依使用者先前決定測試/分析暫停，若要打開 capability 必須先恢復核心驗證。

## 最新：T2-D2 再平行三線安全修補（2026-10-30；仍不啟用）

確認前一批 D1 已在整合 HEAD `0b1f9f4416bf263e9c0e7c4a363f5bb6762400c0`；重用三個**已乾淨的 D1 worktree**，各自切出新的 D2 分支，避免 Windows 同步目錄再次大量 checkout。三條修改分別提交後按 Host → Client → Watcher cherry-pick 至 `refactor/tdd-integrate-20261029`。

| 平行工作線 | 本輪具體修正 | 隔離 commit → 整合 commit |
| --- | --- | --- |
| D2-H：Rust Host 所有權邊界 | `terminate` 在具轉移收據的 Session 上要求目前的 `expectedWorkspaceId`＋`expectedTabId`；拒絕舊視窗直接終止已移轉 PTY；`createOrAttach`／`restart` 拒絕舊 Workspace 重新附加或錯誤重新建立 Shell。序列化 `tab.remove` 改為核對 `expectedWorkspaceId`，具收據的 Terminal **不能僅憑 Tab ID 刪除**；舊視窗拒絕刪除時不會提早取消新位置的 Agent title job。未轉移的 legacy Tab 保持舊操作。 | `f215ce93a` → **`df69aece0`** |
| D2-C：Desktop Client 關閉身分 | PTY `terminate` 請求攜帶當前檢視預期的 Workspace／Tab；Workbench Tab Close 透過獨立的 scoped close port 提供原畫面 Workspace，Runtime Repository 重送同一個 `expectedWorkspaceId`，避免 timeout 後誤刪已移動的 Tab。若同視窗沿用既有 Xterm Handle，`sync` 只調整後續附加／關閉請求的 display Workspace，不關閉或重啟原 PTY。現有非 Runtime repo/fakes 保持原介面。 | `faab3f691` → **`817fcf7e1`** |
| D2-W：跨視窗 Layout watcher | 偵測新進入目的 Workspace 的 `lastTerminalTabTransfer` 收據，或來源 Workspace 已消失但 Host 確認移動的 Terminal；暫停 provisional sanitized Layout 的背景回寫，改讀 Host 權威 Layout 並以 `persist:false` 套用；加上 watcher epoch／tab 清單比對，避免過期的非同步回覆覆蓋最新配置。普通 Terminal Close 在確認沒有搬移後仍可持久化清理結果。 | `2f0914e41` → **`333afb30e`** |

**本批不是支援已上線／測試已通過的聲明。** `tab.transfer.capabilities.supported` 仍明確為 `false`，實驗性 Host transfer 不會被側邊欄接受為可用功能。本次只做 Rust／Dart 格式化與 scoped Git diff/whitespace check，**沒有 Cargo check、Flutter Analyzer、Tests、Build、Windows/macOS 實機驗證，也沒有 merge 到原始 `main`、push 或 release**。原始 `main` 的未提交內容未動。

**剩餘可平行項目**：跨程序共用 Session 的 Close/Detach／輸出交錯與最後一個視窗釋放規則、Runtime crash 後 Store 已提交而 PTY 尚未重新掛接的復原、原始執行目錄不存在時的安全錯誤提示、Watcher 舊版 `layout.upsert` 的 Host 端全域樂觀鎖、跨端能力差異與 operation contracts、Undo/timeout 的 UI 恢復細節。必須完成關鍵路徑的驗證才可開啟 Host capability，不可只靠 format/check 判定可用。

## 最新：T2-D1 再平行三線合入（2026-10-29，仍停用）

從乾淨整合 HEAD `97f21ab02e23cb7eba39791080dedd7f52bd10ae` 分出三個獨立 worktree，各自完成 scoped 修改、`rustfmt`／`dart format` 與 `git diff --cached --check`，獨立 commit 後依 Host→Recovery→Undo 順序 cherry-pick 回整合分支（無合併衝突）：

| 線路 | 本批完成的實際功能／保護 | 隔離 → 整合提交 |
| --- | --- | --- |
| T2-D1 Host / race | `tab.transfer` 遇到背景 Runtime Mutation、來源或目的 Workspace 尚有未完成 shutdown 時拒絕移轉；來源 Workspace 切換後，Host 即時發送 Agent Presence refresh；杜絕舊視窗的 Terminal `tab.upsert` 將已移轉 Tab 靜默搬回原 Workspace。Host-owned `lastTerminalTabTransfer`、原 Execution Workspace、launch directory 在後續 Tab 更新時不可被覆蓋。轉移收據記錄先前 `sourceGroupId`。新增只讀 `tab.transfer.status`，以**精確 Mutation ID＋資料庫收據＋仍在執行且歸屬目的 Workspace 的 PTY**共同確認不明結果。 | `c1f8aa185` → **`43591a4be`** |
| T2-D1 Recovery / lost reply | Dart 轉移 RPC 收到 Timeout／Connection Closed，先用同一 `clientMutationId` 呼叫 Host `tab.transfer.status`，只有收據、Tab、Session、目的地全部一致才能視為已提交。若 Host 無法確認，回報 `TerminalTransferOutcomeUnknownException`，**不重送新 Mutation、不將未知視為失敗成功**。 | `c340bc6fb` → **`5a0f01bdd`** |
| T2-D1 Undo / UX | Controller 正向轉移成功後傳回原 Host mutation receipt id；在側欄顯示 10 秒的 Undo Snackbar。點擊 Undo 時再查目前權威 Tab 收據，只有**最後一次**轉移的 ID 相符、來源仍存在且未封存才用另一筆 `tab.transfer` 反向移回；原 pane group 尚在則恢復到原 group，否則用目前的 active group。**同一個原 PTY 不重啟、不重建**。 | `8ddc98a97` → **`efce4bea6`** |

**這批仍未解決／不可啟用的界線**：Host `tab.transfer.capabilities` 依然回報 `supported:false`；需明確驗證 Terminal explicit close／Detach 在多視窗與多程序同時發生時不會終止已移轉 PTY；清理後若來源 Workspace／原啟動路徑不存在，Shell Restart／Hook 仍需安全降級；確認 Watcher fence、同步、Undo 回讀與不同 process runtime owner 的行為。Host 暫時拒絕有 Pulse、Automation／Orchestration、Pending Agent Prompt 的 Session 移轉，這些拒絕條件及提示仍須產品級整理。正式開關前也要更新 `docs/tab-layout-operation-contract.md` 和 `docs/terminal-operation-contract.md` 並執行目標測試／編譯；目前依使用者先前指示**沒有測試、Analyze、Cargo check、Build 或實機驗證**，不宣稱 green、未推送／發布。

**本批 Git worktree 初始化事件**：三個新 worktree 在 Windows＋同步資料夾大量檔案 checkout 時超過初始命令時限，留下部分檢出與零長度工作樹專用 `index.lock`；已核對無運行中 Git 程序，只針對**這三個新 worktree** 修復缺少的 tracked files、用 `git reset --mixed HEAD` 修復不完整的 index、解除 `locked initializing`，三線提交前均確認 clean。**原本 dirty 的 `main` 和既存工作樹未清除或重設。**

## 最新：T2-B2 再平行三線合入（2026-10-29，仍停用）

本批從整合 `845886d57879b9a5b5801e55ab2d6b549ccb915f` 切出三個**互不修改同檔**的功能 worktree，分別完成來源碼修改、格式化、Git staged whitespace check 與獨立提交；再按 Host→Workbench→Terminal UX 順序 cherry-pick 回 `refactor/tdd-integrate-20261029`。

| 工作線 | 新增內容 | 隔離提交 → 整合提交 | 嚴格限制 |
| --- | --- | --- | --- |
| T2-B2 Rust Host | 新增本地 `tab.transfer` Actor route：要求活 PTY／Session ID、同一來源 Tab，對接已實作的 Rust Store CAS；一個交易提交後同步修改活 Session 的顯示 `workspace_id`，**Session ID／PTY process／output buffer／working_directory 不變**，通知來源和目的 Tab watchers 與 Layout watchers；原 Claude/Codex Hook 可用持久化來源身分驗證，再重新歸到目前顯示 Workspace。新增 `terminalExecutionWorkingDirectory` 保存原啟動資料夾 | `a9985034a` → **`f3977fa99`** | 針對已開啟 Pulse、Automation-owned Tab、Pending Agent Prompt、活 Orchestration Dispatch、同 Tab 多個活 PTY 明確拒絕。直接 RPC 存在但**未經編譯／測試**；`tab.transfer.capabilities` 刻意返回 `supported:false` |
| T2-C1 Flutter Workbench | 新增來源/目的 Workspace watcher fence，避免單側舊 snapshot 把 Rust Store CAS Layout 回寫覆蓋；Host ACK 後讀回兩側 Tabs/Layout **只套用不持久化**，再切換目的地／focus；Runtime transfer capability probe、Provider/Controller/Sidebar callback 接線 | `7d99dc6a5` → **`8443c2f0e`** | Host `supported:false` 時 Sidebar Callback 仍是 `null`，不會接受跨專案拖放；timeout/lost reply、遠端重連與旁路 watcher 等尚未完整驗證 |
| T2-D0 Terminal UX／重新啟動 | Terminal Tab 若顯示 Workspace 與原始 Execution Workspace 不同，顯示來源及原始啟動目錄的 Tooltip；後續重新建立 PTY（顯式 Restart／Host recovery）時，使用保留的原始啟動路徑及原 Agent Hook Workspace ID，避免突然變成目的專案的命令環境 | `359353a10` → **`683b99dc8`** | 保存的是**原啟動路徑**，不是偵測使用者後續 `cd` 的即時 CWD；原路徑失效／跨平台 Shell 實際行為尚未驗證 |

**啟用前仍需解決**：跨視窗／跨程序 Detach+Close 同時執行的終止競態、Host 不明結果／斷線後的冪等重播與讀回、Watcher 更新序與跨 client 事件、Agent Presence 轉移即時性、Pulse／Orchestration 限制下的拒絕 UX、來源 Workspace 已刪除時的重開處理與 Undo、更新兩份 operation contract。依先前使用者指示**本批只格式化／Git diff check，沒有 Flutter/Cargo Tests、Analyzer、Build、實機測試，也未 push／release；禁止宣稱正式支援**。三線程式碼在隔離整合分支，原始 dirty `main` 未動。

歷史 Feature worktree：`.worktrees/feat-cross-project-terminal-tabs-20261029`；歷史基準 `e1ef633ca87e408c0ab5e651c1e827bc161a51f6`。**真正實作時必須從當時最新、乾淨的整合分支另切工作樹，不能沿用此舊基準直接修改。**

## 最新：T2 三條平行功能基礎（2026-10-29）

從乾淨整合 HEAD `061216941` 切出三個隔離 worktree 並各自提交，之後成功 cherry-pick 回 `refactor/tdd-integrate-20261029`：

| 工作線 | 已新增的來源碼／能力 | 隔離提交 → 整合提交 | 尚未完成 |
| --- | --- | --- | --- |
| T2-A Flutter UI | `WorkbenchTabDragData` 增加 tab kind／來源 Workspace／Host，Workspace 側欄和 Project 標題有 Terminal-only Drop Target、同 Host 與未封存限制；Project 多個 Workspace 時顯示選擇對話框 | `42c28ea1e` → **`8c119f0a1`** | 接收 callback 刻意留空（null），沒有 Host 支援時不會接受跨 Workspace Drop；仍未接線實際轉移與 Undo |
| T2-B1 Rust Store | `RuntimeStore::transfer_terminal_tab_storage` 的 SQLite 原子 compare-and-swap：核對來源／目的 Workspaces、Host、終端種類、兩側 Layout 快照與所有 Tab membership，一個交易更新 Tab 歸屬和兩側 Layout；保存來源 execution workspace 和 mutation receipt，方便回應遺失後辨識重送 | `800124981` → **`0fc27062b`** | 目前**只有 Store 方法**，沒有 `tab.transfer` Actor route、Session owner 轉移或 watcher 廣播；此方法在正式 Host flow 中尚未使用 |
| T2-C0 Dart 協調準備 | `WorkbenchTerminalTabTransferCoordinator`、窄 `WorkbenchTerminalTabTransferPort`、`RuntimeTerminalTabTransferPort`：先取得雙側布局與 Tab 清單、建立預期值／新布局、限制同時移動同一 Tab、送出固定 mutation ID，確認返回的 Tab/Session ID | `12cd69a41` → **`97e02eedb`** | 尚未在 Workbench/Riverpod 組合、未呼叫；Client adapter 預備的 `tab.transfer` RPC **還不存在**，禁止先接 UI |

**硬性啟用前提（T2-B2）**：Rust ServerActor 活 Session 的 `workspace_id` 目前同時影響 source workspace 移除時的 Session 終止、terminal restart 的 tab/workspace 驗證、Agent hook `workspace_id` 核對、terminal pulse／自動化路由。必須在同一 Runtime owner 下設計並完成「顯示 Workspace 歸屬」與「實際執行 Workspace／工作目錄」的區分，協調原子 Store commit、活 PTY reconciliation、雙端事件廣播、同時 Detach/Close/Exit、unknown-outcome replay，才可接上 UI callback。**不准直接改寫 Tab DB 的 `workspaceId` 假裝功能已完成**。

本批只執行格式化、差異檢視與 `git diff --cached --check`；**Flutter／Rust 測試、Analyzer、Build 和 macOS/Windows 實機尚未執行，沒有任何 green 或已發版聲明**。既存的同 Workspace 重排、Split、Windows detach 預期保持原狀，但本批尚未回歸驗證。

## Goal and visible behavior

- Drag a Terminal tab onto another project or workspace in the sidebar (select the destination's existing workspace; do not silently create one).
- 拖到「專案」時若存在多個適合的 Workspace，顯示目的地選擇器；無可用 Workspace 時顯示不可放置原因，不在背景建立新的 Workspace。
- Preserve the same tab id, terminal session id, live PTY/process, scrollback, output delivery, and user-entered title. A move is **not** close + create, restart, or a new shell.
- The running process retains its original CWD and inherited environment. The target tab must make this explicit; the app must **not** claim that the process has changed projects.
- Move display placement and persistence to the destination; source layout must remove the tab and destination layout must place it once. Support normal split-tab layout rules and a recoverable error path.
- 轉移完成後能透過**反向轉移／復原**把同一個 Tab 送回原 Workspace，保持原 PTY／Session 不變；Undo 需要再確認原 Workspace 是否仍存在，不可用 close+create 假冒復原。
- 同專案內重新排序、分割面板及 Windows 拖出獨立視窗的既有功能不退化。
- Don't treat detaching a tab into a native window as the same operation. A tab that is being detached/reparented must not be simultaneously transferred.

## Verified current behavior

- `workspace_workbench_tab_strip_drop.dart` only accepts `details.data.workspaceId == workspaceId` for strip and append targets; `workspace_workbench_pane.dart` rejects different workspace ids.
- `_WorkspaceTabDragData` includes the source `workspaceId`, `sourceGroupId`, and `tabId`; `WorkbenchTabLayoutOwnerTabs.moveWorkspaceTab` only mutates the layout of one workspace.
- `WorkspaceTabService` persists the terminal session id in the tab payload. `RuntimeWorkbenchRepository` uses `tab.upsert` and `layout.upsert` separately; they are not an atomic cross-workspace transfer.
- `RuntimeStore.upsert_workspace_tab` can overwrite `workspaceId`, but **must not** be used alone as a transfer API. A live Rust `Session.workspace_id` is separately used to terminate sessions when a workspace is removed; `terminal.restart` rejects workspace/tab metadata mismatches. Updating the DB row alone risks killing the moved PTY or failing a later restart.
- `docs/tab-layout-operation-contract.md` contains no `tab.transfer` contract; `docs/terminal-operation-contract.md` pins the PTY, restart, removal and checkpoint behavior.

## Other refactor: file ownership and collision risks

目前整合分支是 `refactor/tdd-integrate-20261029`；W1/T1/C1 都已有部分實作。**現在已有尚未接線的 Rust Store 原子 transfer 基礎，但仍沒有完整的活 PTY ownership transfer 或可呼叫的 Host RPC。** 最新 T1 `45941055a` 僅保障**同一 Desktop runtime registry** 中多個檢視共用 PTY 不會因關閉其中一個檢視而被殺死，不等於跨專案轉移已完成。

| Concurrent branch / workstream | Observed edits on 2026-10-29 | Overlap decision |
| --- | --- | --- |
| `refactor/tdd-b3-agent-20261029` (A0) | agent profile/adapters, quota icons, settings and tests | No current same-file overlap. Existing running agent processes still require identity-safety tests. |
| `refactor/tdd-b3-git-20261029` (G1) | source-control controller / Git backend query ports | No current same-file overlap. |
| `refactor/tdd-b3-q0-20261029` (Q0) | Git history UI relative-time extraction | No current same-file overlap; Q0's later Terminal file-size split can overlap. |
| `refactor/tdd-b3-rust-20261029` (R1) | `rust/alera-cli/src/terminal_host/server_runner.rs` and child module | Do not simultaneously edit its queue/run-loop or share Cargo build targets. Host route mutations need coordination before merging. |
| W1（已有部分修正） | Workbench workspace switch、layout hydrate、tab lifecycle；第八批 `a7ee48fbd` 拒絕 stale background layout | 高語意重疊：transfer 完成時來源與目的的 watcher／layout 不能覆蓋新狀態。 |
| T1（已有部分修正） | Terminal detach、session ownership、renderer、restore；第八批 `45941055a` 保留同一 registry 的共享 PTY | 高語意重疊：移動僅換 presentation owner，不可殺死 PTY 或清除活躍 agent output。 |
| C1（已有部分修正） | Store transactions、request replay、lost reply；第八批 `4437ee71e` 僅處理 Mobile 偏好設定 timeout 讀回 | 高語意重疊：需獨立的 `tab.transfer` 一次提交／冪等交易，現有範圍不足。 |

**Work ownership:** This feature may own the drag acceptance and a dedicated application transfer coordinator **in its own worktree**. Do not concurrently edit the refactor's `server_runner.rs`, terminal-host queue machinery, shared generated bindings or Git index. Before touching terminal host / store / workspace lifecycle source, recheck the live W1/T1/C1 assignments, branch diffs, and ownership. Keep this feature unmerged until the transfer transaction and PTY semantics are verified against the latest refactor integration branch.

## Required host contract (to implement once ownership is coordinated)

Use an explicit, authenticated transfer operation (proposed `tab.transfer`); do not synthesize it from `tab.remove` and `tab.upsert`:

1. Validate terminal kind, tab id, expected source workspace, destination existence/state and any active detach/closing operation. Reject unsupported tab kinds and stale source ownership before mutation.
2. Persist the new display workspace and both layouts in one transaction or use a proven equivalent reconciliation protocol. Include replay/unknown-outcome handling (stable client mutation id or a conditional source check) so a lost reply does not duplicate or delete tabs.
3. Reconcile the live PTY session's workspace ownership **without** restarting it. Align terminal teardown, pulse, session restart, host checkpoint and automation/agent ownership rules. Distinguish original execution context from display location; existing process CWD/`ALERA_WORKSPACE_ID` cannot be changed retroactively.
4. Broadcast changes to **both** workspaces after successful commit. The UI must wait for acknowledgement and refresh both layouts/subscriptions; on failure keep the source tab visible and operational.
5. Update `docs/tab-layout-operation-contract.md` and `docs/terminal-operation-contract.md` and their existing contract tests before announcing functional support.

## 後續實作分期與平行界線（待執行）

| Phase | Owner／可平行性 | 交付物 | 前置條件／阻擋 |
| --- | --- | --- | --- |
| T2-A：UI 選擇與預覽 | Flutter Shell／Sidebar／Workbench UX；可**與 T2-B 的 API 設計平行** | Terminal-only drop eligibility、目標 project/workspace 選擇、hover/拒絕提示、單一 Workspace 自動選取與多 Workspace 選擇器；維持舊版 same-workspace 拖放 | **主機不支援前不得把跨 Workspace Drop 當作已成功**；UI 只做 preview／feature gating |
| T2-B：Runtime/Store 原子轉移 | Rust Runtime/Store **單一擁有者**；可與 T2-A 平行讀取／設計，但 Host transaction 及 PTY owner 同時修改必須序列化 | `tab.transfer`（預期來源 Workspace、目的 Workspace、Tab、Mutation ID）原子更新 Tab+兩側 Layout+活 PTY owner，明確返回 revision/結果；廣播雙側變更 | 先界定 runtime session、remove/sleep、restart、agent/hook 的所有權；避免單純 `tab.upsert` |
| T2-C：Coordinator + UI 接線 | Workbench application owner；**依賴 T2-B 完整契約** | 轉移中鎖定拖曳、等待 Host ack 再切換 UI；成功清理來源／定位目的地並更新雙側 watcher；失敗維持原 Session/Tab；反向 transfer 支援 Undo | 和 W1 Tab/Layout hydration、T1 Close/Detach 不得併行編輯同檔 |
| T2-D：復原與相容性 | Desktop／Mobile／Runtime 跨層契約；T2-C 後 | 重開 App、Host 重啟、斷線重試、來源 Workspace 移除後仍可尋得同一 PTY；終端輸出不丟序；原 cwd/env 與執行位置清楚標示 | 若 Host 版本不支援 `tab.transfer`，UI 必須停用跨專案拖移，而非退化成關閉後重建 |

**目前分期狀態（以最新 B2 節為準）**：T2-A UI 框架已落地但停用；T2-B1 Store CAS 已落地；T2-B2 Host route／live session 基礎已落地但 capability 仍 OFF；T2-C0 Dart Coordinator/Port 已落地；T2-C1 Workbench watcher fence／controller 接線已落地但仍停用；T2-D0 原執行位置提醒／重新啟動路徑已落地。**T2-D1：驗證和解決跨 client/Detach/Close/丟失回覆／Undo 剩餘風險**，才可對外啟用及宣稱支援。

### 使用者可見的完成定義

- [ ] 將執行中 Terminal Tab 拖至另一專案／Workspace，Session ID、PTY PID、輸出、scrollback、Shell history 不變；不自動 `cd`。
- [ ] 原 tab 與兩側 Layout 在重啟、Watcher 同步和多視窗環境下仍一致；可以從目的地復原回來源 Workspace 而不重啟程序。
- [ ] 運作中的 Claude／Codex 不因轉移被中止；需要明確告知**執行環境與 cwd 未隨顯示專案改變**，避免誤把命令送到錯誤 Git repository。
- [ ] 拖錯目標、重複 Drop、目的工作區已刪除、同時 Close／Detach、失敗／timeout 等情境不會孤立或重複建立 Session。
- [ ] 同 workspace 重排、Split Pane、Windows detach 保留原功能；Host 不支援時安全拒絕。

> 以上是**整體 T2** 未來實作完成條件，並非這次已支援或測試的功能。首次規劃時只有設計文件；目前已另外合入三條**未啟用**的 UI／Dart／Store 基礎。正式啟用仍待 Rust Actor 活 Session owner 的安全轉移與契約／跨端驗證。

## TDD acceptance matrix (not yet executed)

| Case | Red / green condition |
| --- | --- |
| Invalid drop | Same-workspace reorder remains unchanged; non-terminal tab and missing destination are refused. |
| Live session migration | PTY session id and process remain identical across move; output continues during/after drag, with no lost sequence or duplicate renderer. |
| Workspace retirement | Closing/removing the source workspace after a successful move does **not** terminate the moved session; closing the destination follows destination ownership. |
| Restart and recovery | Host restart, terminal reattach/restart and saved layouts resolve the destination tab exactly once; stream/history preserved. |
| Cross-project identity | CWD/environment stay as launched, visible origin indicator tells the truth, agent/hook writes cannot silently retarget the wrong repository. |
| Race and failure | Concurrent close, detach, session exit, workspace switch, double-drop, failed store write and timeout/retry have explicit, tested outcomes. |
| UI target | Dropping on destination sidebar workspace/project places it predictably; dragging across panes inside one workspace still behaves as before. |

## Safe execution plan

1. **Completed:** inspect active worktrees, collect exact current dirty file lists, verify transfer blockers, create an isolated feature worktree based on the integration baseline, record this plan.
2. Pending: define pure Dart drag eligibility/target-resolution tests without changing shared Runtime/Store files. Focus UI source ownership remains separate.
3. Pending / coordination required: agree on host/Store transfer transaction and state ownership with W1/T1/C1; first write failing contract/race tests, then implement the narrow operation. Existing `tab.upsert` alone is insufficient.
4. Pending: wire feature UI, run targeted Flutter + Rust tests, architecture guards, `git diff --check`; merge into the *updated* refactor integration branch only after conflict review. Do not automatically merge `main`, push or publish.
