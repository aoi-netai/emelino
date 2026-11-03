# 固定source/C観測の再利用

取得元はemela-emb `b0a367bc0a29227fae62cee2def75ed925a5218f`。
各fixtureは同じC観測と容量設定を使い、本体frontendからC実行までを確認する。
fixtureの取得元・hash・変更点は
[初期採用記録](../../../../../docs/reports/reuse-fixtures.json)、各フェーズの採用記録、
[旧並行処理85観測](../../../../../docs/reports/reuse-legacy-runtime.json)、
[追加23観測](../../../../../docs/reports/reuse-legacy-more.json) に記録する。

`cargo test --locked -p emelino-cli --test reused_pipeline` で確認する。
C compilerは必須であり、不在・失敗はskipにしない。`CC`で実体を指定できる。

1. 普通の公開関数を持つsourceを本体で型検査・単相化してFlowへ変換する。
2. configの公開名と容量を使ってCを生成する。
3. sourceを削除し、保存済みBackendInputと容量設定だけでCを再生成する。
4. C/headerの再生成結果を比較し、元のC harnessを-O0/-O2でコンパイル・実行する。

公開C aliasは旧sourceの`@export(c)`からconfigへ移している。本体の構文と異なる表記や
external呼出しは採用記録に列挙した変更を行う。製品frontendでソースを書き換える処理ではない。
生成後に`heap-observer.c`を追加する試験は内部heapの解放観測であり、公開ABIを追加しない。
このobserverは旧`tests/support/heap.rs`内のC文字列をそのまま採用した。

`reference/legacy-flow`の対応表とのID一致を同じtest targetで検査する。
fixtureの成功だけで台帳の全項目をverifiedへ変更することはない。

旧corpusは `legacy_runtime` と `legacy_more` test targetで実行する。
`observations.json` のheader型番号置換・release名取得・取消／heap／child観測は
元のC harnessの方法を保つための試験用処理である。sourceと保存入力のC/headerが一致した後だけ適用する。
元からartifactのみを確認する2例は報告で区別し、Cの値を実行したことにはしない。

固定参照の末尾空行も保持するため、移入fixtureの `git diff --check` は
`core.whitespace=-blank-at-eof` を指定する。source本文とC assertionのhashを報告へ保存する。
