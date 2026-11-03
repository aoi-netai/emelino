# emelinoの構成と機能契約

emelinoはEmelaの公開frontend APIが生成するFlowを受け取り、固定容量の組み込みCを生成する。
この文書は現在の入力、意味、所有権、資源、C境界の正本である。
採用範囲と終了条件は[実装計画](implementation-plan.md)に記す。
実装pathは動作の担当箇所、試験pathはその契約を観測する入口であり、試験の実行結果とは区別する。

<a id="foundation"></a>
## 接続基盤・保存入力・基本制御（PR1）

```text
明示したEmelaソース群 + 固定版本体のcore/std
  → CLI frontend: parse → resolve → check → lower(target=c, Roots::Public)
  → verify_monomorphic → from_core → Flow verifier
  → CLI upstream_adapter: Stage::Flow → 所有するProgram
  → BackendInput schema 1: 保存・復元・同一の入力検証
  → compiler: lower → plan → emit
  → C実装・公開header・board hook・生成物manifest
```

本体のsyntax/resolve/types/core/lirはすべて公式v0.26.1のcommit
`6021115103dd830e0efd9bb96a56a0ca97859329`に固定する。
名前解決、型検査、標準単相化、汎用core/stdは本体の責任とし、本体へのpatch、
独自frontend、兄弟checkoutへのpath依存を設けない。
本体crateに依存する処理はCLIに閉じ込め、compilerはメモリー上のProgramとOptionsから生成物を返す。
compilerにソース探索、ファイルI/O、board探索、外部process実行を持たせない。

raw Flowを所有IRへ投影した後にclosure・handler・suspendのlowerを一度だけ行う。
Coreの型・宣言表は参照するが、Core関数本体の再解析・書換え・独自再単相化を行わない。
基本制御では引数、直接call、整数演算、if、block、loop、switch、returnを扱い、
分岐とcallの型・ID参照を検査する。不要コード除去や定数伝播はC compilerへ任せ、
`-O0`でも意味が成立するCを生成する。

### ソースと公開入口

`Roots::Public`でentry moduleのmainと対象pub関数をFlowへ含め、
設定の`exports: [{function, symbol}]`でC公開名を選択する。
通常の`pub fn`を使い、ソースの`@export(c)`との二重指定は診断する。
未選択のpublic関数やmainも本体の到達external検査には影響する。
入口には具体型のwrapperを用意し、残るeffectをhandlerで処理する。
trait bound付きgenericのroot除外やAnyへの具体化を、任意のgeneric公開が可能という意味にしない。

追加ファイルは設定の`sources`へentryソースからの相対moduleパスを列挙する。
user sourceにcore内部権限は与えない。設定のunknown field、重複公開名、不正なC名を拒否する。
`entry`は明示したexportだけを起動入口に選び、引数なしを要求する。
CLIで省略した場合にmainを自動選択しない。
Pome/project discoveryと`@test`/`@internal`のdriver処理は提供していない。

### 保存・復元の契約

| 成果物 | 保持する情報と用途 |
| --- | --- |
| `program.backend.json` | 型・enum・effect・関数・external・公開名・error表示を所有する`emelino.backend-input` schema 1 |
| `program.config.json` | 容量・入口・adapter設定。元ソースの位置や公開名指定は含めない |
| `program.flow.txt` | 本体Flowの目視確認用dump。入力parserは設けない |
| `emela_embedded.c` / `.h` | 実装とC公開ABI。C利用者は生成headerに従う |

ビルド済みCLIの`build --input`はソースと本体checkoutを読まずに再生成する。
CLI binary自体のCargoビルドには固定した本体crateを含む依存が必要であり、依存不要という意味ではない。
これはemelino専用の保存形式であり、本体の汎用Flow交換形式ではない。
format/schema、producer revision、stage、必要feature、型表・ID・body構造を検査する。
保存schemaの型・演算を受理して参照整合性を検証できることと、C生成へ接続済みであることは別である。
PR1のC生成範囲はscalar値・直接call・基本制御であり、各機能PRで対応を追加する。
その段階で未導入の機能はC生成時に`Unsupported`として診断し、保存形式に表現できることを成功根拠にしない。
Floatはbit patternを保存し、計算済みの到達性・layout・位置metadataを入力として信用しない。
旧producer revisionの入力は拒否し、元ソースから作り直す。JSONのrevisionだけの書換えは移行手段にしない。
最小実験用の`emelino.flow.minimal` schema 1は`build --flow`でのみ読み込む。
新しいソース生成と`build --input`はBackendInputを使う。

