# 本体Flow接続の対応表と終了条件

[実装計画](implementation-plan.md) に従い、固定した旧fork相当の機能を組み込み向け制約の
もとで全面的にサポートする。自動生成の対応表は漏れを検出するために使い、各項目の
実装・実行・レビューを新しい経路で確認する。

## 固定した資料

- 比較する旧fork: `c34c0aeefdd966d854371b3110db00a47798751d`。
- 再利用するbackend source: emela-emb `b0a367bc0a29227fae62cee2def75ed925a5218f`。
- 対応表の取得元: emela-emb `2258716bab2e9e4bbaa5a2fd135f6999636125e2`。
- 現在の本体依存: `v0.26.1` / `6021115103dd830e0efd9bb96a56a0ca97859329`。
- S6完了時の本体依存: `ec1031cb0b98386baf0e9b0ced67e8829bb2c529`。
- [固定参照表](reference/legacy-flow/README.md)、[今回の台帳](compiler-coverage.json)、
  [取得元とhash](reports/reuse-reference-import.json)。

参照表は固定したGit objectから取得した。旧checkoutの未コミット変更を含まない。
前回の607テストやverified判定を今回の成功へ引き継がず、全行をpendingから開始する。

## 集計単位

| 集合 | キー | 件数 |
| --- | --- | ---: |
| records | 定義ID | 428（195 variant、43共有宣言、190 field） |
| contexts | 定義IDと文脈 | 226（common 85、sync 58、suspend 58、input 25） |
| tests | 旧targetと試験名 | 115 |
| requirements | 横断要件ID | 10 |

件数だけでなくキー集合・重複・欠落を照合する。fieldを独立した演算の対応数に加えない。
115試験は旧対応表に登録した関連試験であり、全source corpusを表さない。
残りのsource・横断試験はsource_corpus要件で対応付ける。

## 状態と証拠

| 状態 | 条件 |
| --- | --- |
| pending | 接続・実行・意味の対応をまだ確認していない |
| mapped | 今回の実装・試験を対応付けたが、実行またはレビューが未完了 |
| verified | 今回の経路で必要な実行・入力拒否・意味の対応を確認した |
| not_applicable | 入力段階や担当範囲の変更を説明し、必要な意味の移管先を確認した |
| upstream_known_issue | 固定版本体で再現した既知問題が、特定の観測を妨げる |

旧試験にはreplacedも使用できるが、新しい試験と元の観測の対応・置換理由を必須とする。
未実装、試験困難、未使用を理由にnot_applicableやreplacedへ移してはいけない。
親recordの状態をcontext・試験へ自動伝播しない。

verifiedには今回の実装path、試験path、実行報告と確認内容を記録する。
source対象は **ソース → 本体frontend → Flow → backend → C実行** を通す。
raw IR限定、入力拒否、schemaの検査はsource実行と区別する。旧試験でsource生成後に
IRを加工するものは、その加工も明示し、通常sourceだけで通った実績として数えない。
生成Cに追加するheap観測関数は試験用の内部観測であり、公開ABIへ加えない。

同期の確認を中断経路の確認へ流用せず、値・副作用・failure・所有権・取消・容量境界を
必要な文脈ごとに観測する。生成C本文のbyte一致は要求しない。
対象Cを-O0/-O2の両方でコンパイル・リンク・実行し、C compiler不在やskipを成功にしない。

自動生成表のcandidate_foundは構文上の実装候補であり、動作の確認ではない。
台帳検査はID・参照・必須情報の整合性を検査する。試験が各項目の意味を満たすかは
個別にレビューする。あらゆる入力に対する形式的証明を提供するものではない。

## 今回の担当変更

名前解決・型検査・単相化・汎用core/stdは固定版本体が所有する。独自frontendや汎用標準ソースを
再移植しない。emelinoは組み込み用 `Embedded.*`、公開入口・Flow変換・入力契約・組み込み制約と生成Cを所有する。
本体へ移した機能も必要なsourceの確認は残し、単に所有者が変わっただけで試験を捨てない。

旧standalone_execution要件は、今回の依存方針に合わせて意味を変更する。
本体5crateの同一revision固定、新規checkoutでのsource経路の再現、およびソース・本体checkoutが
なくても保存済みBackendInputからCを再生成できることを確認する。
以前の「本体crateへ依存しない」は今回の要件ではない。

## 本体由来の既知問題

[UPSTREAM-001](upstream-known-issues.md) は汎化constの型特殊化をまたぐ単一初期化の問題。
受信Flowの各constの初期化・cache・defect・所有権は通常どおり検証する。
共有が欠ける特定の観測だけをupstream_known_issueへ分類し、const全体を除外しない。
期待仕様・実際の結果・本体revision・再現手順・影響行を揃える。
修正後は依存と必要な接続部を更新し、回帰確認してからverifiedへ変更する。

## フェーズと最終終了

旧P1〜P7の担当情報はlegacy_phaseに保存する。今回は実装計画のS1〜S6をphaseへ使う。
初期割当はP1→S1、P2/P3→S2、P4→S3、P5/P6→S4、P7→S5であり、実装・確認時に調整する。
S6で全対象行と10横断要件を照合する。

最終終了には、全行が根拠付きの最終状態であり、未判定・欠落・重複・説明のない未確認を
残さないことを求める。known issueを追加するだけで移植を完了扱いにしない。
source_scope、実装・試験・実行報告のrevisionを確認し、必要な意味が未実装なら未完了とする。

## 現在の状態

S6では [backend/schemaレビュー](reports/backend-coverage-review.json)、
[source操作レビュー](reports/reuse-source-record-review.json)、
[旧sourceの観測対応](reports/reuse-source-coverage.json) を別々に照合する。
最新の行別状態と終了判定は `compiler-coverage.json` を正とする。

