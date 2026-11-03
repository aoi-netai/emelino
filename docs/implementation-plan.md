# Emela本体のFlowから旧fork相当のC backendへ接続する実装計画

更新日: 2026-11-03。emelinoの今後の実装方針と終了条件は本書を正本とする。
現状の実装は [architecture.md](architecture.md)、本体由来の既知問題は
[upstream-known-issues.md](upstream-known-issues.md) に記録する。

## 1. 到達範囲と現在地

目標は、前回のemela-embで確認した固定旧fork相当のcompiler機能を、
**ソース → Emela本体 → Flow → emelino backend → C実行**の経路で利用できること。
固定した旧fork相当の機能を、組み込み向け制約のもとで全面的にサポートする。対応表の全対象項目について実装・検証を行い、本体由来の既知問題は明示する。JS等の他targetの実装や、あらゆる入力に対する正しさの形式的証明は今回の対象としない。
本体由来の既知バグは明示した例外として管理し、本体の修正に追従する。

| 基準 | 固定する内容 |
| --- | --- |
| 現在の本体依存 | 公式 `ec1031cb0b98386baf0e9b0ced67e8829bb2c529` |
| 再利用する前回の実装 | emela-embのP1〜P7最終source `b0a367bc0a29227fae62cee2def75ed925a5218f` |
| 前回の最終報告・対応表 | emela-embの公開済み `2258716bab2e9e4bbaa5a2fd135f6999636125e2`。compilerのtreeは上記sourceと同一 |
| 機能比較の旧fork | `c34c0aeefdd966d854371b3110db00a47798751d` |

S0で完了した最小実験は、`answer() -> Int { 42 }` の本体Flow生成、実験用JSONの保存、
ソース削除後のC再生成、ホストCからの呼出しである。全14テスト、fmt、strict clippy、
生成Cの `-O0` / `-O2` 実行を確認済み。S1の接続基盤も完了し、[S1報告](reports/compiler-s1.md)に記録した。
S2の同期データ接続も[確認済み](reports/compiler-s2.md)。S3〜S6は検証・接続を進行中。前回の607テストの成功を今回の接続の成功として数えない。

## 2. 保持する方針

- 本体を変更せず、公開APIと標準の単相化・Flow生成を利用する。
- frontend、標準ライブラリー、単相化をemelinoへ複製しない。
- 本体の5 crateを同一revisionへ固定し、core/stdもその版から供給する。
- backend・runtime・テスト資産を前回の最終版から選んで再利用する。
  独自frontend、同梱core/std、古い完了判定を含むコミット列の丸ごとの取り込みは行わない。
- 固定heap/pool、所有権、failure/取消のcleanup、defect時の即時停止、main側scheduler、
  ISR制約、C ABIは前回の組み込み契約を比較基準にする。
- インライン化、定数伝播、不要コード除去等はCコンパイラーに任せる。
  最適化前にも意味が成立するCを生成し、資源制約・所有権・ISRの検証はbackendが担う。
- 物理BSP、startup/vector/linker、firmware build/flash、実機検証は後続の別作業とする。
  今回はそれらの接続先となるheader・adapter・生成物情報までを扱う。

## 3. Flow接続と保存形式

```text
Emelaソース + 本体core/std
  → 本体の公開frontend API・標準単相化
  → emela_lir::Module（Stage::Flow）・本体verifier
  → upstream接続部で型・ID・演算・宣言情報を変換
  → emelinoのBackendInput（前回のProgramを基礎とする）
      ├ version付き成果物として保存／読込み・検証
      └ 既存lower → plan → emit → C・header・manifest
```

本体の型を扱う変換部はCLI側のupstream接続moduleに閉じ込める。
compilerはメモリー上のBackendInputとOptionsから生成物を返す。
本体frontend、ファイル操作、board選択、外部processへの依存は持たせない。

保存するのは**emelino専用のbackend入力形式**であり、本体の標準Flow全体の保存形式ではない。
本体Flowから必要な情報を変換する経路を一本にし、別の汎用Flow DTOとbackend IRを
同時に保守する構成を避ける。保存前と読込み後は同じ入力検証・コンパイル経路を使う。
型・enum・effect・関数・external・公開入口・error表示の参照先を成果物内で完結させ、
元ソースや本体checkoutがなくても再生成できるようにする。

