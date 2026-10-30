# Alera 功能優先續做：第八批四線隔離（2026-10-29）

## 最新交接入口：T2 收尾盤點與有限 Release Gate（2026-10-30）

**不再自動擴充 D18/D19/D20**：完整一次收尾成果、實作對應 Commit、未達驗收門檻、禁止開啟的功能旗標詳見 **`docs/terminal-tab-transfer-closure-20261030.md`**。本輪從 D17 乾淨整合 `7cac43d53a81198fdc10d601f188b02da45cb7de` 開始，Store 原始碼 `d479429e7` → 整合 **`09c8aba3a`**、靜態補正 `99cbe4a4c` → **`46794ddfe`**、Host 原 Scope 停止程序前預檢 `7c7689675` → **`185510980`**、Journal 原始碼 `be6d1922a` → 整合 **`d12ca2e69`**。Store 普通 Tab/Workspace/Project 級聯刪除均以同筆 SQLite 交易保留 Transfer/Exit Marker，Host 清理前再驗來源 Scope；已驗證原生**活 PTY 唯一 View**的 `tab.remove` 走原 Receipt／雙 Layout 名冊 CAS、Tab+Target Layout+永久 SHA256 View Audit 原子退休；Journal Lock v2 有 App Process Epoch、拒絕 PID 重用的誤認。這**不表示** Host Crash 後可還原原 PTY、已完成跨程序 durable View Registry 或安全 stale Unlock；詳 A/B/C 三項驗收門檻。Gate **`supported=false`**，無 Cargo/Flutter compile/check/test/analyzer、E2E、Push/Release，不改動 dirty `main`。

## 歷史：T2-D17 交接入口（2026-10-30）

**唯一整合基準**：`refactor/tdd-integrate-20261029`，`.worktrees/refactor-tdd-integrate-20261029`。從 D16 clean `46845b80ed8fa5eb0ada82bf869d4dd2c4473781` 起，用既有三個 clean source Worktree 分別切 `feat/t2-d17-store-upsert-20261030`、`feat/t2-d17-journal-correlation-20261030`、`feat/t2-d17-mobile-current-host-20261030`，三筆 source commit 順序 cherry-pick。**D17 程式碼整合後、文件提交前 HEAD `52dbe008a`**；交接文件 Commit 後需重新查 `git status`／`git log -1`。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 的 preexisting history、`third_party/xterm`、未追蹤 E2E 檔保持不動。

| D17 工作線 | 已完成的安全防護／限制 | Source → Integration |
| --- | --- | --- |
| D17-S Store SQLite 普通 Upsert 權限下沉 | `RuntimeStore.upsert_workspace_tab` 由無條件單 SQL upsert 改為**單一 SQLite 交易內先讀權威既存 Row 再寫**。若原 Tab 有任何 `lastTerminalTabTransfer`／`lastTerminalTabExit` Key（即使 JSON null/損毀），拒絕一般 Store Caller 修改原 `Workspace/kind`、原 Receipt、退出證據、`terminalSessionId` 或原 `terminalExecutionWorkspaceId`／`terminalExecutionWorkingDirectory`；無原證據的 Row 與全新 Row 也禁止憑普通 Upsert 偽造 Transfer/Exit Marker。Title/正常未轉移 Tab Upsert 仍依原流程。正式移轉及 Exit 證據仍只能走各自受保護的 Store 交易。**這不是跨程序持久 Last-view 或已退休刪除的授權**；其他不經此 Upsert 的直接 SQLite／刪除路徑仍需專門守衛 | `c373fbe75` → **`2cd2b67f3`** |
| D17-J Journal Local Audit 交叉稽核 | `TerminalTransferJournalManualRecoveryDossier.readablePendingArchiveCrossReferences` 從**可讀** Pending + 檔名 SHA256 與本地內容吻合的 Archive，列出同 Tab/同 mutation 的本地封存、同 Tab/不同 mutation 的歷史封存；`archivesWithoutReadableMatchingPending` 列出沒有可讀同 mutation Pending 的本地檔。任何損毀 Pending／Archive 仍保留在原始 Inventory，不會臆測其 Tab ID。不同 mutation 的舊 Archive **可能完全正常**，無法證明異常或 Host Success；`requiresHostVerification=true`、`permitsAutomaticUnlock=false` 沒變；未新增任何刪檔／解鎖 API | `85ca84604` → **`1a0543ead`** |
| D17-M Mobile 新 Host Client 切換保護 | `TerminalSessionController.attachedViewLeaseId` 除已綁定原 View ID、`AsyncData`、Controller 未 Dispose／Recover 外，還同步讀取 `terminalClientProvider(hostId)`，只有目前 Provider 為 `AsyncData<MobileTerminalClient>` 且 Client instance **和原 Controller 綁定的 `_client` 完全同一個**才提供 Close Lease。若新 Socket Provider 已換、但 Controller Listener 還沒完成 D14 Handoff／D16 Revision 更新，舊 ready 狀態也不再授權 moved Tab Close。這只處理同 App Socket 切換時序，不是多 Phone、OS Window／Host Restart durable Lease | `4c45ddb0c` → **`52dbe008a`** |

**D18 待解**：① 仍沒有跨 OS 原生視窗、多 App、多 Phone/Host Restart 的 durable last-view authority，Host 只有記憶體 View Lease，D17 只多一層本 App Socket 狀態守衛。② SQLite Commit → PTY Session reparent Crash 的 definite outcome/補償仍不存在，Original PTY process generation 無法憑名字重建。③ D16 退出證據與雙 Layout 只讀預檢 **不是**合法 retirement：缺 user intent、Host 原 View/Instance 驗證及同筆 SQLite CAS 同步 Tab+兩 Layout+Durable Audit，絕不可自動清理。④ Store 普通 `remove_workspace_tab`／特殊 Direct SQL 寫入還沒有等價保護，須設計與原 Host/View 最終授權整合，不能單純封鎖而破壞合法 existing Tab Remove。⑤ Journal stale Lock 真正 OS process incarnation/liveness、Host-signed outcome、人工解鎖 UX/稽核、實機多裝置 E2E 都尚未完成。

**全部原限制保留**：`rust/alera-cli/src/terminal_host/server/requests.rs` 的 `tab.transfer.capabilities.supported=false`，功能繼續 OFF；僅 Rust `rustfmt --edition 2021`、Dart `dart format`、Git `diff --check`／`diff --cached --check`；**沒有新增或執行 Cargo/Flutter Tests、Cargo check、Flutter Analyzer、Build、Win/macOS/Phone E2E、Push、Release**，沒有 reset/clean/merge/push 或更改原始 dirty main。下一輪由本節接續 T2-D18。

## 歷史：T2-D16 交接入口（2026-10-30）

**最新接手整合基準**：`refactor/tdd-integrate-20261029`，Worktree `.worktrees/refactor-tdd-integrate-20261029`。從乾淨 D15 HEAD **`c4047798ad5ecbe6f9953c5c418351cc563ea5dd`** 起，三個 clean source worktrees 建立 `feat/t2-d16-store-retirement-preview-20261030`、`feat/t2-d16-journal-manual-dossier-20261030`、`feat/t2-d16-mobile-view-close-20261030`，分離提交再依序 cherry-pick。**D16 原始碼整合後，文件提交前 HEAD `e3f771ed1`**；下一輪必須以 `git log -1`、`git status` 查新 HEAD，不能直接假定 docs commit 未更新。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 以及其 preexisting history/xterm/e2e dirty files 未動。

