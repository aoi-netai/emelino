# Emela v0.27.0への対応

2026-11-10。v0.26.1から公式v0.27.0へ更新し、新構文・Std.Exit・Arrayを接続する。

## 固定する版と保存入力

本体のsyntax・resolve・types・core・lirをすべて
`e2f46917939ea5a3479807e04f89a16d04457868`へ固定し、Cargo.lockを一致させる。
[公式release](https://github.com/emela-lang/emela/releases/tag/v0.27.0)の注釈付きtagは
`8188edca220772117edbdb43dff0d11e25236033`、その参照先commitが上記revisionである。
他のCargo依存は変更しない。compiler crateに本体依存を追加しない。

BackendInputと最小Flowのproducer revisionも同じ値へ更新した。
v0.26.1とそれ以前の保存入力は拒否する。元ソースを新構文へ移行して再生成する。
JSONのrevisionだけを書き換える移行は提供しない。
外側の構造は変えず、producerで型・演算の版を限定できるため、両形式のschemaは1を維持する。
Arrayの要素型参照は他の型と同様に検証し、保存・復元後も同じCを生成する。

## 構文と標準API

| 旧表記 | 新表記 |
| --- | --- |
| 無名関数`fn(x) { ... }` | `|x| { ... }`、引数なしは`|| { ... }` |
| 効果節`use E` / `use { E, R }` | `uses { E }` / `uses { E, R }` |
| `Fiber.attempt`、`Fiber.Exit`、`Fiber.Success/Failure` | `Std.Exit`の各API |

関数型の`fn(A) -> B`と、本文のcapability式`use E`は維持する。
行頭の`||`は無名関数になるため、複数行の論理和は括弧内に書く。
製品frontendは旧構文を受理する前処理を持たず、上流のE0147/E0148を返す。

現在の同梱module、examples、boardシナリオ、Rust内のsource fixtureを更新した。
hash付きの旧corpusと旧externalソースは原文を保持する。
テストhelperだけが上流の`rewrite::old_lambda`と`old_uses_clause`で構文を移行し、
AST/tokenに基づいてStd.Exitのimportと参照を変更する。コメント・文字列・関数型は保持する。
移行helperは製品CLIへ組み込まない。

取り込み元6moduleのv0.26.1原文を`tests/compat/embedded-v0.26.1`に保存し、
従来のhash台帳を検証する。取り込み履歴を現在の標準ライブラリーの内容へ読み替えない。

## Arrayと失敗処理

Arrayの型と7演算を所有IR、型検証、保存、layout、heap、C生成へ追加した。
標準のmap・fold・sort・二分探索・Eq/Ord/Showは本体のEmela実装を利用する。
要素を既存の固定heapへ連続配置し、String・List・関数値・入れ子Array・再帰型の所有と解放を扱う。
循環参照の走査にもArray要素を含める。host mallocへの退避は行わない。

`set`ではFlowの後方生存解析、参照数、同期loopの元のborrowed pointerの一致を調べる。
共有中はコピーし、最終使用で共有がなければ同じ領域を更新する。
呼び出し元で後から読むArrayと、同じArrayを複数引数へ渡す場合には、
呼び出し中の保護参照を追加する。
公開C入口のborrowed引数は更新対象にしない。
保存IRで一つの変数に複数回代入する場合は、別の代入の生存情報を流用せずコピーする。

`get`の範囲外はNone。`set`の範囲外は仕様の文言でdefectを起こす。
最小Intを含むindex、結果の破棄、Exit.attemptの内側でもdefectを維持する。
wildcard escapeは上流のFlow生成を使い、armの順序、通常error、汎化error集合を確認する。

Cause.raise・Unit/Anyの接続処理は維持する。汎化constのUPSTREAM-001も継続し、
既存のC試験で再確認する。旧履歴の成功件数を新版の結果として継承しない。

## 検証

製品・試験コードの検証revisionは`9d0296d2dbc9d78a36f7be53a7dd475f01a602ef`。
[JSON報告](compiler-v0.27.0.json)にpackage tree、コマンド、結果、log hashを記録した。
workspace全試験は**419成功、失敗・ignore・filterは0**。
formatとall-target Clippy（warningsをerror扱い）も成功した。
Arrayの新規8試験はsourceから保存再生成・strict C11のO0/O2実行まで確認する。
新構文の3試験はwildcard escapeのC実行、移行helper、旧構文の診断を確認する。
Arrayは全要素表現、共有、標準helper、再帰型、境界defect、非同期と取消を対象とし、
1000回の最終使用更新を100回繰り返す試験ではコピー回数0とheap回収を確認する。

NUCLEO-F446REの7シナリオ、Nano 33 BLEの4シナリオは、OsビルドとRenodeで確認する。
LSM9DS1はI²C/SPIのfirmwareをビルドし、I²Cの5サンプルをRenodeで照合する。
実機での確認と、新版JS/WASMの実行は今回の対象に含めない。

Array専用の小さなプログラムもArm GCCで両boardのfirmwareへ変換し、Renodeのraw flash resetで
`completed=1`、`failures=0`、`boot_initialized=1`を確認した。
入力source・config・再実行用runnerはJSON報告へ保存した。
