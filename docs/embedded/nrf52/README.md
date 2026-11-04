# Arduino Nano 33 BLE / nRF52840

`arduino-nano-33-ble` はGPIO・UART・SPI・I2Cの生成C ABIをnRF52840へ接続する。
Nordic nrfxのMDK（レジスタ定義、CMSIS device、SystemInit）とArm CMSIS Coreを使用する。
Arduino Mbed OSやSoftDeviceは使用しない。通信処理はベンダーのレジスタ定義を使う
bare-metalの実装で、Renode専用の分岐やデバイス応答の代入はない。

## 共通化の境界

- 基板固有の情報は `firmware/boards/arduino-nano-33-ble/board.json` の1ファイル。
  配線と利用可能なピンを定義する。MCU容量・IRQ表は `firmware/chips/nrf52840.json`。
- `firmware/ports/nrf52/` はNordicとの接続、ピン所有権、GPIO・UART・SPI・I2C、時刻処理。
- SPIデバイス表、設定切替、複合転送、CSのsetup/hold・解除、エラー保持は
  STM32と同じ `firmware/common/spi.c` を使用する。
- Cortex-M4Fのreset、data/BSS初期化、linker、runtime入口は
  `firmware/common/cortex_m4f/`、GCCのcompile/link・容量・vector検証は
  `firmware/build/arm_gcc.py` をSTM32と共有する。
- GPIO callbackの登録表・captureの寿命・ISR検査は既存のコンパイラと生成Cを使用する。
- センサーBMP180とフラッシュMT25Qは既存のEmelaアプリ・C probe・Python試験を使用し、
  target設定とnRF52用のレジスタ観測だけを追加する。

## ピンと対応範囲

