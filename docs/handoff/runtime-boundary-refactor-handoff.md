# Alera Runtime-Boundary Refactor Handoff

更新日期：2026-09-11

## 先看這裡

這份文件是給下一位接手 Alera architecture refactor 的開發者使用。

- Repo worktree：`E:\Dropbox\work\alera\alera-runtime-boundary`
- Branch：`refactor/runtime-boundary`
- HEAD：`8722c1f487abd840cea823d0c41e805776af7b10`
- Working tree：交接前確認 clean
- Upstream：未設定
- Merge：尚未 merge 回 main
- Push：尚未 push
- 詳細歷史：`docs/history-session/22.md`
- Main repo 有一個既存且與本工作無關的 `tool/release/build_windows_release.ps1` 修改／檔案狀態；不要碰它。

## 接手原則

延續目前做法，不要大爆改：

1. TDD，小批次。
2. 先寫 red test，證明要解的 policy / race / failure boundary。
3. 做最小實作。
4. 跑 focused tests。
5. 若碰 `WorkbenchController`，再跑完整 `flutter test test/unit/workbench_controller_test.dart`。
6. 跑 `dart format`。
7. 跑 `dart run tool/quality/runtime_architecture_guard.dart`。
8. 跑 `git diff --check`。
9. 看 diff/status，確認沒有夾帶無關修改，再 commit。
10. 不要自行 merge / push，除非使用者明確要求。

Windows Flutter/native build hooks 偶爾 cold-start 很慢。遇到 retained session 請接回既有 session，不要因為畫面暫時沒輸出就重開重複 process。

## 核心架構方向

目前採用的依賴方向：

`feature application -> narrow contract/interface -> runtime adapter/composition -> platform/runtime host`

重要規則：

- application 不應 import presentation。
- `WorkbenchController` parts 不應直接讀 Riverpod Provider/ref；`workbench_controller_internals.dart` 是 composition edge。
- 保持只有一個 `TerminalRuntime` instance，透過 narrow lifecycle / coordination / focus capability 對 application 暴露。
- deterministic state transition 優先做 pure plan/reducer。
- async sequencing、cleanup order、rollback / partial-success、subscription ownership 才值得 coordinator/service。
- 不要為了減少 controller 行數去包 scalar setter、單純 CRUD 或一兩行 delegation。

## 本輪已收斂的 Workbench 邊界

以下區域已經有明確 application boundary，不建議下一位再重複抽一次：

- bootstrap sequencing：`WorkbenchBootstrapOrchestrator`
- concurrent bootstrap gate：`WorkbenchBootstrapGate`
- main workspace preparation/coalescing
- workspace selection hydration + selection coordinator
- layout repository port / resolver / load coordinator
- persisted tab restore/open coordinator
- generic tab placement plan/coordinator
- replaceable file-tab mutation queue + placement transaction
- PR/Merman reusable-tab placement policy
- closed-tab plan / completion orchestration
- sleep workspace coordinator
- delete-workspace / remove-project cleanup coordinators
- retired workspace cleanup coordinator
- retired tabs cleanup coordinator
- workspace tab closing reference-counted scope
- intentionally-cleared layout transient registry
- project -> workspace subscription lifecycle coordinator
- workspace -> tab subscription lifecycle coordinator
- root/workspace/tab subscription registries
- worktree metadata watcher registry
- navigation history service including async back/forward traversal

Watcher sync 現在已拆成三個層次：

- pure state plan
- subscription lifecycle
- resource cleanup

不要重新把這三層揉回 controller。

## 最近完成的 commits

由新到舊：

- `8722c1f4` `fix(workbench): ignore stale tab focus after workspace switch`
- `fd27342c` `refactor(workbench): centralize navigation history traversal`
- `f82aa954` `refactor(workbench): centralize pull request tab placement`
- `81498e92` `refactor(workbench): extract retired tabs cleanup coordinator`
- `85e3c478` `refactor(workbench): extract project workspace subscription coordinator`
- `b2078e1e` `refactor(workbench): extract workspace tab subscription coordinator`
- `5d2fd9f9` `refactor(workbench): extract retired workspace cleanup coordinator`
- `39e5bf85` `refactor(workbench): commit replaceable tab placement in coordinator`
- `e44116bb` `refactor(workbench): extract workspace selection coordinator`
- `eda44d32` `refactor(workbench): extract persisted tab open coordinator`
- `4590777f` `refactor(workbench): extract closed tabs completion coordinator`
- `0642eda5` `refactor(workbench): extract workspace tree pin coordinator`

更早的 runtime-boundary / Workbench refactor commits 請看 `docs/history-session/22.md` 與 `git log`。

## 重要 bug fix：stale tab focus race

commit：`8722c1f4`

原本 `selectWorkspaceTab()` 在跨 workspace 時會 await workspace selection / hydration。

race：

1. 要 focus target tab。
2. 開始切換 workspace。
3. hydration 還沒完成時，tab watcher 把 target tab 移除。
4. await 完成。
5. 舊程式仍呼叫 `_setActiveTabInternal()`。
6. 因 public active-tab fallback 本來允許 layout 找不到 tab，已刪除 tab id 會重新寫回 `activeTabIdByWorkspace`。

