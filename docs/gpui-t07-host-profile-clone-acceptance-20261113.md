# Alera GPUI T07｜Host Agent Profile Clone 對齊與驗收

日期：2026-11-13（Asia/Taipei）。工作樹 `integrate/gpui-central-diff-into-main-20261112`，起始 HEAD `aba5ffa0d`。

## 為什麼不能直接複製 GPUI 的列表資料

Classic 的 Clone 會複製 Agent Type、Launch Mode、完整 Command、Managed Config、Custom Prompt、Description、Quota Group，並建立新的 Host Profile ID。Native 的列表投影刻意只有 ID／名稱／類型／模式／Revision；若直接用列表建立「Copy」，自訂命令與設定會消失。

## 本輪實作

- Alera CLI `agent-profile clone --profile-id ... --expected-revision ... --name ...`：CLI 從現有 Host 取得完整原始設定，核對來源 Revision，以新的名稱透過 Host 原有的 `agentProfile.upsert` 建立副本。新副本由 Host 分配 ID／revision 0，不改來源或本地 Native JSON。
- 對含降低防護選項的 Managed Profile，必須另由使用者傳 `--confirm-reduced-protections` 才允許 Clone。GPUI 不會自動替使用者確認；此情況應改由 Classic 或 CLI 處理。
- Native Agent Bridge 的 allowlist 僅傳來源 ID、Revision、新名稱。完整 Command、Prompt、Managed JSON、Quota Group 不經 GPUI view state，也不放進 CLI argv。回覆只回傳已存在的安全欄位並驗證新 ID、預期名稱、Agent Type、Managed 模式、revision 0。
- Settings → Agent Profiles 新增「Clone selected」。名稱遵循 Classic 的 `Copy`、`Copy 2`、`Copy 3`，保證 256-byte UTF-8 完整字元及名稱不重複；背景請求採 Workspace generation／Mutation ticket 防止舊結果覆蓋目前畫面。成功後重新抓 Host catalog。
- 此版本使用 CLI 讀取來源再提交新的 Host Upsert，**不是同一個 Host 交易裡的來源 CAS + Clone 原子操作**。來源 Revision 會在 CLI 讀取時檢查；如涉及兩個視窗同時改動來源，仍以 Host 最新資料與使用者重新整理為準。不可將這一版宣稱為嚴格的原子 Clone。

## TDD 與驗收範圍

- Native `clone_name_is_unique_and_respects_utf8_byte_limit`：RED（E0425 缺 helper）→ GREEN。
- Native `clone_cli_request_uses_only_host_identity_revision_and_new_name`：PASS；無 Host 時不可自行啟動，亦不可把 Private Host 配置放進 argv。
- GPUI Windows Library **535 PASS／0 FAIL／1 FILTERED**（既有 Git History 長測未執行）；Native Binary **32／32 PASS**；Windows EXE Build、`cargo fmt --all -- --check`、`git diff --check` PASS。
- CLI 整合測試 `agent_profile_clone_preserves_settings_and_refuses_stale_or_unconfirmed_risk` **PASS（1/1）**：測 Command 自訂欄位全保留、修改來源後以舊 Revision Clone 失敗、Managed 風險 Clone 未確認時被拒、使用 `--confirm-reduced-protections` 後完整配置保留。首次測試因測試框架自動啟動隔離 Runtime Host **逾時 FAILED**；明確以 null stdio 啟動測試 Host 後重跑 **PASS（2.56 秒）**，不可掩蓋首次逾時。
- 也修正既有 `agent_profile_cli_manages_the_complete_catalog` 使用的隔離 Host 測試啟動方式；兩支 CLI conformance 全套重跑 **2／2 PASS、0 FAIL（4.57 秒）**。此為測試框架穩定性修正，不改真實 Host 啟動規則。
- Windows 隔離 CLI 正向驗收：使用新 `rust/target/debug/alera.exe` 和專用 Runtime `desktop_gpui/target/smoke-t07-clone-runtime-20261113-0940`。Command Profile `T07 Clone Source` 以 CLI Clone 得到新 ID、revision 0，`command=cmd.exe /Q`、Custom Prompt、Description、Quota Group 全保留；來源先修改為 revision 1 後，舊 `--expected-revision 0` Clone 被 Host 拒絕。Managed fixture (`approvalPolicy=never`) 在沒有旗標時被拒，加入明確 `--confirm-reduced-protections` 才建立完整 Managed 副本。上述 fixture 都未啟動 Provider。
- Windows GPUI 正向驗收：獨立 Native State `desktop_gpui/target/smoke-t07-clone-gpui-state-20261113-0950`，GUI PID `12156` 以新版 CLI 接隔離 Host。Settings → Agent Profiles 選原 Command Profile 按 `Clone selected`，Host 清單由 **4→5**；新增 `T07 Clone Source Updated Copy`，Command／Prompt／Description／Quota Group／Agent Type／Launch Mode 均與來源一致，新 ID、revision 0。再選原 Profile 按 Clone，名稱自動變為 `T07 Clone Source Updated Copy 2`，Host **5→6**，欄位再次獨立讀回 PASS。
- Windows GPUI Managed 風險負向驗收：選擇 `T07 Risky Managed Source` 按 Clone，顯示需要額外安全確認且未完成複製；Host 清單仍 **5 筆**，沒有偷偷新增 `... Source Copy`。CLI 提供明確安全確認指令，但 Native 不代替使用者確認。
- 收尾：已正常關閉 GUI PID `12156`，隔離 Runtime 停止後再次確認 `running=false`；受保護 `.gpui-center-diff-smoke` 仍是先前的 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`，沒有清理使用者 WIP／`target/`，沒有碰正式 Host、帳號或認證。

## 待完成

- T07 尚有 Classic 全欄位進階編輯、Managed Editor／模型與 Persona 控制，以及真實 Provider 未登入／過期憑證提示；Clone 完成不表示整個 T07 DONE。
- 未合併 `main`、Push、部署或 Release。持續 `supported=false`。