| D16 工作線 | 已完成的程式碼與證據限制 | Source → Integration |
| --- | --- | --- |
| D16-S Store 原收據／Exited PTY 保護 | `record_transferred_terminal_exit` 改用 **Key presence** 而非 `Value::is_null()` 判定已存 `lastTerminalTabExit`：不會覆寫 JSON null／損毀的 Host 退出證據。Store `transfer_terminal_tab_storage` 在任何新 Transfer/Replay 前拒絕已有 Exit Marker／不完整 Prior Receipt，要求先前原 PTY Token 必須與現行 token 相同、Receipt `to` 為目前 Workspace；同 mutation replay **重新驗證兩側已存 Layout 必須等於送入結果及兩側全部 Tab registry**，絕不單看相同 ID 就宣稱成功。此為 SQLite 寫入層 fail closed，不等於跨 Host OS PTY 交易或故障補償 | `cbe304c8b` → **`88f09d941`** |
| D16-H 已退出 PTY 的原子唯讀預檢 | `RuntimeStore.inspect_transferred_terminal_retirement_evidence` 與現有 transfer status 共用**單筆 SQLite 讀取交易**，同時核對 Tab 當前 owner/Session/最新 Receipt ID/from/to/原 PTY Token、Host 記錄的 Exit Mutation/Session/Workspace/Token/Code/合法 UTC 時間字串、雙側 Layout split tree 和兩側完整 Tab 名冊。Host `tab.transfer.status` 對 `pty_exit_observed` 新增 **`originalExitAndLayoutsConsistent`**，仍只回 `applied:false,recoveryRequired:true`，Host 世代不一致繼續標 `hostGenerationMismatch`。**這不是 Last-view 證明、也不允許退休 Tab／清 Journal／認定舊 Host 已死** | `fa5130e5a` → **`5837f89b8`** |
| D16-J Journal 人工診斷資料合併 | `inspectManualRecoveryDossier()` 唯讀合併所有 Pending/Lock 檔案的 `inspectRecoveryInventory` 與所有 resolved/Orphan Audit 的 `inspectResolutionArchiveInventory`，回傳觀察時間／未解 Pending 或 Lock／不安全本地證據。兩個根目錄任一出錯就直接拒絕，不能用部分清單誤認為空。快照**非原子**；`requiresHostVerification` 永遠 true、`permitsAutomaticUnlock` 永遠 false，不檢查 PID liveness、不刪除任何檔案 | `6f29de5e3` → **`dd6eeb3e3`** |
| D16-M Mobile 點擊時附著身分 | `TerminalSessionController` 每次真正 `_bindSession` 更新 `_attachmentRevision`，`observedAttachedView` 只在已附著 ready UI 提供 `(原 View Lease ID, attachment revision)`。手機按下 moved Tab Close 後，在 `await workspaceClientProvider` 結束時重新核對**原 Provider/原 Controller/原附著版本仍存在**，否則要求從目前視圖重新按 Close，不可借用另一個 Socket 上相同 UUID 的新附著；即使 legacy fallback 也拒絕只有 Exit Marker 的移轉資料。此仍非跨程序 durable View registry | `429557d4e` → **`e3f771ed1`** |

**D17 待辦與安全界線**：① 沒有真正跨 OS Native Window／多 App Process／多 Phone／Host 重啟的**持久 Last-view Lease/原實體 View owner**，D16 只以當下 Host 記憶體與本地手機 revision 避免部分 race。② Host SQLite Commit 至 Live PTY reparent 的 Crash 區間仍無 definite outcome／可復活原 PTY，D16 不宣稱原 Host 死亡。③ `originalExitAndLayoutsConsistent` **只讀證據**，不是使用者意圖／身分授權；**尚未**實作原 PTY 退出後透過最新 Receipt + Process Generation + 雙 Layout CAS + Durable Audit 同交易的合法退休。④ Journal stale Lock 缺跨程序 Process incarnation/liveness、Host 權威 outcome/人工簽核/安全解除與 UX；Dossier 不包含解除鎖操作。⑤ Cargo/Flutter Tests、Check、Analyze、Build、Win/macOS/Phone 多視窗與多裝置 E2E 仍未獲准執行。

**原限制完全保留**：`rust/alera-cli/src/terminal_host/server/requests.rs` **`tab.transfer.capabilities.supported=false`**，跨專案 Terminal Tab 拖放仍**不可用**。D16 僅執行 `rustfmt --edition 2021`、`dart format`、Git `diff --check`／`diff --cached --check` 與 Git Source Commit/Cherry-pick；**不新增或執行 Cargo/Flutter tests、Cargo check、Flutter analyzer、Build、Windows/macOS/Phone E2E、Push 或 Release**；不 reset/clean/merge 原始 dirty main。下次先讀本節，再看 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新 D16。

## 歷史：T2-D15 交接入口（2026-10-30）

**唯一整合分支**：`refactor/tdd-integrate-20261029`（`.worktrees/refactor-tdd-integrate-20261029`）。本輪起點為 D14 乾淨 HEAD **`ac5d4383c51b8cda5da946e6f1da954622e66256`**；三條乾淨 Worktree 分別新建 `feat/t2-d15-corrupt-receipt-20261030`／`feat/t2-d15-resolution-inventory-20261030`／`feat/t2-d15-mobile-receipt-20261030`，各自 source commit 後依序 cherry-pick。**D15 程式碼最後整合 `6b255be72`**；文件 Commit 會再推進，接手務必重新查 HEAD/status。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 及其歷史記錄、`third_party/xterm`、未追蹤檔案保持不變，**不能** reset/clean/merge/push。

**T2-D15 三線／六筆程式碼安全補強，全部已 source commit 且整合**：

| 工作線 | 真正實作、證據限制 | Source → Integration |
| --- | --- | --- |
| D15-H Host 壞收據生命週期防護 | 新增 `has_transfer_recovery_evidence`／`invalid_transfer_receipt_marker`：**只要**有 `lastTerminalTabTransfer` 或 `lastTerminalTabExit` Key，即使是 JSON `null`、錯型別、缺失 mutation/from/to/original PTY Token，或 Exit marker 與轉移 ID／Workspace／Session／原 Token／Exit Code 不符，都是**未解決的持久化證據**，不得當作 never-moved Tab。Host `terminate`、`createOrAttach`／Restart、`tab.transfer`、read-only `tab.transfer.status`、`tab.remove` preflight + 背景 worker、PTY 自然 Exit cleanup、Workspace/Project cascade 都採保守拒絕。Status 對 malformed receipt 明確回 `applied:false,recoveryRequired:true,reason:malformed_transfer_receipt`；**絕不因資料毀損直接殺 PTY、刪 Tab 或清 Journal** | `2e1aa4995` → **`12ff53b74`**；`8fc786f22` → **`c28d49b45`** |
| D15-H2 Host 保護舊 Client Upsert | Host `tab.upsert` 對已轉移 Tab 的舊 UI 欄位**拒絕**變更 Workspace owner／Kind，對既有損壞收據拒絕覆寫；沒轉移過或新建 Tab 不得由 Client 偽造 `lastTerminalTabTransfer`／`lastTerminalTabExit`。新增 `lastTerminalTabExit` 與原 `terminalSessionId` 為 `tab_compatibility` Host-owned Payload Key，正常舊式 rename/upsert 不得用過期內容覆蓋原 PTY 世代退出證據。**不宣稱跨 Host Process Store CAS 已完成**，另一程序寫入仍需進一步持久交易守衛 | `812de0e13` → **`0499ec5d2`** |
| D15-M Mobile 不允許 unscoped transferred Close | `WorkspaceTabSummary.hasTransferEvidence` 檢查 Transfer 或 Exit Key 是否存在，`hasMalformedTransferEvidence` 驗證 Tab Kind、ID/from/to/原 PTY Token，與 Exit／Receipt 的鏈接。Mobile `TabsController.closeTab` 在任何 `await` 前拒絕壞收據，完好 moved Tab 只用原 Attached View Lease／Receipt 直接 Host guarded `tab.remove`；`removeObservedTab`／`terminateObservedTerminal` 明確拒絕任何 transferred Tab，不再先 `terminate` 後刪除；缺少 Receipt 但存在 Exit Marker 也拒絕，不回退到普通 Tab 操作 | `09cfaf15b` → **`2a3d33934`**；`c1e183f0e` → **`6b255be72`** |
| D15-J Journal Resolved Archive 唯讀盤點 | `FileWorkbenchTerminalTransferJournal.inspectResolutionArchiveInventory()` 獨立列出 `terminal_transfer_resolved_v1` 中的每筆封存，檢查普通 File、合法 `SHA256.json` 名稱、已寫入的 `callerConfirmedHostAndBothLayouts` 格式／Plan、`SHA256([tabId,clientMutationId])` 與檔名相符；不跟隨 symlink、把孤兒／毀損／假名 Archive 納入供人工查核。模型的 `requiresHostVerification` **永遠 true**：本地 archive **不是** Host 簽章、也不允許自動清 Pending/Lock／認定原 PTY 已回復。此輪沒有接到破壞性修復 UI | `3c9c11615` → **`24bfe5751`** |

**整合備註**：最後 Mobile cherry-pick 一度遇到 Git ref 更新錯誤，經核對 `CHERRY_PICK_HEAD`、staged 四檔與 HEAD 後，使用 `git cherry-pick --continue` 完成 **`2a3d33934`**；沒有 reset/清理 dirty `main`。D15 額外兩筆 exit-only 安全追補亦已獨立整合。

