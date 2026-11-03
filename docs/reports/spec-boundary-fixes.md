# 仕様監査で見つかった入力・C名の検証不足を修正する

監査対象は `3f1458e64ea3ed0f55acc88f7f1b96d20f9eef53`。
既存377試験は成功したが、追加の境界確認で以下の2件を再現した。
どちらもemelino側の検証不足であり、本体由来のUPSTREAM-001とは別の修正対象である。
本体v0.26.1の固定revision、保存形式のschema、組み込みライブラリーの所有範囲は維持する。

## 保存入力の関数型

`FnType.sets.errors/effects` がInt型を指す保存入力をenumのfield経由で型置換すると、
通常の入力診断を返さず `fn_with_sets` 内でpanicしていた。CLI終了コードは101だった。
IDが範囲内にあっても、集合を要求する場所に別の型がある入力は不正である。

`BackendInput::new` と `from_json` が共通で使う入力検証で、E/Rの参照先が
`Type::Set` であることを確認する。型置換へ進む前に通常診断を返し、CLIは終了1とする。
汎化した宣言の `SetSummary.params` 内の `Type::Param` は引き続き許可する。
関数型のflagsは未解決集合の保守的な近似であり、集合との単純な一致を要求しない。

## 非同期adapterのC名

task adapterへ `emela_scheduler_run` を指定すると、C生成は成功するが、
runtimeとadapterの異なる宣言が同名で出力され、Cコンパイルが失敗していた。
公開関数 `run` に対する `run_start` の指定でも、生成wrapperとの衝突を見逃していた。

adapterの識別子構文に加え、予約名と生成されるCの宣言名との衝突を出力前に検査する。
公開入口由来の補助名も検査し、同期・非同期のexternalと組み合わせた場合にも
同じC名前空間で判定する。信頼済みの組み込みサービスの接続は維持する。
生成しない条件付きhelper名まで予約しないため、helperが不要なプログラムで
`run_unwind` をadapter名にする正常例も引き続き使える。

レビュー中、異なるsuspend externalに同じadapter名を指定すると、同じ
`<adapter>_push` を二重生成するケースも確認した。生成名の登録時に重複を診断し、
宣言の順序によって見逃さないようにする。現時点では異なるsuspend externalに別々のadapter名が必要である。

## 終了条件と検証

- 不正なE/Rそれぞれを、メモリー上のProgram・保存JSON・CLI再生成で通常診断する。
- 正当な集合を含む宣言型と保存後のコンパイルを維持する。
- runtime名・公開wrapper名の衝突を拒否し、エラー時に出力を作らない。
- 正常なtask adapterをソースから接続し、保存後の再生成とC O0/O2実行を確認する。
- workspace全試験、fmt、strict clippyを実行する。

実装・回帰試験は次の機能別コミットに分けた。

- `1a30c20`: 保存入力の関数型が不正な集合参照でpanicしないよう検証する。
- `ced5789`: 非同期adapterのC名と生成される補助関数の衝突を診断する。

`backend_input` の9試験、`pipeline` の8試験、`adapter_symbols` の6試験が成功した。
新規試験は合計9件で、正常なgeneric集合の保存・復元、条件付きhelper名の利用、
正常taskの32回の中断・完了・解放を含む。生成CはO0/O2で実行した。
既存のボード境界7件、external5件、service3件も成功した。

独立レビューでも型検証の順序と汎化集合の契約を照合し、task helperと公開同期名の
衝突を宣言の両登録順で拒否することを確認した。主エージェントは監査時の元の入力を
CLIで再実行し、不正FnSets・runtime名衝突・公開wrapper名衝突がすべて終了1、
panicなし、出力ディレクトリなしになることを確認した。

製品・試験コードの検証commitは `ced5789af9b06538ab49bda88f12d2cf446bcae3`。
以降の報告・台帳更新は製品・試験コードのtreeを変更しない。

| 検証 | 結果 |
| --- | --- |
| `cargo test --workspace --locked --offline` | **386成功（CLI 244、compiler 142）、失敗・ignore・filteredは0** |
| `cargo fmt --all -- --check` | 成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 成功 |
| 台帳更新後の対応表整合試験 | 2件成功 |
| 監査時の元のCLI再現入力 | 3ケースとも終了1、panicなし、出力なし |
| 独立レビュー | 正当な集合型と内蔵serviceを維持、生成名衝突を両登録順で診断 |

workspace試験ログのhashとコードtreeは `compiler-coverage.json` の
`boundary_validation_fix` / `completion` に保持する。
今回は新規checkoutでの全試験・ネットワークからの依存取得・実機検証は行っていない。

## 対応表の扱い

固定参照表の428定義・226文脈・115代表試験・10要件は増減させない。
FnSets関連定義と入力検証・C ABI要件へ、今回の回帰試験を追加の証拠として接続する。
以前の完了記録は当時の試験結果として保存し、今回の修正・実行結果を別に記録する。

今回の2件の解消は、任意の不正入力や全言語仕様についての形式的な証明ではない。
UPSTREAM-001の8観測、Pome/driver・物理BSP・実機確認の後続範囲も変更しない。
