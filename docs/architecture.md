# Flowを境界にする構成

実装方針・段階・終了条件は [implementation-plan.md](implementation-plan.md) を正本とする。
S0の最小実験から、前回のbackendを再利用する構成へ移行した。

## 処理経路

```text
明示したソース群 + 固定版本体のcore/std
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

本体の5 crate（syntax/resolve/types/core/lir）は同一の公式revisionへ固定し、
core/stdは `emela_core::core_sources()` から供給する。兄弟repoへのpath依存はない。
前回の独自frontendや同梱core/stdは移入しない。

互換namespaceの `Embedded.*` 6moduleも固定本体の `core_sources()` から取得する。
`bundled.rs` は互換用コピーの `import Std.Embedded.` 行だけを `import Embedded.` へ
機械的に置換し、通常のsourceとして公開APIへ渡す。本体の `Std.Embedded.*` とuser sourceの
本文は変更せず、互換コピーへcore権限を付けない。派生した6moduleのbyte列は旧moduleの
固定SHA256と一致することを試験し、独立した `lib/embedded` の実装は保持しない。
互換モジュールは別の通常moduleなので、型・effect・Board handlerのimportは同じnamespaceで揃える。
固定本体に存在しない `Emelino.Bytes` は専用の通常external拡張として保持する。

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

ボード名、SDK、toolchainはFlowやcompilerへ持ち込まない。
CLIが選ぶ追加moduleは本体の公開入力APIへ渡し、C側adapterが生成headerに従って
実機関数へ接続する。BSP、startup/vector/linker、firmware build/flash、実機検証は別作業。
