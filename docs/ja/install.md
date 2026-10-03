# Maris をインストールする

0.1.0 は CLI/TUI のビルド済みアーカイブを配布します。配布者署名済みとは表示しません。導入時にはネイティブ実行形式、公開一覧と SHA-256 を検証し、GUI アプリの署名は別の配布条件として扱います。実機のヘッドフォン・Bluetooth と主観的な聴感の確認は未完了です。プロジェクトのライセンス宣言がなければ、その事実を記録し、独自にオープンソースの許諾を追加しません。

インストーラーは、このコンピューター用にコンパイルされたアプリをダウンロードします。ソースコードや Rust、Cargo、Xcode、C++ コンパイラーは不要です。ソースからのビルドは開発者が選ぶ別の操作です。ダウンロードに失敗した場合はインストールを止めます。

> 公開・承認済みの正式版が必要です。現在は開発段階です。未公開の版、通信失敗、候補パッケージでは導入を止め、既存アプリを変更しません。

## オンライン導入 {#online}

macOS と Linux：

```sh
curl -fsSL https://maris.francis.run/install.sh | bash
```

Windows は通常権限の PowerShell を使います。

```powershell
irm https://maris.francis.run/install.ps1 | iex
```

スクリプトを確認し、公式ソースと承認済み配布者のみを信頼してください。OS のスクリプト・署名保護や隔離属性を解除しないでください。コンパイラー、ドライバー、サービス、ログイン項目、モデルは追加しません。

## 版、保存先、ドライラン {#options}

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

例の版は実際の公開タグへ置き換えてください。latest は一度だけ解決し、それ以降は同じ版に固定します。ドライランは通信もファイル変更も行いません。確認フラグを省略すれば適用前の確認を残します。

| OS | アーキテクチャ | 標準の保存先 | 既存の実行ツール |
| --- | --- | --- | --- |
| macOS | x86_64 / ARM64、Rosetta を検出 | `~/.local/lib/maris; ~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256 |
| Linux | x86_64 / ARM64 | `~/.local/lib/maris`、`~/.local/bin/maris` | Bash、curl、tar、gzip、SHA-256、実行ライブラリー、更新確認用 psmisc |
| Windows | x86_64 / ARM64 | `%LOCALAPPDATA%/Programs/Maris` | PowerShell 5.1+ と既存 .NET HTTP/ZIP |

3 つのシステム音声経路は実装済みです。macOS は CoreAudio process tap、Windows は WASAPI process loopback、Linux はローカルの PulseAudio 互換サービス（PipeWire Pulse を含む）を使います。各 OS の CI、クリーン環境、実機、長時間運転は別途検証が必要です。

## 置き換える前の検証 {#verification}

固定したプロジェクト URL、完全な安定版マニフェスト、一致するネイティブ対象のみを受け付けます。候補、重複対象、版違い、途中までの転送、長さやアーカイブ・実行ファイル SHA-256 の不一致を拒否。HTTPS、リダイレクト、時間、サイズに制限があります。

展開するファイルはパッケージのフォルダー内に限ります。フォルダー外に書き込むパス、重複名、大文字・小文字だけが違う名前、リンク、特殊ファイル、異常に多いファイルや大きな内容は受け付けません。アプリの版と実行ファイルの SHA-256 がダウンロード一覧と一致してから、インストール済みのアプリを置き換えます。

CLI/TUI の導入では公開一覧、OS、CPU、版、サイズ、展開パスと SHA-256 を照合し、候補版、GUI/CLI の取り違え、改変を拒否します。GUI には別途 macOS の署名/Gatekeeper と Windows の Authenticode 検証を維持します。チェックサムだけでは配布者の身元や音質を証明できません。

## 更新、復元、初回起動 {#recovery}

同じインストーラーで次の正式版を取得できます。Maris は先に自分で終了してください。音声プロセスを強制終了せず、ロック、同一ファイルシステムの一時領域、`.maris-backup.*` を使います。処理可能な置換失敗時には復元を試みます。設定、補正、他のファイル、PATH は保持します。

前の正式タグを指定するか、完全なオフラインキットでバックアップを `--from` / `-From` に渡して戻せます。使用中のロックを削除しないでください。停電や強制終了後は残った場所を確認してください。

導入時に表示されたコマンドを使い、macOS/Linux では必要なら自分で `~/.local/bin` を PATH に加えます。`maris` は TUI、`maris --help` はコマンド説明です。macOS の音声録音許可は実際の取り込み開始時に起動元ターミナルへ付与します。導入は許可を変更しません。TUI を閉じても音声処理は続くため、停止には `maris stop` を使います。

## CI と正式配布 {#ci}

push 検査と手動ネイティブビルドを分離します。CI は六つの対象と candidate キットを作りますが、通常の導入は候補を拒否します。Actions のテスト用パッケージにはアクセス権限と有効期限があります。一般向けのアプリは、確認済みのパッケージを GitHub Releases に公開します。

CLI の配布一覧は配布方式を明記し、六対象のネイティブ CI 実行、ソースと版の一致を要求します。導入・更新と 200 回の実行はソフトウェアの証拠であり、実機の受け入れ確認とは別です。署名条件は GUI 配布に適用します。

## オフラインと開発モード {#development}

完全なローカルキットには `--from` / `-From` を使います。明示的な `--build` / `-Build` だけが Rust 1.94 とプラットフォームのビルド環境を必要とします。`--allow-unsigned` / `-AllowUnsigned` は信頼したローカル開発用であり、オンライン正式版の検証を無効にしません。オフライン補助スクリプトは通信しません。

## 問題が起きたら {#troubleshooting}

未公開・HTTP エラーでは導入もビルドも行いません。版や対象の誤りには正しい正式タグを使ってください。チェックサム・展開エラーは回避せず配布元を調査。使用中なら Maris とロックを確認します。署名や OS ポリシーの拒否でシステム保護を弱めないでください。

英語の[ビルド・公開手順](../development/releasing.md)を参照してください。
