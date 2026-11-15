# GPUI U05｜額度彈出視窗：單帳號刷新與 Classic 釘選同步

日期：2026-11-13（Asia/Taipei），整合分支 `integrate/gpui-central-diff-into-main-20261112`。
本輪起始 HEAD `168a4d518`；這份紀錄承接 [U05 Classic 狀態列對齊](gpui-u05-agent-quota-status-classic-parity-20261113.md)。

## 本輪確認的正常流程缺口

- GPUI 原本在 `refresh_agent_quotas(true, Some((provider, account_id)))` 的 CLI／Host 請求失敗時，會把**全部**上次正常的 Provider／帳號快照改為 `stale`。例如只刷新 Claude Work 失敗，也會把 Claude Default、Codex 正常讀數變成過期，導致底部狀態列與彈出明細同時失真。
- 引入 `mark_failed_quota_refresh`：若指定一個帳號，僅該 `provider + accountId` 被標為過期，其餘帳號仍維持原狀；若是全部刷新失敗，才將所有舊的 `ok` 認列為 `stale`。舊讀數只作參考，不假裝是成功取得的新額度。
- 彈出總覽的失敗說明也改為「affected readings may be stale」，避免暗示所有 Provider 都過期。先前的部分成功回覆與完整快照合併邏輯不變。
- `agents.quotaUnpinnedKeys` 是 Classic 與 GPUI 的**同一個 Rust Host 共享設定路徑**，不是第二份 GPUI JSON。新增 `shared_quota_pin_poll_seconds`：Settings 或額度總覽開啟時，每 **3 秒**核對 Host；一般閒置維持 **15 秒**。仍沿用既有版本檢查／Host CAS 寫入，沒有在 UI 製作另一份釘選權威資料。
- 個別帳號刷新、全部刷新、隱藏／釘選仍使用既有 Host RPC 與實際 Provider 認證來源；沒有建立新的 API、偷換帳號、重置認證或啟動未在執行的正式 Host。

## 實際釘選寫入阻塞與 CLI 修正

- 隔離 Host 使用舊編譯版 `rust/target/debug/alera.exe` 執行 `settings --no-start --json set --path agents.quotaUnpinnedKeys --expected [] --value [...] ` 時，實際得到 `Only JSON scalars are accepted`。Host 的 `agents.quotaUnpinnedKeys` 格式是陣列；GPUI `RuntimeCliBridge::desktop_settings_compare_and_set` 又透過這個 CLI 寫入，所以**之前 Native Pin/Unpin 雖然有 UI，實際儲存必然被 CLI 擋下**。
- `rust/alera-core/src/desktop_settings_contract.rs` 新增 `cli_set_value_allowed`：只有 `agents.quotaUnpinnedKeys` 可使用通過 `valid_quota_unpinned_keys` 驗證的陣列；對 CAS 的 `expected` 與 `value` 皆驗證，其他 CLI 可寫路徑仍僅接受 JSON 純量、拒絕物件與陣列，也不放寬原有不可寫設定。
- `rust/alera-cli/src/desktop_settings_commands.rs` 的 `settings set` 改採 Rust Core 共用驗證；並未新增可寫任意 Host 配置的入口。TDD `cli_set_accepts_only_validated_quota_pin_arrays_not_arbitrary_objects_or_lists`：RED（E0425 缺函式）→ GREEN（1/1）。
- Windows `alera-cli` 編譯首次因隔離測試 Host 正持有 `rust/target/debug/alera.exe` 的 EXE lock 而回報 `os error 5`；驗證程序命令列確實指向**本輪隔離 Runtime**後，只停止／清理該測試 Host，再重新 Build，最後 `cargo build --offline --locked -p alera-cli --bin alera` **PASS**。完全未停止正式 Classic Host 或其他 Terminal。

## TDD 與 Windows 編譯結果

