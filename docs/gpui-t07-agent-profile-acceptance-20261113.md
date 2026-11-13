# Alera GPUI T07：Host Agent Profile 冷啟動／Hook 缺席的正常流程驗收

日期：2026-11-13（Asia/Taipei）。起始 HEAD：`34df0459b`；分支：`integrate/gpui-central-diff-into-main-20261112`。**本輪完成 T07 子項，不代表整項 T07／GPUI 發版驗收完成。**

## 可重現的正常流程缺口

1. Host 已註冊專案並存在 Agent Profile，但在 Runtime 尚未註冊該 Workspace 時，Native 直接按「Launch Agent」會顯示 `Current project is not registered with the running Alera Host`。修正前必須先建立一個不必要的 Terminal 才能啟動 Profile。
2. Profile 啟動回覆含 Host `profileId + workspaceId + tabId`，Native 之前仍要求 Session 的 `agentType`（由可選的 Agent Hook 回報）非空才附加 Terminal。Host PTY 實際存在、Provider 正停在登入畫面或 Hook 尚未啟用時，畫面可能一直等候。
3. Host-owned PTY 已建立，**不等同 Provider 登入成功**。需顯示清楚的登入未驗證狀態，而不是把 Host/Hook 狀態誤當帳號認證。

## 修正

- `terminal_bootstrap.rs`：新增明確由使用者 Profile Launch 才能呼叫的 `ensure_host_workspace_for_explicit_profile_launch`，共用原有嚴格專案／canonical checkout 驗證、Host 註冊與回讀；不建立多餘 PTY、Git Worktree 或修改專案檔案。被動輪詢依然 `--no-start`。
- `agent_profiles.rs`：`matching_agent_handle` 接收所選 Profile 的 `agent_type`，僅允許 Host 回覆的精確 Workspace／Tab、`running=true`。優先使用 Hook 相符的 Session；無 Hook 時允許原 Host Tab；Hook 類型衝突或 Session 停止則拒絕附加。
- `main.rs`：Profile 啟動前先註冊精確既有 Workspace，再用已驗證的 Host Session 附加原 Pane。UI 明確顯示「Host Profile PTY attached；Hook not detected；check Terminal for sign-in instructions」，底部狀態標明 **provider sign-in not verified**；即使 Hook 被辨識也不假稱 Provider 已登入。

## TDD 與 Windows Rust 回歸

- 新增 `profile_launch_attaches_authenticated_host_tab_without_optional_hook`：修正前因 matcher 缺 `expected_agent_type` 出現 E0061（RED），修正後 GREEN；另涵蓋錯誤 Hook、已停止 Session 與錯誤 Tab 身分。
- 新增 `explicit_profile_launch_rejects_a_missing_checkout_without_host_start`：修正前 E0599（RED），修正後 GREEN，證實不存在的 checkout 不可藉 Profile Launch 啟動／註冊 Host。
- `cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit`：**526 PASS、0 FAIL、1 FILTERED**。既有 Git History 長測未執行，不能當成 527/527。
- Native Binary `cargo test --offline --locked --bin alera-gpui-desktop`：**32/32 PASS**；Windows EXE `cargo build --offline --locked --bin alera-gpui-desktop` PASS；`cargo fmt --all -- --check`、`git diff --check` PASS。

## Windows 隔離 GUI／Host 正向操作

- 僅建立 `desktop_gpui/target/smoke-t07-profile-runtime-20261113-0630` 與對應 `...state-20261113-0630`，登錄既有受保護 Git fixture `.gpui-center-diff-smoke`。隔離 Host 新建一份 **codex 類型、command=`cmd.exe` 的測試 Profile**，不是正式 Codex，沒有使用或修改任何 Provider 登入資訊或憑證。
- 原版 GUI（PID 6908）先在 Host 已有專案/Profile、無 Workspace 的狀態點 Launch：收到「Current project is not registered」；Host `terminal list` 回傳 **0 個 Session**。這是修正前的 Windows 正常流程 RED，不是推測。
- 新編譯 GUI（PID 31128）使用同一隔離 Host 及 Profile 重新點 Launch：直接建立／確認既有 fixture 的 Host Workspace，GUI 附加真正 `cmd.exe` Terminal。Host `terminal list` 僅有 **1 個 running Session**，handle/tabId `f84b898b-cb02-4008-be87-87ceba120dd4`，`agentType=null`（預期 Hook 未啟用），**沒有額外 dummy PTY**。
- 左側顯示「Host Agent Profile PTY attached; Hook not detected. Check Terminal for sign-in instructions.」，工作台底部顯示「Agent Profile terminal ready; provider sign-in not verified」，沒有聲稱已登入。
- Windows 注音一開始是中文模式，英文字會變成候選字；以 Shift 切換英數模式後，從 **GPUI Terminal** 執行 `echo T07_PROFILE_INTERACTIVE_OK`，畫面回傳 `T07_PROFILE_INTERACTIVE_OK`，Host `terminal --no-start read` 獨立讀到相同回覆，證實可互動。測後 Shift 恢復原先中文輸入模式。
- 正常關閉兩個隔離 GUI PID；僅停止此輪測試的隔離 Host（首次 stop 回覆後 Host 尚未完全退出，針對同一 Runtime 再確認 stop，最終 `running=false`）。受保護 Git fixture 保持原先 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`；不清理 `target/` 或使用者 WIP。

## T07 尚未完成

- 真實 Provider 的**未登入／過期憑證**、登入錯誤畫面是否可辨識：**尚未實測**。本輪只保證登入提示可見、未驗證時不假報成功；不能將 `cmd.exe` 測試當作真實 Codex 登入驗收。
- **同日後續補齊部分管理 UI**：Native 已可建立標準 Command Profile、revision CAS Rename、↑↓ 排序，並通過隔離 Windows GUI／Host 實測；[管理子項詳錄](gpui-t07-host-profile-management-acceptance-20261113.md)。完整 Managed／進階 Edit、Clone、Delete 含 Removal Impact 仍未完成。
- **2026-11-13 後續完成**：執行檔 Override 的無效路徑、合法 `.cmd` Save、空白 Save 與明確 Clear 已完成 Windows GUI／TDD 驗收；詳見 [T07 Override 後續紀錄](gpui-t07-agent-executable-override-acceptance-20261113.md)。CLI 缺失與實際 Provider 登入仍各依對應驗收追蹤。

**NOW 仍為 T07**；`supported=false`，未合併 `main`、Push、部署或 Release。