**D16 未完成**：① 真正跨 OS Native Window/App Process/Mobile Device/Host restart 的持久 Last-view Lease（包括未掛載／隱藏的視圖），仍依賴 Host 記憶體與本地 App 已登記 Detached Window。② Host Crash 於 SQLite 轉移提交與 live Session 改 owner 之間的決定性回復；原 PTY 已失聯不可憑 ID 偽造。③ **使用者授權且與原 Receipt／Process Generation／合法持久退出證據及兩側 Layout 同筆交易**的 transferred Tab retirement 尚未實作；D15 **沒有**增加刪除 API。④ Journal stale Lock 的跨程序 owner incarnation/liveness、Host 簽證決議、人工審查/安全解鎖和 UX 未做，新增封存盤點仍唯讀。⑤ 多手機/原生多視窗、Host restart、跨平台 E2E 與 Cargo/Flutter 編譯/Analyzer/測試均待獲准驗證。

**所有既有限制繼續**：`rust/alera-cli/src/terminal_host/server/requests.rs` 的 **`tab.transfer.capabilities.supported=false`**，跨專案 Terminal Drag 不開放。D15 僅跑 Rust `rustfmt --edition 2021`、Dart `dart format`、Git `diff --check`／`diff --cached --check`；**未新增／執行 Cargo/Flutter Tests、Cargo check、Flutter analyzer、Build、Windows/macOS/Phone E2E、Push 或 Release**。下一輪由本文件和 `docs/terminal-tab-cross-workspace-plan-20261029.md` D15 起續 D16。

## 歷史：T2-D14 交接入口（2026-10-30）

**唯一接續整合分支**：`refactor/tdd-integrate-20261029`，位於 `.worktrees/refactor-tdd-integrate-20261029`。本批從 D13 乾淨 HEAD **`cefc1d80c3a75ebae59217fbc59844c3153135ab`** 起，重用三條 clean source Worktree 分別切 `feat/t2-d14-host-epoch-20261030`、`feat/t2-d14-journal-root-20261030`、`feat/t2-d14-mobile-handoff-20261030`，各自分檔修改、格式化／Git whitespace check、source commit 再 cherry-pick。**D14 原始碼整合最後一筆 `1d9c4b220`**，文件提交後 HEAD 仍會前進；下次務必以 `git log -1`／`git status` 重查。原始 dirty `main` HEAD **`3d232acff9822154c68049ee63c57ed5966b84e0`** 的既有修改、history、xterm 與 untracked 檔案一律不碰。

**T2-D14 三條隔離工作線、四筆來源碼 commits 已整合**：

| 工作線 | 已實作／嚴格邊界 | Source → Integration |
| --- | --- | --- |
| D14-H Host process epoch + SQLite 同筆快照 | 原 `ptyInstanceToken` 本來就包含 **Host 程序級 v4 UUID + 單一 PTY instance counter**，**不重鑄 PTY 或新增另一個不可靠 epoch**。Host `tab.transfer.status` 在 persisted Receipt 與原 owner 完全吻合後，若 Receipt 的可解析 epoch 和目前 Host 不同，回 `applied:false,recoveryRequired:true,reason:host_generation_mismatch`，保留 Journal，**只代表本 Host 無法證明持有原 PTY**，不推論其他 Host 死亡、Transfer 失敗或可回復。如果有真正 persisted `lastTerminalTabExit`，仍優先回 `pty_exit_observed,exitCode,hostGenerationMismatch:true`。追補 D13 `verify_terminal_transfer_layout_and_receipt` 改回傳 `Option<WorkspaceTabRecord>`，**通過兩側 Layout、Receipt、Tab 名冊檢查的 Tab 本身來自相同 SQLite 讀取交易**，Host 不再回傳驗證前讀到的舊 Tab。沒有 Host Crash 後 Live PTY 恢復、Definitive failure 或刪除權限 | `b170bc619` → **`edfe3abdc`**；`c7bc2e67c` → **`1d9c4b220`** |
| D14-J Journal 目錄與證據安全 | 新增 `_requireRealDirectoryOrAbsent` 和 `_requireOrdinaryFileOrAbsent`，在 Pending、Resolved Archive 目錄及讀寫鎖／Pending／Archive 檔案前拒絕不可信 symlink/非普通檔；讀取 Inspector 用 `FileSystemEntity.type(...,followLinks:false)`，非 File Pending 仍標示存在但不可解析，Archive Link 不當作本地成功證據。正常自己的 `.lock` 釋放又驗證原 **Tab SHA256 Hash + Nonce + PID + version**，無法對上就保留。這是**保守檢查而非無競態 OS 鎖**，完全未做自動/手動 stale Lock 清除與 Host 結論代簽 | `a2e840af7` → **`d212d87c6`** |
| D14-M Mobile Socket View Lease 安全重連 | `TerminalSessionController._attachAfterVerifiedHandoff` **先照舊嘗試新連線具名 Attach**；若 Host 已釋放舊 Client，直接成功、不阻礙一般重連。**只有** Host 明確回「同一 `viewLeaseId` 已屬另一 Client」且新舊 Client 都明確支援 D13 的 `terminalNamedViewLeaseV1` 時，才向**原舊 Client** 做 `detachTerminalWithViewLease`、等待確認、解除舊本地綁定，再用**同一原 View ID** 重試新 Client 一次。若舊端不回或無法證明成功，**不搶占、不改 UUID、保留不確定狀態**。未移轉舊 Host 的 legacy path 不變；這不是跨手機/Host restart durable Lease | `058216b57` → **`28336851d`** |

**D15 仍未完成**：① 真正 OS Window/Device/App/Host 跨程序可證明的持久 Last-view Lease，包括 unmounted Views 與連線切換；目前只有 Host 記憶體與本機已登記視窗，D14 的 Socket Handoff 沒有權威跨程序永久 Registry。② Host crash 發生在 SQLite commit 和 Actor session reparent 之間仍不能復活**原 PTY**、確定原 transfer 失敗或自動修復雙側 Layout；**Host epoch mismatch ≠ 死亡證明**。③ 正常 Host 觀察到 transferred PTY Exit 後，仍需具備使用者意圖與身份驗證的**合法 exited Tab + Layout 原子退休**、可靠審計與 explicit recovery API。④ Journal stale Lock 需跨程序 Process liveness/owner incarnation、Host 決定性授權/人工作業／稽核 UX，D14 只會安全拒絕錯誤 Filesystem Type 而不解除鎖。⑤ Mobile 新舊 Socket、多個 Phone、Tab Watcher／native windows 的實機回歸仍未做。

**原限制全部保持**：`rust/alera-cli/src/terminal_host/server/requests.rs` **`tab.transfer.capabilities.supported=false`**，功能不可用。D14 只用 Rust `rustfmt --edition 2021`、Dart `dart format`、Git `diff --check`／`--cached --check`；**沒有新增或執行 Cargo/Flutter Tests、Cargo check、Flutter Analyze、Build、Windows/macOS/Phone E2E、Push 或 Release**，原 dirty `main` 沒有 merge、reset、clean 或修改。接手依本文件和 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最上方 D14 續做 D15。

## 歷史：T2-D13 交接入口（2026-10-30）

**唯一整合基準**：`refactor/tdd-integrate-20261029`，Worktree `.worktrees/refactor-tdd-integrate-20261029`。從 D12 乾淨 HEAD **`13842cc37f1f9027a9a0442dcc50d90909c22af3`** 開始，重用三個 clean 工作樹分線切 `feat/t2-d13-layout-attestation-20261030`／`feat/t2-d13-journal-failclosed-20261030`／`feat/t2-d13-mobile-capability-20261030`，按不同檔案分工。D13 五筆來源碼整合後、文件提交前 HEAD **`f601c1a86`**；接手必須重新查 `git log -1`／`git status`，不能假設 docs HEAD 未改。原始 dirty `main` HEAD **`3d232acff9822154c68049ee63c57ed5966b84e0`** 既有修改/未追蹤檔保持原狀，**不** reset、clean、merge、push 或 release。

**T2-D13 四部分安全補強已各自 source commit，再無衝突 cherry-pick 到整合**：

