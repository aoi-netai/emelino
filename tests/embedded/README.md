# 共通のfirmware試験

```sh
python3 tools/test_embedded.py --board nucleo-f446re --scenario all
python3 tools/test_embedded.py --board nucleo-f446re --scenario all --optimization O0
python3 tools/test_embedded.py --scenario bmp180 --scenario mt25q
python3 tools/test_embedded.py --board arduino-nano-33-ble --scenario all
python3 tools/test_embedded.py --board arduino-nano-33-ble --scenario all --optimization O0
python3 tests/embedded/support/test_runner.py
python3 tests/embedded/ports/stm32f4/test_time.py
python3 tests/embedded/ports/stm32f4/test_events.py
python3 tests/embedded/ports/stm32f4/test_mapping.py
python3 tests/embedded/ports/nrf52/test_uart.py
python3 tests/embedded/ports/nrf52/test_recovery.py
```

`ARM_GCC`、`STM32CUBE_F4`、`NRFX_ROOT`、`CMSIS_ROOT`、`RENODE`の指定は
[firmwareの手順](../../docs/embedded/README.md)を参照する。
既定の出力先は `out/embedded/<board>/<optimization>/<scenario>/`。
`--output-dir`を指定した場合、単一シナリオはそのディレクトリへ、複数シナリオは
その下のシナリオ名ごとのディレクトリへ出力する。
複数実行では個別reportと全体reportを保存し、1件でも失敗すれば終了コード1を返す。

## 配置と追加の単位

- `scenarios/`：Emelaの操作・期待値。BMP180とMT25Qはデバイスの通信手順もここで管理する。
- `targets/`：試験を実行する基板、適用シナリオ、バス番号・CS・I2Cアドレス・補正モデルの選択。
  容量・クロック・物理配線は `firmware/boards/` と `firmware/chips/` の定義を参照する。
- `support/`：生成Cとfirmwareのビルド、Renode起動、ログ・数値の取得、共通C観測処理。
- `ports/stm32f4/`：STM32モデルの構成、レジスター観測、HAL fault injection、固有の回帰試験。
- `ports/nrf52/`：nRF52840モデルの構成とGPIO・UART・SPI・I2Cの観測。
- `../renode/models/`：相手モデルと補正モデル。旧I2Cモデルだけの回避策は
  `stm32f4-i2c-legacy/`へ分ける。モデル自体のMMIO試験は `../renode/i2c/`で独立させる。

BMP180/MT25Qのバス番号・CS・アドレスはtargetから `inputs/test/target.emel` を生成し、
シナリオが `Test.Target` としてimportする。`main.emel`の文字列置換は行わない。
別配線を試す場合は `--target path/to/target.json` で同じ基板に対する試験設定を指定できる。

`test_mapping.py` は一時的な別基板定義を作り、SPI1のPB3/PB4/PB5・CSのPB0・I2C1のPB6/PB7で
共有シナリオを実行する。GPIOレジスタと基板固有初期化hookも確認する。
結果は `out/embedded/stm32f4-mapping/` に保存し、一時基板定義は終了時に削除する。

GPIO/UART/SPI/I2C/同時使用の既存試験には固定のEXTI番号、AF、HALエラー注入、レジスター値の
検査がある。targetの `port: "stm32f4"` を指定した回帰試験として扱い、
他系列への対応は共通シナリオを追加するか、そのport向け観測処理を実装してから有効にする。

## Arduino Nano 33 BLE

`arduino-nano-33-ble` targetは次の共有シナリオを実行する。

| シナリオ | 検査内容 |
| --- | --- |
| `gpio_callbacks` | ピン別callback、スカラーcapture、ハンドラー置換、共有IRQ優先度の競合、8チャネルの満杯・再利用、解除後の無通知。実際のGPIOTE ISRから8回呼ばれることを検査 |
| `peripheral_contract` | 4サービス同時使用、ピン所有権、UARTのポーリング・割り込み受信・送信通知・破棄・timeout、引数エラー、GPIO ISRとPRIMASK設定中の通信拒否、復旧後の実デバイス通信 |
| `bmp180` | STM32と同じ19項目。実センサーモデルに対する1バイト・複数バイト・連続読み取り、repeated START、割り込み、NACK、復旧 |
| `mt25q` | STM32と同じ14項目。JEDEC ID、分割SPI操作とCS、program・read・erase。全19フレームと独立したFlash内容を観測 |

GPIO/UARTを含む配線もtargetから `Test.Target` として生成する。Cの観測用probeは同じ論理バス番号を
`test_target.h`で受け取る。共有シナリオのソースへボード名・レジスター番地を埋め込まない。
`peripheral_contract`のC probeだけは、禁止コンテキストでABIを直接呼び、即座にInvalidStateが
返ることを検査する。通常のデバイス通信にはBSP置換や応答値の注入を使わない。

Renode 1.16.1はUARTのERRORSRCと一部PSEL、SPIのPSEL・FREQUENCY・CONFIG、
SystemInitが参照する製造時情報のアドレス0x10000130・0x10000134を
モデル化していない。既知の警告は番地・期待する書き込み値を限定して許可し、reportに残す。
SPIの電気的なmode・周波数やUARTの物理的な線上エラーは検証範囲に含まれない。
SPIの設定要求はPSEL・FREQUENCY・CONFIGへの書き込みに加え、SCKのPIN_CNFで
出力設定と入力バッファ接続が共に指定されることを検査する。

初期化はNordicのSystemInitを実行し、PC/SPの直接指定なしでFlashのリセットベクターから開始する。
GPIO試験はdata/bss領域を起動前に汚し、通常の初期化で回復することも検査する。
`ports/nrf52/test_uart.py`は64ワードの受信リング境界、65ワード目のoverflowと回復、
受信timeout、8E2の設定要求、方向変更によるピン解放など20項目を追加検査する。
`ports/nrf52/test_recovery.py`はSPIとTWIのpolling/IRQで完了イベントを各1回だけ欠落させ、
timeout・SPIのCS解除と復旧待ち・recover後のJEDEC/BMP180 ID読出しなど22項目を検査する。
fault hookは復旧前に解除し、デバイス応答値やBSPの戻り値は置換しない。

## I2Cモデルの選択

NUCLEO-F446REのBMP180試験はtargetで修正版STM32 I2Cモデルを明示している。
使用モデルとSHA-256を個別reportへ記録する。
`--controller-model path/to/model.cs`で上書きでき、標準モデルとの比較には
`--stock-controller`を指定する。標準Renode 1.16.1では最初の1バイト読み取りがTimeoutになり、
この比較実行は失敗を返す。

```sh
python3 tools/test_embedded.py --scenario bmp180 --stock-controller \
  --output-dir out/bmp180-stock
python3 tools/test_embedded.py --scenario bmp180 \
  --baseline-report out/bmp180-stock/report.json --output-dir out/bmp180-fixed
```

`--baseline-report`は生成firmwareのBINが同一であることも検査する。
`i2c`回帰試験は従来どおり標準モデルの制約とHALエラー変換を検査する。

Nano 33 BLEのBMP180・同時使用試験は `EmelinoNRF52840I2C.cs` を指定する。
標準モデルの受信先読み、BB_SUSPEND、ERRORSRCのクリア動作を補正し、デバイス固有の値は返さない。
独立したレジスター・割り込み試験は次で実行できる。

```sh
python3 tools/test_renode_i2c_model.py --port nrf52
```
