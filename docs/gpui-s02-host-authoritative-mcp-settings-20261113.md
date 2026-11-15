# Alera GPUI S02 — Shared Settings 的 Rust Host 唯一真相與 Native MCP 讀寫分離

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`，起始 HEAD `4a39450c3`。
狀態：**核心 MCP 設定路由補強、TDD／Windows 回歸；真隔離 Host MCP 端到端讀回與雙 GUI 仍待驗。`supported=false`。**

## 發現的真實 split-brain

- 原 `Workbench::apply_native_mcp_setting` 只有在 `NativeSettings::writable(path)` 的舊五個 MCP 可寫欄位才呼叫共享 `RuntimeCliBridge::desktop_settings_snapshot`／CAS。
- **其餘 `get_setting`**，即使路徑實際已歸 Rust Core／Host 所有，仍透過 `NativeSettingsStore::read()` 讀到歷史 `native-settings-v1.json`。例如 `editor.twoRowTabs`、`general.confirmProjectRemoval`、`general.confirmWorkspaceRemoval`、`general.autoArchiveWorkspacesAfterDays`。這些值若已由 Classic 修改或完成一次性遷移，Native MCP 仍可能回傳過期 Native JSON。
- 不能把上述「MCP 不可寫」誤解為「Native JSON 是資料來源」；權限判斷與資料所有權必須分開。

## 本輪修復

1. `NativeSettings::shared_host_owned(path)`：統一依 Rust Core `SHARED_DESKTOP_PATHS`、`extended_portable(path)` 與 quota pins 欄位判定**資料所有權**；包含只讀安全欄位、Two-row tabs、Terminal／Editor 延伸設定及 default Profile 等。
2. `apply_native_mcp_setting`：不再保留一條落回本機 `NativeSettingsStore` 的 `get_setting`／`update_setting` 分支；所有 Native MCP 設定請求統一進入共享 Host Handler，並由既有 Workspace 授權與指定路徑檢查拒絕未知或未授權操作。
3. `apply_shared_mcp_setting`：對 `get_setting`，由 `shared_host_owned` 判定是否能從 Host Snapshot 讀回；對 `update_setting`，**另外**要求 `NativeSettings::writable(path)`（原有的五個明確核准 Scalar）和 expectedValue／value，後端使用 Runtime Host CAS；只讀／extended 路徑不可藉由 MCP Update 打開額外寫權限。
4. 舊 `NativeSettingsStore` 仍保留作為一次性 Native-to-Host 遷移及真正 Native 專屬資料的 legacy 支援，不從 MCP 回覆 Classic 共用設定；設定 GUI 原本就透過 `change_shared_desktop_setting` 使用 Host CAS，沒有另建第二個持久化來源。

## 權限矩陣

| 情況 | MCP get_setting | MCP update_setting |
| --- | --- | --- |
| 五個原本允許的 scalar 欄位（Git Fetch 開關／間隔、Terminal Font Size、Editor Autosave／Tab Size） | Rust Host Snapshot | Runtime Host CAS，原核准寫入規則 |
| Two-row tabs、移除確認、自動封存 | Rust Host Snapshot | 拒絕（MCP 未獲寫權） |
| Terminal／Editor extended portable、額度 Pin、Default Agent Profile | Rust Host Snapshot（需 Host 確認） | 拒絕（不經此 MCP Scalar 入口） |
| 不存在／不屬於 Rust Core 的設定 | 拒絕；不回退舊 Native JSON | 拒絕 |
| 無 Host／未授權 Workspace／Host 快照格式錯誤 | 拒絕，不使用過期 JSON | 拒絕，不寫 Native 本地舊資料 |

## 驗證證據

- **TDD RED**：新增 `s02_mcp_shared_reads_use_host_for_read_only_and_extended_settings`，要求 `shared_host_owned`，原程式 E0599 編譯失敗，確認沒有此所有權判定。
- **GREEN 新測試 1/1**：驗證 `editor.twoRowTabs`、三個安全項、`terminal.fontFamily`、`editor.themeName`、`agents.quotaUnpinnedKeys`、`agents.defaultAgentProfileId`、`editor.quickOpenExcludedDirectories` 等全部是 Host-owned；原五個 writable 仍按原許可；未知與敏感路徑拒絕。
- **原有 Host 快照回歸**：shared snapshot 正確套用 Two-row Tabs、Typed Values／Quota Pin，錯誤資料拒絕部分覆蓋；Native Host 無法連線時不假裝可編輯。
- **Windows GPUI 全量回歸**：Library **570 PASS／0 FAIL／1 ignored**；Native Binary **43/43 PASS**；Windows EXE Build、`cargo fmt --check`、`git diff --check` 全部 **PASS**。隔離 Host 實測未執行、不列 PASS。

## 尚待驗收

- 在隔離 Alera Runtime Host：寫 `editor.twoRowTabs`，由 Native MCP `get_setting` 調用真正 Host Snapshot 並與獨立 CLI／Classic 設定讀回比對，重啟後仍一致。舊 `native-settings-v1.json` 可刻意留相反值，證明不會被 MCP 回傳。
- 驗證 Runtime Host MCP 嚴格授權：`update_setting(editor.twoRowTabs)` 和安全唯讀路徑拒絕；五個原已核准 Scalar 的 CAS 正常寫入，過期 expectedValue 必須拒絕。
- S03 再對同一個隔離 Host 執行 Classic／GPUI 同時開啟、雙方向修改與重啟讀回。此輪**尚未取得真 GUI／CLI 端到端證據**，不誤寫成已完成。

## 安全與後續

- 僅改 GPUI 整合分支的 MCP 路由／Rust Native 分類及測試，沒有重設或刪除舊使用者設定與正式 Runtime Store；原受保護 `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/` 不清理。
- **S02 核心 INTEGRATED／待 Host 端到端驗收 → NOW S03 雙 GUI 設定同步與重啟驗收 → S04 i18n**。保持 `supported=false`；不合 main、不 Push／部署／Release。
