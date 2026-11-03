# emelino

Emela本体の公開APIで生成したFlow IRを、再利用した組み込み向けbackendへ接続します。
本体のfrontend・単相化・core/stdは複製せず、公式revision
`ec1031cb0b98386baf0e9b0ced67e8829bb2c529` に固定して利用します。本体への変更はありません。

## 試す

Rust/CargoとC11コンパイラーが必要です。確認環境はRust 1.99.1とGCC 11.4です。
初回のCargoビルドには依存の取得が必要です。

```emel
pub fn answer() -> Int { 42 }
```

公開入口を `examples/answer.json` で選びます。

```json
{"exports": [{"function": "answer", "symbol": "answer"}]}
```

```sh
cargo run --locked -p emelino-cli -- build examples/answer.emel --config examples/answer.json --out-dir out/answer
cc -std=c11 -Wall -Wextra -Werror -pedantic -O0 \
  -I out/answer out/answer/emela_embedded.c examples/host.c -o out/answer/host
./out/answer/host
```

ホストは `answer() == 42` を確認し、成功時に終了コード0を返します。
`@export(c)` と設定の二重指定は拒否します。通常の `pub fn` を設定から指定してください。

| 生成物 | 内容 |
| --- | --- |
| `program.flow.txt` | 本体のFlow dump。目視確認用 |
| `program.backend.json` | 型・関数・演算・公開名を含むversion付きbackend入力 |
| `program.config.json` | ソース参照を含まない再生成用の容量・adapter設定 |
| `emela_embedded.c` / `emela_embedded.h` | C実装・公開ABI |
| `emela_board.h` ほか | 必要なhost hook・adapter・生成物manifest |

保存とC生成は分けて実行できます。

```sh
cargo run --locked -p emelino-cli -- flow examples/answer.emel --config examples/answer.json --out-dir out/flow
cargo run --locked -p emelino-cli -- build --input out/flow/program.backend.json --config out/flow/program.config.json --out-dir out/c
```

`build --input` はソースや本体checkoutを読まずに再生成します。
このJSONはemelino専用であり、本体の標準Flow交換形式ではありません。
S0の `emelino.flow.minimal` は `build --flow` による旧成果物の読込みだけを残しています。
新しいソース生成に旧形式は使いません。

## 構成と対応状況

本体の通常型検査・単相化を通し、`Roots::Public` のFlowを受け取ります。
設定で選ばれた公開関数のみC公開名を付け、既存のlower・plan・emitを一度だけ実行します。
複数ファイルは設定の `sources` にentryソースからの相対moduleパスを列挙します。
`heap_bytes`、`string_capacity`、`entry`、`isr_entries`、各adapterも設定から渡します。
設定のunknown fieldや重複公開名は拒否します。

旧forkのBytes拡張は `Emelino.Bytes` からimportします。
旧 `Embedded.*` は、固定版本体の `Std.Embedded.*` から供給する互換モジュールです。
標準ライブラリーのソースは本体へ集約し、emelino側に同じ宣言のコピーを置きません。
公開入口には具体型のwrapperを用意し、残るeffectをhandlerで処理してください。

Pome/project loader、`@test` / `@internal` のdriver処理はまだ提供していません。
物理BSP・firmware build・flashは後続です。生成headerがC側のABIの正本になります。

[実装計画](docs/implementation-plan.md)のS1〜S6は、合意した固定旧fork相当の範囲で完了しました。
[対応表](docs/compiler-coverage.md)の各項目に新経路の証拠を記録し、377試験と別ディレクトリでの再実行を確認しました。
公開入口・容量の条件と結果の内訳は[S6報告](docs/reports/compiler-s6.md)を参照してください。
本体由来の汎化constの共有問題は[既知問題](docs/upstream-known-issues.md)として追跡します。
現在の境界は[構成](docs/architecture.md)、移入元は[再利用記録](docs/reports/reuse-provenance.md)を参照してください。

## 検証

検証にはC11コンパイラーと `sha256sum` が必要です。後者は固定版本体から供給する
互換モジュールが旧ソースと一致することを確認します。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

ソースからのC生成と、ソース削除後の保存済み入力からの再生成を確認します。
生成Cは `-O0` / `-O2` の両方でコンパイル・実行します。
