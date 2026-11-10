# LSM9DS1

EmelaでLSM9DS1の加速度3軸と角速度3軸をI²Cまたは4-wire SPIで読み出すexampleです。
`lsm9ds1.emel`はレジスタ操作の`Registers` effectだけを使い、通信方式をhandlerで選びます。
設定値のエンコードと生データのデコードは純粋関数です。

`example/sensor_lib`（`example/stm32f446-sensor-logger`と同じ内容）の
`examples/lsm9ds1`から取り込んでいます。元の追加コミットは`7d96d98`です。

| ファイル | 担当 |
| --- | --- |
| `lsm9ds1.emel` | 共通の計測設定・開始停止・読み出し、純粋なデコード |
| `registers.emel` | `Registers.read/write`、共通エラー、引数と応答の検証 |
| `i2c_registers.emel` | `OverI2c` handler。デバイスアドレスとrepeated STARTを扱う |
| `spi_registers.emel` | `OverSpi` handler。A/GのSPIコマンドを組み立て、managed CSで転送する |
| `main_i2c.emel` / `main_spi.emel` | それぞれの入口から通信設定・計測設定・開始・5回の読み出し・停止・UART出力を直接行う |
| `stm32f446.c` | 使用例の待ち時間をSysTickで実装 |

## 3つの操作

| API | 動作 |
| --- | --- |
| `config(settings)` | IDを確認し、加速度・角速度のレンジ、BDU、アドレス自動加算、little-endianを設定。計測は停止状態になる |
| `power(On / Down)` | 両センサーを119 Hzで開始、またはpower-down。設定したレンジを保持する |
| `dataread()` | 加速度・角速度をそれぞれ6 byteのburstで読み、符号付き16 bitの`Sample`を返す |

レンジは`Accel2g` / `Accel4g` / `Accel8g` / `Accel16g`と、
`Dps245` / `Dps500` / `Dps2000`から選びます。
I²Cの`OverI2c(I2c1, High)`のアドレスは**7 bitの0x6b**、`Low`は0x6aです。
コントローラーの設定と`BoardI2c` / `BoardSpi`の選択は呼び出し側で行います。
入口の通信設定には[省略引数](../../docs/embedded/default-arguments.md)を使い、
I²Cは既定の100 kHz、UARTは115200 baud・既定の8N1で設定します。
SPIはmode 3・1 MHz上限・CS前後の待ち時間1 µsを明示しています。
次のブロックでは、I²Cが設定済みで`BoardI2c`が外側にあるものとします。

```emel
with I2cRegisters.OverI2c(I2c.I2c1, I2cRegisters.High) {
  Lsm9ds1.config(Lsm9ds1.Config(Lsm9ds1.Accel2g, Lsm9ds1.Dps245))
  Lsm9ds1.power(Lsm9ds1.On)
  # 呼び出し側で起動・フィルターの安定を待つ。使用例では150 ms待つ。
  sample = Lsm9ds1.dataread()
  # sample.acceleration.x / y / z、sample.angular_rate.x / y / z
  Lsm9ds1.power(Lsm9ds1.Down)
  sample
}
```

SPIでは`Spi.create_device`で作ったデバイスを渡し、同じブロックを
`with SpiRegisters.OverSpi(device) { ... }`で囲みます。
8 bit、MSB first、mode 3、active-low CSのデバイスを使います。
`main_spi.emel`に具体的な作成コードがあります。

共通関数は`uses { Registers.Registers }`を宣言し、必要な操作を要求します。
I²CとSPIで`config`や`dataread`を複製する必要はありません。
I²C handlerは1回の`write_read`、SPI handlerは1回の`transaction`で各burstを処理します。
SPIではコマンドとデータの両方を同じCS区間に収めます。
加速度・ジャイロ側のSPIはbit 7がREAD、bits 6..0がアドレスです。
連続読み出しは`IF_ADD_INC`で設定し、地磁気側のMS bitを付加しません。

`Sample`の値は**生のLSB値**です。gやdegrees/sへの換算は行いません。
地磁気、温度、FIFO、割込み、data-ready待ちはまだ扱いません。
直前の出力レジスタを読むため、更新前に呼ぶと前回と同じ値になります。
BDUは各軸の上位・下位byteの整合性を保つ設定で、加速度と角速度の2回のburstを
同時刻の値にするものではありません。

センサーの電源投入・リセット後の既定状態（全軸有効、FIFO無効、通常モード）から使います。
`config`はCTRL_REG1_G、CTRL_REG6_XL、CTRL_REG8を上書きし、計測中に呼ぶと計測が停止します。
再開には`power(On)`を呼びます。`power(Down)`は計測停止であり、電源ピンの遮断ではありません。
通信タイムアウトは操作ごとに20 msです。通信失敗は`Registers.RegisterError`へ変換します。
`I2cFailure(kind)`は元のI²Cエラー種別を、`SpiFailure(primary, cleanup)`は
SPI/GPIOの元の失敗と、別途起きたCS解除の失敗を保持します。
handlerの不正な引数は`InvalidArgument`、byte範囲外などの不正応答は`InvalidResponse`です。
ID不一致は`Lsm9ds1.Error(WrongDevice)`です。
途中の通信失敗でレジスタ設定が一部だけ変わった場合、自動的な巻き戻しや再試行は行いません。

