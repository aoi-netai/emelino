# STM32F4 GPIO実装・検証

この文書はGPIO単独実装を完了した時点の履歴記録であり、容量値・試験件数も当時の値を記す。
その後UART・SPI・I2Cと共通pin管理を追加した。現在の対応範囲・検証結果は
[STM32F4 BSP](../../boards/stm32/stm32f4/README.md)を参照する。

2026-11-03。NUCLEO-F446RE向けのGPIOを、Emelaの新しい宣言から生成C、
STM32CubeF4 HAL/CMSIS、firmwareまで接続した。実行手順は
[boards/README.md](../../boards/README.md)に記載する。

## 実装範囲

- `Input` / `OutputPushPull` / `OutputOpenDrain` / `Analog(NoPull)` の設定、read/write/toggle。
- ピンごとのcallback登録、EXTIからISR内で直接呼出し、登録解除と差し替え。
- 型付きGPIOエラー、EXTI lineの競合、共有IRQの優先度競合。
- 登録関数の候補追跡とISR検査、scalar captureの保持・解放。
- ボード・MCU定義、startup/vector/linker、manifestを入力とするfirmware build。

この時点ではUART・I2C・SPIのBSPと残り3ボードは未実装だった。Boardの宣言は型整合のため同期し、
SPI複合操作のC bridgeは未実装として使用時に拒否していた。

## Renode

使用した環境はRenode 1.16.1、xPack ARM GCC 13.2.1、STM32CubeF4 1.28.3。
このPCでは `~/bareruby/prototype/bareruby-prototype/.tools/` 以下に配置されている。
build/testスクリプトにはこの絶対パスを埋め込まず、引数か環境変数で指定する。

| 検査 | 結果 |
| --- | --- |
| raw BINをFlashへ配置してreset vectorから起動 | 成功。PC/SP/VTORの強制設定なし |
| data/bss初期化 | 成功。起動前にRAMを汚してから確認 |
| GPIO設定・状態・エラー | 16項目成功 |
| callback呼出し | 7回成功。IPSRでEXTI ISR内の実行を確認 |
| PushPullの観測出力 | 期待する10遷移と一致 |
| OpenDrainの出力ラッチ | High→Low→Highの3遷移と一致 |
| captureの寿命 | 登録helperがreturnした後もscalar値を取得 |
| 再登録・解除・失敗時の旧登録保持 | 成功 |
| 共有IRQの一方を解除した後の他方 | 継続動作を確認 |
| `-Os` / `-O0` | 両方成功 |

試験firmwareのFlash使用量は `-Os` 6,028 bytes、`-O0` 19,068 bytes。
静的RAMは4,880 bytes、別にstack 16 KiBとguard 256 bytesを予約する。
system allocatorはlinkされていない。
結果は `out/stm32f4-gpio/report.json` と `out/stm32f4-gpio-o0/report.json`、
ELF/BIN・build/Renode logも同じディレクトリに保存する。
ボタン→LEDの実行例も生成Cからfirmwareへのlinkを確認した。

generic STM32F4モデルであり、F446RE実機試験ではない。
SYSCFGのport選択はモデル化されていないため、割り込み入力はPAを使用する。
OTYPERも未実装なので、OpenDrainの指定はbus書込み要求の観測として検証し、
既知の警告1種類をreportに残す。それ以外のwarning/errorは試験を失敗にする。
pull抵抗、open drainの電気的動作、実時間精度は検証対象外。

## コンパイラと回帰試験

新しいGPIO ISR回帰試験では、直接登録に加え、関数引数・戻り値・分岐・loop・集約値・
間接registration helper・scalar captureを通る候補を検査する。
候補の一部が不明・unsafeの場合、heap/再帰/未許可external、ISR内の設定変更、
参照管理を要するcaptureを拒否する。read/write/toggleと型付きエラー処理は許可する。
生成ELFのcallback到達経路にheap確保・retain・releaseがないことも確認した。

compilerの143件と新GPIO ISRの11件が成功。既存CLI試験も実行し、変更後の再検証を含め失敗を解消した。
`cargo fmt --check` と全targetのclippyも成功。
旧APIを使う `p7-embedded` の2つの試験は、採用済みのAPI変更に合わせ、
旧sourceが明示的な診断を返すことを確認する試験へ変更した。
歴史的source/C harness/hashは書き換えていない。
取り込みhashの単体試験も、現行APIのbyte一致ではなく取り込み元の記録を検証する。
