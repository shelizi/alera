# Alera TDD 重構執行進度（持續更新）

> 起點：2026-10-29；Worktree `.worktrees/refactor-tdd-boundaries-20261029`；Branch `refactor/tdd-boundaries-20261029`；main 基準 `3501b0555`。
> 讀取 `docs/refactor/tdd-roadmap-20261029.md` 了解契約/驗收，然後從下列 pending/in_progress 工作續做。
> 變更只在隔離 worktree，未合併 main／未 push／未 release。

## 最新整合狀態 — 第二批三線平行（2026-10-29）

**接手以此節與下方新驗收紀錄為準；以下原有 134 項等數字為第一批歷史快照。**

- 整合工作樹 `.worktrees/refactor-tdd-integrate-20261029`，分支 `refactor/tdd-integrate-20261029`，包含首批 `552c6152e` 和第二批三項已驗證重構：
  - Q0 原分支 `refactor/tdd-quality-20261029`：`b56dbffd6`；整合 `94f900812`。將 Codex/Copilot/Cursor 能力規格移至各自 adapter，保留 registry 登記順序、完整 read-only/diff-only 安全 CLI 參數。
  - G1 原分支 `refactor/tdd-git-20261029`：`722750370`；整合 `3f3eff27b`。抽 `GitRepositoryProbePort`（一方法）與 `GitHostedReviewRetentionPort`（兩方法），不分裂 RustGitBackend。
  - R1 原分支 `refactor/tdd-rust-runtime-20261029`：`b3b0757e4`；整合 `669f93275`。Rust `DeferredAdmission` 依 request type 新增 **常數記憶體** queue-wait histogram，向 `status.get.deferredAdmission.requestTypes[]` 加入 `queueWaitSamples`／`queueWaitP95Ms`／`queueWaitP99Ms`，不改排程/PTY/DB/FRB，保留既有 sum/max。
- Q0 當下超過 500 行檔案 **166**，**133** 項違規（歷史 167/134 → 166/133）；`ai_assist_registry.dart` 約 616 → 466 行；未重設 baseline。`Agent` guard 仍有 8 manual touchpoints（target <=5）。
- **驗證**：Q0 AI Assist 63 tests passed、characterization 15 passed（有重疊）；5 個檔案 targeted Flutter analyze **No issues found**。G1 hosted-review/probe/workbench lifecycle **12 tests passed**；architecture guard passed；擴大 Dart analyze 曾有 4 項 info 已補註記，第二次 analyzer 因高負載 timeout，**不能宣稱第二輪 analyze 通過**。R1 Rust histogram pure test 2 個先紅後綠；Cargo integration metrics 測試先紅後綠（`queue_wait` 篩選 3 passed，另 1 ignored benchmark）；原生 admission/control 9 tests passed。Rust 單一 pending/queue metrics 測試如未有 exit 0 則記為未確認。
- 三批各自 source/test commit 已保存，不合併 dirty `main`，未 push／release。完整 Flutter/Rust/mobile CI 未跑；`check_max_lines.dart` **仍紅（133）**。下次先在整合工作樹 `git status`／`git log -4 --oneline` 並跑 scoped checks，再開下批 worktree。

### 下批互不衝突的工作所有權

| 議題 | 狀態 | 下一個確定性紅測／執行 |
| --- | --- | --- |
| Q0 | 部分完成（133 violations） | 針對 Explorer/Terminal/Diff 的具體責任邊界寫測試再減少；禁止重設 baseline |
| A0 | guard 綠，但尚有 8 manual files | 以 per-agent adapter 鑑別特殊政策，把 touchpoint 逐一消除，驗收 <=5 |
| G1 | 四個 narrow ports 已驗證 | 針對 Git status/diff/mutation 的 consumer/fake 縮減做 TDD |
| R1 | 排隊指標實作、排程不變 | 真正未受控 inbox 的多 client/backlog/quit latency TDD；量 status/quit/cancel p95/p99；避免替換全 bounded inbox 導致死鎖 |
| W1/T1/D1/F1/C1 | 未實作 | 依 roadmap 的所有權與 race/相容測試分 worktree 同步進行 |

## 已完成的 scoped commits