| 工作線 | 確實完成／保守邊界 | Source → Integration |
| --- | --- | --- |
| D13-H Host/Store 雙 Layout 權威核對 | 新增 Store 唯讀 `verify_terminal_transfer_layout_and_receipt`，單一 SQLite transaction 同時檢查 **當下** Tab owner、Session ID、最新 Transfer ID/from/to/原 PTY token、來源與目的地兩份 persisted Layout 有效結構/去重、及兩側 **全部** Tab ID 與 Workspace 名冊吻合。Host 在驗證原 live PTY 後，**只有**這項 SQLite 快照也成功才讓 `tab.transfer.status` 回 `applied:true`；若 Layout/Registry 不一致則 `applied:false,recoveryRequired:true,reason:persisted_layout_inconsistent`，保留 Journal，不把只存活的 PTY/Receipt 誤判成全交易已成功。**仍不能修復 Host Crash、推論缺失 Session 已失敗或聲稱跨不同時刻原子性** | `1f9bbf429` → **`ad91df026`** |
| D13-J Journal 載入與稽核一致 fail-closed | `loadPending()` 先看每筆檔名與檔案類型，任何 `.lock`（包含 symlink/dir）、`.json` 非 File、SHA256 檔名不合法，或**任何不認得的目錄項目** 都阻擋「沒有 Pending」的假象，不再靜默略過。原 Pending JSON/Tab Hash/重複驗證仍保留。追補 `inspectRecoveryInventory()` 也列出**每個**未知副檔名/隱藏檔等 blocker，使唯讀診斷能找到阻擋來源；沒有自動修復、刪檔、釋放 `.lock`，不把 PID 當存活證明。嚴格策略可能讓被 `.DS_Store` 等非 Journal 檔污染的資料夾也停用轉移，需由人員確認原因 | `987090545` → **`c6a6117d9`**；`122bfb48d` → **`f601c1a86`** |
| D13-M Mobile Host 能力協商 | Rust Host `mobile.hello` 明確宣告 **`terminalNamedViewLeaseV1`**：只表示 Host 會驗證具名手機 Attach/Detach/Restart 與 moved Tab Close，**不表示 T2 開放**。手機的新窄介面讀取已驗證 Host `runtimeCapabilities`，只有宣告時才允許具名 RPC 與 receipt/View-bound `tab.remove`；較舊 Host 即使忽略額外 JSON 欄位，也**不得被新 Client 當成有安全 Last-view 證明**。未移轉舊 Terminal 延續 legacy attach/detach/restart | `59cb60152` → **`682a460ac`** |
| D13-W Workbench 原生視窗生命週期近似防護 | 新增在**點擊關閉原同步快照**檢查 `detachedTabWindows.isDetached(tab.id)`／`isLaunching(tab.id)`。若 moved Terminal 屬於已建立/暫時 unmounted/隱藏的 Shared Engine 或 Child Process Window、或正在拖出，要求先移回主視窗，**不授權**利用另一個 mounted Surface 的共享 PTY token 關閉。現有 D11 的 mounted Surface count==1 檢查仍適用。此檢查依賴當前 App DetachedTabWindowService，**不是跨程序、未登記 Windows、手機/Host restart 的 durable Lease** | `ad7a714ff` → **`524cb4c49`** |

**D14 尚待**：① 同一 Terminal 可能由多個 OS Window/Device 使用，仍缺包含 detached/hidden/多程序/Host restart 的**真正持久 per-View/Last-view lease**，D13 只看本地現有 registry 與 Host 記憶體。② Host Crash 落在 SQLite Commit 與 Actor Session owner reparent 中間，仍不能恢復原 PTY 或有權威可驗證的 definite failure/compensation；D12 的 `lastTerminalTabExit` 只是實際觀察 Exit 的證據，**還沒有**合法已退出 transferred Tab 封存／退休與 Layout 同交易。③ Journal stale lock 的跨程序活性、已驗證 Host 結論、手動安全解除、歸檔保留和 UX 沒做；Inventory 仍是非原子唯讀。④ Mobile 同裝置重連時舊 Socket 未 detach 的 Token 占用、跨多手機、Watcher 與真正 OS 的多視窗測試；必須先明確批准 Cargo/Flutter tests/analyzer/check/build、Windows/macOS/Phone E2E。

**原限制全部保留**：`rust/alera-cli/src/terminal_host/server/requests.rs` 的 **`tab.transfer.capabilities.supported=false`**，跨專案 Terminal 拖放仍不可用；本批**只有** `rustfmt --edition 2021`、`dart format`、Git `diff --check`／`diff --cached --check`。**沒有新增／執行 Cargo/Flutter Tests、Cargo check、Flutter Analyzer、Build、Win/Mac/Phone E2E、Push、Release**，沒有更動既有 dirty `main`。

## 歷史：T2-D12 交接入口（2026-10-30）

**唯一接續整合分支**：`refactor/tdd-integrate-20261029`（`.worktrees/refactor-tdd-integrate-20261029`）。從 D11 乾淨 HEAD **`fff99a3c66446b26eedf61bdc746821d846c30d3`** 起，重用三條 clean 隔離 Worktree 新切 `feat/t2-d12-exit-evidence-20261030`／`feat/t2-d12-journal-inventory-20261030`／`feat/t2-d12-mobile-view-20261030`。程式碼整合最後一筆 **`62ce1d3b3`**，文件 Commit 後會再往前，接手必須重新 `git status`、`git log -1`。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 保持不變，沒有 merge、reset、clean、push 或 release。

**T2-D12 三條隔離來源碼工作線，分別獨立提交且已無衝突整合**：

| 工作線 | 真正實作／不可越界聲稱的事項 | 來源 → 整合 |
| --- | --- | --- |
| D12-H Host/Store 原 PTY 退出持久證據 | `RuntimeStore.record_transferred_terminal_exit` 在 SQLite 交易裡驗證**當下 persisted Tab 的 Session ID、Workspace owner、最新轉移收據與原 `ptyInstanceToken`**，以原 `payloadJson` CAS 寫入 `payload.lastTerminalTabExit`（receipt ID、Session、Workspace、PTY token、Exit Code、UTC）。只在 Host `handle_session_exit` **真正收到原 Session 正常 Exit event** 才觸發，不能由缺失的 Host PTY 推論退出。 `tab.transfer.status` 對此精確證據回 `applied:false,recoveryRequired:true,reason:pty_exit_observed,exitCode`，與未觀測的 Host Crash／PTY Missing 分開；**絕不因此宣告移轉已確認、允許清 Journal 或刪 Tab**。原本 D11 退出保留持久 Receipt 不變 | `60b1eb3d8` → **`f259d98df`** |
| D12-J Desktop Journal 唯讀復原清單 | `FileWorkbenchTerminalTransferJournal.inspectRecoveryInventory()` 逐一列出原本 `loadPending()` 遇第一筆 `.lock` 就會停住而看不到的其他 pending／Lock；可辨識合法 SHA256 檔名、無效檔名、JSON／Tab Hash 不符或損壞、原 Mutation ID、相符 local archive、Lock nonce metadata 有效性／標示 PID/UTC。**快照並非原子、只供稽核**；既有 `loadPending()` fail closed 不變，沒實作自動解除鎖、重播、清 Pending、Host 授權或失敗定論 | `0f50c913f` → **`535db1096`** |
| D12-M Mobile 全鏈路 View Lease 與安全 Close | Mobile 真正來源在 `mobile/lib/`。新增可選 `MobileScopedTerminalViewLeaseClient` 及 `MobileObservedViewLeaseTabRemoval`，不變更既有 Legacy Fake 介面。每個手機 `TerminalSessionController` 使用 `Random.secure()` 產生 **24-byte 隨機 view ID**，Attach、重連、Detach、Restart 沿用；Host `restart_terminal` 驗證 ID 不屬別的 Client 並在新 Session 註冊具名 ID，舊 Client Restart 則繼續 unscoped。對已轉移 Tab，Mobile `TabsController.closeTab` 必須**當次點擊時已存在、已 attach 的 controller Lease**，直接送原 Tab/Workspace/Transfer Receipt/View ID 至 Host `tab.remove`；**不可先 `terminal.terminate`** 再移除，避免先殺原 PTY 破壞收據驗證。未 attach／舊版／不支援 narrow Port 一律安全拒絕，不補建 UI 來取得 Token。一般未移轉 Tab 沿用舊版流程 | `29a74ed32` → **`62ce1d3b3`** |

**D13 尚未完成**：① Host crash 後 SQLite commit 與 live owner 失配時，現有觀測 Exit Marker **不能復活 PTY**，也不能對沒觀測到 Exit 的狀況當成確定失敗。需要權威 Host process-generation、補償／合法退休 exited transferred Tab。② 真正逐 OS Window 的 **mounted/unmounted/同 Engine/多程序** View 所有權，以及持久跨 Host restart/多裝置 Last-view Lease，目前仍只有單 Host Session 記憶體與 UI mounted 近似值。③ Journal 殘留 Lock 的 OS Process liveness 與安全的手動稽核／解鎖、Host 正式決定性解除和 UX 尚未做；D12 Inventory **不改寫**任何 pending。④ Mobile 舊版裝置、重連時舊 Socket 尚活、Multiple Phone、Terminal Watcher、正常／異常退出和跨 OS 真機須實驗；需要明確恢復 Flutter/Cargo 編譯、analyzer、測試與 Windows/macOS/Phone E2E 之後才可評估開 Gate。

**限制嚴格沿用**：`rust/alera-cli/src/terminal_host/server/requests.rs` **`tab.transfer.capabilities.supported=false`** 不變、跨專案 Terminal 拖放仍不可用。這輪**只跑** `rustfmt`、`dart format`、`git diff --check`／`--cached --check`；**沒新增／執行 Cargo／Flutter tests、Cargo check、Flutter analyze、Build、Win/Mac/Phone E2E、Push 或 Release**。原始 dirty `main` 不動。下一輪依本文件與 `docs/terminal-tab-cross-workspace-plan-20261029.md` 的 D12 再開 D13 隔離工作線。

