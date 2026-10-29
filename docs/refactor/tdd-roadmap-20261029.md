# Alera TDD 架構重構總計畫（2026-10-29）

> 狀態：執行中。工作分支 `refactor/tdd-boundaries-20261029`，基準 `3501b05550f6ca476528ca63c3bb7673c444b985`。
> 每次接手先讀此檔、`docs/refactor/tdd-progress-20261029.md` 與 `skills/alera-di-architecture/SKILL.md`，以進度文件中的下一個未完成驗收項目繼續。
> 目標是可逐批合併的 TDD 重構；不改產品語意、不清理第三方既有工作樹、不自動推送或發布。

## Spec

- 維持 Flutter Desktop / Mobile、Rust runtime-host、Git Diff/Graph、Explorer、持久化 PTY 等既有功能及 UI。
- 降低跨層依賴、Fake 的必需模擬方法數、未受保護的重度工作、共享狀態與資源清理競態。
- 每個批次先建立最小可重現的紅測（功能、架構 guard、競態或契約），再實作、重跑綠測與回歸，留下可重跑指令。
- 有明確而可量測的 CPU/IO 熱點時優先 Rust；Flutter widget、Focus/scroll/listenable、Riverpod composition 及僅一兩次資料映射不強行跨 FFI。
- 多個 workstream 可平行 *調查／紅測／執行互不衝突的測試*。同一檔案、共享 Cargo target、Flutter generated output、Git index 禁止平行寫入。

## Design rules / Rust decision record

1. 不推倒重寫：保留四個 Workbench owner、單一 runtime PTY/session owner、SQLite shared pool 與 transaction 邊界、per-agent hook adapter、GitRustBackend concrete，以及舊版 host 相容。
2. Flutter application 只依賴窄能力 port。資料轉換可純 Dart；避免 widget 持有非同步生命週期、cache 與取消狀態。
3. Native Rust 合適位置：Git status/diff/history/diff algorithm、檔案索引/快速搜尋、批次 diff projection 與 native workspace I/O；仍需 benchmark + parity + cancellation + UTF-8/CRLF 測試後才能搬移。禁止因重構複製整份字串、建立同步 FFI 阻塞 UI、或更改既有 git credential helper 介面。
4. Rust server 有 `DeferredAdmission`、mutation queue、PTY 專屬背壓，不能把「主 inbox 無界」誤寫成所有路徑均無界；先量控制路徑 p95/p99、queue/in-flight/pending bytes，再設入站 admission。completion/quit/cancel 路徑不可自鎖。
5. 資料權威：project/workspace/tab/layout/orchestration/prefs 優先 runtime。明確 revision、instance ID、重播、commit/event、未知操作結果；desktop/mobile UI 狀態不強行共用。
6. 每個批次的 Git 修改只落在獨立 worktree 分支；驗收後按檔案清單 commit，是否合併 main 另行判斷，保留原本 main 的未提交變動。

## Workstreams（可平行的範圍、TDD 驗收）

| ID | Priority | 專責／相依 | TDD 紅測 → 綠測標準 | Rust 決策 | 狀態 |
| --- | --- | --- | --- | --- | --- |
| G0 | P0 | Architecture | `runtime_architecture_guard.dart` 原 2 項違規 → 0；Resource Tree label 更新、listener 清理/關閉不退化 | UI/provider port 保留 Dart | verified (batch 0) |
| A0 | P0 | Agents／Quality，與 G0 無檔案衝突 | synthetic fixture 正/反例通過；per-agent adapter 規則維持；新 touchpoint 需有減耦理由 | Agent launch 能力後續再研究，hook adapters 保留 Dart/Rust owner | guard verified；8 → 5 ongoing |
| Q0 | P0 | CI／品質 | max-lines 依 generated/locale/test/產品邏輯分類；新增檔案不灌水；基準不直接整批重設；逐步縮短已長檔 | 純品質工具不用 Rust | inventoried；134 violations remain |
| G1 | P1 | Git Capability Ports；先於 Git UI 大重構 | loader/cache/retention 只實作所需 narrow fake，成功同時不破壞單一 production backend；Git/history/scope 切換測試 | 重運算維持 RustGitBackend | history/compare ports verified；more ports pending |
| R1 | P1 | Runtime scheduler（Rust，與 G0/A0 可平行研究） | blocked heavy request + status/quit/cancel、超限拒絕、client disconnect、stale completion 確保成功；採樣 queue wait/latency | 全部 Rust | read-only audit complete；Rust TDD pending |
| W1 | P2 | Workbench | 針對 workspace switch/tab removed/late hydration 設 red race；縮 Owner Host 介面後保持狀態不變式 | 純 UI/owner Dart，IO 走 Rust | pending |
| T1 | P2 | Terminal | detach≠kill、兩個 view 的同一 session、背景 output/order、buffer release、restore 不回退 | PTY Rust owner；Renderer/畫面 Dart，必要 native parser 用 benchmark 再決定 | pending |
| D1 | P2 | Git Diff / Editor | 長行 Myers、side-by-side/editable、scope cache/in-flight、第一差異與水平位置回歸；CPU projections 可基準後 native 化 | Rust 優先算法，UI Dart | pending |
| F1 | P2 | Explorer / Files | 隔離 nested worktree、模糊查找、reveal/persist、conflict/rename、watch cancellation、path traversal parity | index/IO 優先 Rust；UI Dart | pending |
| C1 | P2 | Contract / Store / Mobile | lost reply after DB commit / replay，direct↔relay、restart/resume、old/new host wire fixtures | Rust store/transport，Mobile view Dart | pending |

## 執行批次與依賴

- Batch 0（本次）：G0 與 A0 先各自寫紅測，再綠測；Q0 分類並落檔；G1 可先做邊界試點。並行的是互不衝突的調查/讀取/測試，不同檔案寫入先約定 ownership。
- Batch 1：G1 query port 穩定後拓展 mutation ports；R1 單獨 Rust 測試/metrics（不改 wire schema）。
- Batch 2：W1、T1、D1、F1 可在分支或獨立 worktree 並行，分別擁有自己的 source/test；同一 Git Diff/Terminal 測試需隔離資源。
- Batch 3：C1 共享 fixtures、cross-version roundtrip、baseline 性能；再選擇整合時機。
- 跨語言 API 異動須執行 `make frb-generate` 並提交 Dart bindings；本次 G0/A0/G1 不改 FRB，無需重新產生 native binding。

## 每批 DoD（Definition of Done）

- [ ] 紅測先失敗且失敗理由符合目標（保留 red 結果）
- [ ] 做最小實作；相同測試變綠
- [ ] focused Flutter/Rust 測試與架構 guard
- [ ] 必要時 build_runner 單次產碼、normalize_generated_eof、dart format
- [ ] `git diff --check`、受影響 package analyze、檢查 task/git status
- [ ] 更新 progress 的測試、風險、未完成、下一步與 commit SHA
- [ ] 僅提交 scoped file；不以成功的單元測試宣稱完整 Release／全平台合格

## 假設、限制與不做事項

- 初始主分支 2026-10-29 12:43 的 `3501b0555`，main 存在舊歷史文件未提交與其他測試檔，不觸碰。
- 舊 `docs/architecture-review-2026-09-11.md` 不是這版的即時測試結果；本文件記錄新的校驗。
- 不重寫 terminal emulator、Git backend、cloud 架構，不做沒有 baseline 的盲目 Rust 遷移，也不批次刪除既存資料／Worktree。
- 工作可能跨多個對話；務必逐批更新進度。每次接手以 git status 與實際測試重查，避免將曾通過視為永久通過。