関数型の`FnType.sets.errors/effects`はIDが範囲内であるだけでなく参照先が`Type::Set`か検査する。
不正な集合を型置換へ渡してpanicさせず、通常の入力診断を返す。
宣言集合の`SetSummary.params`内の`Type::Param`は許可する。
flagsは未解決集合の保守的近似なので集合との単純一致は要求しない。

実装は`crates/emelino-cli/src/{frontend.rs,upstream_adapter.rs}`と
`crates/emelino-compiler/src/{contract,backend}`が担当する。
`crates/emelino-cli/tests/pipeline.rs`はsource削除後の再生成、引数・call・if、複数module、
不正入力時の出力不生成を観測する。
`crates/emelino-compiler/tests/backend_input.rs`は保存前後に共通する検証と型集合を扱う。
`pipeline::invalid_saved_function_sets_are_diagnosed_without_outputs`は不正集合を通常診断し、
panicや不完全な生成物を残さないことを確認する。

<a id="data-memory"></a>
## データ表現・固定heap・所有権（PR2）

tuple、record、enum、List、Any、再帰型を型表からC表現へ割り当てる。
record更新はfieldの評価順を保ち、旧値を必要な間保持してから不要な所有値を解放する。
matchはtagとpayloadの型に従い、再帰型は有限の宣言代入として解決する。
型の`Param`が宣言の具象代入に現れることは許すが、未解決のruntime slotは受理しない。
Unitの格納省略は`Repr::Void`で表し、任意のVoid変数をraw Flowへ認める意味ではない。

所有値は生成module専用の固定heapで管理し、`heap_bytes=0`ではheap-backed値を無効にする。
必要なheapや固定poolが不足した場合は定義されたresource defectとしてboard停止hookへ渡す。
ホストの`malloc`へ暗黙に退避したり、容量超過を値の切り詰めで隠したりしない。
静的な値・borrowed viewと所有値を区別し、コピー・返却・capture・payloadへの格納では
生存期間に必要な所有を取得する。最後の参照をdropすると入れ子の管理fieldも解放する。
deepな再帰値の解放と、Stringを含むList・error payloadが元ownerより長く生存する場合も対象とする。

所有権の正しさは値だけでは判定できないため、同じ操作を反復し、元ownerの寿命終了後の値と
heapの回収量をC harnessで確認する。試験が生成Cに追加するheap観測関数は内部計測用であり、
製品の公開headerへ追加しない。cleanup、取消、非同期境界における所有権は該当機能の節でも確認する。

実装は`contract/ir.rs`、`semantics/{types.rs,representations.rs,derived.rs}`、
`backend/plan/heap.rs`、`backend/emit/heap.rs`と値・dropのlower/emitが担当する。
pathは`crates/emelino-compiler/src/`からの相対指定である。
`crates/emelino-cli/tests/legacy_ownership.rs`は入れ子の所有値、borrowed subview、
型を変える有限再帰とdeep releaseを観測する。
`reused_pipeline::{recursive_types_and_deep_release,record_updates_order_and_ownership}`と
`legacy_collections_helpers.rs`は再帰・更新順・collectionの構造を確認する。

<a id="numeric-text-bytes"></a>
## 数値・String・Unicode・Bytes（PR3）

Int、Int64、Floatの演算・比較・parse・表示は型ごとの表現を保つ。
Intは符号付き32 bit、Int64は符号付き64 bitとし、加減乗算とnegateはその幅でwrapする。
Cの符号付きoverflowや不定な`long`幅へ意味を依存させない。
整数の除算は0方向へ丸め、剰余は被除数と同符号とする。
0除算はdefect、最小値/-1は最小値、同じ組合せの剰余は0とする。
負のshift量はdefectとし、幅以上の左shift・unsigned右shiftは0、算術右shiftは符号に応じ0または-1とする。
Floatはbinary64の表現を使い、保存・adapter境界ではbit patternを維持する。
FloatからIntへのfloor/ceil/round/truncateはそれぞれの丸めを行い、roundの中間値は0から遠ざける。
NaN・Infinityや丸め後のInt範囲超過は変換defectとする。
境界値、NaN等のFloat観測、演算の異常条件、parse失敗をCの最適化有無で確認する。
評価順、短絡評価、引数の副作用順もsourceから検査し、C compilerの最適化へ意味を委ねない。

Stringは長さ付きのbyte viewであり、埋込みNULをC文字列終端と混同しない。
静的・borrowed文字列の比較は不要なheapやscratchを要求しない。
動的Stringの出力は`string_capacity`で制限し、範囲は1〜512 byte、既定は256 byteとする。
入力と出力のviewが重なるappend/concatやsubviewで、元ownerの寿命・byte列・容量検査を保つ。
`String.length`はgrapheme cluster数、`String.byte_size`はUTF-8 byte数を返す。
Unicode scalarとgrapheme clusterを区別し、
scalar変換・grapheme分割・Unicodeを含むList変換は各演算の意味に従う。
不正UTF-8のStringをgrapheme走査へ渡した場合はdefectとする。
Unicode graphemeの期待値はrepo内の公式test vectorでも検査する。

