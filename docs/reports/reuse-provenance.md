# backend・対応表・試験資産の再利用元

## 固定した取得元

| 資産 | revision | 採用範囲 |
| --- | --- | --- |
| 前回のコンパイラーsource | emela-emb `b0a367bc0a29227fae62cee2def75ed925a5218f` | backend、contract、semantics、writer、artifactと必要な試験資産 |
| 前回の最終対応表 | emela-emb `2258716bab2e9e4bbaa5a2fd135f6999636125e2` | `docs/reference/legacy-flow/`、台帳のID・関連情報 |
| 比較する旧fork | `c34c0aeefdd966d854371b3110db00a47798751d` | 旧Flow定義・実装・関連試験の固定基準 |
| 現在の本体frontend | 公式Emela `ec1031cb0b98386baf0e9b0ced67e8829bb2c529` | 公開frontend API、標準単相化、Flow生成、core/std |

コードと資料は固定したGit objectから取得し、元checkoutの未コミット変更を含めない。
Apache-2.0のLICENSEと既存の出自・著作権表示を保持する。
対応表の各ファイルのhashは [取得記録](reuse-reference-import.json) に保存する。
既存の対応表は固定参照であり、今回の進捗は [新台帳](../compiler-coverage.json) に記録する。

## 採用する境界

旧独自frontend、source loader、同梱core/stdを新repoへ複製しない。
本体crateに依存する接続部はCLI crateに置き、compiler crateは所有するProgramとOptionsを受け取る。
backend内のlower・plan・emit・runtime実装は再利用し、接続に必要な修正を機能別に記録する。

前回の607テスト、verified、独立frontendの受理結果は今回の成功として継承しない。
本体への委譲によって不要になるテスト実装も、必要な言語機能の観測自体は新経路へ移す。
IR単体試験とsource試験は分け、元frontendを呼ぶ試験を無効化しただけで検証済みにしない。

backend採用commitは `021cf01`。`src/{backend,contract,semantics}/`、
`src/{writer,artifact}.rs`、Unicode公式vector、LICENSEを採用した。
旧独自frontendを直接呼ぶ次の7試験moduleはコピーせず、source経路への移行を残している。

- `backend/lower/fiber_source_tests.rs`
- `backend/lower/p7_cause_source_seam_tests.rs`
- `backend/lower/p7_cleanup_source_seam_tests.rs`
- `backend/lower/p7_const_source_tests.rs`
- `backend/lower/p7_integration_tests.rs`
- `backend/lower/p7_recursive_source_tests.rs`
- `backend/lower/source_builtin_tests.rs`

これらだけが使う `backend/lower/p7_source_seams.rs` も未採用である。
この未移行分は検証済み件数へ含めない。採用したfrontend非依存のbackend試験124件は
新compilerで成功し、失敗・ignoreは0件。保存形式commit `68a6ab7` の7試験と
既存最小profileの6試験も成功した。これはbackend側の証拠であり、本体source経路の全件成功ではない。

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
