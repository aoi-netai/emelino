# ボード向けfirmware

[STM32F4 / NUCLEO-F446RE](stm32f4/README.md) と
[nRF52840 / Arduino Nano 33 BLE](nrf52/README.md) のGPIO・UART・SPI・I2Cに対応する。
Emelaソースから生成したCを、SDKのデバイス定義・driver、BSP、startup、linker設定と
組み合わせてELF/BINにする。

```text
Embedded.Gpio / BoardGpio
  → 生成C bridge・emela_gpio.h
  → firmware/ports/stm32f4/gpio.c
  → STM32CubeF4 HAL / CMSIS

EXTI IRQ → HAL_GPIO_EXTI_IRQHandler → HAL_GPIO_EXTI_Callback
  → emela_gpio_dispatch_interrupt → ピンに登録されたEmela関数
```

## 構成と追加単位

基板、MCU、SDKとの接続実装を分ける。同じ接続実装を使う基板はCファイルやビルドスクリプトを
コピーせず、基板定義から参照する。SDK自体は同梱せず、インストール先を指定する。

```text
firmware/
  boards/nucleo-f446re/board.json  # MCU・portの選択、配線、クロック方針
  boards/arduino-nano-33-ble/board.json
  chips/stm32f446re.json           # CPU、メモリ、package pin、IRQ vector
  chips/nrf52840.json
  ports/stm32f4/                  # HAL/CMSIS接続、時刻・SDK別ビルド
  ports/nrf52/                    # Nordic MDK/CMSIS接続
  common/                        # SDKに依存しないSPIの状態・複合操作
  common/cortex_m4f/              # 共通reset、linker、runtime入口
  build/                         # manifest検証、profile、ARM GCC・成果物管理
tests/
  embedded/scenarios/            # 通信手順・期待値・Emelaアプリ
  embedded/targets/              # 試験配線、適用シナリオ、使用モデル
  embedded/support/              # 共通runner、build、Renode実行、観測
  embedded/ports/stm32f4/         # レジスタ検査・HALエラー注入・固有回帰試験
  embedded/ports/nrf52/           # Nordicのレジスタ観測と試験接続
  renode/models/                 # 共通peerと試験用コントローラーモデル
tools/build_firmware.py          # ボード名を指定する共通ビルド入口
tools/test_embedded.py           # ボードとシナリオを指定する共通試験入口
docs/embedded/                  # 接続仕様・手順・検証結果
```

| 追加対象 | 主な追加箇所 |
| --- | --- |
| 対応済みMCUを使う基板 | `firmware/boards/<board>/board.json`。固有初期化がある場合だけ `init.c` |
| 同じ系列の別MCU | `firmware/chips/` の定義とportに必要な差分 |
| 別SDK | `firmware/ports/<port>/` のC ABI接続とビルドadapter |
| 別の通信相手 | `tests/embedded/scenarios/` とtargetの接続設定 |

boardは配線を選び、chipはCPU/FPU・容量・アドレス・CMSIS defineを所有する。
portは対応するコントローラーとSDK操作を実装する。未実装のコントローラーは定義に書くだけでは
使用可能にならない。現在のSTM32F4 portはUSART2・SPI1・I2C1とreset HSI 16 MHzに対応する。
stack/guardの予約量はビルド設定で、既定値は16 KiB / 256 bytes。
基板固有初期化は `init.c` に `void emela_board_init(void)` を定義する。
portの時刻初期化後、生成アプリの起動前に呼ばれる。定義がなければ弱い空実装を使う。

共通ビルドはportを選んで実行し、SDKの選択はportへ委ねる。
STM32F4とnRF52はCortex-M4F起動処理とARM GCCのcompile/link・成果物検証も共有する。
Pico SDK・ESP-IDF・Arduino core向けのportは未実装。Nano 33 BLEはNordicのbare-metal接続を使う。
共通C層のSPIはdevice管理、操作列、CSのsetup/hold・解除、複合エラーを扱い、
実際の転送、時刻取得、排他、GPIO操作はportのhookを使う。GPIO callbackの登録表と寿命管理は
生成C側が所有し、port側には複製しない。

