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
CLIは[組み込みAPIの省略引数](embedded/default-arguments.md)に限り、名前解決済みHIRへ
省略分を追加する。effect操作の識別と補完後の型検査は本体APIを使い、宣言とC ABIは維持する。
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
source観測は`crates/emelino-cli/tests/source_semantics.rs`、`legacy_strings.rs`、
`legacy_string_alias.rs`、`legacy_static_strings.rs`、`legacy_bytes.rs`、`legacy_bytes_runtime.rs`にある。
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

<a id="async-runtime"></a>
## 中断・task・Scope・Fiber・Deferred（PR5）

suspendするcallは生存変数をframeに保持して戻り、完了eventをmain側schedulerが処理して再開する。
handlerの即時resumeではoperation本体の終了順を保ち、遅延resumeではcallerが次の中断まで進む。
同じcontinuationの重複resumeは定義された診断を行う。
値とerrorの所有をframe・task context・callerの間で移し、成功・失敗・取消のいずれでも回収する。

Scopeは子の寿命を囲む。終了時は未開始の子を取消し、進行中の子とoperation tailのcleanupを待つ。
Fiberのjoin/awaitは完了済み結果を再利用でき、複数waiterは完了側の処理が終わってからFIFOで再開する。
interruptは子のcleanup完了を待つ。中断したcleanupを途中で捨てて外へescapeしない。
終了したScope由来のcallbackはslot再利用後も古いscopeとして拒否する。
取消後の古いtask通知やready eventを、新しいslotの結果へ誤配信しない。

Deferredは最初のcompleteで値を保存し、pollはyieldせず、awaitは実際のresume境界になる。
String/List/tuple等はscope終了後も必要な所有を保つ。
Scopeの終了はcapture経由の循環参照を切り、waiterの連結をhead・middle・tailそれぞれで解除する。
Deferredを使わないプログラムにはそのpoolを生成しない。

| 容量設定 | 既定値 | 境界 |
| --- | ---: | --- |
| `fiber_capacity` | 8 | 完了後でも未解放のhandleを含む |
| `frame_capacity` / `frame_bytes` | 32 / 256 | headerを含む型付きframe。実際のsizeofはC側でも検査 |
| `task_capacity` / `event_capacity` | 8 / 8 | pending externalと完了eventの固定数 |
| `deferred_capacity` | 8 | 同時に生存するDeferred |

pool枯渇はresource defectとして停止し、暗黙に拡張しない。
`frame_bytes`はtargetのlayoutにも依存するため、あるhostでの成功を別のtargetへ一般化しない。
実装は`crates/emelino-compiler/src/backend/lower/fiber*`、`backend/plan/task_values.rs`、
`backend/emit/{deferred.rs,task_values.rs}`と`backend/emit/runtime/`のscheduler/scope/taskが担当する。
`crates/emelino-cli/tests/reused_concurrency.rs`、`legacy_runtime.rs`、`legacy_more.rs`、
`legacy_externals.rs`、`legacy_bytes_runtime.rs`は再開順・取消・所有値・slot再利用・pool枯渇を観測する。
`source_clock.rs`はhost taskを待つClockと共有仮想時刻を扱う。

<a id="const"></a>
## constの評価・cache・既知差分（PR6）

定数化できる値は静的表現とし、runtime constは最初の参照で初期化し、値またはdefectをcacheする。
初期化順、再帰的初期化、依存するconst間の順序を検査し、再呼出しで副作用を繰り返さない。
heapを含む値のcacheは所有を保持し、runtime constのdefect表示には
`const_diagnostic_bytes`（既定1024 byte）の専用領域を使う。
その領域が不足した場合もheapへ逃がさず、別の固定緊急bufferでresource defectを報告する。

本体仕様は汎化constにも型特殊化をまたぐ単一の値・単一初期化を求めるが、
固定版本体は特殊化ごとにconstを分離したFlowを出力する。
emelinoは受信したconst単位でcacheするため、異なる具体型から参照したときの共有を保証できない。
これは[UPSTREAM-001](upstream-known-issues.md)として受け入れた未修正の本体差分であり、
型別初期化を新しい言語仕様へ変更するものではない。
名前からの同一性推測、由来追跡、private marker、追加単相化をemelinoへ設けず、
C最適化で共有が復元されることも前提にしない。