**D12 安全審查追補**：若 `lastTerminalTabExit` 已存在非 Null 的損壞／異常證據，Store 現在也**拒絕覆寫**，避免毀損稽核資訊，來源 `3773a5512` → 整合 **`04aff070f`**。Journal 復原 Inventory 另外標示檔名像 `.json`／`.lock`、實際卻是**符號連結或資料夾等非一般 File** 的可疑項目，不跟隨或自動刪除，來源 `e11d3d570` → 整合 **`37bbe6b03`**。D12 文件整理前最新程式碼 HEAD **`37bbe6b03`**；文件 Commit 後必須重新查 HEAD。

## 歷史：T2-D11 交接入口（2026-10-30）

**唯一整合基準**：`refactor/tdd-integrate-20261029`，`.worktrees/refactor-tdd-integrate-20261029`。從 D10 乾淨 HEAD **`42b0deee2`** 出發，重用三個先前乾淨的隔離工作樹，分別新開 `feat/t2-d11-host-lease-20261030`／`feat/t2-d11-journal-20261030`／`feat/t2-d11-workbench-close-20261030`。D11 源碼最後整合 **`2ea227339`**，在開始編輯此文件前確定整合工作樹乾淨。**此文件提交後 HEAD 將改變，接手先查 `git log -1`、`git status`，不可使用舊 SHA 猜狀態**。原始 `main` HEAD `3d232acff` 的既有 dirty 檔案、history、xterm 和未追蹤測試資料不曾更改／reset／clean／merge。

**T2-D11 三線獨立提交並全部無衝突 cherry-pick 到整合**：

| 工作線 | 本輪確實完成的行為與侷限 | 來源 → 整合 |
| --- | --- | --- |
| D11-H Host 退出紀錄保留 | `pty_events_session_lifecycle.rs::remove_terminal_session_tab` 現先查 Store Tab；若是含 `lastTerminalTabTransfer` 的 Terminal，**一般 PTY 自然 Exit 與顯式 Terminate 的通用 Session 清理皆不能直接刪除持久化 Tab/收據**，回 `false` 以保留 crash/exit 證據，交由目前既有 exited checkpoint 路徑。非轉移 Terminal 照舊清理。**這不是**合法的 dead transferred Tab 結束／回收流程；已轉移 Terminal 顯式 Terminate 後 Tab 仍留待安全處置 | `831bdb1e0` → **`673566e07`** |
| D11-J Journal 跨 App Lock 身分稽核 | 原本 per-Tab `.lock` 用 exclusive create 後，現在 flush 持久化 JSON `version/tabIdHash/nonce/processId/createdAtUtc`；`remember`／`forget` 的 finally 需確認**相同隨機 nonce + processId**，才清除自己持有的 Lock。唯讀 `inspectRecovery(tabId)` 額外回報 lock metadata 是否有效、檔案標示 PID／UTC。空檔／毀損／Crash 殘留仍 **fail closed**，**PID 不是仍在執行的證據**，不允許自動 unlock 或清不明 Journal；操作終止/跨程序安全人工解除仍待設計 | `62fd828ef` → **`95ab5e5b6`** |
| D11-C Workbench 真實 UI Lease 傳遞 | 從 Production `TerminalHostPtySession` 只讀取**已 attach／尚未 dispose** 的自身固定 UUID；`XtermTerminalRuntime.peekSession` 不新建 PTY，經窄介面 `TerminalRuntimeAttachedViewLeaseSource` 讓 Workbench 在**點擊關閉的同步快照時**一併抓 `tab + workspace + lastTransferReceipt + viewLeaseId`。只針對已轉移 Terminal 要求 `WorkbenchViewBoundObservedTabCloseStore`、`WorkbenchViewBoundTabRemovalRepository`，不允許退化為 legacy close；`RuntimeWorkbenchRepository` `tab.remove` 帶 `expectedViewLeaseId`，Timeout／Connection Closed 的一次重播仍沿用**原快照**與原 Token，不從 Host 重新抓新 View，避免 ABA。普通 Tab 仍走原有路徑。對同一 Xterm Handle **有 0 或超過 1 個 mounted visibility lease** 的情況保守不提供該 Token，避免多原生視窗共用同一 PTY Token 造成最後視圖誤判；但**未掛載視窗仍不可觀測**，不是完整 per-OS-window Lease | `a4eef3eba` → **`3adc4f465`**；追加 `abc61e686` → **`2ea227339`** |

**D12 阻擋項**：① 真正跨原生 Window、Mobile、App process、Host restart 的 View ID／durable Last-view Lease；一個共享 Xterm Handle 可能代表多個未掛載視窗，D11 只拒絕 0／多個 mounted Surface。② Host SQLite 成功但 Session 未重設 owner 時 Crash 仍不能還原舊 PTY 或權威宣告確定失敗；需 Durable process generation/可靠補償、合法的 transferred PTY 退出後 Tab 清除與終止交易（目前刻意保留收據）。③ Journal 手動判斷 stale lock 仍沒有跨程序活性證明／安全解鎖／Host 簽署決定性稽核，仍沒有 UX；Mobile 版本與 Terminal watcher、不同 runtime clients 的終止流程要審。④ 需依原限制另行批准完整 `cargo check`、Cargo/Flutter tests、Flutter analyzer/build、Windows/macOS 多 Window/Client/Crash E2E 才可考慮開 Gate。

**限制原封不動**：`tab.transfer.capabilities.supported=false`（不可用、不發布）。本批**僅** `rustfmt`、`dart format`、`git diff --check`、`git diff --cached --check`；**沒有新增或執行測試、分析、編譯、Win/Mac 實機、Push、Release**，不得以格式化通過宣稱功能已可用。接手請再讀 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最上方 D11 與 `docs/refactor/tdd-roadmap-20261029.md`。

## 歷史：T2-D10 交接入口（2026-10-30）


**最新可接續整合分支／安全起點**：`refactor/tdd-integrate-20261029`，工作樹 `.worktrees/refactor-tdd-integrate-20261029`。先檢查 D9 乾淨 HEAD `dfc9c2fe7c55ecc055e8534eba82ff2c98a09d11`，重用三條 clean D9 隔離工作樹切 `feat/t2-d10-view-registry-20261030`、`feat/t2-d10-view-close-20261030`、`feat/t2-d10-client-leases-20261030`，按檔案所有權分開修改。D10 來源碼整合後、更新本文件前 HEAD **`6c5571280` 已確認乾淨**；文件提交後 HEAD 會改變，接手一定要重新 `git status`／`git log -1`，不能硬套此 SHA。原始 `main` 的 `3d232acff` 仍是原有 dirty，沒有 reset、merge、clean 或覆蓋。

**T2-D10 已平行完成並整合三條來源碼安全工作線**：

| 隔離工作線 | 精確實作與安全邊界 | 原始 → 整合 Commit |
| --- | --- | --- |
| D10-V Host Session View Registry | 在每個 in-memory `Session` 以 `viewLeaseId` 記錄 **Host Client → 多個具名 UI 視圖**；`createOrAttach` 綁定 Lease，單一 `detach` 只釋放指定視圖、同 Client 其他視窗保持 attached；Token 不可同時轉給另一 Host Client。`unscoped_clients` 分別記錄舊版無 Lease 視圖，防新版同 Client 額外註冊後掩蓋它。Mobile `terminal.attach` 轉傳可選 Lease，舊版未轉移 Terminal 保留行為；**已轉移**視圖缺原 Lease 則拒絕。這仍是 **Host 執行期記憶體**，不是 SQLite 持久跨重啟 Lease | `6ea692df8` → **`dae1db212`** |
| D10-G Host Close/Transfer + Receipt Status | 已轉移 Terminal 的顯式 `terminate` 現要求**要求者 Host Client 所附著的唯一 UI View Token**；`tab.remove` 的序列化 RPC 接受 `expectedViewLeaseId` 並在 Actor 檢查整個 Session 只有該 Token 一個活視圖。因 Runtime RPC 和 PTY I/O 可能不同 Host Client，`tab.remove` 以不可猜測 Token 作憑證，不硬比不同 socket 的 ID。移轉原 PTY 前確保所有已附著 Client 均有合法具名視圖，舊版不支持則拒絕。失聯回覆查證分辨 `receipt_conflict`／`persisted_owner_inconsistent`，仍回 `recoveryRequired:true`、**不認定確定失敗**，絕不清 Journal | `e6262d93a` → **`c22cf91f3`** |
| D10-C Desktop Lease Port + Journal Inspector | Production `SocketTerminalHostClient` 透過窄介面 `TerminalHostViewLeaseClient` 支援 attach／detach／terminate 帶 Lease；每個 `TerminalHostPtySession` 以隨機 UUID 建立固定 view ID，Transport 重連仍沿用，處理 Shared Host connection 下的一個 View dispose 不會連帶 detach 其他視圖；Fake/legacy 仍維持既有介面。補齊 `TerminalHostClient.terminate` 原本缺漏的 observed transfer receipt 介面宣告與兩個 Fake。App Support Journal 提供 **唯讀** `inspectRecovery(tabId)`，可辨識 `.lock`、Pending 可讀性、精確 matching 本機成功 archive，但**不自動解除鎖或移除不明 Journal** | `6aa4a890f` → **`6c5571280`** |