`GpioPin` はArduinoのD番号ではなく、**P0.n = n、P1.n = 32 + n**。
ピン対応は[Arduino公式variant](https://github.com/arduino/ArduinoCore-mbed/blob/main/variants/ARDUINO_NANO33BLE/variant.cpp)
と[pins_arduino.h](https://github.com/arduino/ArduinoCore-mbed/blob/main/variants/ARDUINO_NANO33BLE/pins_arduino.h)に基づく。

| 公開ID | コントローラー | 既定配線 |
| --- | --- | --- |
| `Uart0` | legacy UART0 | TX D0/P1.03=35、RX D1/P1.10=42 |
| `Spi1` | legacy SPI2 | SCK D13/P0.13=13、MOSI D11/P1.01=33、MISO D12/P1.08=40 |
| `I2c1` | legacy TWI0 | SCL A5/P0.02=2、SDA A4/P0.31=31 |
| SPI CSの例 | GPIO | D10/P1.02=34 |
| GPIOの例 | GPIO/GPIOTE | 緑LED P0.16=16（active-low）、外部入力 D2/P1.11=43 |

SPI2とTWI0は別のハードウェアブロックなので同時に使用できる。
D13の通常LEDはSPIクロックと共用のため、同時使用サンプルでは緑LEDを使う。
利用可能なピンは基板定義で制限し、GPIO・UART・SPI・I2C間の競合は`ResourceConflict`を返す。
この表以外の公開controller IDは`Unsupported`。

| 機能 | 実装範囲 |
| --- | --- |
| GPIO | Input、PushPull、OpenDrain、pull、read/write/toggle。AnalogはAIN対応ピンの入力切断・NoPullのみで、ADC変換ではない |
| GPIO割り込み | Rising/Falling/Both、最大8ピン。GPIOTEは共有IRQなので同時登録の優先度を揃える。相違・channel不足は`ResourceConflict` |
| UART | 8 bit、parityなし/even、1/2 stop、flow controlなし、`Native` sampling。1200〜1 Mbaudの設定値から最寄り（誤差3%以内）を選択 |
| UART受信 | exact/some/discard、pollingまたは64語のIRQ受信buffer。送信完了待ちもIRQへ切替可能 |
| SPI | Master、8 bit、TwoLine/TwoLineReceiveOnly、mode 0〜3、MSB/LSB。125 kHz〜8 MHzで指定上限以下の最大値を選択 |
| I2C | 7 bit master、Standard duty、100/250/400 kHzから要求以下の最大値。任意prefix＋Repeated START、pure read、memory_write、address-only ready |
| 復旧 | SPIの未読RXD処理、disable/reconfigureと全CS解除。I2Cはcontroller再初期化、必要時最大9 SCL pulseとSTOP、期限付き |

UARTの7/9 bit・odd parity・RTS/CTS・固定oversampling、SPIのslave・16 bit・OneLine、
I2CのFast16To9など、対応外の設定は黙って置換せず`Unsupported`を返す。
GPIOのread/write/toggleはISR内で利用可能。設定変更・登録変更はmain contextで行う。
UART・SPI・I2Cはmain contextかつ割り込み有効で同期実行し、呼出し全体にtimeoutを適用する。

SPIのSCKは出力時も入力bufferを接続する。また、READYとは別に未読受信の有無を保持し、
完了イベントが失われた場合も`recover`でRXDを処理する。これらは
[NordicのSPI仕様](https://docs.nordicsemi.com/bundle/ps_nrf52840/page/spi.html)の
ピン設定と受信bufferの動作に基づく。

## ビルド

検証用SDKは[Nordic nrfx v3.2.0](https://github.com/NordicSemiconductor/nrfx/tree/v3.2.0)と
[CMSIS 5.9.0](https://github.com/ARM-software/CMSIS_5/tree/5.9.0)。SDKはリポジトリへ同梱しない。
`NRFX_ROOT`は`mdk/`を含むroot、`CMSIS_ROOT`は`CMSIS/Core/Include/`を含むrootを指定する。

```sh
export ARM_GCC=/path/to/bin/arm-none-eabi-gcc
export NRFX_ROOT=/path/to/nrfx-3.2.0
export CMSIS_ROOT=/path/to/CMSIS_5-5.9.0
cargo build --locked -p emelino-cli
target/debug/emelino build examples/nano33ble-peripherals/main.emel \
  --config examples/nano33ble-peripherals/config.json \
  --out-dir out/nano-example/generated
python3 tools/build_firmware.py --board arduino-nano-33-ble \
  --generated-dir out/nano-example/generated --output-dir out/nano-example/firmware
```

`--sdk-dir`は`NRFX_ROOT`を上書きする。既定clockはreset時のHFINT 64 MHz。
SystemInitを実行し、1 MiB Flash・256 KiB RAM、stack 16 KiB・guard 256 bytesを使用する。
成果物はELF/BIN/MAP、resource・symbol・SDK入力hashを含むJSON、build log。

このBINは**Flash 0からの独立起動用**で、ArduinoのUSB bootloader経由でuploadする形式ではない。
実機でFlash 0へ書き込むと既存bootloaderを上書きするため、SWD等での書込み・復元を別途準備する。
今回の作業では実機への書込みは行わない。USB CDC・BLE・Arduino coreとの共存は対象外。

## Renodeでの検証

```sh
export RENODE=/path/to/renode
python3 tools/test_embedded.py --board arduino-nano-33-ble --scenario all
python3 tools/test_embedded.py --board arduino-nano-33-ble --scenario all --optimization O0
python3 tests/embedded/ports/nrf52/test_uart.py
python3 tests/embedded/ports/nrf52/test_uart.py --optimization O0
python3 tests/embedded/ports/nrf52/test_recovery.py
python3 tests/embedded/ports/nrf52/test_recovery.py --optimization O0
python3 tools/test_renode_i2c_model.py --port nrf52 --output-dir out/nano-model
python3 tests/firmware/test_nrf52_build.py
```

Renode 1.16.1同梱nRF52840モデルを基に、UART0をlegacy modeとして登録する。
GPIO/GPIOTE・SPI・UART・相手デバイスは同梱モデルを使用する。
Flashへraw BINを配置し、PC/SPを書き換えずreset vectorから起動する。

I2Cは[試験用TWI修正モデル](../../../tests/renode/models/EmelinoNRF52840I2C.cs)を使う。
BB_SUSPEND/BB_STOP、必要数だけの`Read(1)`、Repeated START、ERRORSRCのW1Cをモデル化する。
センサー固有の値やfirmware内の状態には依存しない。独立したMMIO試験では別のcounting peerで
読み取り数・prefix・STOP・IRQ・NACK・resetを検査する。
`--stock-controller`でRenode同梱I2Cモデルと同じfirmwareを比較できる。
同梱モデルでは存在しないアドレスへの`is_device_ready`が誤って成功することを、
同一BINで確認した。修正版ではBMP180の19項目すべてが成功する。

試験の範囲はGPIO callback、全4driver同時使用、UART送受信とエラー、BMP180のID・校正・温度・
圧力未補償値・NACK・復旧、MT25QのID・書込み・読み戻し・消去・CS区間。
CPU実行とレジスタ通信の検証であり、実機の電気的timing、pull-up、clock stretching、
flashの書込み待ち時間・寿命、USB/BLEは検証しない。

2026-11-04、Renode 1.16.1で次を確認した。

| 試験 | 結果 |
| --- | --- |
| GPIO callback | 10項目・8回の実ISR呼出し。`-Os` / `-O0`で成功 |
| 4ペリフェラル同時使用 | 24項目。両最適化で成功 |
| BMP180 | 19項目、30トランザクション、4 NACK、温度24.0℃/32.0℃。両最適化で成功 |
| MT25Q | 14項目、19 CS区間、実フラッシュモデルの書込み・読み戻し・消去。両最適化で成功 |
| UART境界・エラー | 64語の受信buffer、65語目のoverflowと復帰、timeoutなど20項目。両最適化で成功 |
| 異常後の復旧 | SPI・I2C polling/IRQの完了イベント欠落、約2 msのtimeout、CS解除、復旧後の実デバイスモデル通信など22項目。両最適化で成功 |
| TWIモデル独立試験 | firmwareを使わない114項目が成功 |
| 配布サンプル | reset vectorから起動、UARTの`READY`、D2入力のfalling edgeによる緑LED反転が成功 |
| STM32回帰 | 既存7シナリオが`-Os`で成功。共通起動処理・build抽出前後のBINも7本すべて一致 |

結果は`out/nano-final-Os/`、`out/nano-final-O0/`、`out/nano-verification.json`、
`out/embedded/arduino-nano-33-ble/{Os,O0}/uart_regression/`、`out/nano-model/`、
`out/nano-recovery-{Os,O0}/`、`out/nano-example/`、`out/nano-stm32-regression/`に保存した。

復旧試験では、相手モデルが応答した後のREADY/RXREADYイベントを各1回だけ消す。
I2Cでは同時に立つSTOPPEDも消し、復旧前に注入hookを解除する。
故障を明示的に作る試験であり、BSPの差替えやデバイス応答値の注入は行わない。