已新增 integration red test 真正重現，再修成：只有跨 workspace 且 await 回來後，重新確認 target tab 仍存在；不存在就不再 focus。

不要把這個 guard 下沉成全域 `setActiveTab` validation，因為既有 public `setActiveTab(...missing-tab)` fallback 是被測試保護的既有語意。

## 已確認不需要再處理的地方

### Sections root stream

不要額外加一層 Sections reconnect supervisor。

runtime implementation 使用 `runtimeSnapshotStream`，其設計就是長存活且 transient IPC failure 會自行 retry。application 再加 retry 會重複責任。

### Git worktree metadata refresh

目前 watcher 已具備：

- debounce
- single-flight refresh
- refresh-again coalescing
- suspend / resume
- suspended 期間保留一次 queued refresh
- periodic poll 補 missed filesystem event
- watcher error/done 後由 poll 重新建立 metadata watches

`WorkbenchController._refreshProjectWorktreesInBackground()` 現在只剩 disposed/project capability guard + best-effort reconcile，屬於合理 glue，不值得再包一層 service。

### Scalar controller operations

例如簡單 view-prefs setter、Section CRUD、純 delegation，不要為了「controller 變短」硬抽 coordinator。

## 下一位應該先做什麼

### 第一優先：從「實際風險」而不是 controller 行數找下一批

先 audit 其他 Workbench async entry points，找這幾類：

- await 前後 state snapshot 可能 stale
- watcher/event 可以在 await 期間改掉 target entity
- partial-success 後 retry 可能重複 side effect
- cleanup failure 會留下 ownership/resource inconsistency
- subscription replacement/cancel race
- layout/tab persistence failure 後 state 與 repository 的語意不清楚

每發現一個候選，先寫能在舊實作上穩定變紅的 test；無法證明就不要改。

### 第二優先：考慮離開 Workbench，audit 其他 subsystem

Workbench application/controller 現在大多已是合理 orchestration glue。若沒有新 race 或 duplicated workflow，建議把同樣的 architecture audit 方法用到其他 subsystem，而不是繼續在 Workbench 做微小抽象。

可以優先看：

- app/runtime bootstrap ownership
- runtime_host 與 feature boundary 是否還有反向依賴
- shutdown / quit orchestration 的 failure/ordering boundary
- Rust daemon/runtime host 中仍過度集中的 application orchestration

這些方向源自早期 architecture audit；具體背景可看 `docs/history-session/20.md`、`21.md`、`22.md`。

## 若仍要繼續 Workbench，請先做這個 audit

先跑：

```powershell
rg -n "await |unawaited\(|try \{|catch \(|state =|_applyLayout|_setActiveTabInternal" lib/src/features/workbench/application/workbench_controller*.dart
```

逐項分類：

- thin glue：不動
- pure policy：抽 pure function + unit test
- async transaction：考慮 coordinator
- race candidate：先 integration red test

目前已確認 controller parts（排除 composition edge `workbench_controller_internals.dart`）沒有 direct `Provider` / `ref.read` / `ref.watch` / `ref.listen` leakage。

## 驗證基線

最近完整驗證皆通過：

```powershell
flutter test test/unit/workbench_controller_test.dart
dart run tool/quality/runtime_architecture_guard.dart
git diff --check
```

stale-tab race additionally covered by：

```powershell
flutter test test/unit/workbench_controller_test.dart --plain-name "selectWorkspaceTab does not focus a tab removed while workspace selection is in flight"
```

navigation history service 有 7 個 tests，包含：

- back/forward resolve
- stale history prune
- select before commit
- selection failure 不 commit cursor
- no target 不呼叫 select

## Git / 合併注意事項

交接時預期：

```text
branch: refactor/runtime-boundary
HEAD:   8722c1f487abd840cea823d0c41e805776af7b10
merge:  no
push:   no
```

接手者第一件事：

```powershell
git status --short --branch
git rev-parse HEAD
```

若不是上述 clean HEAD，先確認是不是前一位留下的新修改；不要直接 restore/reset。

不要碰 main repo 既有的 `tool/release/build_windows_release.ps1` 狀態。

## Session / MCP 接手資訊

完整持續記錄：`docs/history-session/22.md`

目前 session key：

`v1/4TwHgs3ubvxRqCjURKwXgtyVOedZHBtUrz0LbH45abH5e3VcWWEoMzHehyg9c6QtlfrzvKdF9ElR`

若 coding-tools MCP 支援 retained session/operation id，長測試或 Windows Flutter cold-start 請 reattach，不要重複啟動相同測試。

## 完成定義

後續每一批改動至少滿足：

- 有明確問題或 architectural boundary，不是為拆而拆
- red-first test（若是行為/race/policy）
- focused tests green
- controller touched 時 full controller suite green
- format green
- runtime architecture guard green
- `git diff --check` green
- diff 僅含該批相關修改
- 獨立 commit
- 不自行 merge / push

如果 audit 後發現剩下都是 thin glue，應該停止 Workbench refactor，轉去下一個 subsystem，而不是繼續製造 coordinator。
