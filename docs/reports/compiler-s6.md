# S6: 対応表照合と全体検証

2026-11-03。固定旧fork相当のcompilerを、本体の公開APIからC実行まで接続した。
合意した範囲のS1〜S6は完了。本体の既知問題と、以下の容量・入口条件は維持する。

本書は旧pin `ec1031c` での移植完了時点の記録である。
現在の依存はv0.26.1へ更新し、独自Bytesを廃止した。[更新と再検証](compiler-v0.26.1.md)を参照。

## 完了範囲

前回のbackend・runtime・source/C harness・raw IR試験を再利用した。
本体のfrontend・単相化・core/stdは固定revisionへ委譲し、本体を変更していない。
標準モジュールのコピーも保持せず、`Embedded.*` 6moduleは固定本体から
互換import名だけを変えて供給する。未提供のBytes拡張は `Emelino.Bytes` として所有する。

接続時に見つかったenum比較、失敗値の型変換、Scope callback、Unit/Anyの接続、
不要なFiber結果の解放を修正した。公開入口のeffectと保存入力の検証、
表記違いのソースパスによる上書き拒否も確認した。
公開入口には必要なhandlerを供給する具体型wrapperを使う。

## 対応表の照合

| 対象 | 今回の判定 |
| --- | --- |
| 固定定義・演算・宣言の428行 | すべて接続先・処理方針・検証証拠へ対応 |
| 文脈別の226行 | すべて確認済み |
| 対応表の代表試験115件 | 114件確認済み、1件はUPSTREAM-001 |
| 旧source観測174件 | 166件確認済み、8件はUPSTREAM-001の影響を個別記録 |
| 追加の非runtime観測3件 | 2件確認済み、1件は本体の標準module供給へ置換 |

[対応表](../compiler-coverage.md)と[旧source観測の索引](reuse-source-coverage.json)を正本とする。
428行には型定義やmetadataの照合も含まれ、428個のソース実行試験を意味しない。
通常source、明示IR入力、試験専用IR加工、artifactだけの観測を区別した。
固定169programの生成確認も、C runtimeの証拠とは別に記録している。

本体は `Std.Embedded.*` を供給するため、旧独自frontendの「そのnamespaceを供給しない」
判定は引き継がない。互換 `Embedded.*` をユーザーソースで上書きすることは引き続き拒否する。
これは標準ライブラリーの所有元の変更であり、未実装機能や本体の既知バグではない。

## 実行結果

実装・試験の検証revisionは `70dfbe6b3da306dfe62ed26fad1ba798890c71a9`。
以後の対応表・最終報告の更新は製品・試験コードのtreeを変更しない。
[実行記録](compiler-s6-validation.json)にcode tree、コマンド、件数を記録した。

- `cargo fmt --all -- --check`: 成功。
- workspace全targetのstrict clippy: 成功。
- workspace試験: **377件成功、失敗・ignore・filterは0件**。
- コミットを新しいディレクトリへ展開し、空のtargetから同じ377件を再実行して成功。
- CLIでsourceから保存入力を生成し、元sourceと元設定を削除後、保存入力だけでCを再生成。
  厳格なC11オプションのO0/O2でリンク・実行して42を確認。

377件の内訳はCLI側237件、compiler側140件。
compiler側には再利用backend127件、保存入力7件、最小profile6件を含む。
前回の607件を今回の成功数として引き継いだものではない。
隔離した展開先でも依存キャッシュは再利用した。新規ネットワーク取得や実機動作の検証ではない。

## 明示した条件と差分

[UPSTREAM-001](../upstream-known-issues.md)による汎化constの初期化cache分離は残る。
受信したFlowの単位で実装し、由来の推測や独自の追加単相化で補正しない。
既知問題を再現する回帰テストの成功は、単一初期化仕様への対応済みを意味しない。

補足試験の容量は次の差分を明記する。旧容量での成功として扱わない。

| 試験 | 旧設定 → 今回 | 理由 |
| --- | --- | --- |
| TestClock/Listの中断処理 | frame 1024 → 2048 byte | 本体の詳細なassert診断を含むraw Flowで、host上の最大frameは1624 byte。内訳は[測定記録](reuse-source-clock.json) |
| 評価順序の外部観測 | heap 0 → 4096 byte | 副作用の観測を本体仕様どおりeffect/handler経由へ接続。[記録](reuse-source-semantics.json) |

生成フレームのbyte単位の同一性は保証しない。指定したheap/poolの境界、
所有権、取消・失敗時の解放、ISR制約はbackendが検証する。
C側のstack測定や定数評価回数の計測は試験専用instrumentationとして記録した。

Pome/project loading、`@test` / `@internal` のdriver処理、物理BSP、startup/vector/linker、
firmware build/flash、実機確認は後続作業。今回の成果物は生成C・公開header・adapter契約までである。

## 作業履歴

`feat/compiler-01-flow-bridge` から `feat/compiler-06-coverage` まで、各段階のローカルブランチを保持する。
再利用、接続差分の修正、試験移行、対応表の確定を日本語の機能別コミットへ分けた。
この実装作業ではpush・PR作成は行っていない。
