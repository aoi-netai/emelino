# ESP32-C3 / ESP32-C3-DevKitM-1-N4X

`--board esp32-c3-devkitm-1-n4x` で生成CをESP-IDF 5.4.1のアプリとしてビルドする。
RISC-V RV32IMCの起動処理、ROM関数、リンカー、割り込み割り当てはSDKを使う。
Emelaコードは静的に確保した専用FreeRTOS taskで実行する。標準のstackは16 KiB、
下側のguardは256 bytes。guardのパターンとFreeRTOSのstack canaryを検査する。
SDKのdriver・FreeRTOSはシステムheapを使用する。生成Cの`heap_bytes`とは別の領域である。

## 初期配線

ピン番号はGPIO番号で、ヘッダーの物理番号ではない。

| Emela API | ESP32-C3のcontroller | 配線 |
| --- | --- | --- |
| `Uart0` | UART0 | TX GPIO21 / RX GPIO20。オンボードUSB-UART bridgeへ接続 |
| `Spi1` | SPI2_HOST | SCK GPIO4 / MISO GPIO5 / MOSI GPIO6、サンプルのCS GPIO10 |
| `I2c1` | I2C0 | SCL GPIO1 / SDA GPIO0 |
| GPIOサンプル | GPIO | 外付けLED GPIO7、BOOTボタン GPIO9 |

GPIO0〜10、18〜21だけをボードの利用可能ピンとする。GPIO11、フラッシュ用GPIO12〜17は
拒否する。UART・SPI・I2C・GPIO・SPI device CSは同じピン予約表を使う。
各controllerの配線は`firmware/boards/esp32-c3-devkitm-1-n4x/board.json`が所有する。

オンボードのRGB LEDはGPIO8に接続されたアドレサブルLEDで、GPIOのHigh/Lowだけでは
通常のLEDとして使えない。GPIO7のサンプルには外付けLEDと直列抵抗を接続する。
GPIO2・8・9はstrapping pinなので、起動時の外部回路のレベルに注意する。
GPIO18・19をGPIOとして使用すると内蔵USB Serial/JTAGの機能を兼用できない。

## セットアップとビルド

Ubuntu / WSLで共通ツールに加えて公式SDKとRISC-V toolchainを導入する。
SDKはGit revision `4c2820d377d1375e787bcef612f0c32c1427d183`へ固定する。
既存SDKのrevisionや追跡ファイルの変更は検査し、自動でresetしない。

```sh
bash tools/setup/common.sh
bash tools/setup/esp32-c3-devkitm-1-n4x.sh
source .local/env.sh
cargo build --locked -p emelino-cli

target/debug/emelino build examples/esp32c3-peripherals/main.emel \
  --config examples/esp32c3-peripherals/config.json \
  --out-dir out/esp32c3-peripherals/generated
python3 tools/build_firmware.py --board esp32-c3-devkitm-1-n4x \
  --generated-dir out/esp32c3-peripherals/generated \
  --output-dir out/esp32c3-peripherals/firmware
```

既存のSDKは`IDF_PATH`または`--sdk-dir`、コンパイラーは`RISCV32_ESP_GCC`または`--cc`で
指定できる。手動で導入したSDKでも、その`export.sh`を読み込んでPython環境とROM symbolの
パスを設定する。`ARM_GCC`はこのportでは使わない。

通常のプロジェクト作成・ビルドにも対応する。

```sh
target/debug/emelino new my-esp32c3 --board esp32-c3-devkitm-1-n4x
target/debug/emelino build my-esp32c3
```

テンプレートは外付けLED用のGPIO7をHighにする。生成アプリのC exportは
`emelino_app_main`として、SDKの`void app_main(void)`との衝突を避ける。
明示的な設定ファイルを使う場合も、C exportに`app_main`を指定しない。

GPIOだけの例は`examples/esp32c3-gpio/`。BOOTボタンのfalling edgeでGPIO7を反転する。
`examples/esp32c3-peripherals/`は4driverを設定し、UART0へ`READY`を送る。
接続されていないSPI/I2Cの相手への転送は行わない。

ビルドは`firmware.elf`、`firmware.map`、アプリ単体の`firmware-app.bin`、
`bootloader.bin`、`partition-table.bin`と、offset 0からの結合済み`firmware.bin`を作る。
SDKの`flasher_args.json`から作った配置を`flash-plan.json`へ保存する。
標準のpartition tableはoffset `0x10000`に1 MiBのfactory appを置く。
基板全体のFlash容量は4 MiBだが、アプリ単体のサイズ上限はこのpartitionの容量で、SDKが検査する。
実機への書き込みは自動実行しない。

`firmware.json`にはSDK/toolchainの識別情報、成果物のSHA-256、メモリ使用量を記録する。
成功マーカーは全成果物の検証後だけ作成する。再ビルドの失敗時に古い成功マーカーは残さない。
`sdkconfig`、`sdk-inputs.json`、`symbols.json`、`resources.json`、`build.log`も保存する。
専用taskのstack/guardはBSSに含まれるため、`static_ram_bytes`にも含む。

