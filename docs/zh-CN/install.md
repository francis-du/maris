# 安装 Maris

0.2.0 以 CLI/TUI 预编译归档发行，不宣称程序已获得发行者签名。安装器核对原生可执行文件、正式下载清单与 SHA-256；GUI 应用签名属于另一种发行流程。实体耳机、蓝牙与主观听感仍未完成验收。源码未声明项目许可证时，归档如实记录该状态，不自行添加开源授权。

安装器下载适合你电脑的安装包，不需要下载源码或安装 Rust、Cargo、Xcode、C++ 编译器。只有开发者主动选择时才会从源码编译；下载失败会停止安装。

> 在线安装必须有已经通过验收并发布的正式版本。当前仍是开发阶段；版本不存在、网络失败或拿到候选包时，安装会明确停止，不改变现有应用。

## 在线安装 {#online}

macOS 或 Linux：

```sh
curl -fsSL https://maris.francis.run/install.sh | bash
```

Windows：使用普通权限的 PowerShell。

```powershell
irm https://maris.francis.run/install.ps1 | iex
```

运行前检查脚本，只信任项目官方来源与已批准的发行方。不要为了运行下载文件而关闭系统脚本、签名策略或移除隔离标记。脚本不安装编译器、驱动、服务、开机启动项或模型。

## 指定版本、位置或查看安装计划 {#options}

```sh
bash install.sh --dry-run
bash install.sh --version v0.2.0 --yes
bash install.sh --prefix "$HOME/Audio Tools" --yes
```

```powershell
.\install.ps1 -DryRun
.\install.ps1 -Version v0.2.0 -Yes
.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools" -Yes
```

示例版本要换成实际已发布版本。默认只解析一次最新稳定版清单，后续安装包下载固定到该版本，不会在下载途中跟随另一个 latest。在线预演不请求网络、不创建文件；不加 `--yes` 或 `-Yes` 时保留安装确认。

| 系统 | 原生架构 | 默认安装位置 | 已有运行工具 |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64；识别 Rosetta 下的原生 ARM64 | `~/.local/lib/maris; ~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256 |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`；自有启动链接 `~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256、运行库；升级检查需要 psmisc |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ 及已有 .NET HTTP/ZIP 支持 |

三套系统音频路径都已有实现：macOS 使用 CoreAudio process tap，Windows 使用 WASAPI process loopback，Linux 使用本地 PulseAudio 兼容服务（包括 PipeWire Pulse）。各平台的 CI、干净机器、真实设备与长期运行验证仍是独立验收项。

## 替换应用前会检查什么 {#verification}

安装器会检查下载地址、版本、系统和处理器类型，以及安装包和程序文件的 SHA-256 校验值。开发候选包、下载不完整或文件不匹配时不会安装。下载使用 HTTPS，并限制跳转次数、等待时间和文件大小。

解压时不允许文件写到安装包目录之外，也不接受重复文件名、链接、特殊文件或异常大的内容。程序的版本和校验值符合下载清单后，才开始替换安装。

CLI/TUI 安装核对正式下载清单、平台、架构、版本、长度、归档路径与程序 SHA-256，拒绝候选包、GUI/CLI 身份混用与被改动的文件。GUI 套件保留独立的 macOS 签名/Gatekeeper 和 Windows Authenticode 检查。校验值本身不能独立证明发行者身份或音质。

## 升级、回滚与首次启动 {#recovery}

以后重复运行同一在线安装器即可获取新的正式版本。先主动退出旧 Maris，安装器不会杀掉正在处理音频的进程。替换前会锁定安装目录、准备新文件，并把旧版保存在 `.maris-backup.*` 目录；替换失败时会尝试恢复旧版。个人偏好、校正、无关文件与 PATH 保持不变。

回滚可指定先前正式版本，也可在完整的离线安装套件中将打印出的备份目录传给 `--from` / `-From`。不要删除正在使用的安装锁。断电或强制终止后，先检查保留路径和进程再恢复。

使用安装器打印的命令；macOS/Linux 可自行将 `~/.local/bin` 加入 shell PATH。运行 `maris` 打开 TUI，`maris --help` 查看命令。macOS 在真正启动采集后可能要求为启动程序的终端授予音频录制权限，安装不会代为授权。关闭 TUI 不会停止已有音频会话；使用 `maris stop` 停止。

## CI 构建与公开下载分开 {#ci}

Push 检查不发布应用。独立的手动原生 CI 构建生成优化程序、本地包和六种系统/架构的在线安装候选套件；候选仍标为 candidate，正常安装明确拒绝。

GitHub Actions 里的测试包需要仓库访问权限，且会过期。普通用户下载的是正式发布在 GitHub Releases 中的安装包。清单为 `maris-release.tsv`，Unix 包为 `Maris-VERSION-PLATFORM-ARCH.tar.gz`，Windows 为 `.zip`。

CLI 清单明确标记发行类型，六个原生归档须对应同一次 CI、同一源码与版本。安装、升级和 200 轮记录提供软件验证证据，实体设备验收仍单独记录。签名门槛适用于 GUI 发行流程。

## 离线安装与开发编译 {#development}

完整且可信的 CLI 离线套件使用 `bash install.sh --from /absolute/Maris --sha256 TRUSTED_SHA256 --dry-run`；Windows 使用 `./install.ps1 -From /path/to/Maris -Sha256 TRUSTED_SHA256 -DryRun`。将占位值换成正式清单中的真实 SHA-256。开发信任选项仅限显式本地来源。

只有源码开发者明确使用 `--build` / `-Build` 时，才需要 Rust 1.94 与对应平台编译工具。原生离线安装辅助脚本不联网。

## 失败时怎么办 {#troubleshooting}

版本未发布或下载失败：检查发布状态和网络。系统、处理器类型或版本不符：重新选择正确安装包。文件校验或解压失败：停止安装并检查下载来源。安装被占用：退出 Maris 或检查安装锁。签名或脚本检查失败：使用正式安装包，不要关闭系统保护。

维护者流程见英文[构建与发布](../development/releasing.md)。
