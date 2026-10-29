# R1 — Rust runtime admission / Actor 調查與 TDD 驗收契約

> **第六批 `996962ef4`（隔離 `cbf78ff11`）**：第五批跨 localhost 的 64 pending command count 限制升級為**兩個共享 Tokio Semaphore**：最多 64 個已入 Actor 的本機請求，另按每 4 KiB 向上取整為 byte permits、同時最多 **32 MiB 已 admission 請求負載**。新的 `LocalClientIngressAdmission` 在 `tokio::select!` 其中一個 branch 等候 bytes→command，若其他控制帧先執行則 future cancel 自動釋放取得的 permits；`AdmittedClientLine` 同時攜帶兩個 `OwnedSemaphorePermit`，Actor `handle_line` 返回才釋放。無需搬動 PTY 完成／shutdown／restart／mobile／relay／hook 所用獨立入列路徑；滿載時 Socket writer 繼續服務 control/terminal。紅測最初因無新 limit/type 編譯失敗；首輪 green Rust 中 byte單元斷言錯把單筆 16 MiB 當成 32 MiB，修正**測試預期**後 Cargo `-p alera-cli --bin alera` Client **11 pass**、完整 Wire Fixture **48 pass**、priority **2 pass**、deferred admission **9 pass**，四組 **70 tests / 0 failed，exit0**，rustfmt check 與 diff check exit0。**嚴格限制的是已接納 localhost request payload，未接納時每 socket 可暫存一條最多 16 MiB 的 String，socket 連線數、mobile/relay、completion/hook、主 Actor `mpsc::unbounded_channel` 均仍未獲全域記憶體限額。**尚需真實狀態／取消／退出延遲 p95/p99、全傳輸來源 quotas、socket 數量和未 admission bytes 可控上限。單一 Runtime 所有輸入全面 bounded 的結論仍不成立。
> **第五批本機 multi-client request admission 已整合 `e369c2232`，test owner follow-up `00cc090c9`**：新 `AdmittedClientLine` 與全域共享 `Semaphore(64)` 僅限制 **localhost request command count**，跨 socket 共享；Actor 完成處理才釋放 permit。每 connection 僅保留一個讀完、尚未 admission 的字串，使用 `tokio::select!` 讓配額耗盡時仍可寫入回覆／PTY 控制訊息。multi-client backlog 新測試由缺 variant/signature 編譯 Red → Cargo Green；client **9 passed**、priority receiver **2 passed**、deferred admission **9 passed**，均 exit0，test module 獨立檔案後 client **9 passed**，`client.rs` **447 行**。**額外 Wire Fixture 初輪 47 pass／1 fail**：釐清 `mobile_hello_matches_shared_fixture` 的**固定 JSON 只記錄 39 個能力**，但來源基準及 Runtime 已提供 51 個；只更新共用 `response.ok.mobile_hello.json` 中遺漏的 12 項能力（commit `262a8d16d`），未修改產品功能，重新直接執行已編譯的 Rust Wire 測試 **48/48 passed、exit0**。原 actor `mpsc::unbounded_channel`、mobile/relay/hook/PTY completion、每 request 16MiB 與多 client 累計 bytes **不受本配額全面限制**，不能誤寫成全域有界；後續仍需真實多 client status/cancel/quit p95/p99、byte-weighted admission、跨 transport 一致性與完整 Rust CI。
> **第四批 ingress 改善已整合 `89ac5306e`**（原隔離 commit `d4dabecc0`）：本機 TcpStream 的 `BufReader.lines()` 原本可對單一無換行輸入持續配置記憶體。第三批 Actor 64 command lookahead 並不限制該行為。本輪本機 request line 新增 16 MiB 硬上限及取消安全的 partial bytes，append 前檢查容量，紅測 4 fail / 1 pass → green 6/6；再次 Cargo 複驗含原 client Socket EOF/causal control order **8/8 passed、exit0**，rustfmt --check exit0。**只限制單一 localhost request line**，不是 actor unbounded channel、mobile gateway 或 relay 的解決方案。仍需真實 multi-client pending/inbox bytes、status/cancel/quit p95/p99、封包接收背壓。