## ビルド

共通で必要なものはRust/Cargo、Python 3、ARM GNU Toolchain。
以下はSTM32CubeF4を使用するSTM32の例で、環境変数は手元のインストール先に合わせる。
Nano 33 BLEのSDK指定とサンプルは[nRF52の手順](nrf52/README.md#ビルド)を参照する。

```sh
export ARM_GCC=/path/to/bin/arm-none-eabi-gcc
export STM32CUBE_F4=/path/to/STM32CubeF4-1.28.3
cargo build --locked -p emelino-cli
target/debug/emelino build examples/stm32f4-gpio/main.emel \
  --config examples/stm32f4-gpio/config.json \
  --out-dir out/stm32f4-gpio-example/generated
python3 tools/build_firmware.py --board nucleo-f446re \
  --generated-dir out/stm32f4-gpio-example/generated \
  --output-dir out/stm32f4-gpio-example/firmware
```

サンプルはPA5のLEDを設定し、PC13のボタンのfalling edgeでLEDを反転する。
機械式ボタンのdebounceは行わない。
`firmware.elf` / `firmware.bin` / `firmware.map`、容量・symbol情報、build logを出力する。
ビルド入力は `emela_artifact.json` のファイル一覧と生成headerを正本とする。
`firmware.json` は成功後だけ書き、出力のSHA-256とtoolchain・SDK情報を記録する。
書き込みコマンドは今回の実装に含めない。

UART・SPI・I2Cを同時に設定する[サンプル](../../examples/stm32f4-peripherals/main.emel)もある。
上記コマンドの `examples/stm32f4-gpio` を `examples/stm32f4-peripherals` に置き換えるとビルドできる。
USART2を115200 baudで設定し、SPI1のCSをPA4、I2C1を100 kHzに設定して `READY\n` を送信する。
接続デバイス固有のコマンドは送らない。SPI1のSCKとLD2はともにPA5なので同時にGPIO LEDとしては使えない。

| 操作 | 初期配線 | 実装内容 |
| --- | --- | --- |
| [UART](stm32f4/uart.md) | USART2: TX PA2 / RX PA3 | 送受信、部分受信、破棄、polling / IRQ受信buffer |
| [SPI](stm32f4/spi.md) | SPI1: PA5 / PA6 / PA7、CSは別途指定 | Master、8/16 bit、Hz上限、device別設定、複合操作、recover |
| [I2C](stm32f4/i2c.md) | I2C1: SCL PB8 / SDA PB9 | Master、任意prefix＋Repeated START、memory_write、ready、recover |

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
python3 tools/test_embedded.py --board nucleo-f446re --scenario all
python3 tools/test_embedded.py --board nucleo-f446re --scenario all \
  --optimization O0 --output-dir out/embedded-o0
python3 tools/test_embedded.py --board nucleo-f446re --scenario bmp180
python3 tools/test_renode_i2c_model.py
python3 tests/firmware/test_build.py
python3 tests/firmware/test_spi_common.py
python3 tests/embedded/support/test_runner.py
python3 tests/embedded/ports/stm32f4/test_time.py
python3 tests/embedded/ports/stm32f4/test_events.py
python3 tests/embedded/ports/stm32f4/test_mapping.py
```

試験はEmelaの[テストアプリ](../../tests/embedded/scenarios/gpio/main.emel)を毎回Cへ変換し、SDKとlinkし、
raw BINをFlashへ配置してreset vectorから起動する。GPIO入力を注入し、出力変化と
EXTI割り込み内のcallback、再登録・解除、競合エラー、captureの寿命を検査する。
`report.json` とRenode logを指定先へ保存する。
UART・SPI・I2Cと同時使用試験も同じ方法で生成Cから起動し、共通コマンドで `--optimization O0`
を指定できる。個別試験はデータ転送とエラー処理、同時使用試験は全driverのlinkとピン所有権を検査する。
host側の `peripheral_bridge` 試験は複合SPIのC表現・出力長・複合エラー・heap解放も検査する。

`--scenario` は複数回指定できる。単一シナリオの `--output-dir` はそのまま使い、複数の場合は
配下をシナリオごとに分ける。BMP180・MT25Qのアプリはtargetから生成するテスト設定moduleを参照し、
バス番号やCSピンの違いでソースをコピーしない。STM32F4のレジスタやHALに依存する検査は
対応portを持つtargetに限定する。モデルの警告例外や未対応動作は共通APIの仕様に含めない。

`test_mapping.py` は一時的な基板定義を作り、SPI1をPB3/PB4/PB5、CSをPB0、I2C1をPB6/PB7に
変更して同じBMP180・MT25Qシナリオを実行する。GPIOのAF・クロック設定と基板初期化hookも検査し、
試験後に一時定義を削除する。対応ボードの追加や実機のピン配線確認を意味するものではない。

Renodeのgeneric STM32F4モデルを使うため、F446RE実機の検証とは区別する。
SYSCFGのport選択、pull抵抗、電気的なopen drain動作・timingの検証範囲は
生成されるreportの `model_limitations` とBSPのREADMEに記録する。
標準Renode 1.16.1のI2Cでは、SR2単独readによるADDR解除と不正なBTF遷移が1バイト受信を妨げる。
これを修正した試験用コントローラーモデルと標準BMP180を接続し、同じfirmwareで1バイト受信を確認した。
BMP180ではtarget設定が修正版を選択し、reportへモデル名とhashを記録する。
`--controller-model` で別のモデルを指定でき、`--stock-controller` で同梱モデルとの比較を行える。
インストール済みRenodeは変更しない。

2026-11-04の[実デバイスとの通信試験](stm32f4/bmp180-test.md)では、BMP180のID、校正値、
24.0℃／32.0℃の読み取り、圧力の未補償値、NACKを含む19項目が `-Os` / `-O0` で成功した。
[SPIのMicron MT25Q試験](stm32f4/spi-device-test.md)も、標準モデルのまま
ID・書込み・読み戻し・消去を含む14項目と19回のCS区間に成功した。
I2Cの修正版モデル自体はHALを使わない[122項目のMMIO試験](../../tests/renode/i2c/README.md)で確認する。

## 構成移行後の確認

2026-11-04に、新しい共通入口からGPIO・UART・SPI・I2C・同時使用・BMP180・MT25Qの
7シナリオを `-Os` / `-O0` で実行し、14実行すべて成功した。
結果は `out/layout-final-Os/` と `out/layout-final-O0/` に保存した。
標準I2Cモデルでは従来どおり最初の1バイト受信がTimeoutとなり、修正版とのfirmware BIN一致も確認した。

共通SPIはSTM32に依存しない2バスのfake portで6ケースを両最適化＋UBSanで検査した。
build設定・失敗処理7件、runner4件、時刻・I2Cイベント処理2件、モデルのMMIO122項目も成功した。
別配線の一時基板では、同じBMP180・MT25Qアプリの通信、SPIピンのAF・クロック設定、
基板初期化hookの呼出しを確認した。

## 旧APIの試験

以前の6組み込みmoduleと `p7-embedded` のsource/hash/C harnessは履歴として保持する。
新GPIO callback、I2C読み取り統合、SPI Hz指定を採用したため、その旧sourceは
現在の宣言ではコンパイルできない。該当する2つの試験は旧APIの診断を確認する形へ変更し、
新GPIOの実行はISR回帰試験と上記Renode試験で検証する。
新UART・I2C・SPIは上記試験で検証する。SPI複合操作には専用のC表現を設け、
転送時の主エラーとCS解除時のエラーを両方保持する。
