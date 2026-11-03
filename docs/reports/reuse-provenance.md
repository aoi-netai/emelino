# backend・対応表・試験資産の再利用元

## 固定した取得元

| 資産 | revision | 採用範囲 |
| --- | --- | --- |
| 前回のコンパイラーsource | emela-emb `b0a367bc0a29227fae62cee2def75ed925a5218f` | backend、contract、semantics、writer、artifactと必要な試験資産 |
| 前回の最終対応表 | emela-emb `2258716bab2e9e4bbaa5a2fd135f6999636125e2` | `docs/reference/legacy-flow/`、台帳のID・関連情報 |
| 比較する旧fork | `c34c0aeefdd966d854371b3110db00a47798751d` | 旧Flow定義・実装・関連試験の固定基準 |
| 現在の本体frontend | 公式Emela v0.26.1 `6021115103dd830e0efd9bb96a56a0ca97859329` | 公開frontend API、標準単相化、Flow生成、汎用core/std |

コードと資料は固定したGit objectから取得し、元checkoutの未コミット変更を含めない。
Apache-2.0のLICENSEと既存の出自・著作権表示を保持する。
対応表の各ファイルのhashは [取得記録](reuse-reference-import.json) に保存する。
既存の対応表は固定参照であり、今回の進捗は [新台帳](../compiler-coverage.json) に記録する。

## 採用する境界

旧独自frontend、source loader、同梱の汎用core/stdを新repoへ複製しない。
組み込み専用の `Embedded.*` はユーザー指定によりemelinoで管理する。
本体crateに依存する接続部はCLI crateに置き、compiler crateは所有するProgramとOptionsを受け取る。
backend内のlower・plan・emit・runtime実装は再利用し、接続に必要な修正を機能別に記録する。

前回の607テスト、verified、独立frontendの受理結果は今回の成功として継承しない。
本体への委譲によって不要になるテスト実装も、必要な言語機能の観測自体は新経路へ移す。
IR単体試験とsource試験は分け、元frontendを呼ぶ試験を無効化しただけで検証済みにしない。

backend採用commitは `021cf01`。`src/{backend,contract,semantics}/`、
`src/{writer,artifact}.rs`、Unicode公式vector、LICENSEを採用した。
旧独自frontendを直接呼ぶ次の7試験moduleは、そのままコピーせず本体source経路へ移した。

- `backend/lower/fiber_source_tests.rs`
- `backend/lower/p7_cause_source_seam_tests.rs`
- `backend/lower/p7_cleanup_source_seam_tests.rs`
- `backend/lower/p7_const_source_tests.rs`
- `backend/lower/p7_integration_tests.rs`
- `backend/lower/p7_recursive_source_tests.rs`
- `backend/lower/source_builtin_tests.rs`

旧専用helper `backend/lower/p7_source_seams.rs` の観測方法は
`crates/emelino-cli/tests/compat/seams/` へ移し、source生成を本体frontendへ接続した。
[const・cleanup・cause・統合試験の移行記録](reuse-source-seams.json) は
通常sourceと試験専用IR加工を区別し、UPSTREAM-001の8観測を互換成功へ含めない。
再帰型は [追加23観測](reuse-legacy-more.json)、source builtinは
`crates/emelino-cli/tests/source_builtins.rs` で確認する。

S1採用時のfrontend非依存backend試験124件、保存形式commit `68a6ab7` の7試験、
既存最小profileの6試験はすべて成功した。S6の接続修正後はbackend試験127件が成功した。
これらはbackend側の証拠であり、本体source経路の成功件数とは別に数える。

## source・C harness

同じC観測と容量設定を使える試験から再利用する。公開C aliasはソースに付けた
`@export(c)`からemelino設定へ移す。固定版本体の構文・external規則と相違するfixtureは、
同じC観測を保つ変更を明記し、旧独自frontendの構文を受理するための独自parserを作らない。

