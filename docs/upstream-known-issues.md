# 本体依存の既知差分

現在のconst契約は[構成](architecture.md#const)に定義する。以下の旧実行記録は各記載revisionの履歴として保持する。

本体を変更せず、固定した依存版の挙動を受け入れる項目を記録する。
言語仕様の変更や、emelinoが独自に修正済みであるという判定にはしない。

## UPSTREAM-001: 汎化constの特殊化ごとに初期化cacheが分かれる

| 項目 | 状態 |
| --- | --- |
| 判定 | 既知の本体差分として受入済み・未修正 |
| 確認日 | 2026-11-03 |
| 初回確認revision | `ec1031cb0b98386baf0e9b0ced67e8829bb2c529` |
| 現在の本体revision | v0.26.1 `6021115103dd830e0efd9bb96a56a0ca97859329` |
| 初回の実行確認 | 旧pinの本体公開APIによるJS生成、Node v24.21.0で実行 |
| emelinoへの方針 | 本体を変更せず、受信したFlowのconst単位を維持する |

### 仕様と観測

本体仕様の [`[data.const]`](https://github.com/emela-lang/emela/blob/ec1031cb0b98386baf0e9b0ced67e8829bb2c529/spec/language/05-data.md#L92-L98)
は、汎化constも一度だけ初期化し、単一の値を記憶すると定める。
この意味は将来の対応目標として維持する。型ごとの初期化を新しい言語仕様とは定義しない。

一方、この固定版ではconstが型ごとに単相化され、JS出力も関数ごとに別のcacheを作る。
JSはFlowを経由しないが、共通のCore単相化で分かれた初期化がそのまま残る。
JS emitterの `emit.rs` は各constant関数に専用cellを生成し、runtimeの `$constant`
は渡されたcell単位で値とdefectを記憶する。元のconst名は診断用であり、共有キーではない。

再現入力 `probe.emel`:

```emel
const EMPTY = {
  dbg("INIT")
  []
}

fn ints(xs: List[Int]) -> Int { List.length(xs) }
fn strings(xs: List[String]) -> Int { List.length(xs) }

@export(js, "probe")
pub fn probe() -> Int {
  ints(EMPTY) + strings(EMPTY)
}
```

確認では同じrevisionのsyntax・resolve・types・core・codegen-jsの公開APIを使用した。
本体の構文解析・名前解決・型検査の後、`lower(target = "js", Roots::Exported)` による
標準単相化と `verify_monomorphic` を通し、`emela_codegen_js::emit_module` へ渡した。
runtimeは `RuntimeMode::Inline` として `probe.mjs` に含めた。
本体CLIを起動した試験ではなく、同じ本体lowerとJS emitterを呼ぶ実験である。

実行用 `run.mjs`:

```js
import { probe } from "./probe.mjs";
console.log("first:", probe());
console.log("second:", probe());
```

`node run.mjs` の終了コードは0。stdout:

```text
first: 0
second: 0
```

stderr:

```text
"INIT"
"INIT"
```

最初の呼出しで2回初期化され、2回目には追加の初期化出力がない。
生成JSには `EMPTY$Int$cell` と `EMPTY$String$cell` が別々に存在した。
本書に再現入力・生成経路・観測結果を保持し、ローカルの未追跡実験ファイルを根拠の必須条件にしない。
これは確認した固定版のJS経路の結果であり、全targetで実行確認したものではない。
v0.26.1でも標準Core単相化・Flowに共有情報の追加はなく、修正は含まれていない。
その版のC経路の回帰結果は依存更新報告（履歴資料 `docs/reports/compiler-v0.26.1.md` は検証台帳PRで収録）に分けて記録する。
上記のNode実行結果は旧pinでの記録であり、新版でのJS再実行は主張しない。
上流へのissue登録・修正時期は未確定である。

### 現在の受入範囲

- emelinoで由来追跡、追加単相化、共有initializerの生成、名前からのconst統合を実装しない。
- backend移植時は受信Flowのconstごとに初期化・cacheを扱い、特殊化をまたぐ共有を保証しない。
- Cコンパイラーの最適化で初期化回数や共有identityが復元されるとは期待しない。
- 旧forkとの対応判定では本項を例外として別記し、共有動作を `verified` として数えない。
- この既知差分は合意した範囲の移行を妨げないが、新たな差分を自動的に許容する根拠にはしない。

現在は汎化constを含むbackend移植が完了し、以下のC経路の回帰試験を継続している。

### 本体の修正版を採用するとき

1. 本体の修正内容と実際のCore/Flow契約を確認し、共有初期化と型別アクセスの表現を特定する。
2. 本体の5 crate（syntax・resolve・types・core・lir）を同一の修正revisionへ更新し、`Cargo.lock` も更新する。
3. 新しい表現に合わせてFlow変換部・入力形式・backendを必要に応じて修正する。依存更新だけで直るとは仮定しない。
4. 複数型からの一度きり初期化、defectの記憶、所有値の寿命・解放を、本体からC実行まで確認する。
5. 関連する旧試験と通常constの回帰試験を実行し、結果と採用revisionを記録してから本項を解消済みにする。

## emelino接続後の回帰試験

S5で `upstream_known_issues::upstream_001_specialized_consts_keep_two_initializers_until_upstream_fix` を追加した。
実際の本体FlowからCを生成し、最初の呼出しでdebug hookが2回、2度目の呼出しでは追加呼出しが
ないことを `-O0` / `-O2` で確認する。保存後の再生成でも同じ結果になる。
この成功は既知問題の再現確認であり、汎化constの単一初期化仕様への対応済みを意味しない。
修正版へ追従するときに期待値と閉じる条件を更新する。


## S6で照合した旧観測

旧source観測の対応表（履歴資料 `docs/reports/reuse-source-coverage.json` は検証台帳PRで収録） の174件中8件で、
この問題による初期化回数・cache名などの差分を個別に記録した。
const・cleanup等の移行記録（履歴資料 `docs/reports/reuse-source-seams.json` は検証台帳PRで収録） に、元の期待値と今回の観測を保持する。
値・所有権・解放など影響を受けない観測は引き続き検証し、const全体を除外しない。

これらの回帰テストが成功しても、単一初期化の仕様が修正されたことを意味しない。
評価回数だけを測るC側instrumentationは試験専用であり、通常のEmela sourceの副作用契約とは分ける。
本体の修正版に更新するときは、この8件を含めて仕様の期待値へ戻せることを確認する。