Bytesのソース・型・8個のbuiltinはv0.26.1の`Std.Bytes`を使う。
独自の`Emelino.Bytes`やexternal名によるmarker、Bytes専用の型ID振り直しは設けない。
本体のcanonical型IDと同名のユーザー型を区別し、8 builtinを所有IRへ直接投影する。
要素はunsigned byte、viewの長さはu32で保持し、Stringへ変換する前に長さとUTF-8を検証する。
16進変換、Eq/Ord/Show、Listとの変換、borrowed snapshot、Owned結果と取消時の解放を扱う。
所有Bytesが必要な演算は、結果を捨てる場合でも必要heapの検証を省略しない。

実装は`crates/emelino-compiler/src/semantics/{numeric.rs,strings.rs,unicode_grapheme.rs}`と
`backend/emit/{numeric_runtime.rs,string_runtime.rs,unicode_grapheme.rs}`、
`backend/emit/features/builtins/bytes.rs`、CLIのbuiltin投影が担当する。
source観測は`crates/emelino-cli/tests/source_semantics.rs`、
`legacy_string_alias.rs`、`legacy_bytes.rs`にある。
中断と取消を含むBytes観測はPR5、const Stringのfold観測はPR6で追加する。
`legacy_bytes::bytes_decode_checks_the_full_u32_length_before_narrowing_to_string`は長さの切詰めを防ぎ、
`legacy_string_alias::append_handles_both_inputs_overlapping_the_output_and_the_capacity_limits`はaliasと容量境界を確認する。
Unicode vectorは`crates/emelino-compiler/tests/data/GraphemeBreakTest-18.0.0.txt`に保持する。

<a id="functions-effects-failure"></a>
## 関数値・effect・State・failure（PR4）

closureは関数とcapture環境を組み合わせ、高階callの引数・戻り値・失敗経路へ同じABIを適用する。
captureしたString、tuple、enum、Listはfactoryのstackが消えた後も必要な所有を保つ。
handler/layerはoperationとevidenceの対応を保持し、labelの異なるhandlerを混同しない。
State/Cellは逃げたclosureと共有する値を保持し、最終参照の解放まで生存させる。
sourceから受信したclosureをbackendで一度だけlowerし、既に変換済みの`MakeClosure`を
raw Flow入力として受理して変換を重ねることはしない。

recover可能なfailure、escape、取消に関わるCauseと、即時停止するdefectを区別する。
failureはerror payloadの所有を保ってcall・handler境界を伝播し、`protect`等のcleanupを実行する。
defectは設定されたboard停止hookへ直行し、`Cause.attempt`でもrecover可能な値に変えない。
公開C境界まで届いた未処理failureも停止方針に従う。
Causeのkind参照は値を消費せず、表示はerrorのShowとcallerの出力bufferを使う。

本体v0.26.1の標準Flowにはbackendが必要とするCause.raiseの失敗伝播と、
Unit→Any callbackのUnit box補完が含まれないため、
`crates/emelino-cli/src/upstream_adapter/{cause.rs,unit_any.rs}`で補う。
空String placeholderをUnit値として扱う箇所を区別し、一般の空Stringを書き換えない。
本体の別backendが持つ私的な補完は標準Flowの契約とはみなさない。
`OnFail::Handle`はraw入力用の互換形式であり、本体sourceがそれを生成するとは主張しない。

CLIの保存再生成も`Config::validate_entry`を通し、未処理Evidenceを公開引数・戻り値へ露出させない。
低レベルraw IRの`compile_program`が明示Evidenceを扱えることとは別の境界である。
`pipeline::saved_raw_evidence_input_cannot_bypass_the_cli_entry_contract`はCLI入口の迂回を拒否する。

実装はCLIの上記補完、`crates/emelino-compiler/src/backend/lower/{closure.rs,effects.rs,failures.rs}`、
`backend/emit/cause.rs`とfailure emitが担当する。
`crates/emelino-cli/tests/reused_effects.rs`はcapture順・handler層・failure payloadを確認し、
`legacy_state.rs`は逃げたclosureの共有と繰り返し回収を確認する。
`legacy_traits.rs`は構造表示・typed Cause・公開failure、`source_builtins.rs`は表示の一度だけの評価と
panic/todo/assertのboard停止を観測する。
`legacy_runtime.rs`の`legacy_cause_*`と`legacy_aggregate_*`はCause・所有capture・間接callを確認する。