**D11 尚未完成／不可宣稱功能已可用**：① Workbench `tab.remove` 呼叫端尚未有可靠的目前視窗 `expectedViewLeaseId` 綁定／傳播，因此已轉移 Tab Close 目前仍會保守拒絕；Mobile View ID 及 cross-process Window 生命週期需完整對齊。② View Lease 是 **per Host Session memory**，尚非可跨 Host Restart 的 Durable Lease；原 PTY crash 中段仍不能復活或權威認定確定失敗，亦沒有可稽核 PTY Generation Renewal、正常退出或孤兒 transferred Tab 的合法清理策略。③ Journal 的 stale `.lock` 只有可讀診斷，沒有安全的跨程序人工驗證／解除 UI、正式 Host refusal 判定／Audit；需照顧 Mobile 舊版與 watcher。④ 必須依原政策恢復完整 Flutter/Cargo Build／Analyze／Tests／Windows/macOS 多程序 E2E 後才能考慮啟用；本輪 **沒跑**。

**沿用既有限制**：Host `tab.transfer.capabilities.supported=false` 不變，跨專案 Terminal 拖放未開放；本批只跑 `rustfmt`、`dart format`、`git diff --check`／staged check，**沒有執行 Flutter/Cargo test、Analyzer、Cargo check、Build、實機 E2E、Push、Release**。Rust/Dart 既有 Fake／Test 檔案僅按新欄位更新定義，**未執行測試**。原始 dirty `main` 未碰，三線各自提交後 cherry-pick 至整合，沒有衝突。下一輪按 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最上方 D10 入口進行 D11。

**D10-C 後續 UI 補正已整合**：`tab.transfer.status` 的 `recoveryRequired:true` 可能是 `receipt_conflict`、`persisted_owner_inconsistent` 或原 PTY 消失，不可一律提示原交易「已儲存收據」。Desktop Sidebar 改為保守提示原始收據／Workspace owner／PTY 可能有衝突，RuntimePort 註解同步；來源 `e38e0c9c7` → 整合 **`c7def499b`**。這筆在 D10 主程式與首份交接文件 Commit 之後合入，最新 HEAD 應重新 `git log` 確認。

## 歷史：T2-D9 交接入口（2026-10-30）

**當前唯一整合基準**：`refactor/tdd-integrate-20261029`（`.worktrees/refactor-tdd-integrate-20261029`），從 D8 乾淨 HEAD `632f1429b` 起做，最後一筆 D9 原始碼整合提交 **`75161c9bf`**，在開始更新此文件前已確認整合工作樹乾淨。**文件 Commit 後 HEAD 將前進，接手必須重新查 `git log -1`、`git status`**。原始 `main` 仍為既存 dirty `3d232acff`，沒有修改／reset／merge／清理原本的歷史 Session、xterm 或其他未追蹤資料。

**T2-D9 已完成三條來源碼隔離工作線及一筆保守補強，全部無衝突整合**：

| 隔離工作線 | 真的完成的防護與明確侷限 | 來源提交 → 整合提交 |
| --- | --- | --- |
| D9-H：Host requester-bound Close + Cascade | 序列化 `tab.remove` 把真正的 `client_id` 傳到 Actor preflight；對**已轉移 Terminal** 先核實 Store 最新收據、原 live PTY 的 instance Token、Workspace／Tab 歸屬，並要求「要求移除的 Client 本身仍 attached，而且其他 Client 都已 detach」。原 PTY 消失／新程序冒充時直接拒絕。Workspace/Project/Managed/Tab For Workspace/Sleep 的清理也保守拒絕**任何尚有持久轉移收據的 Tab**，即使 PTY 已死或無人附著；另外檢查 Store/Session owner 因 Crash 分歧的 live PTY。因為 Host attached set 以 Client 連線為單位、尚無每視窗 Lease，**不是**跨程序持久 Last-view 協議 | `c9468fd77` → **`3f1cf30d7`**；追加來源 `3753d60d7` → **`75161c9bf`** |
| D9-R：Host 原 PTY 不可偷偷重鑄 | `verify_terminal_attachment_owner` 對有轉移收據的 Tab 強制核對 live PTY + `ptyInstanceToken`，不再用相同 `sessionId` 在 Host 重啟時 `createOrAttach` 新 Shell；`spawnOnCreate` 不重生已轉移 PTY，`tab.upsert` 的既有一般錯誤 cleanup 不會刪掉其 Tab／殺掉原程序；對 moved Tab 的直接 `terminal.restart` 也先保守拒絕，直到有**獨立、可稽核的 process-generation 變更協議**。因此目前不提供已轉移 Tab 的一般 Restart 功能 | `7775ff028` → **`f10fb0f52`** |
| D9-J：跨 App Journal Lock + 本機稽核 | 原本 App Support per-Tab 排他 Pending Journal 外，再用相同 Tab 雜湊 `.lock` 以 `File.create(exclusive:true)` 協調不同 App 程序的 remember／forget；有中斷 Lock 則讀取與新移轉 fail closed，**不自動清掉**。在 Controller 完成 Host 原收據與雙 Layout 核實後，`forget` 先寫入 `terminal_transfer_resolved_v1` 的完整原計畫、caller-confirmed marker 與 UTC 時間、flush，核對一致才刪 Pending；重試時缺 Pending 但有精確 Archived Resolution 才可視為本機已清。這是**本機 caller 稽核**，非 Host 簽章或跨裝置 Lease | `2d9244d25` → **`f536c68b6`** |

**下一批 D10 的真實阻擋項**：① Host Client connection 不等於每個 App 視窗／Mobile view ID，仍缺真正跨程序、跨重啟 Last-view Lease、已轉移 PTY 關閉／正常退出後的合法 Tab cleanup；② Crash 落在 SQLite 交易與 live PTY reparent 中間仍**不能找回原 PTY**，需要權威 terminal generation transition、補償或可審計確定失敗方案，且不得清掉待決收據；③ Host 明確拒絕與 partial-commit 的決定性結果／安全解除 Journal、跨程序中斷 `.lock` 的人工修復流程及 Mobile 舊版相容；④ 需另獲恢復完整驗證之許可，再執行 Flutter/Cargo 測試、Analyzer、Build、Windows/macOS 多視窗、多程序 E2E。禁止把 D9 單 Host／本機檔案鎖誤稱持久全域 Lease。

**限制維持**：`tab.transfer.capabilities.supported=false`，跨專案 Terminal 拖放仍不能用。本批只有 `rustfmt`、`dart format`、`git diff --check` 與 `git diff --cached --check`，**沒有新增／執行測試、Flutter/Cargo check、Analyzer、Build、Win/Mac 實機、Push、Release**。只是修改既有 `managed_workspace_cleanup_tests.rs` 以配合 Actor 訊息參數，**沒有執行測試**。原始 dirty `main` 保留。

## 歷史：T2-D8 交接入口（2026-10-30）

**起點與 Git 安全**：從乾淨 `refactor/tdd-integrate-20261029` HEAD `379ec62ca` 延續；三條獨立工作樹切新 D8 分支，各線隔離改檔並提交，無衝突 cherry-pick 回整合，**更新本交接前已確認 D8 整合 HEAD `0ffb9b3a3` 乾淨**。本文件提交後 HEAD 還會前進；接手先重新 `git status`／`git log -1`，不得以舊 SHA 推斷現況。原始 dirty `main` 仍是 `3d232acff`，沒有清理、合併或修改其既有文件、xterm 子模組／測試資料。

**T2-D8 已整合的三條獨立安全工作線**：