共有に関係する8件のsource観測だけを既知差分として分け、const全体を試験対象から外さない。
修正版本体を採用するときは5 crateとlockを同一revisionへ更新し、共有初期化・型別参照・
defect cache・所有権を回帰確認してから既知問題を閉じる。

実装は`crates/emelino-compiler/src/backend/lower/{const_eval.rs,const_groups.rs}`、
`backend/plan/{const_runtime.rs,const_bridge.rs}`と対応するemit/runtimeが担当する。
`crates/emelino-cli/tests/reused_consts.rs`と`reused_source_seams.rs`は初期化・defect・所有値を扱う。
`upstream_known_issues::upstream_001_specialized_consts_keep_two_initializers_until_upstream_fix`は
未修正のFlow形状を明示的に確認する。
source後にIRを加工するseam試験は通常sourceだけの成功と分けて記録する。

<a id="c-abi-embedded"></a>
## C ABI・ISR・組み込みライブラリー（PR7）

C公開関数の型、公開名、失敗、effect/evidence、suspend、所有権を検査してheaderを生成する。
通常C targetのString/callback ABIを自動的に継承せず、生成headerを接続先の正本とする。
外部関数はscalarの直接bindingか、生成値表現に対応する明示adapterを選ぶ。
非scalar値を通常host C backendのABIへ誤ってリンクしない。

| 同期adapterの結果 | 契約 |
| --- | --- |
| `Borrowed` | 戻る前にEmela側が所有copyを取得する。取得完了までboardが元値を保持する。C calleeのstack上のpointerは禁止 |
| `Owned` | 管理fieldを含む完全な所有値を移す。ownerはこの生成moduleのheap由来とし、mallocやC stack由来を認めない |
| `StringBuffer` | 追加のcaller bufferへownerless Stringを返す。boardがbyte容量を確認する。String結果だけに使う |

同期callの引数はcall終了までのborrowであり、保持したいboard側は有効な間に独立の所有を取得する。
ownerless Stringのbyteを保持するときはcopyする。
非同期externalは明示したtask adapterを通し、完了通知、取消、context寿命、成功/errorの所有移転を扱う。
通知はmain側schedulerが処理し、ISR内で一般のEmela処理を再開する契約にはしない。

adapterのC名は識別子構文、runtime予約名、公開wrapper、実際に生成するhelperとの衝突を事前検査する。
`emela_scheduler_run`、生成される`run_start`、異なるtaskから重複生成する`<adapter>_push`等は拒否する。
生成しない条件付きhelperの名前まで予約せず、helper不要なら`run_unwind`も使える。
衝突時はC出力を作らず通常診断を返す。

`isr_entries`はexportをISR入口として指定する。到達するheap、参照管理、再帰、動的call、
suspend、runtime const、外部callを検査し、一般処理をISRから誤実行させない。
外部callは明示した`isr_safe_externals`の契約に従う。
static borrowとscalar bit値を扱う安全な入口は保持する。

`Embedded.Peripheral/Gpio/Uart/I2c/Spi/Board`の6moduleはemelinoが
`crates/emelino-cli/lib/embedded/`に保持し、`bundled.rs`で通常sourceとして供給する。
本体の`Std.Embedded.*`は登録せず、互換aliasも提供しない。
ユーザーによる同名moduleの上書きを拒否し、組み込みsourceにもcore内部権限を与えない。
汎用core/stdと`Std.Bytes`は引き続き本体から供給する。
本体の`emela-codegen-c`/`emela-codegen-emb`には依存せず、ローカルmoduleを将来も本体と同期する契約にはしない。

生成するboard header、service adapter、manifestはCとの接続点である。
GPIO/UART/I2C/SPI等の操作はhandlerとC bridgeを経由し、buffer・error・取消を維持する。
物理BSP、startup/vector/linker、SDK/toolchain選定、firmware build/flash、実機動作確認は後続範囲とする。

実装は`crates/emelino-compiler/src/contract/options.rs`、ABI/ISRのlowerとplan、
CLIの`bundled.rs`とlocal moduleが担当する。
`crates/emelino-cli/tests/{board_boundary.rs,adapter_symbols.rs,legacy_externals.rs,legacy_services.rs}`が
header/manifest、公開型、sourceと保存再生成、所有移転、ISR制約、名前衝突、service取消を観測する。
`adapter_symbols::valid_task_adapters_execute_after_saved_input_replay_including_unused_suffix_names`は
正常なtaskと使用しないhelper suffixを含む設定のC実行を確認する。
