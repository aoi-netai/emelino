# Flowを境界にする最小実験

2026-11-03。旧emela-embの独立フロントエンドを新設し直すのではなく、
Emela本体がFlow以前を所有する構成でemelinoを開始する。
この文書は今回の実装と残作業を区別して記録する。

## 今の経路

```text
Emelaソース + 本体のcore/std
  → emelino-cli / frontend.rs
      本体のparse → resolve → type check → lower（標準単相化を含む）
      → 本体のverify_monomorphic → from_core → Flow verifier
  → Stage::Flowの対応範囲を検査
  → program.flow.json
  → emelino-compiler（JSON検証 → C11 / ヘッダー生成）
  → ホストCとリンクして実行
```

本体には調査した固定版で汎用Flowファイル出力CLIがない。
そのため今回はCLI内の小さな接続部が本体の公開Rust APIを呼ぶ。
字句解析器、構文木、型推論、単相化、標準ライブラリーの独自実装は持たない。
本体のC backendや旧emela-emb compilerも取り込まない。

本体APIの呼び出しは `target = "c"`、`Roots::Exported` の通常経路とする。
本体による単相化の後に独自の単相化・由来追跡・Core書換えを追加しない。
本体の `from_core` はCore関数とFlow関数の先頭部分の順序・名前の対応を保証する。
その対応を確認し、Flowが所有するCoreの**宣言情報**からC export aliasを読む。
Core関数本体を調べてCを生成する処理はない。

## ディレクトリと依存

| 場所 | 所有する処理 |
| --- | --- |
| `crates/emelino-cli/src/frontend.rs` | 本体公開APIとの一時的な接続、最小Flow profileの書出し |
| `crates/emelino-cli/src/main.rs` | 引数、ファイル入出力、処理の組立て |
| `crates/emelino-compiler/` | メモリー上の成果物検証とC生成。ボード・ファイル・本体frontendに依存しない |
| `examples/` | Emela入力と、それを呼ぶホストC |
| `docs/` | 境界、対応範囲、検証の記録 |

本体の5 crate（syntax / resolve / types / core / lir）は同一の公式Git revisionへ固定する。
依存バージョンは `Cargo.lock` に保存する。兄弟ディレクトリへのpath依存は置かない。
core/stdはその版の `emela_core::core_sources()` から供給する。
本体を更新するときはCargoのrevisionとFlow profileのproducer条件を合わせて更新し、
Flowの形とC実行結果を再確認する。

## 実験用Flow profile

`format = "emelino.flow.minimal"`、`schema_version = 1`、`stage = "flow"` と、
producer名・固定revisionを照合する。未知フィールド・未知演算・重複exportも拒否する。
関数は本体上の名前、C export名、`Return(Int)` のみを持つ。
引数・文・型表などを省略できるのは、書出し時点でその情報を必要としない最小のFlowだけを
受理するためである。未対応の情報を黙って捨てて汎用Flowとして扱わない。

`program.flow.txt` は実際の本体dumpだが、逆変換には使わない。
JSONは全Flowのcodecでも本体が保証する安定形式でもない。
将来の正式な交換形式にはID参照、型・enum・effect・external・export等の定義表、
feature/version、診断、入力の出自を含める必要がある。
正式な本体CLI/交換形式ができた段階で、この接続部と実験profileを置き換える。

今回の接続部は単一ファイル用で、Pome依存解決や本体driverの全検査を代替しない。
専用のdriver処理が必要な `@test` / `@internal` は、ユーザー入力では未対応として拒否する。
将来のproject入力でも依存解決や許可検査を独自実装せず、本体の入口に委譲する。

## ボードとの接続を追加するとき

生成Cは `main` を持たず、ホスト所有の起動コードからexport関数を呼ぶ。
現在の `Int` 境界は `int32_t` であり、C11のヘッダーだけで利用できる。
ボード名・GPIO番号・SDKのinclude・toolchain指定をFlowやC生成部へ混ぜない。

後続ではCLI側がboard設定を読み、必要なEmela moduleとC bindingを選ぶ。
Emelaソースの追加・置換は、本体が公開する入力APIへ渡す。
external宣言の言語上の検査は本体、受信した宣言のC表現とABI検査はemelinoが所有する。
board側のadapter/BSPが実機関数へ接続し、firmware生成・flashを組み立てる。

現在の通常C targetにはsuspend exportなどの制約がある。
ISR・非同期・固定容量runtimeを拡張するときは、本体の言語検査とbackend固有の制約の
分離を改めて確認する。今の単純なC呼出し成功をISR安全性の保証として扱わない。

## 最初の終了条件

- 本体の固定版で `examples/answer.emel` から実際のFlowを生成できる。
- 保存済みFlowだけからCとヘッダーを生成できる。
- ホスト側のCから `answer()` を呼び、42が返る。
- export名と値を変更しても、その変更が生成Cへ反映される。
- `-O0` / `-O2` の実行結果が一致し、未対応入力は成功扱いしない。

旧forkの全機能移植、全Flow交換形式、board接続はこの終了条件に含めない。
