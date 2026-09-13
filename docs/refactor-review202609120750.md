結論
目前重構 沒有實質偏離原本方向。核心仍維持「按 state ownership / capability / contract 拆分」，而不是 rewrite 或只切檔案。需要調整的是接下來的優先順序：max-lines 已經從交接時的 14 個降到 8 個，現在不應再把「清到 0」當主線，應回到 Phase 2 correctness。
目前實際狀態是 `refactor/architecture-guard-ci`、HEAD `23434339`。Architecture guard fixture 與正式 guard 都通過；最新 dispatch-context focused tests 實際跑到 4/4 通過。
這段重構有沒有走歪？
交接後幾個 commit 我逐一比對過。`91707c90` 把 Project Clone 從 `project_requests.rs` 抽出，屬於完整 clone-job capability 拆分，不是機械切行；這是正確方向。`23434339` 更符合原設計：dispatch context 改成「actor 建立 owner → background install → `ServerCommand` completion → actor revalidate → commit continuation」，有 generation、防 stale completion、owner 已死時丟棄等測試，這正是之前規劃的 Phase 2 模式。
比較偏的是節奏：交接後不少 commit 都在拆 test-only max-lines debt。這些沒有破壞架構，而且成功把 offender 從 14 降到 8，所以不是錯誤；但現在應該停止把它當主要 KPI。若接下來為了 500 行限制硬拆 `orchestration_requests.rs`、`managed_workspace.rs`，才會真正開始偏離方向。
另外 audit 過程中出現一個我沒有建立的 untracked `docs/refactor-recommond.md`。我只做了唯讀檢查。它是以 `main 8e4e89e8` / `runtime-boundary 6ab6b9b9` 為基線的舊建議，部分原則仍正確，但執行順序已過時。例如它擔心 `runtimeHostClient` 暴露 `SocketTerminalHostClient`，目前 `runtime_host_providers.dart` 已經對 feature 暴露中立的 `RuntimeHostClient`；它提到 snapshot refresh 重疊，現在 `runtime_snapshot_stream.dart` 也已有 `refreshRunning/refreshQueued` 序列化。因此不要照那份文件重新從 protocol/provider 開始做。
現在真正需要修的 4 個結構問題
第一優先是最新 dispatch context async 化之後的 crash/restart durability gap。`orchestration_requests.rs:1213-1228` 會先把 dispatch 持久化成 `awaiting_acceptance`，context install continuation 則只存在記憶體的 `pending_dispatch_installs`；server restart 時這個 map 在 `server_runner.rs:90` 會重新是空的。更關鍵的是 DB 只保存 `context_token_hash`，沒有 plaintext token，所以 restart 後根本不能安全重建原 context。
目前雖有 `expire_unaccepted_orchestration_dispatches()`，但它只在 coordinator tick 的 stale sweep 中執行（`coordinator_requests.rs:484-499`）；manual dispatch 或 coordinator 未重新啟動時，不能把這當成 startup recovery。這是目前最值得優先補的 correctness contract。
第二個問題是 shared deferred semaphore 開始有「萬用 queue」傾向。現在 8-slot capacity 可以限制 active I/O，但 `start_deferred_request()` 是先 `tokio::spawn`，task 再等待 permit，因此 pending task 數仍無界。而 prompt upload、snapshot/Git read、agent hook reconcile、現在連 dispatch context install 都共用同一批 slots。Actor 不會卡住是好事，但 critical dispatch continuation 有可能排在大量 bulk I/O 後面。
第三個問題是 `workspace.removeManaged` ownership 重複。現在 routing 在 `deferred_requests.rs:296-338` 先做 automation / removal / storage validation；進 runtime mutation queue 後，`prepare_managed_workspace_removal()` 又再跑 `validate_managed_workspace_removal()`。而 validation 內有 `canonicalize`、`symlink_metadata`、Git worktree 掃描。也就是目前同一 safety policy 被跑兩次，而且其中一次仍在 actor mailbox 內。這是非常適合下一輪 prepare/commit 重構的地方。
第四個是剩餘 actor I/O。其中 `runtimeSettings.update(automation)` 在 persist setting 後仍同步 `reconcile_autostart()`；`coordinator_probe_drift()` 雖用了 `spawn_blocking`，但 actor 還是 `.await` 它，所以 Git probe 期間 mailbox 一樣停止前進。這兩項都是真的 actor stall，不是單純檔案太長。
我建議重新排序後續重構
優先
批次
TDD / 完成標準
P0
Dispatch startup restart recovery
模擬 DB 已 commit `awaiting_acceptance`、context continuation 尚未完成就 restart。新 host 啟動後要把 orphaned startup exactly-once 轉成 `startup_failed`，task 回到既有 retry/stall policy，清 context；已 accepted/completed 不受影響。不依賴 coordinator tick 才恢復。
P1
Deferred admission / QoS
先記錄 active、pending、queue wait、request class；再加 bounded pending。測試大量 bulk jobs 時 `status/quit` 仍前進，而且 dispatch-critical continuation 不可永久 starvation。維持 global total cap，不建立數個無界 pool。
P2
Managed workspace removal ownership
把 DB/FS/Git external preflight 移到 mutation worker；actor 只保留 session、pending-shutdown 等 actor-owned state。測試 invalid path/automation 時 不能先殺 session；blocked Git/FS preflight 時 status 仍可回。
P3
Automation autostart + coordinator drift
先定義 autostart「setting persisted 但 filesystem reconcile 失敗」的產品契約，再 background 化；Git drift probe 改 completion 模式。兩者都以 barrier test 證明不堵 actor。
P4
Transaction / replay matrix
Project、Workspace、Tab/Layout、Orchestration、SharedPrefs 分批做「commit 後 reply lost」、「同 operation replay」、「stale owner/revision」。這時順勢按 message/task/gate/run/dispatch 拆 `orchestration_requests.rs`。
P5
Wire contract
Rust / Desktop / Mobile 共用 golden fixtures：optional/unknown field、old host capability、shutdown/error、revision、terminal output resync。
P6
Workbench 真正 owner 拆分
Catalog → Tab/Layout → Selection/Navigation → Resource Lifecycle；保留 `WorkbenchController` facade，不是再拆更多 `part`。
P7
Terminal / Git / Mobile
Terminal session vs view ownership、Git history/cache loader、Mobile reconnect transition matrix。
P8
Agent / Settings / Release / Observability
capability matrix、settings authority、平台 contract、operation/request ID、queue wait、stale completion 等量測。
剩下 8 個 max-lines 檔案可以自然跟上述階段一起消化：`orchestration_requests.rs` 跟 P4、`managed_workspace.rs` 跟 P2、`workspace_service.dart` / `workbench_dialog_launchers.dart` 跟 Workbench、`git_diff_impl.rs` 跟 Git phase、settings 兩檔留到 Settings phase；`alera_localizations.dart` 最低優先單獨拆 catalog。這樣 max-lines 會下降，但它是架構改善的結果，不是目的。
所以接下來我會把主線改成：先補 dispatch crash/restart recovery → bounded admission/QoS → managed workspace ownership。這三批完成後，再重新掃一次 ServerActor，才進 transaction/replay 與 Workbench。這樣方向會比繼續清剩餘 8 個大檔更準。
本次 audit 沒有修改或提交 source，也沒有 push；`docs/history-session/23.md` 已更新這次檢查結果與新的後續順序。