> **第三批 R1 實作 `4be5d7f5a`，已整合為 `dc5067e11`**：`rust/alera-cli/src/terminal_host/server_runner.rs` 新增 64-command bounded lookahead buffer，僅 `RequestedShutdown`／`RequestedRestart` 可以超前，`ClientConnected`／`ClientLine`／PTY／completion 等一般命令維持 FIFO。獨立 Rust 單測 2 Red→3 Green；正式 Cargo async actor tests 2 pass；原 `deferred_admission_tests` 9 pass、exit 0。**沒有把主 `unbounded_channel` 改為 bounded**；真實 status.get/cancel latency 分布、inbox bytes、極端多 client backlog、actor blocked await 仍待補測；不能宣稱已完成整個 control-plane admission。

> **第二批更新**：`b3b0757e4` 新增 bounded queue wait latency histogram (19 buckets / request type, <=256B)，`status.get` 新增 `queueWaitSamples` / `queueWaitP95Ms` / `queueWaitP99Ms`；保留 queueWaitMs 累計與 maxQueueWaitMs。單獨 rustc TDD 2 fail→2 pass，Cargo 新 metrics 3 pass，原 admission/control 9 pass。**主 Actor inbox 仍 unbounded，本輪未改排程**；原驗收規格後續仍要進行 control p95/p99 / backlog / quit tests。

日期：2026-10-29。Code baseline：`3501b0555`，工作分支：`refactor/tdd-boundaries-20261029`。

## 實際已存在的能力（避免重複造輪子）

| Code | 現有機制 | 尚需量測 |
| --- | --- | --- |
| `rust/alera-cli/src/terminal_host/server/deferred_admission.rs` | 8 active、64 capacity、8 dispatch reserve；critical / maintenance / bulk classes；工作在 Actor 外執行 | 高頻/未經 admission 路徑流量、fairness 在極端回填情境 |
| `rust/alera-cli/src/terminal_host/server/runtime_mutation_queue.rs` | 256 mutation 上限、client disconnect 取消 pending mutation、active worker off actor | teardown/quit 相對於 in-flight mutation 的控制延遲 |
| `rust/alera-cli/src/terminal_host/server/deferred_admission_metrics.rs` | active/pending/queued/rejected/disconnected/queueWaitMs/maxQueueWaitMs/staleCompletions | 尚缺按操作的 percentile 分布與全控制路徑實測 |
| `rust/alera-cli/src/terminal_host/server/request_route_policy.rs` | RuntimeMutationPolicy, DeferredRead/Write, CoalescedRead, DeferredJob, ConditionalDeferred routes | 每個可能造成 I/O/CPU 的 request 實際 handler 是否有離開主 actor |
| `rust/alera-cli/src/terminal_host/server_runner.rs` | `mpsc::unbounded_channel<ServerCommand>` + `actor.handle(command).await`，持續擔任唯一狀態權威 | 主 inbox 是否存在可能無界的輸入、哪類 event 造成長 await |

已存在的紅綠測試在 `deferred_admission_tests.rs`，包括 reserve/bulk backpressure typed error、dispatch critical 優先於 queued bulk。不可用「沒有 backpressure」概括現況。

## Rust 下一批最小 TDD（此輪尚未寫入 runtime source）

1. **Red test：控制面隔離。** 用 Barrier 阻塞實際慢請求的 worker；同時送 `status`、`RequestedShutdown`、`cancel`；先斷言終止與觀測事件均可前進，避免測試只能模擬 admission 自己。
2. **Red test：入站容量。** 模擬多 client 連續送入重查詢與 completion，量 `inbox size` / accepted / rejection 是否可預期；不拒絕 critical completion/quit。
3. **Red test：取消與 stale generation。** Client disconnect 後 queued work 不得執行，舊 generation completion 不得覆蓋新 owner。
4. **Green：最小 admission/actor scheduling 修正。** 優先分類/路由，必要時控制命令優先入口，切勿直接把同一 inbox 換成全 bounded，因 `RequestedShutdown` 和 completion 可能互鎖。
5. **Perf baseline：** queue wait、in-flight count、pending bytes、status/quit/cancel p95/p99。先量同硬體冷/熱 baseline 後設定門檻。
6. **回歸：** 現有 Rust `deferred_admission_tests.rs`、`runtime_mutation_queue` tests、terminal reconnect、mobile old-host fixture、workspace sleep/cleanup。

Rust/native 主責仍是 RuntimeHost、DB、PTY、Git/File 重工作；Flutter 只保留 UI/Focus/listenable。不為追求 Rust 比例把輕量列表映射送過 FFI。