source後のIR加工と、生成Cへ追加するheap観測関数は通常source/APIの利用と区別する。
生成Cの観測関数は試験出力だけに追加し、製品の公開ABIを変更しない。
source・harnessの取得元とhash、変更点、元の容量設定を個々のfixture採用時に記録する。

## 検証の記録

対応表の428定義・226文脈・115関連試験・10横断要件はpendingから開始する。
新たに実行した試験とレビュー結果だけを今回の証拠として追加する。
S1以降の終了判定は [対応表の終了条件](../compiler-coverage.md) と実装計画に従う。
本体由来のconst共有差分は [UPSTREAM-001](../upstream-known-issues.md) として個別に追う。

## 旧source観測の移行

固定source174試験・169programと、追加の拒否／bundle検査3試験を
[今回のsource対応表](reuse-source-coverage.json) で照合する。
同じprogramを使う複数の観測、同じ試験内の複数fixture、C実行を伴わない旧観測は別々に記録する。

[並行処理・所有値85fixture](reuse-legacy-runtime.json) と
[closure・中断再開・再帰型・子の寿命23fixture](reuse-legacy-more.json) は、
元の容量設定とC assertionを保ち、本体source、保存入力の再生成、C実行の順に確認した。
計108fixtureのうち106は値・副作用・defect・所有権の実行観測、2は元からartifactだけを確認する観測である。
保存入力のC/header一致を確認した後だけ、旧試験と同じ内部heap・取消・child観測を追加する。

SharedStateの返却callbackを公開する1例では、本体が残すeffect evidenceを
普通の `State.run` wrapperで処理する。旧Cの83という結果を含むassertionは変えない。
このwrapperはfixtureの明示的な接続変更であり、製品parserを拡張しない。
旧pinで必要だった `Std.Bytes` → `Emelino.Bytes` のimport変更は、v0.26.1で撤去した。

補足source試験の容量変更も隠さず記録する。TestClockは本体Flowのframe構造に合わせて
frame_bytesを1024から2048へ変更し、短絡評価・引数順序のfixtureは副作用hookを
正しいeffect/handlerで扱うためheapを0から4096へ変更した。
[時計の記録](reuse-source-clock.json) と [数値・Unicodeの補足記録](reuse-source-semantics.json) を参照する。

## 組み込みmoduleの供給元と所有者

`Embedded.Peripheral/Gpio/Uart/I2c/Spi/Board` はemelinoが所有し、
`crates/emelino-cli/lib/embedded/` の6ソースを `bundled.rs` から供給する。
取り込み内容はv0.26.1の `Std.Embedded.*` からimport行を `Embedded.*` へ変更したものと一致し、
旧emela-embの6moduleとも同じ基準hashを持つ。これは移管時の由来であり、実行時に
本体の組み込みソースを読む仕組みや、今後も本体と同期する契約ではない。

[供給元とhash](reuse-boundary-fixtures.json) は本体側の取り込み元とローカル実装の基準hashを
別々に保持する。`bundled` 単体試験はローカル6ソースの基準hashを照合する。
組み込みmoduleはordinary Sourceとして渡し、core権限を付けない。
本体の `Std.Embedded.*` を登録対象から除外し、同名ローカルmoduleの上書きも拒否する。
名前空間、標準moduleからの独立、旧C harnessの確認は
[管理移管の報告](embedded-library-ownership.md)に記録する。

Bytesはv0.26.1の `Std.Bytes` から供給する。旧pin `ec1031c` 用に保持していた
`Emelino.Bytes` ソースとexternal識別adapterは削除した。旧fixtureの `Std.Bytes` importを
書き換えずに、本体の型・intrinsicと既存のC runtimeを接続する。
S1〜S6の実行記録は旧pinでの履歴として保持する。v0.26.1での検証は
[依存更新報告](compiler-v0.26.1.md)、組み込みmoduleの管理移管後は
[所有範囲の報告](embedded-library-ownership.md)に分けて記録する。
