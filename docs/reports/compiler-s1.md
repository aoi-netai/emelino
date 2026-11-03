# S1: 本体Flowと再利用backendの接続

2026-11-03。S1の終了条件を満たした。S2〜S6の機能全体の完了を意味しない。

## 実装

- 固定版backend/contract/semantics/runtimeを再利用。frontend/core/stdは複製しない。
- 本体の標準型検査・単相化・raw Flow検証を使用。公開関数はJSON設定で選ぶ。
- owned Programを保存するBackendInputを導入。schema/producer/stage/ID/構造を検証する。
- Float bit保存、計算metadataの除外、ソースなしの再生成に対応。
- 対応表の428定義・226文脈・115関連試験・10条件をpendingとして移入した。

## 確認

| 試験 | 成功数 | 主な観測 |
| --- | --- | --- |
| 再利用backend単体 | 124 | データ、メモリ、effect、非同期、C ABI等の既存IR経路 |
| BackendInput | 7 | 保存再生成、Float/Int64 bit、未知schema/演算、ID/cycle、偽metadata拒否 |
| S0 profile回帰 | 6 | 旧入力契約と整数境界のC実行 |
| frontend/adapter | 9 | 本体診断、公開入口、型ID変換、call/if/複合型/closure/failure/handler |
| CLI | 6 | 引数、alias、複数module、ソース削除後の再生成、不正設定/入力 |

合計152成功、失敗・ignoreなし。C実行を持つ試験では `-O0` / `-O2` を確認した。
compiler単体のstrict clippyも成功。以後の全体検証ではCLIと追加fixtureも含めて再確認する。

```sh
CARGO_HOME="$PWD/.local/cargo-home" cargo test -p emelino-compiler --locked --offline
CARGO_HOME="$PWD/.local/cargo-home" cargo test -p emelino-cli --lib --test pipeline --locked --offline
```

## 次段階へ引き継ぐ差分

S2以降の既存fixtureも接続し始めているが、本体と独自frontendの構文・external規則の差、
raw Flowのfailure/Anyとbackend表現の差が残っている。検証途中のfixture成功は上表へ加えていない。
公式固定版には旧forkのBytes intrinsicがない。これはUPSTREAM-001とは異なる旧fork拡張であり、
本体無変更の追加moduleと明示的なbinding契約で接続する。未接続を適用外に置き換えない。

出自・除外したfrontend依存の旧試験は[再利用記録](reuse-provenance.md)に残した。
