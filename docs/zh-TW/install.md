# 安裝 Maris

0.1.0 以 CLI/TUI 預編譯封存檔發行，不宣稱程式已取得發行者簽章。安裝器核對原生執行檔、正式下載清單與 SHA-256；GUI 應用簽章屬於另一種發行流程。實體耳機、藍牙與主觀聽感仍未完成驗收。來源未宣告專案授權時，封存檔如實記錄該狀態，不自行新增開源授權。

安裝程式下載適合電腦的安裝包，不需要下載原始碼或安裝 Rust、Cargo、Xcode、C++ 編譯器。只有開發者主動選擇時才從原始碼編譯；下載失敗會停止安裝。

> 線上安裝需要已通過驗收並發布的正式版本。目前仍在開發；版本不存在、網路失敗或候選套件會使安裝停止，既有應用程式保持不變。

## 線上安裝 {#online}

macOS 與 Linux：

```sh
curl -fsSL https://francis-du.github.io/maris/install.sh | bash
```

Windows 使用一般權限 PowerShell：

```powershell
irm https://francis-du.github.io/maris/install.ps1 | iex
```

執行前檢查腳本，只信任專案來源及已核准的發行者。不要關閉系統腳本、簽章政策或移除隔離標記。安裝器不會安裝編譯器、驅動、服務、登入項目或模型。

## 版本、位置與安裝預覽 {#options}

```sh
bash install.sh --dry-run
bash install.sh --version v0.1.0 --yes
bash install.sh --prefix "$HOME/Audio Tools" --yes
```

```powershell
.\install.ps1 -DryRun
.\install.ps1 -Version v0.1.0 -Yes
.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools" -Yes
```

請以實際發布版本取代範例。預設只解析一次最新穩定清單，後續下載鎖定該標籤，避免 latest 在途中變更。線上預演不連網、不寫檔；省略確認旗標會保留互動式安裝確認。

| 系統 | 架構 | 預設位置 | 現有工具 |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64；辨識 Rosetta | `~/.local/lib/maris; ~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256 |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`；`~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256、執行函式庫；升級檢查需 psmisc |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ 與既有 .NET HTTP/ZIP |

三個系統音訊路徑都已有實作：macOS 使用 CoreAudio process tap、Windows 使用 WASAPI process loopback、Linux 使用本機 PulseAudio 相容服務（包含 PipeWire Pulse）。各平台 CI、乾淨系統、真實裝置與長時間運作仍需獨立驗證。

## 安裝前的驗證 {#verification}

安裝器只接受固定專案位址、完整穩定清單及正確原生目標。候選、重複目標、錯誤版本、截斷資料、長度或套件/執行檔 SHA-256 不符都會被拒絕。傳輸具有 HTTPS、重新導向、逾時與大小限制。

封存內容必須留在指定根目錄。路徑穿越、重複與大小寫衝突、符號連結、特殊檔案、過多檔案及異常解壓大小都會被攔截。安裝包資訊、應用版本與程式檔案的校驗值都符合下載清單後，才開始替換安裝。

CLI/TUI 安裝核對正式下載清單、平台、架構、版本、長度、封存路徑與程式 SHA-256，拒絕候選套件、GUI/CLI 身分混用與變更檔案。GUI 套件保留獨立的 macOS 簽章/Gatekeeper 與 Windows Authenticode 檢查。摘要本身不能獨立證明發行者身分或音質。

## 升級、復原與啟動 {#recovery}

重跑線上安裝器可取得下一個正式版，先自行關閉 Maris。安裝器不終止音訊程序，使用鎖、同檔案系統暫存及 `.maris-backup.*` 備份；替換失敗時會嘗試還原舊版。偏好、校正、其他檔案與 PATH 保留。

可指定先前正式標籤回復，或使用完整離線套件及備份路徑的 `--from` / `-From`。不要移除使用中的鎖；斷電或強制結束後先檢查保留路徑。

使用安裝器印出的命令；macOS/Linux 可自行把 `~/.local/bin` 加入 shell PATH。執行 `maris` 開啟 TUI，`maris --help` 查看命令。macOS 真正啟動擷取後可能要求授予啟動終端音訊錄製權限，安裝不會代為授權。關閉 TUI 不停止既有音訊工作階段；使用 `maris stop` 停止。

## CI 與正式發布 {#ci}

Push 檢查不發布應用。獨立手動 CI 產生六個原生目標、暫時產物和標示 candidate 的安裝套件。Actions 產物是有存取限制且會到期的開發測試渠道；核准後的 GitHub Release 附件才用於公開穩定下載。

CLI 清單明確標示發行類型，六個原生封存檔須對應同一次 CI、同一來源與版本。安裝、升級與 200 輪紀錄提供軟體驗證證據，實體裝置驗收仍另行記錄。簽章門檻適用於 GUI 發行流程。

## 離線與開發選項 {#development}

完整本機套件使用 `--from` / `-From`；開發者明確選 `--build` / `-Build` 才需要 Rust 1.94 及編譯工具。`--allow-unsigned` / `-AllowUnsigned` 只供可信本機開發來源，不能繞過線上正式版驗證。離線輔助腳本不連網。

## 錯誤處理 {#troubleshooting}

未發布/HTTP 錯誤不會觸發安裝或編譯；版本與架構錯誤需使用正確原生標籤。摘要或封存錯誤應停止並檢查來源。占用時關閉 Maris 或檢查鎖。簽章/腳本政策拒絕時取得核准版本，不降低系統防護。

完整驗收見[發布進度](status.md)，工程程序見英文[建置與發布](../development/releasing.md)。
