# S3: 関数値・effect・失敗の接続

2026-11-03。既存のclosure/handler/failure処理を再利用し、本体raw Flowとの接続を確認した。

- closure captureと高階呼出し、管理値の所有権を前回のC harnessで確認。
- effect/handler/layer、label、失敗する初期化とcleanupの実行順を維持。
- raw Flowが使う具体error値からAny失敗blockへのbreakを、既存のerror表現へ接続。
- Cause/Exitのerror rowは既存のruntime ABI射影をbackendへ移し、Core本体を書き換えない。
- closure・handler loweringはbackendに一度だけ通す。

`reused_effects` の3試験は、source→本体→Flow→C、保存入力からの同一C再生成、
既存C harnessの `-O0` / `-O2` 実行をすべて成功した。失敗・ignoreなし。
正常値・失敗payload・回復・関数値のcapture・cleanup・メモリ回収を観測する。
source調整・元hashは [reuse-effects-fixtures.json](reuse-effects-fixtures.json) に記録。

```sh
CARGO_HOME="$PWD/.local/cargo-home" cargo test -p emelino-cli --test reused_effects --locked --offline
```

固定旧fork由来の全source corpusとの一対一照合はS6で行う。
この段階の機能試験だけでその台帳を一括verifiedへ変更しない。
