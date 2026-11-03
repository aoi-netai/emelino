# emelino

Emelaの公開APIが生成するFlowから、固定容量の組み込みCを生成します。
本体のfrontend・単相化・汎用core/stdは公式 **v0.26.1** のcommit
`6021115103dd830e0efd9bb96a56a0ca97859329`へ固定して利用します。
本体の改変や兄弟repoのcheckoutは必要ありません。

## 試す

Rust/Cargo、C11 compiler、`sha256sum`が必要です。初回ビルドにはCargo依存の取得が必要です。

```emel
pub fn answer() -> Int { 42 }
```

`examples/answer.json`で公開する関数とC名を選びます。

```json
{"exports": [{"function": "answer", "symbol": "answer"}]}
```

```sh
cargo run --locked -p emelino-cli -- build examples/answer.emel --config examples/answer.json --out-dir out/answer
cc -std=c11 -Wall -Wextra -Werror -pedantic -O0 \
  -I out/answer out/answer/emela_embedded.c examples/host.c -o out/answer/host
./out/answer/host
```

ホストは`answer() == 42`を確認し、成功時に終了コード0を返します。
通常の`pub fn`を設定から指定し、ソースの`@export(c)`との二重指定は行いません。

| 生成物 | 用途 |
| --- | --- |
| `program.flow.txt` | 本体Flowの目視確認用dump |
| `program.backend.json` | 型・関数・演算・公開名を所有するversion付きbackend入力 |
| `program.config.json` | 元ソースの位置を含まない再生成用の容量・adapter設定 |
| `emela_embedded.c` / `emela_embedded.h` | C実装・公開ABI |
| board header・adapter・manifest | C側の接続点と生成物情報 |

保存とC生成を分ける場合は次を使います。

```sh
cargo run --locked -p emelino-cli -- flow examples/answer.emel --config examples/answer.json --out-dir out/flow
cargo run --locked -p emelino-cli -- build --input out/flow/program.backend.json --config out/flow/program.config.json --out-dir out/c
```

ビルド済みCLIの`build --input`は元ソースを読まずに再生成します。
CLI binaryのCargoビルドには固定した本体crateを含む依存が必要です。
このJSONはemelino専用形式です。producer revisionが異なる保存入力は元ソースから再生成してください。
`build --flow`は最小実験用の旧profile読込みを保持します。

## 機能と契約

各機能の入力・意味・拒否条件・実装・試験は[構成・機能契約](docs/architecture.md)を参照してください。

- [接続・保存入力・基本制御](docs/architecture.md#foundation)
- [データ表現・固定heap・所有権](docs/architecture.md#data-memory)
- [数値・String・Unicode・Bytes](docs/architecture.md#numeric-text-bytes)
- [closure・effect・State・failure](docs/architecture.md#functions-effects-failure)
- [async task・Scope・Fiber・Deferred](docs/architecture.md#async-runtime)
- [const・UPSTREAM-001](docs/architecture.md#const)
- [C ABI・ISR・Embedded.*・board hook](docs/architecture.md#c-abi-embedded)

Bytesは本体の`Std.Bytes`からimportします。

汎化constの型特殊化をまたぐ共有には未修正の[UPSTREAM-001](docs/upstream-known-issues.md)があります。

組み込みmoduleはemelinoが管理する`Embedded.*`からimportします。`Std.Embedded.*`は供給しません。

公開入口には具体型のwrapperを用意し、残るeffectをhandlerで処理します。
追加ソースは設定の`sources`へentryからの相対moduleパスを列挙します。
容量・起動入口・adapter・ISR入口も設定で明示し、生成headerをC側のABIの正本とします。

Pome/project loader、`@test`/`@internal`のdriver処理、物理BSP、firmware build/flash、実機検証は後続範囲です。
採用範囲と終了条件は[実装計画](docs/implementation-plan.md)に定義します。

## 検証

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

sourceからの生成と保存入力の再生成を照合し、生成Cを`-O0`/`-O2`で実行します。
既存cacheだけを使う場合はCargoコマンドに`--offline`を追加し、依存の新規取得とは区別します。

現在の検証revision・結果は[対応表](docs/compiler-coverage.md)と[再現検証記録](docs/reports/reconstruction-validation.json)を参照してください。
旧S報告・取得元・ライセンス・過去の実行SHAは[履歴索引](docs/reports/README.md)に分けて保持します。
