# 組み込みライブラリーをemelinoで管理する

2026-11-03。ユーザー指定により、組み込み専用のライブラリーをemelinoへ移管する。
本体側の `emela-codegen-emb` / `emela-codegen-c` 削除予定を踏まえ、組み込み機能の管理場所を明確にする。
本体依存はv0.26.1（`6021115103dd830e0efd9bb96a56a0ca97859329`）を維持する。

## 所有範囲

| 所有者 | 管理するもの |
| --- | --- |
| Emela本体 | 構文・名前解決・型検査・標準単相化・Flow生成、List/String/Std.Bytesなどの汎用core/std |
| emelino | 組み込み用 `Embedded.*`、組み込み向けC backend・runtime、公開ABI・C header・adapter |
| emelinoの後続作業 | BSP、startup/vector/linker、board toolchain、firmware build/flash、実機検証 |

本体の5 crate（syntax/resolve/types/core/lir）だけを同じrelease commitから利用する。
`emela-codegen-emb` / `emela-codegen-c` crateへの依存や新しいCargo依存は追加しない。
既存の `emelino-compiler` は組み込みC実装を引き続き所有する。

## 組み込みソース

```text
crates/emelino-cli/lib/embedded/
  peripheral.emel
  gpio.emel
  uart.emel
  i2c.emel
  spi.emel
  board.emel
```

`bundled.rs` がローカルの6ファイルを埋め込み、通常のEmela sourceとして本体公開APIへ渡す。
型・effect・handlerは `Embedded.Peripheral/Gpio/Uart/I2c/Spi/Board` からimportする。
core内部権限を付けず、ユーザーが同じmoduleを上書きする入力は拒否する。
`Embedded.Local` など関係のないユーザーモジュールは利用できる。

本体の `core_sources()` に含まれる `Std.Embedded.*` は登録対象から除く。
これらが本体から削除されても、ローカル組み込みmoduleの供給は失われない。
`Std.Embedded.*` の互換aliasも供給しないので、既存のimportは `Embedded.*` へ変更する。
List/String/Std.Bytes/Std.Clock/Std.Consoleなど、組み込み専用ではない標準moduleは本体から供給する。

取り込み内容は、v0.26.1の組み込みソースのimport行を `Embedded.*` へ変更したものと一致する。
旧emela-embの6moduleとも同じhashであり、既存のC bridgeと接続できる。
[取り込み元と基準hash](reuse-boundary-fixtures.json)を記録し、Apache-2.0の出自を維持する。
hashは移管時の基準であり、今後の組み込みライブラリー変更を本体と同期させる契約ではない。

## 既存方針との関係

前の[依存更新](compiler-v0.26.1.md)で削除した独自 `Emelino.Bytes` は復活させない。
Bytesは本体の汎用標準であり、`Std.Bytes` のまま利用する。
今回戻す `lib` は組み込み専用ライブラリーの管理場所である。

Flowから所有IRへの投影、Cause・Unit/Any補完、UPSTREAM-001の扱いは変更しない。
frontend本体は変更せず、BSPや物理boardの実装を今回追加したとは扱わない。

## 対応表

428定義・226文脈・115代表試験・10横断要件の集合を維持する。
旧source174観測の166検証済み／8既知問題という分類も維持する。
追加nonruntime観測3件のうち、旧「Std.Embeddedを供給しない」という期待は今回復活する。
したがって過去の `replaced` 判定を履歴に保存し、現行の3件はすべて `verified` とする。
これは未実装項目の除外ではなく、ユーザーが指定した管理範囲・名前空間の変更である。

## 検証

製品・試験コードの検証commitは `7967eb96f13610dd30a29363ee1cfc5f7826564c`。
その後の報告と台帳の更新は製品・試験コードのtreeを変更しない。

| 検証 | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 成功 |
| `cargo test --workspace --locked --offline` | **377成功、失敗・ignore・filterは0** |
| ローカル組み込み6ソース | 取り込み基準hashと一致 |
| 本体組み込みmoduleの供給停止 | 実際のsource経路で `Std.Embedded.*` を除外し、汎用標準は維持 |
| module境界 | `Embedded.*` の供給・上書き拒否、`Std.Embedded.*` のimport拒否、`Embedded.Local` の利用を確認 |
| C bridge・保存入力 | 旧ボードC harness、所有権・取消、保存後の再生成とC O0/O2実行を維持 |

全試験の内訳はCLI 237件、compiler 140件。組み込みsourceの2単体試験とnamespace境界試験を
新しい所有範囲へ合わせて更新し、試験数は変更していない。
先行したCLI単体・board boundary・servicesの25試験も成功した。
本体の組み込みソースを実際の入力から外した状態で全体を検証しているため、
ローカル実装がそれらの供給に依存していないことを確認できる。
台帳確定後に対応表の整合試験2件も再実行して成功した。

旧S6およびv0.26.1更新時点の試験数・commit・隔離checkout結果は過去の記録として保持する。
今回は新たな隔離checkoutでの全試験、本体JSの再実行、実機boardの検証は行っていない。
