# Alera / AGY 真正獨立 Windows 使用者多帳戶：本機 SSH 路由

> 分支：`feat/agy-multi-account-20261111`。此功能需要新版 Alera Rust Host、Windows OpenSSH Server，與**另一個標準（非管理員）Windows 使用者**。不複製、匯出或覆寫 Google/AGY 憑證，也不讓 SSHD 對外網監聽。

## 1. 架構及安全界線

主 Windows 帳戶執行 Classic／GPUI 及主 Runtime Host。第二個 Windows 帳戶使用自己的 Credential Manager 中 `gemini:antigravity` 認證，獨立執行 `alera runtime start`、AGY CLI。主 Host 只透過 **127.0.0.1 上的 OpenSSH** 使用「預先認可的 SSH 主機公鑰」及「單獨使用者的 SSH 公鑰登入」。不新增 Alera 高權限共享 Token 或任意遠端指令 API。

路由前執行 `ssh -T <OS-user>@127.0.0.1 'alera runtime --json os-identity'`：遠端 Runtime CLI 從實際 Windows token 取得 SID 的雜湊指紋。配對當下要求遠端與主 Host 的 SID 指紋不同，存入與 SSH Target ID 綁定的指紋。**每次額度查詢、每次新 AGY Terminal 啟動均重新驗證遠端使用者身分**；若指紋變更、SSH 金鑰驗證失敗或遠端 Host 離線，拒絕執行，不會改用預設帳戶。這證明不同 OS 使用者，並不證明 Google 帳戶電子郵件不同。

額度：SSH 到第二使用者執行 `alera quota --no-start --json snapshot --force-refresh --provider agy --account-id default`，主 Host 只把遠端 `agy:default` 結果映射到註冊的本機帳戶 ID。Terminal：主 Host 先驗證遠端，PTY 以 `ssh -tt` 開啟遠端使用者的互動 Shell，再送入預設 `agy` 指令；不用本機預設帳戶啟動 AGY。SSH 故障時保留已保存的 AGY Tab／Transcript。

## 2. Windows 的最小化先決條件

1. 由管理員建立一個**標準使用者**（例如 `AleraAGY2`，不要加入 Administrators），讓它登入一次 Windows，並在該使用者的終端安裝／登入 `agy` 與新版 `alera.exe`。該使用者必須能在自己的 Windows 工作階段執行 `alera runtime start`、`alera quota --no-start --json snapshot --provider agy --account-id default`。**切勿共用或複製原使用者的 Credential Manager**。
2. 目前偵測到 `ssh.exe` 已存在，但 `sshd` **尚未安裝**。經管理員審核後安裝 Windows Optional Feature `OpenSSH.Server~~~~0.0.1.0`，編輯 `C:\ProgramData\ssh\sshd_config`：
   - `ListenAddress 127.0.0.1`（不要監聽 `0.0.0.0` 或公網介面）
   - `PubkeyAuthentication yes`、`PasswordAuthentication no`、`AllowUsers aleraagy2`（實際帳號依照 OS 設定，Windows OpenSSH 會正規化帳號）
   - 必要時建立 loopback-only 防火牆規則。若原本已部署 SSHD，不應直接覆蓋它的既有設定或強制重啟；請另行規劃。
3. 在主帳戶產生專用 Ed25519 公鑰，**只把公鑰**授權給第二使用者的 `authorized_keys`。不要將私鑰送給第二使用者，也不要把第二使用者密碼存入 Alera。對應的主帳戶 OpenSSH 設定可為 `Host 127.0.0.1` 指定專用 `IdentityFile`。使用交互式 SSH 首次核對並信任 localhost 的實際 SSHD 主機金鑰；不要透過 `StrictHostKeyChecking=no` 繞過首次驗證。
4. 主帳戶確認 `ssh -T -o BatchMode=yes -o StrictHostKeyChecking=yes AleraAGY2@127.0.0.1 'alera runtime --json os-identity'` 可回 JSON，且其 `subjectHash` **不同於**主帳戶 `alera runtime --json os-identity`。注意預設 Windows OpenSSH 的 Shell／PATH 設定可能不同；要確認第二使用者可直接執行 `alera` 與 `agy`。

## 3. 使用已存在的 Alera CLI 配對及啟用

先在主帳戶確保新版 Runtime Host 已啟動；以下指令例子的 Revision 必須以實際 `agy-accounts get` 回傳值為準，不能盲填 0/1：

```powershell
alera ssh-target add --id agy2-ssh --alias 'AGY 2 localhost' --host 127.0.0.1 --port 22 --username AleraAGY2 --auth key
alera runtime --json agy-accounts get
alera runtime --json agy-accounts upsert --account-id agy2 --alias 'Google Account 2' --expected-revision <上一版Revision>
alera runtime --json agy-accounts get
alera runtime --json agy-accounts bind --account-id agy2 --ssh-target-id agy2-ssh --expected-revision <最新版Revision>
alera quota --no-start --json snapshot --force-refresh --provider agy --account-id agy2
```

配對時 Alera 會先以目前使用者的 SSH 金鑰連到第二個 Windows 帳戶、核對 OS SID 指紋不相同，再將 `sshTargetId` 及 `hostIdentityHash` 存入 Host 版本化設定。若需要取消綁定，使用 `alera runtime --json agy-accounts unbind --account-id agy2 --expected-revision <最新版Revision>`，不會刪除第二使用者的 Credential Manager、SSH 金鑰或歷史檔案。

Classic Settings → Google / Antigravity Accounts 選擇 `Google Account 2` 後點 New Agent · Antigravity：只有配對資訊完整且 Host 宣告 `agySshRoutingVersion=1` 時才允許新建 Tab。新 Tab 會固定原本的帳戶 ID，切換下拉選項不會改變現有 Tab 的登入路徑。Agent Profile 的 Prompt Workspace 同樣要求新版 Host 版本及配對後才轉送請求。

## 4. 目前驗收界線

已實作的是**可用 SSH/獨立 Windows 使用者的執行路由**，實際端到端測試須具備：第二個 OS 使用者、已信任 OpenSSH host key、適當權限的 key-only SSH 存取、該使用者已登入 AGY、其 Host 可提供額度。此測試環境尚未裝 SSHD，因此不得將「編譯通過」宣稱為「雙帳戶登入已通過」。

不支援且不應承諾：跨帳戶原生 AGY Resume、讀取另一使用者的 Credential Manager、以同一 Windows 使用者不同 HOME 模擬第二個登入。
