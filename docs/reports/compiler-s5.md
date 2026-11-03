# S5: const・公開ABI・ISR・組み込みmodule

2026-11-03。前回のruntimeとC ABIを利用し、次を本体Flowへ接続した。

- monomorphic constの一度だけの初期化・defect記憶。
- scalar/Float bit、String/tuple/Listの同期外部adapterの借用・所有契約。
- 管理値のtask完了・取消・producer/release・NULL結果・誤った型/重複完了。
- ISR入口の明示、実adapter symbolの安全指定、heap/再帰/runtime constの拒否。
- Embedded.*の通常moduleと生成サービスheader。旧GPIO/UART/I2C/SPI/Clock/Consoleの
  C mock、nativeテスト用C provider、バッファ境界・エラー・回収の既存観測を維持。
- 公開入口に未処理effectのevidence引数が残る場合は診断する。Cへ暗黙のvoid*を出さず、
  handlerを供給する具体型の公開wrapperを要求する。

`reused_consts` 1、`board_boundary` 5、`legacy_externals` 5、`legacy_services` 3の
各試験が成功。保存後の再生成と生成Cの `-O0` / `-O2` を確認した。
[境界fixture記録](reuse-boundary-fixtures.json)と
[元external/service観測との対応](reuse-external-services.json)へ出自・差分を記録する。

別枠の `upstream_known_issues` 1試験はUPSTREAM-001を実Cで再現する。
型特殊化ごとの2初期化を記録するもので、言語仕様の単一初期化を満たしたという判定にはしない。
const全体を適用外にしない。詳細は[既知問題](../upstream-known-issues.md)を参照。

S6で全169program/174関連観測と対応表を照合する。
汎用callback・資源回収等についてそこで発見した接続差分は解消してから全体完了を判定する。
