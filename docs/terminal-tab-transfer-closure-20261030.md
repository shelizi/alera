# Alera T2 跨 Workspace Terminal Tab：一次收尾成果與最後驗收門檻（2026-10-30）

> **狀態：收斂開發，尚未可啟用。** 不再依照 D18、D19、D20 無限分批。現在的任務是完成確定的可安全修改項目，然後按本文件的**有限驗收清單**決定能否開 Gate。沒有編譯與真實 OS/Phone E2E 就不宣稱完成，不為了趕完成而刪原 PTY 或解鎖 Journal。

## 整合分支與原限制

- 唯一整合分支：`refactor/tdd-integrate-20261029`（`.worktrees/refactor-tdd-integrate-20261029`）。本輪起點：D17 **`7cac43d53a81198fdc10d601f188b02da45cb7de`**。
- 本輪兩條隔離來源線：Store **`d479429e7`** → 整合 **`09c8aba3a`**，Hash 編碼靜態補正 **`99cbe4a4c`** → **`46794ddfe`**，Host 停止程序前再檢查來源 Scope **`7c7689675`** → **`185510980`**；Journal **`be6d1922a`** → 整合 **`d12ca2e69`**。本文件提交後 HEAD 再前進，接手先以 `git log -1` 查證。
- 原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 及所有原有 dirty files 不動（僅既定 session checkpoint 會依工作流程寫歷史）。
- `rust/alera-cli/src/terminal_host/server/requests.rs` 的 `tab.transfer.capabilities.supported=false`；無自動開放跨 Workspace 拖曳、無 Push 或 Release。
- 本輪依先前限制，**只使用 Rust/Dart format、Git whitespace checks、Git commit/cherry-pick**。沒有執行或新增 Cargo/Flutter Tests、Cargo check、Flutter analyzer、Build、Win/macOS/Phone 原生 E2E。

## 真正收尾且已整合的可檢查項目

| 邊界 | 實際處理 | 尚不可推論 |
| --- | --- | --- |
| 普通 Store 單 Tab 刪除 | `RuntimeStore.remove_workspace_tab` 在**同一 SQLite 交易**內確認該 Tab 沒有 `lastTerminalTabTransfer` 或 `lastTerminalTabExit` Key（包含 JSON null／畸形），否則拒絕；普通非 transferred Tab 照舊可刪 | 原流程 `remove_workspace_tab` 不再是移轉 Tab 的合法刪除 API |
| Workspace/Project 級聯／Sleep | `remove_project`、`remove_workspace`、`remove_workspace_tabs_for_workspace`、`sleep_workspace` 均在各自的**實際 SQLite 刪除交易**內確認不存在無法安全丟棄的 transferred Evidence；也掃描**跨到其他 Workspace**但還把即將刪除的 Workspace 記作來源／執行 scope 的原 Receipt。**Host 在不可逆程序關閉之前也做相同來源 Scope 唯讀預檢**，刪除交易再次重驗。遇到孤兒／壞收據 fail closed | 不能以 Target 沒有此 Tab 為由任意移除 Source Workspace；此防線可能令無法歸屬的壞 Receipt 對 Workspace 清理保持阻塞，需人工處理 |
| 主動關閉的唯一原 View（尚須實測） | 新 `remove_transferred_terminal_after_verified_live_view` 只供 Actor 已核對**原始活 PTY + 最新 Receipt + 唯一 View Token**後的序列化 `tab.remove`。Store 再核對 original Host Generation/Session/PTY Token、owner、兩側完整持久 Layout/Tab registry、Exit Marker **不存在**。同一交易：刪 Tab → 從 Target Pane 移除並必要時修剪 split tree → 寫入 `terminalTabRetirementAudits` 永久稽核（只存 SHA256 View Token，不存 Bearer Secret）。任一 CAS/版面失敗整筆回滾 | SQLite 不能自己證明其他 Host Process/OS View 的生命狀態；目前 Actor 的 per-View 判定仍依原 Host in-memory 證據；**不能**拿這條途徑假裝 Host Crash 後原 PTY 仍活著 |
| Journal 跨程序 Lock | Lock 版本更新為 `2`，新增 App Process 隨機 `processEpoch`，正常釋放仍需同 nonce、Tab Hash、PID、Process Epoch；不會因 PID 被回收重用就視為原 App。舊格式/中斷 Lock 只在 Inspect 顯示異常並保留 | `processEpoch` **不是**跨 OS Process liveness check，也**不是**安全 unlock 權限；不提供 stale lock 自動刪除 |

以上在 D1–D17 的已整合安全守衛基礎上完成，含轉移單筆兩 Layout CAS、原 PTY Token、Host 狀態唯讀核實、Mobile 原 View Revision/Host Client 检查、Journal Pending/Archive Dossier。

## 目前仍未完成、因此不能開 Gate 的**有限**驗收門檻

| 必要門檻 | 可接受的完成證據 | 現狀 |
| --- | --- | --- |
| **A. 崩潰及跨程序/視圖權威** | Windows/macOS 不同原生視窗／不同 App Process、兩支手機同時附著／解除、Host 重啟／崩潰後，能對原 Host Generation 和最後 View 做**安全且一致**的判定。當原 PTY 確實無法恢復時，顯示清楚人工確認與安全退休／重新開 Shell 的選項，**不冒用原 PTY 世代** | 未完成跨程序 durable registry、Host Crash definite resolution；現有邏輯採拒絕誤刪 |
| **B. 已退出 PTY / Journal 證據安全人工復原** | 不僅有唯讀原 Exit+兩 Layout 同快照，還有使用者意圖、Host/OS Process Incarnation 和已確認 View 的完整驗證；可安全解鎖 stale Journal 且保留 Host 權威／本地稽核，在未知狀態不自動清理 | 原 Host 正常活 PTY 的最後 View Close 已有專用 CAS 交易；**已退出／Host 已崩潰**的可靠人工恢復與 stale Lock unlock **尚未實作** |
| **C. 靜態／功能／原生整合驗收** | `cargo check`、Rust/Flutter tests、Flutter analyzer、Windows/macOS/手機/跨多視窗 E2E、反覆失敗重試/DB Crash injection 全部有可重現結果；Gate ON 前確認原有 Terminal、Tab 重新排序、Detach、Diff、其他 Store 刪除流程沒有退化 | **先前明確限制不允許執行**，因此無任何通過證據；不會聲稱測試成功 |

### 決策

- **目前可以交付的**：整合分支上的 Store 安全處置、原 View 主動關閉專用交易與稽核、Journal Lock 世代保護及既有 D1–D17 功能安全補強；Git whitespace／格式層級驗證。
- **目前不可以宣告的**：跨 Workspace Terminal 移動已通過最終 E2E、已能跨 Host Process Crash 保留原 PTY、所有 View 的 durable Last-view、Stale Lock 已可安全人工解鎖、可合併 main／發布。
- **不再新增新編號階段**：後續只用上面 A/B/C 三項**明確的 Release Gate**關閉剩餘工作。若持續限制測試／編譯／實機操作，C 無法達標，功能必須保持 `supported=false`。
