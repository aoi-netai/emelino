# emelino

Emela本体が生成したFlow IRを入力に、組み込み向けのCを生成する実験です。
最初の到達点は、42を返す関数をCへ変換し、ホスト側のCから呼び出せることです。

```emel
@export(c, "answer")
pub fn answer() -> Int { 42 }
```

構文解析・名前解決・型検査・Core生成・単相化・Flow生成には、本体の公開Rust APIを使います。
本体と標準ライブラリーのソースは複製せず、公式リポジトリの
[`ec1031cb0b98386baf0e9b0ced67e8829bb2c529`](https://github.com/emela-lang/emela/commit/ec1031cb0b98386baf0e9b0ced67e8829bb2c529)
へ依存を固定しています。本体への変更はありません。

## 試す

Rust/CargoとC11コンパイラーが必要です。確認環境はRust 1.99.1とGCC 11.4です。
初回のCargoビルドには依存の取得が必要です。

```sh
cargo run --locked -p emelino-cli -- build examples/answer.emel --out-dir out/answer
cc -std=c11 -Wall -Wextra -Werror -pedantic -O0 \
  -I out/answer out/answer/program.c examples/host.c -o out/answer/host
./out/answer/host
```

ホストプログラムは `answer() == 42` を確認し、成功時に終了コード0を返します。
生成されるCの関数は次の形です。

```c
int32_t answer(void) {
    return INT32_C(42);
}
```

出力先には次の4ファイルを保存します。

| ファイル | 内容 |
| --- | --- |
| `program.flow.txt` | 本体のFlow dump。目視確認用 |
| `program.flow.json` | 対応するFlowの部分集合を保存した実験用ファイル |
| `program.c` | C11の関数定義 |
| `program.h` | C側から呼び出すための宣言 |

Flow生成とC生成を分けることもできます。

```sh
cargo run --locked -p emelino-cli -- flow examples/answer.emel --out-dir out/flow
cargo run --locked -p emelino-cli -- build --flow out/flow/program.flow.json --out-dir out/c
```

`build --flow` は保存済みJSONだけを読み、Emelaソースを再解析しません。
ビルド済みの `target/debug/emelino` は本体のcheckoutなしで実行できます。

## 今回の範囲

単一のEmelaソースと、本体に同梱されたcore/stdを入力にします。
対応するFlowは、引数・文・副作用・failure・suspend・const初期化を持たず、
`Int` の即値を返す、明示的なC export付き関数だけです。
42への固定出力ではなく、Flow中の整数値と本体のexport宣言を使って生成します。
未対応の演算や関数、互換性のないファイルは診断して停止します。

このJSONはemelinoの実験用profileであり、本体の公式Flow交換形式ではありません。
全Flowの保存・復元や、本体CLIの全入力規則への対応はまだ行っていません。
Pomeプロジェクト・追加ユーザーモジュール・`@test`・`@internal` も今回の対象外です。

ボード選択、外部関数のbinding、BSP、firmwareビルド、flashは後続の作業です。
C生成部はボードを知らず、生成したヘッダーを介してホスト側から呼び出す境界を保ちます。
所有範囲と今後の接続方針は [docs/architecture.md](docs/architecture.md) に記録しています。

## 検証

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

テストには、本体によるFlow生成、ソースを削除した後の保存済みFlowからのC生成、
export名・整数値の変化、整数境界値、異常入力の拒否を含みます。
生成Cは `-O0` / `-O2` の両方でコンパイルして実行します。
