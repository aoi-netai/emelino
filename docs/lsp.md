# emelinoの言語サーバー

`emelino lsp`は標準入出力でLSP通信を行い、`Emelino.toml`プロジェクトを解析する。
Emela本体のソース変更、patch、兄弟checkout、`Pome.toml`は不要。
実行にSDK、Python、ARMツールチェーンは必要なく、ファームウェアのビルドや書き込みは行わない。

## VS Codeでの接続

ビルド後、既存のEmela用VS Code拡張の設定にバイナリの絶対パスを指定する。

```sh
cargo build --locked -p emelino-cli
```

```json
{
  "emela.serverPath": "/absolute/path/to/emelino/target/debug/emelino"
}
```

Emela拡張はこの実行ファイルへ`lsp`引数を渡す。
プロジェクトのフォルダーを開き、`.emel`ファイルの言語モードをEmelaにする。
設定変更後に接続し直す場合は`Emela: Restart Language Server`を実行する。
このコマンドがない旧版の拡張では`Developer: Reload Window`で再読み込みする。
シンタックスハイライトも既存のEmela用定義を使用する。
`#`コメント内の型名や数値まで色が付く場合は、拡張が旧構文の`--`コメントを
認識している可能性がある。現行構文のハイライト定義と、`#`を使うコメント切替設定へ更新する。

エディタから自動起動するコマンドなので、ターミナルで`emelino lsp`を直接実行すると
初期化メッセージを待つ。ログやエラーは標準エラーに出し、標準出力にはLSPメッセージだけを出す。

## 対応機能

| 機能 | 内容 |
| --- | --- |
| プロジェクト認識 | `Emelino.toml`の探索、`src/`以下のモジュール、設定したentry |
| 診断 | 構文・名前解決・型検査のエラーと警告。修正後は以前の診断を消去 |
| 補完 | スコープ内のローカル変数、宣言、import、モジュールの公開名、capabilityの操作、フィールドとメソッド |
| hover | 宣言や式の型、宣言に付いた`##`コメント |
| 定義ジャンプ | ユーザーコードの名前、import、組み込みAPIの宣言 |
| 編集内容 | 未保存の`.emel`、ディスクにまだない新規モジュール、文書のopen/change/save/close |
| 変更通知 | `Emelino.toml`と`.emel`のファイル監視。動的登録対応クライアントに監視を要求 |

編集中のコードにエラーがあっても、取得できた名前解決・型情報から補完などに応答する。
位置はLSPのUTF-16列とコンパイラのUTF-8 byte offsetの間で変換する。
日本語のパスや、カーソルより前に絵文字がある行も扱う。

設定は解析のたびに読み直す。VS CodeのEmela拡張は`.emel`を文書として同期するため、
通常の`Emelino.toml`の変更は保存後のファイル監視通知で反映する。
TOMLの文書内容も同期するクライアントでは、未保存の設定も使用できる。
複数のworkspaceフォルダーと、開いたファイルが属するプロジェクトを扱う。

現時点でrename、フォーマット、signature helpなどは広告・実装していない。
未対応のリクエストには`MethodNotFound`を返す。
core/stdの仮想ソースには定義ジャンプ先を返さない。
ISR制約やC ABI、固定容量などのbackend検査は`emelino build --emit c`で確認する。

## ビルドと共通の解析

`emelino_cli::analysis::analyze_sources`がソース群とentry名を受け取り、以下を返す。

- モジュール表、ソーステキストと構文木
- Emelaの名前解決結果（HIR）と型検査結果
- ファイル・範囲・段階・メッセージ・重大度を持つ構造化診断

構文解析・名前解決・型検査は既存の固定版Emelaの公開APIを使用する。
`Embedded.*`の登録と省略引数の補完も、ビルドとLSPで同じ処理を使う。
解析エラーがあっても部分的な結果を保持し、ビルド側は`require_valid`でエラーを検査してから
既存のCore/Flow lowerとC生成へ進む。言語の型検査をLSPに再実装しない。

エディタのLSP通信・位置変換・応答組み立てはCLI binaryの`src/lsp/`が所有する。
`emela-driver`、`emela-lsp`、本体の各codegen crateへの依存は追加していない。
外部processやfilesystemの操作を`emelino-compiler`へ追加しない。

## 組み込みソースの定義ジャンプ

解析に使う組み込みソースは、実行中のCLIに同梱されたもの。
`target/emelino/lsp/library-<内容ID>/embedded/`へ同じ内容を読み取り専用で展開し、
定義ジャンプ先のfile URIを返す。元のemelinoリポジトリを参照しない。

これらのモジュールにはcore内部権限を与えない。
ライブラリへのエディタ上の変更をビルド入力として採用せず、renameなどで同梱ソースを変更しない。
`target/`のシンボリックリンクによる出力先の転送は、既存のプロジェクトビルドと同様に拒否する。

## 開発時の確認

```sh
cargo test --locked -p emelino-cli --test lsp
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

stdio試験のドライバーにはPython 3を使用する。これは開発時のテスト依存であり、
言語サーバー自身はPythonを起動しない。
試験はSDK設定を除き、PythonやARM compilerの実行先を存在しないパスにして、
実際の`Content-Length`付きJSON-RPC通信、補完・hover・定義ジャンプ、未保存の編集、
診断の消去、設定の再読込、UTF-16位置と特殊文字を含むURIを検査する。

関連: [Issue #23](https://github.com/aoi-netai/emelino/issues/23)。
