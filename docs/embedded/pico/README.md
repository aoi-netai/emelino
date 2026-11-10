# Raspberry Pi Pico / Pico 2

[検証結果](validation.md)：両ボードの20構成でELF/BIN/UF2生成を確認した。
rp2350jsで確認できた機能と、素SDKでも再現したモデル制約を分けて記録している。

`raspberry-pi-pico` と `raspberry-pi-pico-2` は共通の `pico_sdk` portで
GPIO・UART・SPI・I2Cの生成C ABIをPico SDKへ接続する。
SDKの通常のCMake・startup・linkerを使用し、core 0の `main()` から生成アプリを起動する。
アプリのsetupが終了した後もGPIO購読と割り込みを維持する。

| board ID | MCU / CPU | SDK設定 | Flash / SRAM |
| --- | --- | --- | --- |
| `raspberry-pi-pico` | RP2040 / Cortex-M0+ | `pico` / `rp2040` | 外付け2 MiB / 264 KiB |
| `raspberry-pi-pico-2` | RP2350A / Cortex-M33 | `pico2` / `rp2350-arm-s` | 外付け4 MiB / 520 KiB |

Pico W・Pico 2 W、RP2350のRISC-V、multicore、USB CDC、Wi-Fi、実機への書込みは対象外。
既存の公開APIを接続し、PIO・DMA・PWM・ADCの公開APIは追加しない。

## ピンとdriverの契約

`GpioPin(n)` の `n` は **GP番号**。ヘッダー上の物理ピン番号ではない。
GP0〜GP22・GP26〜GP28と基板LEDのGP25を公開し、基板内部のGP23・GP24・GP29は拒否する。

| 公開ID・用途 | コントローラー | 両ボード共通の配線 |
| --- | --- | --- |
| LED | SIO GPIO | GP25、active-high |
| 外部入力の例 | GPIO | GP2、pull-up、GNDへのfalling edge |
| `Uart0` | UART0 | TX GP0、RX GP1 |
| `Spi1` | SPI0 | SCK GP18、MOSI GP19、MISO GP16、CSの例 GP17 |
| `I2c1` | I2C0 | SDA GP4、SCL GP5 |

`Spi1` / `I2c1` は公開enumの先頭IDで、C ABIの値は0。
ハードウェアのSPI1/I2C1を指す名前ではない。この表以外のcontroller IDは `Unsupported`。
4driverとSPIのCSでpin所有権を共有し、競合は `ResourceConflict` になる。

| 機能 | 実装範囲 |
| --- | --- |
| GPIO | Input、PushPull、OpenDrain、pull、read/write/toggle。OpenDrainのHighは入力方向にしてlineを解放する |
| Analog設定 | GP26〜GP28のNoPull・digital入力切断のみ。ADC変換は行わない |
| GPIO割り込み | Rising/Falling/Both、利用可能な各pinに登録可能。IO_IRQ_BANK0は共有なので同時登録の優先度を揃える |
| UART | 7/8 bit、parityなし/even/odd、1/2 stop、flow controlなし、`Native` sampling。SDK分周で誤差3%以内に実現できるbaud |
| UART受信・通知 | exact/some/discard、pollingまたは64語のIRQ受信buffer、IRQ送信通知 |
| SPI | Master、8/16 bit、TwoLine/TwoLineReceiveOnly、mode 0〜3、MSB-first。実現SCKが指定上限を超えない分周を選ぶ |
| I2C | 7 bit master、Standard duty、最大1 MHz。任意prefix＋Repeated START、pure read、memory_write、polling/IRQ転送 |
| 復旧 | SPI controller再設定と共通層によるCS解除。I2C再初期化、必要時最大9 SCL pulseとSTOP、期限付き |

UARTの9 bit・half stop・RTS/CTS・固定oversampling、SPIのSlave・OneLine・LSB-first、
I2CのFast16To9は `Unsupported`。SPIで指定上限以下のSCKを作れない場合も同じエラーになる。
I2Cのaddress-only `is_device_ready` と空の `master_transmit` は `Unsupported` とし、ダミーの送受信は行わない。

GPIOのread/write/toggleはISR内で使用できる。設定・購読変更はcore 0のmain contextに限定する。
UART・SPI・I2Cの同期操作はcore 0のmain contextかつIRQ有効時に行い、一つの呼出し全体にtimeoutを適用する。
時刻はSDKの単調増加するmicrosecond counterを使用する。
GPIO callback表・capture保持・ISR検査は生成C側、SPI device管理・CS・複合エラーは
既存の `firmware/common/spi.c` が所有する。