format/schema、入力段階、producer revision、必要featureを照合し、型・IDの範囲、
参照整合性、演算ごとの型を検証する。Floatのbit pattern、Int64、Bytes等を損失なく保存する。
到達性・layout・文の位置など後段で算出するmetadataは外部入力として信用しない。
未知の情報や未対応演算は診断し、黙って捨てない。

生の `Stage::Flow` を変換し、closure・handler・suspend等の変換は既存lowerで一度だけ行う。
旧forkのadapterはこれらの変換後に呼ばれていたため、旧lower一式をそのまま前置しない。
Coreの型・宣言表は参照できるが、Core関数本体の再解析・書換え・独自の再単相化は行わない。

S0の `program.flow.json` は `emelino.flow.minimal` の実験用profileである。
S1で新形式 `emelino.backend-input` / `program.backend.json` を導入した。
ソース生成には `--config`、保存済み入力の再生成には `--input` と再生成用設定を使う。
旧 `--flow` は最小profileの読込みのみ保持し、同じ名前の汎用Flowへ拡張した扱いにはしない。
本体のdumpは目視確認用として継続し、パーサーの入力にはしない。

この選択は、旧PLANの「本体CLIが汎用Flowファイルを出す」方式からの明示的な変更である。
本体CLI/汎用codecの新設を今回の前提にしない。本体が将来そのAPIを提供した場合は、
接続部の置換を別の変更として検討する。

## 4. 公開入口・C境界

本体の `lower(target = "c", Roots::Public)` でentry moduleのmainと公開関数をFlowへ含め、
emelino側の設定でC公開名・起動入口・ISR入口を選ぶ方式へ移行する。
通常の `pub fn` を組み込みABIへ接続する契約であり、`@export(c)` の検査を無効化したり、
ソースの属性を削除して本体へ渡したりしない。

固定版で、List戻り値とAsyncのpub関数が正規の型検査・単相化・Flow検証を通り、
C bindingのない到達externalが引き続き拒否されることを小実験で確認した。
S0の `Roots::Exported` / `@export(c, "answer")` はS1で新入口に移行した。
公開名の正本は設定にまとめ、ソース属性との二重指定は診断する。
S1でJSON設定の `exports: [{function, symbol}]` を実装した。例は `examples/answer.json`。

| 対象 | 設定と検証 |
| --- | --- |
| 一般のC公開関数 | Flow関数・C alias・引数/戻り値・所有権を検証しheaderを生成 |
| 起動入口 | 実行開始・終了・suspend時のruntime起動契約を検証 |
| ISR入口 | 到達するheap、参照管理、再帰、動的call、suspend、runtime const等を検証 |
| 同期external | bindingと生成値のABI、Borrowed/Owned/StringBuffer等のadapter契約を検証 |
| 非同期external | task adapter、完了通知、取消、所有権を検証 |

通常C targetのString/callback ABIを自動的に継承する方式ではない。
emelinoの生成header・adapter契約をC側の正本とする。
本体が認めるFlowであっても、未処理effect/evidence・failure・公開型・ISR条件を検査し、
接続できない入口は診断する。

`Roots::Public` は選択した関数だけでなく、entry moduleのmain（存在する場合）と対象pub関数を
まとめてrootにする。未選択のmainやpub関数でも到達externalの検査に影響する。
trait bound付きgenericはrootから除外され、
他のgenericはAny等に具体化され得るため、入口moduleには具体型の公開wrapperを置く。
ソース上の汎化の有無を、Flowだけから完全に検査済みとは報告しない。
将来の汎用指定root APIへの追従も、この接続部の範囲に留める。

## 5. 本体由来の不具合とC最適化

汎化constの型特殊化をまたぐ初期化/cacheの分離は、[UPSTREAM-001](upstream-known-issues.md) として扱う。
本体仕様の「汎化constも単一の値を記憶する」を変更せず、固定版本体の現状として許容して進める。
恒久的なemelino仕様として共有を禁止したり、前回の正常観測を誤りへ書き換えたりしない。

emelinoは受信したFlowのconst単位で初期化・cache・defect等を実装する。
共有情報が出力されない問題を埋めるための由来追跡、private marker、追加単相化、
名前からの同一性推測は実装しない。C最適化が初期化回数やidentityを復元することも前提にしない。

