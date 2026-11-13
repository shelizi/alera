# GPUI T07 — Provider 啟動錯誤提示（2026-11-13）

工作分支：`feat/gpui-sweflash-t07-provider-ux-20261113`
基線：`f6d09cdd8326e2d6522ffde849805e79e018a382`
狀態：**定向 Rust 驗證完成；真 Provider 未登入/過期狀態的直接辨識未實作；GUI 待驗。**

## 本輪

- SWE-2 Flash High 子代理讀碼盤點啟動，確認 Classic/Host 與 CLI 可用性來源不同，但因未在工作時間內產生程式修改，主控中止該子代理，改做可驗證的窄範圍改動。
- `desktop_gpui/src/local_agent_detection.rs` 新增純 `AgentProfileLaunchFailure` 分類，把 Host disconnected、缺少 CLI/資源、Host permission denied、Profile 已變、輸入不合法及未分類錯誤分開，提供固定的下一步指引。
- `desktop_gpui/src/main.rs` Agent Profile Launch 失敗改用分類後的靜態文案，維持「檢查 Attach existing，Host 可能已建立 Tab」的恢復提醒，不把任意錯誤內容直接呈現於狀態列，避免命令/憑證資料進入 UI。
- 不會因 CLI 已安裝就推定 Provider 已登入；Host `PermissionDenied` 或泛用錯誤也**不能**推定 Token 已過期。真正的未登入/過期判斷需要 Host 提供可驗證、型別化的 Provider 認證狀態，不能單靠任意錯誤字串猜測。
- 保留原 Agent/PTY 所有權、Profile launch 邏輯，不觸碰真實 Provider 帳號或 Runtime。

## 實測

- `cargo test --offline --locked --lib provider_launch_guidance_distinguishes_runtime_missing_permissions_and_unknown_auth`：**1/1 PASS**。
- `cargo check --offline --locked --bin alera-gpui-desktop`：**PASS**。
- `cargo fmt --all` 與 `git diff --check`：**PASS**。
- 使用標準外部 Cargo target 快取，沒有啟動 Host/PTY/Provider；**尚未做 Windows GUI 滑鼠／鍵盤驗收**。

## 尚待

1. 引入真正的 Host provider-auth typed response（logged-out / expired / unavailable），保留隱私及可驗證來源；屆時才可顯示精確的登入提示。
2. 隔離 Windows GUI 驗證「Host 未啟動／CLI 未安裝／一般 launch 失敗」的訊息與 Attach existing 恢復流程；本次受原工作區 GUI Host 啟動安全政策限制，未宣稱 GUI PASS。
3. T07 Managed Model/Persona 尚未完成，E01 仍不得提前整合。整體 `supported=false`，沒有合併 main、Push、部署或 Release。