- `57fdaf2349b80d604226fdc1d0aa3b101f44ca37` — `refactor: isolate terminal metadata and agent launch capabilities`（G0/A0；包含必要的 Riverpod 產碼與測試）。
- `378d62d7e3360b79bf6dd090f6ac898d73f23761` — `refactor(git): separate history and commit-compare query ports`（G1 history/compare；RustGitBackend concrete 保持不變）。
- 本文與 roadmap、Q0 inventory、R1 Rust audit 作為第三個 docs scoped commit；查詢最新 SHA 請用 `git log -3 --oneline`。
- `build_runner` 的三個 unrelated generated hash／註解修改已回復至此分支原狀，避免夾帶無關程式碼。必要的 G0/A0 產碼則隨第一個 commit 保留。

## 儀表板

| ID | 狀態 | 最新證據／阻塞 | 下一步 |
| --- | --- | --- | --- |
| G0: Resource Manager DI | verified / committed | architecture guard：2 項 → 0；Riverpod generation exit 0；Resource title / runtime binding 等在 79 passed；app smoke + workbench boundary 10 passed | 依本文件追蹤後續更多高壓回歸 |
| A0: Agent extension | verified / committed | agent guard 5 additions+1 stale → passed（仍有 8 處，target <=5）；guard fixture 4/4 passed；AI Assist/Grok/FX/AGY 相關包含在 79 passed | 後續 8 → 5 的 Agent touchpoints |
| Q0: max-lines | inventoried / gate_red | 167 大於500行（33 baseline, 19 grew, 115 new），其中 134 offenders；新增分類文件 | 依所有權 TDD 分批降低，不一鍵放寬 baseline |
| G1: Git ports | history/compare verified / committed | 新增 History/CommitCompare ports，狹義 Fake + 原既有測試 9/9 passed；保持 RustGitBackend concrete | 擴更多 Git query/mutation ports 及跨 scope/race tests |
| R1: Rust runtime admission | pending | main inbox unbounded，但 deferred admission 存在 | 先量 coverage，再寫 Rust race/admission test |
| W1/T1/D1/F1/C1 | pending | 見 roadmap | 待前置護欄穩定 |

## Red → Green 稽核紀錄

| 日期／批次 | Red（先失敗） | Green（實作後） | 檔案／commit |
| --- | --- | --- | --- |
| 2026-10-29 baseline | `dart tool/quality/runtime_architecture_guard.dart`: 2 違規；`dart tool/quality/check_max_lines.dart`: 134 offenders；`dart tool/quality/agent_extension_guard.dart`: 5 additions + 1 stale；`dart tool/quality/agent_extension_guard_test.dart`: synthetic positive fixture 失敗 | `dart tool/quality/runtime_architecture_guard_test.dart`：5 passed；其他尚未實作 | baseline `3501b0555` |
| 2026-10-29 G0 | 架構 guard：presentation import 與 full TerminalRuntime provider（2 個 fail） | architecture guard exit 0 | terminal session title port、app composition、Resource Manager、main overrides、title tests |
| 2026-10-29 A0 | synthetic fixture 正例因缺 adapter/shared policies 失敗；目前 5 additions、1 stale | agent extension guard tests 4/4 PASS；agent extension guard exit 0（8 manual touchpoints） | Agent adapter detection、AI Assist capability policies、guard fixture/baseline |
| 2026-10-29 G0 + A0 focused | G0 guard 2 violations、Agent synthetic fixture fail | `flutter test --no-pub`（resource title、runtime binding、resource providers、AI Assist settings/service、local Agent detection）：79 passed；generator exit 0；`git diff --check` exit 0 | G0 + A0 來源與 .g.dart；部分 .g.dart 為 one-shot generator 的 hash/註解更新 |
| 2026-10-29 G1 | 新單測需 `GitHistoryQueryPort` / `GitCommitCompareQueryPort` 才能編譯；此前大 Backend Fake 是唯一測試入口。嘗試的單檔 `dart analyze` 在編譯暖機時逾時，未取得可靠 red 執行輸出 | `flutter test --no-pub test/unit/workspace_git_query_ports_test.dart test/unit/workspace_git_history_loader_test.dart test/unit/workspace_git_commit_compare_cache_test.dart`：9 passed | GitQueryPorts + backend interface + history loader/commit cache + narrow fake tests |
| 2026-10-29 Final verification | 第一輪 `flutter analyze` 出現既存/新 lints；在嘗試公開不可得 `Override` 型別後 smoke compile red | 已改用 opaque Riverpod return type 推導；`flutter analyze --no-pub`（11 個影響範圍）**No issues found** exit 0；Widget app smoke + workbench terminal boundaries：10 passed；AI Assist/Git ports/widget 重跑：65 passed；guards 測試 runtime 5/5、agent 4/4 passed | G0/A0 `57fdaf234`；G1 `378d62d7` |