本体由来の差分は、対象revision・再現入力・影響する観測・期待する仕様を記録する。
今回の終了判定では「本体由来の既知問題」として別区分にし、成功・未対応演算・
試験省略と混同しない。const試験全体を除外せず、影響する観測だけを特定する。
新しい差分は原因を確認し、例外を増やすだけで今回の移植を完了扱いにはしない。

本体の修正パッチが利用可能になったときは、同一revisionの5 crateとCargo.lockを更新し、
修正後のCore/Flow契約を調べて接続部・backendの必要箇所を調整する。
依存更新だけで直ると仮定せず、単一初期化、型別参照、defect、所有権の回帰試験を確認してから
既知問題を解消扱いにする。修正時期・修正版は未定であり、自動更新は行わない。

## 6. 実装段階

S0の最小実験とS1の接続基盤は完了済み。S1〜S6を次の機能単位で管理する。
backendの共通コードはS1でまとまって採用できるが、コードが存在するだけでは各段階を完了にしない。

| 段階 | 作業 | 終了条件 |
| --- | --- | --- |
| S1 接続基盤 | backend/contract/semantics/runtimeの固定版採用、対応表の移入、公開入口設定、Flow→BackendInputと保存・検証の骨格 | 引数・直接call・整数演算・ifを本体からC実行へ通す。保存済み入力だけでも再生成でき、未知形式/ID/演算を拒否 |
| S2 同期データ・メモリ | 型/enum表、tuple・record・List・Bytes・String・Any・再帰型、match/loop、固定heapと所有権を接続 | 値・分岐・再帰・解放をCで観測し、小容量の枯渇も旧契約どおり診断/停止 |
| S3 関数値・effect・失敗 | closure、高階関数、handler/layer、State/Cell、fails/escape/Causeと導出関数を接続 | capture・effect伝播・回復・cleanupの観測が旧対象試験と一致。変換の重複がない |
| S4 中断・並行処理 | suspend/frame、task、Scope/Fiber/Deferred、取消と中断cleanupを接続 | 中断/再開・失敗・取消・資源回収を固定容量で検証し、host mockで完了通知を実行 |
| S5 const・公開ABI・ISR | 標準Flowのconst、同期/非同期adapter、所有値公開、ISR到達性、組み込みmodule/C bridgeを接続 | header/manifestとC呼出しが整合。constの正常観測とUPSTREAM-001の影響を分離し、ISR制約を検証 |
| S6 対応表と全体確認 | 前回のsource/C harness/IR試験の接続先を照合し、回帰試験と新規checkoutでの再現を確認 | 対象行が新経路で検証済み、理由付き適用外、または明示した本体既知問題へ分類され、理由不明の未接続・未確認を残さない |

S5の組み込みmoduleはemelinoが所有する追加ソースとして供給する。
本体のcore/stdを複製・差し替えず、外部ソースへcore内部権限を与えない。
利用可能な公開APIの範囲で接続し、供給できない場合はその項目を未完として記録する。
Pome・project loadingの独自実装は作らず、必要な入口は本体へ委譲する。

## 7. 移植・検証・コミット

前回の対応表と旧試験の対応付けを引き継ぎ、採用元revision・ファイル・ライセンスを記録する。
各項目の今回の判定と実行証拠は新たに作り、前回のverifiedを無条件にコピーしない。
本体への所有範囲の移管や入力段階の変更による適用外には理由と移管先を記録し、
必要な言語機能の未実装を適用外へ置き換えない。
source試験は本体経由へ接続する。raw IR試験と旧frontend後の手動IR加工を必要とする試験は
別区分を維持し、通常sourceだけで通った実績として数えない。

公開C名・値・副作用・失敗・所有権・資源境界を比較し、生成C本文のbyte一致は要求しない。
対象の生成Cを `-O0` / `-O2` でコンパイル・リンク・実行し、C compiler不在を成功扱いしない。
fmt、strict clippy、必要なRust試験を行う。source生成と保存済み入力からの再生成を両方確認する。

各段階は日本語の機能別コミットに分け、移植元と追加修正を後から追えるようにする。
作業ブランチを使用し、pushやPR作成はその時点のユーザー指示に従う。
