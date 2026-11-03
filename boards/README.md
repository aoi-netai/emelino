# ボード向けfirmware

最初の実装は [STM32F4 / NUCLEO-F446RE](stm32/stm32f4/README.md) のGPIO・UART・SPI・I2C。
Emelaソースから生成したCを、STM32CubeF4のHAL・CMSIS、BSP、startup、linker設定と
組み合わせてELF/BINにする。ほかのボードのdriverは未実装。

```text
Embedded.Gpio / BoardGpio
  → 生成C bridge・emela_gpio.h
  → boards/stm32/stm32f4/gpio.c
  → STM32CubeF4 HAL / CMSIS

EXTI IRQ → HAL_GPIO_EXTI_IRQHandler → HAL_GPIO_EXTI_Callback
  → emela_gpio_dispatch_interrupt → ピンに登録されたEmela関数
```

`common/` は生成artifactのmanifest読み取り、`stm32/stm32f4/` はfamilyのdriverとbuild、
その下の `boards/` と `chips/` は基板配線とMCU容量・pin・vectorの定義を持つ。
ほかのfamilyは `boards/` 直下へpackageを追加し、周辺機器はfamily内でdriverを分ける。
SDK自体はこのリポジトリへコピーせず、インストール済みのディレクトリを指定する。

## ビルド

必要なものはRust/Cargo、Python 3、ARM GNU Toolchain、STM32CubeF4。
実行例の環境変数は手元のインストール先に合わせる。

```sh
export ARM_GCC=/path/to/bin/arm-none-eabi-gcc
export STM32CUBE_F4=/path/to/STM32CubeF4-1.28.3
cargo build --locked -p emelino-cli
target/debug/emelino build examples/stm32f4-gpio/main.emel \
  --config examples/stm32f4-gpio/config.json \
  --out-dir out/stm32f4-gpio-example/generated
python3 boards/stm32/stm32f4/build.py \
  --generated-dir out/stm32f4-gpio-example/generated \
  --output-dir out/stm32f4-gpio-example/firmware
```

サンプルはPA5のLEDを設定し、PC13のボタンのfalling edgeでLEDを反転する。
機械式ボタンのdebounceは行わない。
`firmware.elf` / `firmware.bin` / `firmware.map`、容量・symbol情報、build logを出力する。
ビルド入力は `emela_artifact.json` のファイル一覧と生成headerを正本とする。
`firmware.json` は成功後だけ書き、出力のSHA-256とtoolchain・SDK情報を記録する。
書き込みコマンドは今回の実装に含めない。

UART・SPI・I2Cを同時に設定する[サンプル](../examples/stm32f4-peripherals/main.emel)もある。
上記コマンドの `examples/stm32f4-gpio` を `examples/stm32f4-peripherals` に置き換えるとビルドできる。
USART2を115200 baudで設定し、SPI1のCSをPA4、I2C1を100 kHzに設定して `READY\n` を送信する。
接続デバイス固有のコマンドは送らない。SPI1のSCKとLD2はともにPA5なので同時にGPIO LEDとしては使えない。

| 操作 | 初期配線 | 実装内容 |
| --- | --- | --- |
| [UART](stm32/stm32f4/uart.md) | USART2: TX PA2 / RX PA3 | 送受信、部分受信、破棄、polling / IRQ受信buffer |
| [SPI](stm32/stm32f4/spi.md) | SPI1: PA5 / PA6 / PA7、CSは別途指定 | Master、8/16 bit、Hz上限、device別設定、複合操作、recover |
| [I2C](stm32/stm32f4/i2c.md) | I2C1: SCL PB8 / SDA PB9 | Master、任意prefix＋Repeated START、memory_write、ready、recover |

ピン予約は4 driverで共有し、競合は型付きエラーになる。未配線のcontroller IDは `Unsupported`。
UARTのRTS/CTSとSPI Slaveは初期配線に含めない。
UART・I2Cの割り込み設定は内部受信処理・HAL state machineを有効にする。
これらの公開操作にはcallback引数がないため、ユーザーへのイベント配送APIは追加していない。

## GPIO callbackの契約

登録された関数はISR内で直接呼ばれる。コンパイラは登録口に到達する関数値の全候補を追い、
既存ISR検査を適用する。手動の `isr_entries` 列挙は不要。
候補不明、heap操作、再帰、suspend、未許可externalなどは拒否する。
`BoardGpio` の `read` / `write` / `toggle` はISRで使え、型付きエラーをcallback内で処理する。
設定変更・登録・解除はmain contextで行う。

callbackの環境は登録時に保持し、置換・解除時に解放する。scalar captureも使える。
ISRへのdispatchでは保持・解放・確保を行わない。登録変更とdispatchはBSPのcritical sectionで
排他し、登録失敗時は以前のcallbackを維持する。未登録pinへの通知は無視する。
生成側の登録表は最大32件（`EMELA_GPIO_INTERRUPT_CAPACITY`）。満杯なら`Busy`。
STM32F4ではさらにEXTIの物理line・共有IRQの制約を検査する。
マルチコアのBSPを追加する際は、全dispatch実行主体を登録変更から排他する必要がある。

## Renode試験

Renodeをインストールし、上記の環境変数に加えて実行ファイルを指定する。

```sh
export RENODE=/path/to/renode
python3 tools/test_stm32f4_gpio.py
python3 tools/test_stm32f4_gpio.py --optimization O0 --output-dir out/stm32f4-gpio-o0
python3 tools/test_stm32f4_uart.py
python3 tools/test_stm32f4_spi.py
python3 tools/test_stm32f4_i2c.py
python3 tools/test_stm32f4_peripherals.py
```

試験はEmelaの[テストアプリ](../tests/boards/stm32f4/main.emel)を毎回Cへ変換し、SDKとlinkし、
raw BINをFlashへ配置してreset vectorから起動する。GPIO入力を注入し、出力変化と
EXTI割り込み内のcallback、再登録・解除、競合エラー、captureの寿命を検査する。
`report.json` とRenode logを指定先へ保存する。
UART・SPI・I2Cと同時使用試験も同じ方法で生成Cから起動し、各スクリプトで `--optimization O0`
を指定できる。個別試験はデータ転送とエラー処理、同時使用試験は全driverのlinkとピン所有権を検査する。
host側の `peripheral_bridge` 試験は複合SPIのC表現・出力長・複合エラー・heap解放も検査する。

Renodeのgeneric STM32F4モデルを使うため、F446RE実機の検証とは区別する。
SYSCFGのport選択、pull抵抗、電気的なopen drain動作・timingの検証範囲は
生成されるreportの `model_limitations` とBSPのREADMEに記録する。
特にgeneric I2Cモデルは1バイト受信のADDR/BTF遷移を再現しないため、その成功はRenodeでは未検証。
公開仕様は `read_size = 1` を引き続き受け付け、HALの1バイト受信処理へ渡す。

## 旧APIの試験

以前の6組み込みmoduleと `p7-embedded` のsource/hash/C harnessは履歴として保持する。
新GPIO callback、I2C読み取り統合、SPI Hz指定を採用したため、その旧sourceは
現在の宣言ではコンパイルできない。該当する2つの試験は旧APIの診断を確認する形へ変更し、
新GPIOの実行はISR回帰試験と上記Renode試験で検証する。
新UART・I2C・SPIは上記試験で検証する。SPI複合操作には専用のC表現を設け、
転送時の主エラーとCS解除時のエラーを両方保持する。