## 並行策略／檔案所有權

- G0：`features/resource_manager/application`、`app/terminal_runtime_composition.dart`、resource title test；需 Riverpod 一次性 codegen。
- A0：`tool/quality/agent_extension_guard*.dart`、`agent_extension_touchpoints.txt` 與 agent-specific app 檔。不得同時修改 G0 檔。
- G1：`shared/infra/git`、`features/workbench/application/workspace_git_{history_loader,commit_compare_cache}.dart` 與專用單測。
- R1：`rust/alera-cli/src/terminal_host`；Cargo target 與其他 Rust build 互斥。
- Shared generated output、main Git index、整包 Flutter build runner 在不同 worktree 保持單寫者。

## 需持續追蹤

- baseline max-lines 134 項：先把分類清單/原則紀錄，不應直接修改整份 baseline 掩蓋產品檔案。
- 分類明細與處理政策：`docs/refactor/q0-max-lines-inventory-20261029.md`；167 = Desktop 45、Rust 50、Tests 57、Localization 7、Other 8。舊 baseline 33，134 違規仍存在，不宣稱整體 PR CI 通過。
- 已以 robocopy 從原 main 複製缺少的 `third_party/xterm`、`third_party/dart_terminal` 到新 worktree 供離線 Flutter 測試；移除複製所帶入的失效 .git pointer。主分支原先的 dirty source 沒有改動。
- Rust runtime 先驗證既有 admission：DeferredAdmission 上限 8 active / 64 total / 8 critical reserve；runtime mutation queue 上限 256，且已有 queue wait / rejected / disconnect metrics。下一階段 R1 應先測未經 admission 的主 inbox 路由，不要重做上述現成功能。
- Rust R1 實際排程 code / route policy / admission tests 盤點與第一組 red/green 驗收契約：`docs/refactor/r1-runtime-admission-audit-20261029.md`（Rust source 尚未修改）。
- Window/Flutter tests 可能受 cold-start native build、missing submodules、Flutter generated dependencies 影響；失敗要記錄分類，不得宣稱通過。
- 已有其他進行中 release task 的 main scope；所有本次修改留在新的 refactor worktree。

## 後續接手起點（精確）

1. `git -C .worktrees/refactor-tdd-boundaries-20261029 status -sb`、`git -C .worktrees/refactor-tdd-boundaries-20261029 log -3 --oneline`；禁止直接對原 main 的 dirty docs/history 進行清理。
2. 優先 `Q0`：既存 134 個 max-lines offenders、`A0` Agent touchpoint 8 → 5；不得以一次性 baseline 重設偽裝成 CI 通過。
3. 可與 Q0 平行的 Rust `R1`：見 `docs/refactor/r1-runtime-admission-audit-20261029.md`，第一個 red test 是 slow worker 同時讓 status/quit/cancel 前進，保留現有 admission semantics。
4. `G1` 下批候選是 Git diff/status mutation 能力 ports 與相對應小型 fake。Rust heavy operations 已位於 native Git backend，未經 benchmark 不搬 UI 事件跨 FFI。
5. `T1/D1/F1` 有各自的 benchmark/docs；維持小批 TDD，只有明確的 CPU/IO 熱點才優先遷移 Rust。

## 本輪不宣稱

- **全套 PR CI 綠燈**：`check_max_lines.dart` 仍有 134 violations。
- **Rust actor 改造已完成**：本輪完成 R1 調查與後續紅綠規格，尚未寫入 Rust source。
- **跨平台 Release 驗證**：未跑完整 Rust/mobile/Windows/macOS build，也未推送。

## 最後接手確認

- [ ] 工作樹 diff/status
- [ ] G0/A0 focused red-green
- [ ] 品質檢查重跑與結果
- [ ] 重要測試及生成檔比對
- [ ] commit SHA 與下一批目標