| 工作線 | 實際安全修補／未涵蓋範圍 | 隔離提交 → 整合提交 |
| --- | --- | --- |
| D8-H Rust Host Cascade | Workspace／Project Delete、Project Workspaces、Workspace Tabs Remove／Sleep 改由 Actor 在背景 SQLite 刪除前檢查：只要受影響 Workspace 有**已轉移且仍在執行並有附著觀看者**的 Terminal，整筆拒絕，避免批次關閉繞過 Tab 移轉收據；Managed Workspace Close 亦先檢查再做不可逆的 PTY Cleanup。這只是 Actor 期間的保守拒絕，**不是**持久全域 Lease，也不保護已全部 Detach 的 Session | `6fa141bfc` → **`4bd4a8f96`** |
| D8-I Rust Store＋Host PTY Instance | 把隨機 Host process epoch + 每個活 PTY 的 `Session.instance_id` 組成 `ptyInstanceToken`，在同一筆 SQLite transfer 交易中存進 `lastTerminalTabTransfer`；`tab.transfer.status` 除 ID／Workspace／Receipt／running PTY 外再核對此 Token，重啟後相同 Session ID 但**新生 PTY**改回 `recoveryRequired:true,reason:pty_instance_unverified`。同 Mutation 的 `tab.transfer` 重播若 PTY Token 不符也拒絕；舊收據沒有 Token 時保守無法核實 | `28612d716` → **`463725075`** |
| D8-J Desktop Journal Check | `confirmUnknownTerminalTransfer` 的**同 Tab in-flight 排他**涵蓋 Host 狀態查證、兩 Workspace 權威 Layout 讀回、Journal 正確清除與 UI layout fence 釋放；重複點 Check status 或同時新移動不能穿插放行。依然沒有明確 Host「確定失敗」證明或自動安全清 Journal | `b5b654d24` → **`0ffb9b3a3`** |

**D9 仍未完成**：① 真正跨 App／跨 Host 重啟的 PTY Last-view Lease、Close/Detach/RemoveTab requester 身分及完整 Workspace/Project 清理原子性；② Crash 在 SQLite commit 至 live PTY reassignment 間發生的權威復原／確定失敗處理（本批 Token 只防新生 PTY 冒充，**不復活舊程序**）；③ `recoveryRequired`／Host 確定拒絕時的持久 Journal 可審計解除、跨程序安全 UX、Mobile 版相容；④ 恢復正式 Flutter/Cargo 測試、Analyzer、編譯與 Win/Mac 多程序 E2E。請讀 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最上方 D8 與 `docs/refactor/tdd-roadmap-20261029.md`，不要重做 D1–D8。

**發布與驗證限制維持不變**：`tab.transfer.capabilities.supported=false`，使用者跨專案 Terminal Tab 拖放**尚未啟用**。這一批只做 `rustfmt`、`dart format`、各分支 `git diff --check`／staged whitespace check，**沒有新增／執行 Flutter/Cargo 測試、Cargo check、Analyzer、Build、Windows/macOS 實機、Push、Release**。不可宣稱編譯通過或上線安全。

## 歷史：T2-D7 交接入口（2026-10-30）

**最新功能整合分支**：`refactor/tdd-integrate-20261029`，工作樹 `.worktrees/refactor-tdd-integrate-20261029`。本批 D7 程式與契約整合後，更新本交接文件前已確認 **HEAD `375c89568` 且工作樹乾淨**；本文件 Commit 後 HEAD 將再前進，接手務必重新查 `git status`、`git log -1`，**不要硬套舊 SHA**。原始 `main` 確認為 `3d232acff`，已有未提交文件、xterm 子模組和未追蹤資料，整批未動。

**已完成的 T2-D7 小範圍三線（均已獨立提交並整合）**：

| 線路 | 真正已修改的安全行為 | 獨立 Commit → 整合 Commit |
| --- | --- | --- |
| D7-H Rust Host／Mailbox | `tab.remove` 排隊或背景執行期間保留 Tab-specific Actor fence，`Attach`／顯式 `Terminate` 在 SQLite Tab row 已消失但 Session 未結束的空檔仍拒絕。對完全匹配的原轉移收據，如 Host PTY 缺失／不吻合，`tab.transfer.status` 回報 `applied:false,recoveryRequired:true`（`pty_missing`／`pty_owner_mismatch`），不謊稱成功 | `a6f810b04` → **`291a1c4c2`** |
| D7-C Dart Client／Workbench | Host 確認 SQLite 收據但無法確認活 PTY 時，保留原 Mutation、App Support Journal 及阻擋狀態，投射明確需復原錯誤與側邊欄提示；不自動關閉、重送、回滾或放行第二次轉移 | `2cfe61566` → **`c32d3662f`** |
| D7-O 操作契約 | 更新 `docs/terminal-operation-contract.md` 與 `docs/tab-layout-operation-contract.md`，明列 Actor-side Close/Attach fence、三態收據確認和仍待處理的持久 Lease 邊界 | `6a7306e9a` → **`375c89568`** |

**真實狀態與限制**：上述只降低單 Host Actor 的 `tab.remove`／新 Attach 競態，**不是**跨程序持久 Last-view Lease；`recoveryRequired` 只是辨識 SQLite 已轉移但 PTY 不在，**不是**自動找回原程序、確定失敗或安全清 Journal。未完成下一輪 D8：① Host 全域跨程序視圖 Lease／Close/Detach 與 Workspace/Project cascade 規則；② SQLite Commit 至活 PTY owner 之間 Host Crash 的明確復原或阻擋協議；③ `recoveryRequired` 和 Host 明確拒絕時 Journal 確定失敗／可審計解除，Mobile 舊版相容；④ 恢復後的 Flutter/Cargo 編譯、測試、Analyzer、Windows/macOS 多程序實測。

**限制不變**：`tab.transfer.capabilities.supported=false`，真正跨專案拖曳尚不能用。依使用者既有決定，本輪只做 `rustfmt`、`dart format` 與各線 `git diff --cached --check`；**沒有** Flutter/Cargo tests、Cargo check、Analyzer、Build、Win/Mac 實機、Push、Release，也**沒有**修改或合併 dirty `main`。不可將格式化成功寫成測試 green。新的 D7 詳細計畫見 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新段落，完整優先序見 `docs/refactor/tdd-roadmap-20261029.md`；D1–D6 已整合，勿重做。

## 歷史：T2-D6 交接入口（2026-10-30）

**可以直接依本檔交接，不需重做前面已提交的 D1–D6。** 本檔按最新批次在上方，較舊段落是歷史狀態；功能設計、每線細節和剩餘安全邊界以 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新 **D6** 為準；整體優先順序以 `docs/refactor/tdd-roadmap-20261029.md` 為準。

- **已確認的 D6 功能基準**：`refactor/tdd-integrate-20261029`，Worktree `.worktrees/refactor-tdd-integrate-20261029`，本次更新交接文件前已核對乾淨 HEAD **`b5836e8f5ead907db7d507a3958ca851aeb702e9`**（`docs(terminal): hand off T2 D6 four parallel safety fixes`）。**本交接檔若有新的文件 Commit，整合 HEAD 會再前進**；動手前必須重新 `git status`／`git rev-parse HEAD`，以當前乾淨整合 HEAD 為新起點，不硬套此 SHA。
- **已整合的最後批次**：D6 Rust Host 移動收據強制驗證 `8a9e78d83`、Rust Store 精確 Pane Tree 剪枝 `74f8062a9`、Desktop 未決 Journal 同 Tab 原子排他建立 `dd71d3401`、Desktop 關閉使用視圖當時所見收據 `c23dd4189`；此前 D1–D5+ 亦已合入，無需重做。
- **真實狀態**：跨專案 Terminal Tab 拖放 **尚未對使用者開放**，`tab.transfer.capabilities.supported=false`。尚未得到 Flutter/Cargo 編譯、Analyzer、測試或 Windows/macOS 多視窗實機通過的證據；不得宣稱可用或安全發布，也不得自行打開旗標。
- **下一批可平行範圍**：① Rust Host／PTY 唯一 owner 處理跨程序最後視圖 Lease、Attach/Detach/Close；② Rust Store／Host 協調 Crash 發生在 SQLite 提交和 live PTY owner 更新之間的可驗證復原；③ Desktop Journal／Workbench 提供「確定成功／確定失敗／結果未知」的權威確認與安全清理 UX。三線必須先按**檔案擁有者**隔離、避免 Store／Host 同檔同時寫，並在串接前核對操作契約。
- **工作樹慣例**：優先**重用乾淨的既有** `feat-t2-*` Worktree 各切新分支；Windows／Dropbox 下避免同時大量 `git worktree add`，之前曾因 checkout 逾時留下暫時的初始化鎖及 Index 差異。當前驗證 D6 的 Host `.worktrees/feat-t2-host-d1-20261029`、Journal `.worktrees/feat-t2-recovery-d1-20261029`、Client `.worktrees/feat-t2-undo-d1-20261029` 各已乾淨且在 D6 隔離提交上，**不是**最新整合基準，重用前先確認工作樹 HEAD 與目標 branch。
- **不要動原始 `main`**：原 `main` HEAD `3d232acff`，具有既存未提交文件、xterm 子模組與未追蹤資料；不可 `reset --hard`／清理／覆蓋。先前使用者指示功能優先，暫停完整測試／靜態分析／編譯；若準備啟用或發布 T2，必須另外確認恢復核心驗證，不可憑格式化與 Git whitespace check 宣稱 green。既有批次未 Push／Release。

