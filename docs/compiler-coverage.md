# 現在の機能対応表と検証記録

対応範囲の意味は[構成・機能契約](architecture.md)、採用範囲と終了条件は
[実装計画](implementation-plan.md)、行単位の実装・試験参照は[JSON台帳](compiler-coverage.json)を正本とする。
機能契約とrepo内の試験から現在の挙動を確認できるようにし、旧版のソース閲覧を必須条件にしない。

## 現在の検証を読む

JSONの`completion`は再編後の現在の完了判定であり、`finalized: false`は検証未確定を表す。
再編後の実行は[再現検証記録](reports/reconstruction-validation.json)へ、実際に実行したSHA・tree・
コマンド・件数・log hashとともに記録する。空欄や`pending`を成功と解釈しない。
`pre_reconstruction_completion`、`baseline_completion`、`release_completion`、
`embedded_ownership_completion`と既存reportsは、それぞれに記した過去のrevisionの結果である。

行の`status`は保存した観測の分類を表す。再編によってIDや観測を変えず、
履歴で確認した分類を保持しているが、新しいtreeで全試験を実行した事実は`completion`で別に確定する。
各行の`contract_group`から台帳の`current_contracts`を引くと、現在のローカル契約と試験入口が分かる。
`delivery_pr`は、その行の主題機能を導入する実装順序番号（PR1〜PR7）であり、GitHubのPR番号ではない。
当該行の全実装・観測が最初に揃うcommitや、実行成功の時点を表さない。
複数機能を組み合わせる試験は後段にもあり、実際のcommit対応は対照表、実行revisionは再現検証記録で確認する。
`phase`/`legacy_phase`は過去の分類として保持し、現在の契約分類の推定には使わない。

分類は機能IDと文脈を基準にする。`input`は保存・入力拒否、`suspend`はframe・再開・取消を含む経路、
`sync`/`common`はその機能IDの契約へ案内する。代表試験と横断要件は主題を個別に対応付ける。
例えば`Term::Return`は基本制御、同じ演算のsuspend文脈は非同期、`Type::Any`はデータ表現へ結ぶ。
`Const::Int`等のliteralと`Value::Const`/`Function.constant`の初期化・cacheは区別する。
これは最初に読む契約の案内であり、複数機能にまたがる行の根拠を一つのPRへ限定しない。

