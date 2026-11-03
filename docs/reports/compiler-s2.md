# S2: 同期データ・メモリの接続

2026-11-03。前回の実装を再利用し、本体の型IDとbackend表現を接続した。

- Int/Int64/Float、String、tuple/record/enum/List、再帰型、Anyの既存処理を使用。
- fieldを持たないenumの比較は同じ公称型であることを確認してI32表現へ接続。
- 固定heap、所有権、Stringの容量・view、深い再帰値の回収を既存C観測で確認。
- Bytesは公式固定版にない旧fork拡張。`Emelino.Bytes` の外部型・8つの明示bindingを
  既存Bytes表現・演算へ変換する。標準ライブラリーの差し替えやintrinsic権限を使わない。
- 保存入力からも同じC/headerを生成する。旧公開名・値・資源容量・C観測を保持。

`reused_pipeline` は8つのsource fixtureと対応表の集合検査、計9テスト成功。
各source fixtureはソース削除後に再生成し、元のC harnessを `-O0` / `-O2` で実行した。
数値境界、external scalar、文字列、再帰型、標準値/trait、Bytes2群、record更新を含む。
入力の文法・型名・external handler調整と元hashは [reuse-fixtures.json](reuse-fixtures.json) に記録。
固定heapの枯渇と不正な型・演算の拒否は再利用backend試験も維持している。

```sh
CARGO_HOME="$PWD/.local/cargo-home" cargo test -p emelino-cli --test reused_pipeline --locked --offline
```

sourceをそのまま受け入れるために本体側の規則を弱めていない。
旧frontendの `Unit` 表記等は本体の `()` へ適応した。旧 `Std.Bytes` は
emelinoが所有する拡張moduleの `Emelino.Bytes` をimportする。
raw Flowのfailureとconcurrency表現の接続差分も次段階の前提として整理した。