レジスタ定義は[STのデータシート](https://www.st.com/resource/en/datasheet/lsm9ds1.pdf)の
§5.2、7.11、7.12、7.24、7.26、7.19–7.21、7.31–7.33を参照しています。

## 純粋なデコード

`Lsm9ds1.decode(angular_rate_bytes, acceleration_bytes)`は`uses {}`の純粋関数です。
6 byteずつの保存データから、通信なしで`Sample`を作れます。
長さやbyte値が不正なら`Lsm9ds1.Error(InvalidData)`になります。

## STM32F446で試す（I²C）

3.3 V対応のブレークアウトを使用し、GNDを共通にします。

| LSM9DS1 | NUCLEO-F446RE |
| --- | --- |
| SDA | PB9 / I2C1 SDA |
| SCL | PB8 / I2C1 SCL |
| SDO_A/G / SA0_A/G | High（アドレス0x6b） |
| CS_A/G、CS_M | High（I²C） |
| 電源、GND | モジュールの仕様に従って3.3 V、GNDへ接続 |

SCL/SDAには3.3 Vへのプルアップが必要です。UART2のPA2/PA3は通常のNucleo配線で
ST-LINKのVirtual COM Portへ接続されます。端末を115200 baud、8N1で開いてリセットします。

リポジトリのルートで実行します。ARM GCCとSTM32CubeF4の場所を指定してください。

```sh
cargo build --locked -p emelino-cli
target/debug/emelino build examples/lsm9ds1/main_i2c.emel \
  --config examples/lsm9ds1/target-stm32f446.json \
  --out-dir out/lsm9ds1/generated

python3 tools/build_firmware.py --board nucleo-f446re \
  --generated-dir out/lsm9ds1/generated --output-dir out/lsm9ds1/firmware \
  --cc "$ARM_GCC" --sdk-dir "$STM32CUBE_F4" \
  --source examples/lsm9ds1/stm32f446.c
```

生成物は`out/lsm9ds1/firmware/firmware.elf`と`firmware.bin`です。
使用例は±2 g、±245 dpsで計測を開始し、150 ms待ってから100 ms間隔で5回読み出し、停止します。
UARTには`accel x y z`と`gyro x y z`、最後に`power down`を出力します。
入口の戻り値は0が成功、1がセンサー応答異常、2がレジスタ通信エラー、3がUARTエラーです。

別のプログラムで使う場合は`lsm9ds1.emel`、`registers.emel`と、使う方式の
`i2c_registers.emel`または`spi_registers.emel`をentryと同じディレクトリへ置き、
コンパイラー設定の`sources`へ追加します。
このリポジトリではまだ外部ライブラリのpackage loaderを使用しません。

## STM32F446で試す（SPI）

| LSM9DS1 | NUCLEO-F446RE |
| --- | --- |
| SPC | PA5 / SPI1 SCK |
| SDI | PA7 / SPI1 MOSI |
| SDO_A/G | PA6 / SPI1 MISO |
| CS_A/G | PA4（active low） |
| CS_M | High |

電源とGNDはI²C版と同様です。PA5はNucleoのLD2と共用するため、GPIO LEDとして同時に使いません。

```sh
target/debug/emelino build examples/lsm9ds1/main_spi.emel \
  --config examples/lsm9ds1/target-stm32f446-spi.json \
  --out-dir out/lsm9ds1-spi/generated

python3 tools/build_firmware.py --board nucleo-f446re \
  --generated-dir out/lsm9ds1-spi/generated --output-dir out/lsm9ds1-spi/firmware \
  --cc "$ARM_GCC" --sdk-dir "$STM32CUBE_F4" \
  --source examples/lsm9ds1/stm32f446.c
```

mode 3、1 MHz上限で動作します。計測設定・開始・読み出し・停止は`main_spi.emel`から直接呼び出します。
SPIデバイス作成時の失敗は戻り値4、作成後の転送失敗は2になります。

## 検証

ビルドしたI²C版firmwareをRenodeで実行する場合:

```sh
python3 examples/lsm9ds1/test_renode.py --renode "$RENODE"
```

Renode標準の`Sensors.LSM9DS1_IMU`へ5組の加速度・角速度を注入し、実際のSTM32用HAL/BSPを
通ったUART出力を照合します。I²Cコントローラーは既存の
[`EmelinoSTM32F4I2C`](../../tests/renode/models/README.md)を使用し、センサーモデルは変更しません。
結果は`out/lsm9ds1/renode/report.json`と各ログへ保存します。
標準のLSM9DS1モデルはI²C接続です。SPI版はfirmwareビルドで確認します。

モデルのサンプル列は読み出し時に進み、ODRに従う時間更新ではありません。
BDUやpower-downの物理的動作、実際の感度・起動時間・消費電流はこのテストでは検証できません。
モデル内の換算係数には簡略化があるため、注入したg/degrees/sと実機の換算結果の一致も
検証対象にしていません。実機での動作確認は未実施です。