- `failing_one_account_refresh_does_not_stale_unrelated_provider_readings`：**RED**（E0425 找不到 `mark_failed_quota_refresh`）→ **GREEN**。測試含 Claude Default／Claude Work／Codex：只刷新 Work 失敗，只有該筆 `stale`；全部刷新失敗，才全部 `stale`。
- `quota_popover_prioritizes_shared_classic_pin_sync_without_fast_idle_polling`：**RED**（E0425 缺少同步間隔 helper）→ **GREEN**，覆蓋 Settings、Popover、一般背景的 3／15 秒間隔。
- GPUI Library：**540 PASS／0 FAIL／1 FILTERED**（既有 Git History 長測未執行）；Native Binary：**33／33 PASS**；一般 Windows EXE build、`cargo fmt --all -- --check`、`git diff --check`：**PASS**。
- 隔離 Runtime：`desktop_gpui/target/.quota-refresh-live-test/isolated-host`，所有控制檔、SQLite、認證 Token 都只建立在被忽略的測試資料夾。新版 CLI 的獨立 `settings get` 初始 `agents.quotaUnpinnedKeys=[]`、`present=false`。第一條 CLI 連線以 CAS 將 `[]` 改成 `["claude:default","agy","codex:work"]` → **PASS**；第二條獨立 CLI 連線讀回相同清單 → **PASS**，再取消 AGY 成為 `["claude:default","codex:work"]` → **PASS**。
- 舊 `expected` 再嘗試刪除所有釘選：Host 回覆 `Shared desktop settings changed elsewhere. Refresh and retry.`，確實拒絕覆蓋。非法 `["unsupported"]` 被新 CLI 拒絕，合法純量 `general.language` 仍能修改與還原。隔離 Host 停止／重啟後再讀回，釘選清單與語言都**持久化正確**；最終僅關閉該隔離 Host（程序守衛確認無殘留）。
- 曾嘗試直接執行已有的 `isolated_host_quota_pin_roundtrip_via_two_authenticated_clients` Rust 雙持續連線測試，第一次指錯 `--lib` Target，修正 `--bin alera` 後又在 Windows 測試 Binary 編譯期限內**timed_out**，沒有取得這支測試的 PASS。以上新版 CLI 驗收是**先後獨立啟動的已驗證 Host 客戶端連線**，不能冒稱同時連線的單元測試或兩個實際桌面 GUI 驗收。

## 本次續驗（提交前）

- GPUI 配額模組 `agent_quota::tests::`：**9/9 PASS**（含單帳號刷新失敗範圍及彈窗釘選同步間隔）。
- Rust Core `cli_set_accepts_only_validated_quota_pin_arrays_not_arbitrary_objects_or_lists`：**1/1 PASS**。
- Rust CLI `runtime_host_client::tests::required_capability_must_be_advertised`：Windows CLI 測試執行檔成功編譯，**1/1 PASS**；屬 CLI 煙霧測試，非額度 RPC 實機測試。
- 全套 GPUI Library 本次續跑進入後段仍持續回報 `ok`，但**超過 180 秒時限，不能列為全套 PASS**。`cargo fmt --all -- --check` 本次也逾時；前述 540 PASS／format PASS 屬之前的另一次執行。`git diff --check` 本次 PASS。

## 範圍與限制

- 不混用合成測試數值與真實 Provider 剩餘額度。既有 U05 合成 Windows 介面驗收已完成五項 Antigravity 顯示、浮動面板及刷新操作；本輪新增的是**單帳號失敗時的正確狀態與共用釘選更新頻率**。
- 目前正式執行中的舊版 Classic Host 曾缺少 `desktopSettings.get` RPC，正式 Classic + GPUI 雙視窗的釘選即時互通，需安全升級 Host 後另行驗收。不得替換正式 Host 或停掉使用中的 Terminal／PTY 來求通過。
- 還未逐一拿真實帳號測全 Provider 的額度刷新、Codex Reset 或所有 Classic 專屬進階動作。**U05 部分完成；全域 `supported=false`，不合併 main／Push／部署／Release。**
