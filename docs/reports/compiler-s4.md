# S4: 中断・並行処理の接続

2026-11-03。既存のsuspend/frame/task/Scope/Fiber/Deferred処理を本体Flowへ接続した。

本体がUnitをAnyへ消去したcallback wrapperには、文書化された空Stringのplaceholderが
現れる。関数型・capture・call・終端がそのwrapperの契約と一致する場合だけ、backendの
Unit boxへ変換する。普通の空String値をUnitとみなさない。Core関数本体は変更しない。

`reused_concurrency` の3source試験が成功した。通常の非同期external完了とstale通知、
Scope/Fiber/Deferred/Cause、入れ子scopeの取消・cleanup・メモリ回収を、既存のC harnessと
元の固定容量で確認。ソース削除後の再生成と `-O0` / `-O2` 実行も成功、ignoreなし。
元source/C/Optionsと適応差分は [reuse-concurrency-fixtures.json](reuse-concurrency-fixtures.json) に記録。

```sh
CARGO_HOME="$PWD/.local/cargo-home" cargo test -p emelino-cli --test reused_concurrency --locked --offline
```

callback表現の回帰試験はadapter単体にも追加した。多数の旧concurrency観測との
一対一照合はS6で追加し、全台帳の完了判定はそこで行う。
