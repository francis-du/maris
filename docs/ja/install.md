# Maris をインストールする

インストーラーは、このコンピューター用にコンパイルされたアプリをダウンロードします。ソースコードや Rust、Cargo、Xcode、C++ コンパイラーは不要です。ソースからのビルドは開発者が選ぶ別の操作です。ダウンロードに失敗した場合はインストールを止めます。

> 公開・承認済みの正式版が必要です。現在は開発段階です。未公開の版、通信失敗、候補パッケージでは導入を止め、既存アプリを変更しません。

## オンライン導入 {#online}

macOS と Linux：

```sh
curl -fsSL https://francis-du.github.io/maris/install.sh | bash
```

Windows は通常権限の PowerShell を使います。

```powershell
irm https://francis-du.github.io/maris/install.ps1 | iex
```

スクリプトを確認し、公式ソースと承認済み配布者のみを信頼してください。OS のスクリプト・署名保護や隔離属性を解除しないでください。コンパイラー、ドライバー、サービス、ログイン項目、モデルは追加しません。

## 版、保存先、ドライラン {#options}

```sh
bash install.sh --dry-run
bash install.sh --version v1.2.3 --yes
bash install.sh --prefix "$HOME/Audio Tools" --yes
```

```powershell
.\install.ps1 -DryRun
.\install.ps1 -Version v1.2.3 -Yes
.\install.ps1 -Prefix "$env:LOCALAPPDATA\Programs\Audio Tools" -Yes
```

例の版は実際の公開タグへ置き換えてください。latest は一度だけ解決し、それ以降は同じ版に固定します。ドライランは通信もファイル変更も行いません。確認フラグを省略すれば適用前の確認を残します。

| OS | アーキテクチャ | 標準の保存先 | 既存の実行ツール |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64、Rosetta を検出 | `~/Applications/Maris.app` | Bash、curl、tar、gzip、SHA-256、Apple 署名ツール |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`、`~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256、実行ライブラリー、更新確認用 psmisc |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ と既存 .NET HTTP/ZIP |

3 つのシステム音声経路は実装済みです。macOS は CoreAudio process tap、Windows は WASAPI process loopback、Linux はローカルの PulseAudio 互換サービス（PipeWire Pulse を含む）を使います。各 OS の CI、クリーン環境、実機、長時間運転は別途検証が必要です。

## 置き換える前の検証 {#verification}

固定したプロジェクト URL、完全な安定版マニフェスト、一致するネイティブ対象のみを受け付けます。候補、重複対象、版違い、途中までの転送、長さやアーカイブ・実行ファイル SHA-256 の不一致を拒否。HTTPS、リダイレクト、時間、サイズに制限があります。

展開するファイルはパッケージのフォルダー内に限ります。フォルダー外に書き込むパス、重複名、大文字・小文字だけが違う名前、リンク、特殊ファイル、異常に多いファイルや大きな内容は受け付けません。アプリの版と実行ファイルの SHA-256 がダウンロード一覧と一致してから、インストール済みのアプリを置き換えます。

macOS では署名と Gatekeeper、Windows では Authenticode を確認します。Linux では正式なダウンロード一覧の SHA-256 と実行ファイルを照合します。チェックサムはファイルの変更を検出するためのもので、それだけで配布者の身元や音質を証明するものではありません。

## 更新、復元、初回起動 {#recovery}

同じインストーラーで次の正式版を取得できます。Maris は先に自分で終了してください。音声プロセスを強制終了せず、ロック、同一ファイルシステムの一時領域、`.maris-backup.*` を使います。処理可能な置換失敗時には復元を試みます。設定、補正、他のファイル、PATH は保持します。

前の正式タグを指定するか、完全なオフラインキットでバックアップを `--from` / `-From` に渡して戻せます。使用中のロックを削除しないでください。停電や強制終了後は残った場所を確認してください。

導入は音声を起動せず、OS 音量・既定出力も変更しません。macOS は準備後に Maris.app を開いて承認し、他の OS は `--help` と明示デバイス・オフライン機能を確認します。実行中の旧プロセスは自動で差し替わりません。

## CI と正式配布 {#ci}

push 検査と手動ネイティブビルドを分離します。CI は六つの対象と candidate キットを作りますが、通常の導入は候補を拒否します。Actions のテスト用パッケージにはアクセス権限と有効期限があります。一般向けのアプリは、確認済みのパッケージを GitHub Releases に公開します。

マニフェストは `maris-release.tsv`、Unix は `.tar.gz`、Windows は `.zip`。正式キットは署名・受け入れ確認と展開後の再検証を必要とし、六対象の版とソースが一致しなければなりません。200 回のテストだけでは承認されません。

## オフラインと開発モード {#development}

完全なローカルキットには `--from` / `-From` を使います。明示的な `--build` / `-Build` だけが Rust 1.90 とプラットフォームのビルド環境を必要とします。`--allow-unsigned` / `-AllowUnsigned` は信頼したローカル開発用であり、オンライン正式版の検証を無効にしません。オフライン補助スクリプトは通信しません。

## 問題が起きたら {#troubleshooting}

未公開・HTTP エラーでは導入もビルドも行いません。版や対象の誤りには正しい正式タグを使ってください。チェックサム・展開エラーは回避せず配布元を調査。使用中なら Maris とロックを確認します。署名や OS ポリシーの拒否でシステム保護を弱めないでください。

[公開状況](status.md)と英語の[ビルド・公開手順](../development/releasing.md)を参照してください。