## セットアップとビルド

```sh
bash tools/setup/common.sh
bash tools/setup/pico-sdk.sh
source .local/env.sh
cargo build --locked -p emelino-cli

target/debug/emelino build examples/pico-gpio/main.emel \
  --config examples/pico-gpio/config.json --out-dir out/pico-example/generated
python3 tools/build_firmware.py --board raspberry-pi-pico \
  --generated-dir out/pico-example/generated --output-dir out/pico-example/pico
python3 tools/build_firmware.py --board raspberry-pi-pico-2 \
  --generated-dir out/pico-example/generated --output-dir out/pico-example/pico2
```

同じ生成Cを両ボードへビルドできる。GPIOサンプルはGP2のfalling edgeでGP25のLEDを反転する。
機械式スイッチのdebounceは行わない。
`examples/pico-peripherals` へ置き換えると4driver同時使用サンプルになる。
UART0へ `READY\n` を送り、SPI0のGP17 CS・1 MHz deviceとI2C0を設定する。
SPI/I2Cの相手固有コマンドは送らない。

[Pico SDK 2.3.1](https://github.com/raspberrypi/pico-sdk/tree/2.3.1)と
[picotool 2.3.1](https://github.com/raspberrypi/picotool/tree/2.3.1)をsetupで固定する。
SDK・picotoolは同梱せず、通常のビルド中にダウンロードしない。
USB・無線を使わないためSDK submoduleの取得は不要。
`--sdk-dir` / `PICO_SDK_PATH`、`--cc` / `ARM_GCC` で手元の配置を指定できる。
picotoolは `picotool_DIR` に `picotoolConfig.cmake` を含むディレクトリを指定する。
Git checkoutは固定revisionと変更がないことを確認する。SDKの2.3.1 source archiveも使用でき、
`sdk-inputs.json` に全source fileのhashを記録する。CMakeとpicotoolの版・実行ファイルhashも成果物へ保存する。

成果物は `firmware.elf` / `firmware.bin` / `firmware.uf2` / `firmware.map`、
容量・symbol・hash・SDK/toolchain情報、build log。
SDKのboot2/image情報とvector位置を踏まえ、CPU・配置・未解決symbol・system allocator混入を検査する。
stack/guardの既定予約量は16 KiB / 256 bytes。guardは予約領域であり、実行時保護を保証するものではない。
`firmware.json` は全成果物と検査の成功後だけ発行する。

## 無料のローカル検証

[rp2350js](https://github.com/c1570/rp2350js)を固定revision
`eefa1d62e82902c8f9986ab9293cf273bda30583` からビルドする。
Node.js 18以降とnpmは、この任意のエミュレータ用setupでのみ必要。
有料アカウント・Wokwi・クラウド実行環境・RenodeはPico試験の前提にしない。

```sh
bash tools/setup/rp2350js.sh
source .local/env.sh
python3 tools/test_embedded.py --board raspberry-pi-pico --scenario all
python3 tools/test_embedded.py --board raspberry-pi-pico-2 --scenario all
```

シナリオ名は `pico_gpio` / `pico_uart` / `pico_spi` / `pico_i2c` / `pico_peripherals`。
`--optimization O0` で最適化なしの同じ試験を実行する。
SDKによるビルドだけを確認する場合、engineとNode.jsの導入は不要。

```sh
python3 tools/test_embedded.py --board raspberry-pi-pico --scenario all --build-only
python3 tools/test_embedded.py --board raspberry-pi-pico-2 --scenario all --build-only --optimization O0
python3 tests/firmware/test_pico_gpio.py
python3 tests/firmware/test_pico_serial.py
```

各シナリオの `report.json` は `build_status` と `runtime_status` を分ける。
`--build-only` の実行結果は `not_run` で、ビルド成功を実行成功には変換しない。
`passed` / `failed` / `model_limited` を区別し、モデル制約には素のSDKサンプルやengine実装の根拠を付ける。
engine未導入・設定誤り・host timeoutだけではモデル制約にしない。
通常の出力先は `out/embedded/<board>/<optimization>/<scenario>/`。

今回のボード追加では**正規Pico SDKによるELF生成を主要な受入条件**とし、
rp2350jsで実行できる範囲を追加検証する。ELF生成はコンパイル・ABI・linkの確認であり、
実機の電気的timingや全周辺機器の動作確認とは区別する。
host GPIO試験は生成headerに対する所有権・OpenDrain・IRQ契約の検査で、CPUエミュレーションではない。
設計と受入条件の詳細は[実装計画](implementation-plan.md)を参照する。
