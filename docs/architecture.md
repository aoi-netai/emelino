# Flowを境界にする構成

実装方針・段階・終了条件は [implementation-plan.md](implementation-plan.md) を正本とする。
S0の最小実験から、前回のbackendを再利用する構成へ移行した。

## 処理経路

```text
明示したソース群 + 固定版本体の汎用core/std + emelinoのEmbedded.*
  → emelino-cli/frontend.rs
      本体parse → resolve → check → lower(target=c, Roots::Public)
      → 本体verify_monomorphic → from_core → Flow verifier
  → upstream_adapter.rs
      raw Stage::Flow → 所有するProgram（型・ID・演算・公開名）
  → emelino.backend-input schema 1
      ├ 保存・復元・入力検証
      └ emelino-compiler lower → plan → emit
          → C / header / board hook / manifest
```

本体APIへの接続部はCLI crateが所有する。compiler crateには本体依存、frontend、
ファイル操作、board探索を持たせない。Coreの型・宣言情報を参照するが、関数本体の
再解析・書換え・独自の単相化・const由来追跡は行わない。
closure・handler・suspendの変換は再利用backendで一度だけ行う。

本体の5 crate（syntax/resolve/types/core/lir）は公式v0.26.1のcommit
`6021115103dd830e0efd9bb96a56a0ca97859329` へ固定し、
汎用core/stdは `emela_core::core_sources()` から供給する。兄弟repoへのpath依存はない。
前回の独自frontendや汎用core/stdは移入しない。

組み込み用の `Embedded.Peripheral/Gpio/Uart/I2c/Spi/Board` はemelinoが所有する。
`crates/emelino-cli/lib/embedded` の6ソースを `bundled.rs` が埋め込み、通常のsourceとして
公開APIへ渡す。本体の `Std.Embedded.*` は登録対象から除き、本体側の削除や変更へ依存しない。
組み込みソースにはcore内部権限を与えず、ユーザーによる同名moduleの上書きは拒否する。
取り込み時のソースとhashは記録するが、以後の変更を本体と同期させる契約にはしない。
`Std.Embedded.*` の互換aliasは供給せず、型・effect・Board handlerは `Embedded.*` からimportする。
Bytesも本体の `Std.Bytes` をそのまま使う。`Type::Bytes` と8個のBytes builtinを
所有IRへ直接投影し、旧 `Emelino.Bytes` のソース・external識別処理・型ID再割当ては持たない。
`lib/emelino/bytes.emel` は保持しない。ローカルの `lib` は組み込み用moduleの管理場所である。

`upstream_adapter.rs` は本体Flowと所有IRの境界として残る。v0.26.1の標準Flowには
Causeの失敗伝播とUnit→Any callbackのbackend向け補完が含まれないため、
`upstream_adapter/cause.rs` と `unit_any.rs` は継続する。
本体の別backend `emela-codegen-emb` にある内部処理を、標準Flowの契約とはみなさない。
確認した差分と回帰試験は[依存更新報告](reports/compiler-v0.26.1.md)に記録する。

## 公開入口と設定

`Roots::Public` はentry moduleのmainと対象pub関数をrootにする。
その中から設定の `exports` で関数とC名を選び、公開型・失敗・effect/evidence・
suspend・所有権・ISR到達性はbackendでも検証する。
未選択のpublic関数も本体のexternal target検査には影響する。

通常C targetの `@export(c)` ABIを迂回して継承した扱いにはしない。
ソースのC export属性と設定の併用は診断し、生成headerをABIの正本とする。
`entry` は設定に明示されたexportだけを起動入口として選ぶ。省略時にmainを自動選択しない。
通常の引数付き公開関数も設定できるが、起動入口は引数なしを要求する。

追加ソースは `sources` にentryファイルからの相対moduleパスを列挙する。
user sourceにcore内部権限を付けない。Pome/project discoveryを独自実装しない。
`@test` / `@internal` はdriver固有処理が未接続のため診断する。

## 保存形式

`program.backend.json` は型・enum・effect・関数・external・公開名・error表示を所有する
BackendInput。format、schema、producer revision、stage、必要featureを照合する。
ID参照・型表・body構造を検査してからbackendへ渡し、Floatはbit patternとして保存する。
到達性・位置・layout等の計算済みmetadataは保存入力として信用しない。
producerは上記release commitに固定する。旧依存版の保存入力は元ソースから再生成する。

高レベルCLIの `build --input` でも `Config::validate_entry` を通し、公開引数・戻り値の
未処理Evidenceを拒否する。ソース入力と同じ具体的wrapperの契約を再生成にも適用する。
明示Evidenceを扱う低レベル `BackendInput` / `compile_program` のraw IR ABIとは別の境界である。
`pipeline::saved_raw_evidence_input_cannot_bypass_the_cli_entry_contract` は、raw IR単独では
compileできる入力もCLIの保存再生成では拒否することを確認する。

`program.config.json` は容量・入口・adapter等のOptionsを保存し、元ソース名・公開名指定を
含めない。公開名はBackendInput内で確定する。再生成で元ソースを読むことはない。
`program.flow.txt` は本体のdumpであり、パーサーの入力にはしない。

S0の `emelino.flow.minimal` schema 1とは別形式である。旧形式は `build --flow` でのみ
読める。新形式は `build --input` を使い、旧profileを汎用Flowへ拡張した扱いにしない。

## 再利用と完了判定

旧emela-embのbackend/contract/semantics/runtime・試験資産を固定revisionから採用する。
原則としてそのまま使い、接続差分と新しい入力検証を独立した変更として記録する。
backendの単体試験と本体経由のsource試験は区別し、移入だけで対応表をverifiedにしない。

[本体由来の既知バグ](upstream-known-issues.md)は影響する観測だけを特定し、
修正パッチ後に依存・接続部を調整する。標準Flowにない旧fork拡張は既知バグと混同せず、
追加moduleと接続契約を検討する。未接続項目は対応表に残す。

## ボード境界

組み込み用Emela module、C backend・runtime・生成header・adapterはemelino側で管理する。
本体の `emela-codegen-emb` / `emela-codegen-c` crateには依存せず、本体側での削除に備える。
管理範囲と移管検証は[組み込みライブラリーの所有範囲](reports/embedded-library-ownership.md)に記録する。

ボード名、SDK、toolchainはFlowやcompilerへ持ち込まない。
CLIが選ぶ追加moduleは本体の公開入力APIへ渡し、C側adapterが生成headerに従って
実機関数へ接続する。BSP、startup/vector/linker、firmware build/flash、実機検証は別作業。