**新對話接手指令**：請先讀本檔最上方交接入口、`docs/terminal-tab-cross-workspace-plan-20261029.md` D6、`docs/refactor/tdd-roadmap-20261029.md`，查 `git status`／`git log`，再從仍未完成的 T2-D7 安全邊界安排平行工作；做完每條線獨立提交、整合並把進度寫回這些文件。不要把以前 B2／D1 段落的「待做」誤認為當前剩餘項目。

## 最新：T2-D6 四條安全工作線已平行提交、整合（2026-10-30）

從乾淨 `6bdb0cbb8` 重用四個既有 Worktree，各切隔離分支：**Host** 已轉移 Tab 的 `tab.remove` 強制持有原視圖最新收據 `8a9e78d83`；**Rust Store** 來源 Pane 刪除時限定精確 Split Tree 剪枝與 active Group `74f8062a9`；**Desktop Journal** `File.create(exclusive:true)` 原子佔有同一 Tab 的持久未決操作檔 `dd71d3401`；**Workbench Desktop Close** 從關閉時捕捉的原始 Tab Snapshot 傳同一 Workspace/Receipt 給 Host，timeout 重送不偷更新收據 `c23dd4189`。各線源提交 `19a9c3dd0`／`7ea2bf150`／`b16c0b58`／`b63c637d1`，四線 cherry-pick 全部成功且無衝突。

**仍不可用：`tab.transfer.capabilities.supported=false` 保持不變。** 排他 Journal 不等於跨程序 Host lease；Host crash 中途提交、跨程序 PTY 最後一個視圖與不明結果紀錄的安全解除仍是阻擋。依先前指示僅執行 `rustfmt`、`dart format`、Git staged whitespace check，**沒有** Flutter/Cargo 測試、Analyzer、Build、實機驗證、Push 或 Release。原始 dirty `main` 未動。完整交接見 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新 D6。

## 最新：T2-D5+ 三線平行已合入（2026-10-30）

另一組 D5 已將 Rust Store／Workbench readback／Agent watcher 完成並合入 `0b95fbf7b`。本輪在 D4 基準 `7b7b3b877` 上隔離並行處理不同檔案，再合入最新整合：Rust Host **移轉後多視圖終止／Tab Remove 防護＋可選收據 ABA 比對** `32f365b79`、Desktop **重啟仍保存原 transfer mutation 的 App Support Journal** `db7eca96b`、Mobile **觀察時的 Workspace/Tab/收據 scoped close 與失敗安全停止** `ea881429b`。獨立來源為 `bcd7f7cb0`、`1e85f25a1`、`bef757b7d`；三線提交／cherry-pick 無衝突。

**跨專案 Terminal Tab 拖放仍停用：`tab.transfer.capabilities.supported=false`。** 本批額外補強不等於 Host 全域最後視圖 lease；重啟後的未決 Journal 仍有 Host Crash／拒絕結果解除的安全 UX 待做。沒有執行 Flutter/Cargo 測試、Analyze、Cargo check、編譯、Win/Mac 實機、push、release。原本 dirty 的 `main` 未動；完整契約與下一批建議見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

## 最新：T2-D5 三線安全修補已合入（2026-10-30）

整合已包含 D4 `7b7b3b877`；本輪另從乾淨 D3 基準平行做三個**互不修改同檔**的安全修補，再 cherry-pick 到最新版整合：Rust Store 原子轉移不得順手重排其他 Pane，並保護目的分割結構與未修剪來源幾何 **`5b4d15db5`**；Dart Workbench 二次權威讀回兩側完整 Tab/Layout，拒絕不一致／讀回中變更的快照 **`a7a673591`**；Watcher-driven Tab 退役保留仍在執行的 Codex transcript watcher，只有確證 Session Exit 才清理 **`9d420bf8d`**。隔離來源分別是 `416dca6bf`、`6c5e3505a`、`c0f1fa5c0`，全線提交並合入，無同檔衝突。

**仍未開啟 T2：`tab.transfer.capabilities.supported=false`**。剩餘跨程序 PTY lease/last-view owner、Host Crash 中間結果恢復、重啟後 unknown mutation journal、Mobile 舊版相容與確實退出的 watcher cleanup，以及端到端驗證。只跑 `rustfmt`、`dart format`、Git whitespace check；沒有 Cargo/Flutter Tests、Analyzer、Build、Win/Mac E2E、Push 或 Release。未操作 dirty `main`。詳細見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

## 最新：T2-D4 平行三線安全防護已整合（2026-10-30）

從整合 `146482845` 重用三個乾淨 D3 工作樹切 D4 分支，不碰 `main`。Rust Host 驗證最後轉移收據避免 A→B→A 的 ABA 舊視窗誤終止 **`e74cda072`**；Dart Coordinator 保留 Host 失聯後的原 mutation guard，直到收到原收據且重新套用來源／目的權威 Layout 才清除 **`ad35694ac`**；Terminal PTY client 送本視圖所見收據、側邊欄未知結果有「Check status」手動查證入口 **`469b64c54`**。均由獨立分支提交後 cherry-pick，沒有整合衝突。

**功能仍停用**：`tab.transfer.capabilities.supported=false` 未變；跨程序真正 Last-view lease、Crash/Restart 的部分提交恢復、跨 App 重啟保存未知操作、Mobile 舊版 terminate 相容性與實機驗證仍待做。這輪僅 `rustfmt`／`dart format` 和 staged whitespace check；**沒有 Cargo/Flutter test、Analyze、Check、Build、Win/Mac E2E、Push 或 Release**。參見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

## 最新：T2-D3 三線安全補強已合入（2026-10-30，仍關閉跨專案拖曳）

確認 D1／D2 已在乾淨整合 HEAD `ca42a1be76e967580921f3ece25299765c5eba35`，重用三個既有乾淨 worktree，從最新整合點分別切出獨立 D3 分支，不新建大量檢出工作樹：

| 工作線 | 已修改的實際程式碼與防護 | 獨立提交 → 整合提交 |
| --- | --- | --- |
| D3-H Rust Store Layout | `RuntimeStore::upsert_workbench_layout` 改在**單一 SQLite Transaction** 內核對有 `lastTerminalTabTransfer` 收據的 Tab 目前真正所屬 Workspace；舊視窗 `layout.upsert` 不得把已轉移的 Tab 恢復到來源，或從目的 Layout 移除。一般無轉移收據的 Layout 行為維持原契約 | `01109e5c1` → **`4c6fc975d`** |
| D3-C Flutter lost reply | `WorkbenchTerminalTabTransferCoordinator` 保留 timeout/disconnect 後的原 `WorkbenchTerminalTabTransferPlan` 與原 Mutation ID；同 Tab 的第二筆移動被擋住，直到 Runtime 確認原 Receipt 及 Session。新增可明確查詢未知結果的 Recovery Port；不可盲目重送或 mint 新 ID | `69a81acd7` → **`76e093931`** |
| D3-O Terminal 起源/Host Restore | Desktop 明確 Restart 前確認原啟動資料夾仍存在；Rust Host 的 `createOrAttach` 只在要**建立新 PTY** 時比對移動收據的原啟動路徑（活 PTY 仍可附加）；`terminal.restart` 在結束原程序前驗證；Host `spawnOnCreate` 重建使用原路徑，失敗保留移動過的 Tab 而不刪除 | `e9e22859d` → **`0809bbe3f`** |

三線皆有 `rustfmt`／`dart format` 與 `git diff --cached --check`，逐一提交、無 cherry-pick 衝突。**本批依使用者此前指示未跑 Cargo/Flutter Tests、Analyze、Build、實機、跨程序及效能驗證，不能宣稱已可用。** `tab.transfer.capabilities.supported=false` 沒有改，跨專案 Drop 仍停用；原始有未提交資料的 `main` 未改動，未 Push／Release。

剩餘主要阻擋：**全域跨程序 PTY lease／Close/Detach 的最後 owner 規則**、Host Crash/Restart 與跨視窗 Tab/PTY 恢復一致性、SQLite 轉移收據／布局已更新但尚未重掛 PTY 的結果處理、實際編譯及 Win/Mac 回歸。D3 的 Runtime Store 保護僅限**具轉移收據的 Terminal**，不等於所有一般 Layout 都有版本化 CAS。接手見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

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