## 対応範囲

| driver | 対応 |
| --- | --- |
| GPIO | Input、PushPull、OpenDrain、pull、read/write/toggle、edge IRQ。AnalogはADC対応ピンのデジタル切断のみ |
| UART0 | 7/8bit、1/2stop、parity、RX/TX/Both、polling、64ワードのIRQ受信リング、TX完了IRQ、receive_some、discard |
| SPI2_HOST | Master、TwoLine、8/16bit、CPOL/CPHA全組み合わせ、MSB/LSB、共通層のdevice/複合操作/CS/復旧 |
| I2C0 | 7bit Master、1〜400 kHz、任意prefixとrepeated START、8/16bit memory_write、address probe、bus reset |

UARTは300〜5,000,000 baud、oversamplingは`Native`だけに対応する。RTS/CTSと9bitは
`Unsupported`。IRQ受信overflowは`Overrun`を返し、discard後に再利用できる。
受信モードをpollingへ戻しても、すでに受信リングにあるデータは保持する。

SPIの周波数指定上限は40 MHzで、SDKの分周結果が指定した上限を超えない値を選ぶ。
低速側はハードウェア分周器の範囲にも制約される。Slave、OneLine、ReceiveOnlyは
`Unsupported`。1/2 bytesずつSDKへ渡すため、連続大容量転送の性能最適化はしていない。
タイムアウト後もSDKが参照する転送構造体は保持する。`recover`が`Busy`なら、転送が
完了してから再試行する。共通SPI層はエラー時もCSの解除を試みる。

I2Cのduty cycleは`Standard`だけで、周波数に応じた実際の波形はSDKが選ぶ。
転送中のpacked byte bufferはportが確保し、終了時に解放する。確保失敗は
`CapacityExceeded`。publicな`enable_interrupt`は`Unsupported`を返す。
同期IDF driverが内部で使う完了ISRはSDKが所有し、無効化しない。
IDFがNACKと区別できないハードウェアエラーは`BusError`へまとめる。

GPIOのread/write/toggle以外の設定・転送はtask contextから呼び出す。
各driverは同時使用を`Busy`として拒否する。UART/I2Cのtimeoutは正の符号付き32bitの
ミリ秒値。FreeRTOSのtickは1 msで、SDKのブロッキング処理にはtick単位の粒度がある。

## 検証

```sh
python3 tests/firmware/test_esp_idf_build.py
python3 tests/firmware/test_esp_idf_contract.py
python3 tools/test_embedded.py --board esp32-c3-devkitm-1-n4x --build-only
```

ホストのcontract試験は生成したC ABI headerとSDK呼び出しのtest doubleを使う。
ピン競合、UARTのリング境界とエラー回復、16bit SPIのbyte順とCS解除、I2Cのpacked byte、
repeated START、NACK、timeout、確保失敗と解放をO0/Os＋UBSanで検査する。
これは実機・Renodeの通信試験とは別である。
今回の実行結果は[検証記録](validation.md)と[機械可読の記録](validation.json)に保存する。

## Renode

標準RenodeにはESP32-C3のボードモデルがないので、
[mithro/renode-espemu](https://github.com/mithro/renode-espemu)の外部モデルを使う。
モデルのrevisionとROM ELFのSHA-256は`tests/embedded/ports/esp_idf/dependency.json`へ固定する。

```sh
bash tools/setup/esp32c3-renode.sh
source .local/env.sh
export RENODE=/path/to/renode
python3 tools/test_embedded.py --board esp32-c3-devkitm-1-n4x
```

モデルの位置は`--esp32c3-models-dir`、ROMは`--esp32c3-rom`で指定できる。
smoke試験はSDK taskの起動、GPIO7の出力、未公開ピンの拒否、UARTの`EMELINO`出力を観測する。
試験の失敗やtimeoutを自動的に`passed`・`model_limited`へ分類しない。

このモデルはフラッシュbootloaderを実行してアプリを読み込む方式ではない。
ROMの初期化後にELF entryへ移るフック、CLICのtrap dispatchフック、cycle counter CSR、
WFIのNOP化を使う。`report.json`に`boot: rom-init-elf-bootstrap`、`sets_pc: true`、
`flash_boot_validated: false`とフック一覧を記録する。
GPIOモデルは出力のloopbackで、外部回路やGPIO callbackの動作保証には使わない。

外部モデルのI2C・汎用SPIはスタブのため、このsmoke試験には含めない。
両driverの実機通信と、フラッシュからの通常起動は別途実機で確認する必要がある。

基板とSDK仕様の参照元:
[DevKitM-1公式ガイド](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32c3/esp32-c3-devkitm-1/user_guide.html)、
[ESP-IDF 5.4.1 GPIO](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32c3/api-reference/peripherals/gpio.html)、
[UART](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32c3/api-reference/peripherals/uart.html)、
[I2C](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32c3/api-reference/peripherals/i2c.html)。