| 機能 | ローカル契約 | 主な試験入口（CLI tests） |
| --- | --- | --- |
| 接続・保存・基本制御 | [PR1](architecture.md#foundation) | [pipeline](../crates/emelino-cli/tests/pipeline.rs)、[backend_input](../crates/emelino-compiler/tests/backend_input.rs) |
| データ・所有権 | [PR2](architecture.md#data-memory) | [legacy_ownership](../crates/emelino-cli/tests/legacy_ownership.rs)、[reused_pipeline](../crates/emelino-cli/tests/reused_pipeline.rs) |
| 数値・文字・Bytes | [PR3](architecture.md#numeric-text-bytes) | [source_semantics](../crates/emelino-cli/tests/source_semantics.rs)、[legacy_bytes](../crates/emelino-cli/tests/legacy_bytes.rs)、[legacy_string_alias](../crates/emelino-cli/tests/legacy_string_alias.rs) |
| 関数値・effect・failure | [PR4](architecture.md#functions-effects-failure) | [reused_effects](../crates/emelino-cli/tests/reused_effects.rs)、[legacy_state](../crates/emelino-cli/tests/legacy_state.rs)、[legacy_traits](../crates/emelino-cli/tests/legacy_traits.rs) |
| 中断・task・並行処理 | [PR5](architecture.md#async-runtime) | [reused_concurrency](../crates/emelino-cli/tests/reused_concurrency.rs)、[legacy_runtime](../crates/emelino-cli/tests/legacy_runtime.rs)、[legacy_more](../crates/emelino-cli/tests/legacy_more.rs) |
| const | [PR6](architecture.md#const) | [reused_consts](../crates/emelino-cli/tests/reused_consts.rs)、[reused_source_seams](../crates/emelino-cli/tests/reused_source_seams.rs)、[upstream_known_issues](../crates/emelino-cli/tests/upstream_known_issues.rs) |
| C ABI・ISR・組み込み | [PR7](architecture.md#c-abi-embedded) | [board_boundary](../crates/emelino-cli/tests/board_boundary.rs)、[adapter_symbols](../crates/emelino-cli/tests/adapter_symbols.rs)、[legacy_externals](../crates/emelino-cli/tests/legacy_externals.rs)、[legacy_services](../crates/emelino-cli/tests/legacy_services.rs) |

## キー集合と状態

| 集合 | 一意キー | 固定件数 |
| --- | --- | ---: |
| records | 定義ID | 428（variant 195・共有宣言43・field 190） |
| contexts | 定義IDと文脈 | 226（common 85・sync 58・suspend 58・input 25） |
| canonical tests | 旧targetと試験名 | 115 |
| requirements | 横断要件ID | 10 |
| source corpus | 旧試験名・program ID・source hash | 174観測・169program・170source file |
| 追加nonruntime観測 | 旧試験名 | 3 |

fieldを独立演算の対応数に加えず、115代表試験を全source corpusとみなさない。
件数に加えてキー集合・重複・欠落を照合する。

| 状態 | 条件 |
| --- | --- |
| `pending` | 接続・実行・意味をまだ照合していない |
| `mapped` | 実装と試験を対応付けたが実行またはレビューが未完了 |
| `verified` | 記録された経路で必要な実行・拒否・意味の対応を確認 |
| `not_applicable` | 入力段階や担当範囲の変更理由と必要な意味の移管先が明示されている |
| `replaced` | 元の観測、新しい観測、置換理由を揃えた旧試験の置換 |
| `upstream_known_issue` | 固定版本体の既知問題で特定の期待観測が満たされない |

親recordからcontextや試験へ状態を自動伝播しない。
同期の成功をsuspend経路へ流用せず、source、raw IR限定、入力拒否、宣言代入、schemaを区別する。
`MakeClosure`の入力拒否はsource closure未対応を意味せず、`OnFail::Handle`はraw互換形式である。
`Type::Param`の宣言代入と未解決runtime slot、Unitの`Repr::Void`とraw変数も分ける。
`Hold`/`Retire`/`Unhold`のsource cleanupと試験用IR seamは別の観測である。

## 既知問題と容量条件

[UPSTREAM-001](upstream-known-issues.md)は汎化constの型特殊化間の共有に関する8観測へ影響する。
履歴の分類はrecords 428/contexts 226がverified、代表試験114 verified＋1既知問題、
要件9 verified＋1既知問題、source観測166 verified＋8既知問題、追加nonruntime観測3 verifiedである。
この集計を「すべてのconst共有が旧仕様と一致する」という主張には使わない。

TestClockの補足観測は`frame_bytes=2048`（旧1024、host上の最大frame 1624 byte）、
短絡評価・引数順序のeffect観測は`heap_bytes=4096`（旧0）の条件を持つ。
設定差は[時計](reports/reuse-source-clock.json)と[評価順](reports/reuse-source-semantics.json)に記録する。
容量差をUPSTREAM-001と混同しない。

## 固定参照と出典

[固定参照表](reference/legacy-flow/README.md)は旧forkのGit objectから取得した資料であり、
現在の実装表へ書き換えない。4 JSONと4 Markdownの配布hashは同directoryの`provenance.json`に保持する。
表は旧環境で自動生成されたが、**その生成器はこのrepoには含まれていない**。
`candidate_found`は構文上の実装候補であり動作確認ではない。
ここでは固定表のhashと現在の台帳とのキー集合を検査する。再生成できたとは報告しない。

旧表の199ファイルへの固定commit URLは出典追跡用であり、現在の必須判断はローカル契約・実装・試験へ結ぶ。
199ファイル全体や旧frontendを製品依存としてvendorしない。
リンク先が利用できない場合でも、固定表・元hash・採用ライセンス・現在の対応判断はrepo内に残る。
出典と旧SHAの読み方は[履歴索引](reports/README.md)、旧commitから機能PRへの対応は
[対照表](reports/history-crosswalk.json)を参照する。

## 再現手順

Rust/Cargo、C11 compiler、`sha256sum`を用意し、rootで次を実行する。
依存cacheだけで行う検証では`--offline`を追加し、ネットワークからの新規取得とは区別する。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --locked -p emelino-cli --test reused_pipeline coverage_ledger_retains_every_fixed_definition_context_and_test -- --exact
cargo test --locked -p emelino-cli --test reused_pipeline source_coverage_retains_every_fixed_program_and_original_observation -- --exact
```

source実行を担うfixture試験はsource生成・保存再生成・C実行を行う。
schema検証・入力拒否・artifactのみの試験にはC実行を要求する意味を持たせない。
source削除の独立観測は`pipeline::upstream_flow_survives_source_removal_and_links_from_c`で行う。
台帳整合試験は参照と分類の一貫性を検査するもので、各項目の意味の個別レビューを置き換えない。
独立archiveでも同じ製品/試験treeとコマンドで再実行し、実行ディレクトリと環境を記録する。
新しい完了判定を確定した後には、`completion.finalized=true`を読む台帳試験も再実行する。