source対応表は174試験・169program・170source fileと、追加3件の拒否／bundle観測を保持する。
各試験のC値観測、artifactのみの確認、source後の試験専用IR加工を区別する。
対応表の115関連試験に含まれないsource観測も、この集合で追跡する。

本体Flowの段階と表現については、次を個別にレビューした。

- `MakeClosure` は後段の入力として拒否し、source closureの実行は今回のclosure変換で確認する。
- `OnFail::Handle` はraw入力の互換形式であり、本体sourceが生成するという主張をしない。
- `Type::Param` は宣言の具象代入として扱い、未解決のruntime slotを受理しない。
- `Repr::Void` はUnitの格納省略であり、raw FlowにVoid変数が存在する証拠とは分ける。
- `Hold`／`Retire`／`Unhold` は本体のcleanup生成と、元の試験用IR seamの両方を区別して確認する。

[TestClockの追加観測](reports/reuse-source-clock.json) は元のsourceとC assertionを保つが、
`frame_bytes` を旧1024から2048へ変更している。host Cで最大frameは1624 byteだった。
本体Flowの変数保持量の違いによる容量条件であり、旧memory budgetがそのまま保たれるとは主張しない。
[短絡評価・引数順序の追加観測](reports/reuse-source-semantics.json) では、
副作用hookを正しいeffect/handlerに置き換えるためheapを旧0から4096へ変更している。
これらの容量条件はUPSTREAM-001のconst共有差分とは別の事項である。

`use {}` は本体仕様で純粋性を宣言する。通常の副作用を持つtest hookはeffect/handlerで扱い、
同じ値を返すconstの初期化回数をC側で測るような観測用hookは、試験専用の計測として明示する。


固定版本体には `Std.Embedded.*` が存在するが、組み込みライブラリーの管理移管後は登録しない。
emelinoの `Embedded.*` 6moduleを通常のsourceとして供給し、予約名の上書きを拒否する。
旧frontendの「Std.Embedded namespaceを供給せずimportを拒否する」という追加観測は、
今回のユーザー指定により元の期待が復活するため `verified` へ戻す。
過去の `replaced` 判定と理由は歴史として保存し、変更後の診断・供給元を別途検証する。

## S6完了時の確定結果

| 集合 | 検証済み | 本体既知問題 | 方針による置換 |
| --- | ---: | ---: | ---: |
| records | 428 | 0 | 0 |
| contexts | 226 | 0 | 0 |
| canonical tests | 114 | 1 | 0 |
| requirements | 9 | 1 | 0 |
| 固定source観測174件 | 166 | 8 | 0 |
| 追加nonruntime観測3件 | 2 | 0 | 1 |

未判定・pending・mappedは残していない。recordsは表現と接続の各責任を確認する単位であり、
constの型特殊化間共有の8観測が旧仕様と一致するという意味ではない。
sourceの160定義と宣言代入1定義、schema238定義、入力限定18定義、raw IR限定11定義を区別した。

[全体の実行記録](reports/compiler-s6-validation.json) は実装・試験revision `70dfbe6` の
workspaceと独立archiveで各377試験成功、失敗・ignore・filteredとも0を記録している。
最終帳票の分類変更は実装と試験のtree一致を確認し、台帳の整合試験を再実行した。
ネットワークからの依存の新規取得は試していない。straceはsandboxのptrace制約で使えず、
traceによるfilesystem access不在の証明は行っていない。

## v0.26.1への更新

現在の接続先は公式releaseの `v0.26.1` に固定する。
`Std.Bytes` のソース・型・8個のintrinsicは本体が供給するため、独自の
`Emelino.Bytes` とexternal markerの置換を削除した。旧sourceの `Std.Bytes` importも
書き換えずに本体へ渡す。対応表のBytes行は公式Flowの型・演算への投影と、
既存の所有権・UTF-8・公開ABI・task試験へ接続し直す。

標準FlowのUnit callback表現とinfallible `Cause.raise` はこのreleaseでも変わらない。
これらのbackend ABIへの変換と、Flowから所有するProgramへの投影は引き続き必要である。
汎化constの単一初期化も未修正であり、UPSTREAM-001の8観測を解消済みには変更しない。

更新後の実行結果は [release更新報告](reports/compiler-v0.26.1.md) に記録する。
S6の377試験・隔離checkout結果は当時の履歴として保持し、新revisionの検証へ読み替えない。
JSON台帳の `baseline_completion` はS6の完了記録、`completion` は現在の完了判定を表す。
既存の個別reportも旧revisionと実行記録を保持し、`release_migration` で現在の実装参照・
fixture hash・名前空間の変更を区別する。

更新後も377試験が成功し、失敗・ignore・filteredは0。fmtとstrict clippyも通過した。
既存の行別分類を維持し、UPSTREAM-001に該当する8観測は引き続き既知問題として扱う。
検証した実装・試験revisionは `fce69bbb87e7c18ca556e099ae16c8ae828a550d`。

## 組み込みライブラリーの管理移管

ユーザー指定により、組み込み用の6moduleをemelinoの `lib/embedded` へ移管した。
本体に残る `Std.Embedded.*` は登録対象から除外し、`Std.Bytes` などの汎用標準は引き続き本体から供給する。
428定義・226文脈・115代表試験・10要件の集合と既知問題の分類は維持する。
追加nonruntime観測3件は、旧Std namespaceの拒否を含めて3件とも `verified` とする。
移管元、現行供給経路、検証結果は[所有範囲の報告](reports/embedded-library-ownership.md)に記録する。
JSON台帳の `release_completion` はv0.26.1更新時点、`completion` は管理移管後の判定を表す。
