# Alera GPUI T07｜Host Agent Profile 建立／重新命名／排序 Windows 驗收

日期：2026-11-13（Asia/Taipei）。整合分支 `integrate/gpui-central-diff-into-main-20261112`，起始 HEAD `feaf52253`。本輪僅完成 **T07 Host Profile 基本管理子項**，**非完整 Classic Profile Editor parity／T07 DONE**。

## 調查與實作

- Classic `lib/src/features/settings/presentation/panes/agent_profiles_pane_profile_actions.dart` 的建立、更新、排序皆透過 Host Agent Profile Provider，更新採 revision compare-and-set；Host CLI 提供 `agent-profile create/update/reorder`、`--expected-revision`、`--no-start`。
- GPUI 原本只讀取／顯示／選擇預設／啟動 Host Profile，沒有管理按鈕。現在 Settings → Agent Profiles 新增 **Create command profile**、**Rename selected** 和逐筆 **↑／↓**；適用全部共用 Rust Agent Descriptor 的類型。建立採預設 `descriptor.default_command` 的 Command 模式。變更僅送到現有 Host，無本地 Profile 複本或自動 Host 啟動。
- Host catalog 解析增加 `revision: Option<i64>`（同時只保留 id、name、agent_type、managed），**不保留 command、managedConfig、customPrompt 或憑證**。沒有版本的舊 Host 仍可唯讀，但不允許不安全的 Rename／Reorder。
- Rename 要求選取的 Host id、先前觀察到的 revision；Reorder 帶完整 Profile ID 順序與**每筆** revision，Host 拒絕過期寫入。建立與重新命名的回覆必須由安全投影確認，而不能因 CLI 回傳成功就宣稱已儲存；排序則逐筆驗證 Host 回讀順序。
- Profile 名稱由 Rust 共用檢核：非空、UTF-8 最多 256 bytes、拒絕控制字元；輸入法仍在組字時拒絕 Save。UI 寫入在背景執行，使用 Request／Workspace generation 過濾過期回覆；成功後重新抓 Host catalog。
- **實機發現的缺口**：Host 新建 Profile 初始 `revision=0`，新增的 GPUI 投影起初誤判必須大於 0，故第一次 GUI 建立成功但畫面回報未確認。獨立 Host 查到 **只有 1 筆**，沒有重複建立；修正驗證為 `revision >= 0`，新增 0 可用、負值拒絕的測試後，以原有 Profile 繼續驗證。

## TDD／Windows 回歸

- `editable_profile_catalog_carries_revision_but_never_commands`：先 RED（E0609、E0425），補安全欄位投影及名稱驗證後 GREEN；加入初始 revision 0 與負值拒絕。
- `profile_create_and_rename_are_explicit_no_start_and_revision_guarded`：先 RED（E0599，缺 CreateCommand／Rename），補有限 CLI Adapter 後 GREEN，驗證不會隱式啟動 Host；原本 `no_host_never_executes_agent_or_creates_runtime_directory` 同時擴充 Create／Rename／非法輸入。
- `profile_reorder_sends_full_snapshot_and_each_expected_revision`：先 RED（E0599，缺 Reorder），補 `--id` 全部列表和 `--expected-revision=id=revision` 後 GREEN。
- GPUI Library **531 PASS、0 FAIL、1 FILTERED**（既有 Git History 長測未執行）；Native Binary **32／32 PASS**；Windows EXE Build、`cargo fmt --all -- --check`、`git diff --check` **PASS**。

## 隔離 Windows GUI／Host 實機驗收

- 隔離 Host：`desktop_gpui/target/smoke-t07-profile-crud-runtime-20261113-0820`；隔離 Native State：`desktop_gpui/target/smoke-t07-profile-crud-state-20261113-0820`。僅註冊既有受保護 fixture `.gpui-center-diff-smoke` 為測試專案，Host 初始 **0 個 Profile**；沒有使用正式 Agent 帳號／憑證或正式 Runtime。
- Windows GUI 首次按 Create：建立 `T07 Native Review`（claude、command），Host 實際回報 `revision=0`。修復 UI 的 revision 驗證後，重開 GUI，用既有同一筆 Profile Rename 為 `T07 Native Review Renamed`；Host 獨立讀回 **revision=1**，原 `command=claude` 保留。
- 從修正後 GUI 建立第二筆 `T07 Native Codex Secondary`（codex、command）並成功讀回；Host 只有 **2 筆**，第二筆 `revision=0`，原預設 `command=codex`。
- GUI 第二筆「↑」將順序從 claude→codex 改為 **codex→claude**；Host 的 `agent-profile --no-start --json list` 獨立確認順序、兩筆 id 與 command 不變。第一筆「↓」再恢復 **claude→codex**；Host 最後讀回仍為 **2 筆**、原指令均保持。
- 特別對 `revision=1` 的 Claude Profile 用舊 `--expected-revision 0` 提交不同名字：CLI 非零失敗，獨立 Host 驗證名稱未變、revision 仍為 1、筆數仍為 2。
- 本輪測試 GUI PID `23216`、`17136`、`29764` 已正常關閉；僅停止這個隔離 Host，最終 `running=false`。測試產物保留在 `target/`，原 fixture 仍為先前既有 `M center-diff-smoke.txt` 與 `?? center-diff-source-switch-untracked.txt`，未修改／清理 WIP。

## 待辦與限制

- 此輪 Create 僅支援**標準 Command Profile + 共用 Descriptor 預設命令**；Rename 只改顯示名稱，不改 agent_type、command、Managed Config、custom prompt 或配額分組。Classic 的完整 Edit／Clone／Managed options、拖曳式排序 UX **尚未對齊**。Up／Down 已完成排序功能本身。
- **Delete** 尚未接上 GPUI；應先由 Host 讀取 Removal Impact、阻擋仍被 Automation／Tab／Policy 引用的 Profile，要求明確確認並使用 revision CAS。不得直接用一鍵刪除替代 Classic 的防護。
- 真實 Provider 未登入／過期憑證的 UI 提示尚未完成獨立實測；Host Profile 被建立／Hook 出現／PTY 存在均不等於 Provider 登入成功。

**NOW 仍 T07，supported=false**。沒有合併 `main`、Push、部署、Release，Classic 與正式 Runtime 的資料未更動。
