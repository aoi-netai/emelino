# BMP180を相手にしたSTM32F4 I2C試験

生成Emela firmwareから、Renodeに組み込まれた `Sensors.BMP180`（アドレス `0x77`）へ
通信する。簡易応答器への置換、HAL completionの注入、CPUの戻り値やregister値の書換えは行わない。
fixtureのC追加ソースは結果を記録するだけで、BSPとSTM32CubeF4 HALは通常のものをlinkする。

## 試験内容

[Emelaアプリ](../../../tests/embedded/scenarios/bmp180/main.emel)は次の19項目を検査する。

| 対象 | 確認する結果 |
| --- | --- |
| IDと受信長 | `0xD0`の1 byte読み取りが`0x55`。校正領域は2 / 22 byteで一致 |
| standalone read | 書込みで校正領域のpointerを設定し、空prefixの1 byte読み取りを2回実行。`0x1B`、`0xCB`の順になり、過剰読み取りがない |
| 温度 | 測定commandを書き、SCO解除と2 byte結果を確認。通常の正方向補正式で24.0℃、32.0℃を再現 |
| 圧力 | OSS=0 / OSS=3のcommandに対し、設定した未補償値1100 / 2200に対応する3 byte結果を確認 |
| 制御と割り込み | software reset、polling→IRQ→polling、100 kHz→400 kHz、recover後に通信が継続 |
| エラーと再試行 | 未登録アドレス`0x76`でNack、readyの3試行後False、その後の正常通信 |

sensorの温度と未補償圧力はRenode側から途中で変更する。firmwareはsensor modelの内部変数を参照しない。
校正データの期待値は、使用した公式BMP180モデルのreset値に基づく。温度計算はモデルの逆算処理を
コピーせず、Bosch datasheetの正方向補正式を使用する。

firmwareの結果に加え、CR1への書込みを観測して30 transaction全体のSTART/STOP列を検査する。
prefix付き読み取りはSTART→Repeated START→STOPとなり、途中にSTOPを挿入しない。
未登録アドレスでのNACK発生は読み取り1回とreadyの3試行の合計4回を観測する。

## 再実行

ARM GCC・STM32CubeF4・Renodeの指定は [共通手順](../README.md#renode試験) を参照する。
リポジトリのrootから次を実行する。

```sh
python3 tools/test_embedded.py --board nucleo-f446re --scenario bmp180 \
  --controller-model tests/renode/models/EmelinoSTM32F4I2C.cs
python3 tools/test_embedded.py --board nucleo-f446re --scenario bmp180 \
  --controller-model tests/renode/models/EmelinoSTM32F4I2C.cs \
  --optimization O0 --output-dir out/stm32f4-bmp180-o0
```

`--controller-model` を省略すると、target定義の修正版controller modelを使う。
`--stock-controller` を指定すると、インストール済みのSTM32F4 controller modelをそのまま使う。
Renode 1.16.1の同梱modelでは最初のID読み取りがTimeoutとなり、試験は終了コード1で失敗する。
この失敗を成功に置き換えず、baseline reportとして別に保存できる。

```sh
python3 tools/test_embedded.py --board nucleo-f446re --scenario bmp180 \
  --stock-controller --output-dir out/stm32f4-bmp180-baseline
python3 tools/test_embedded.py --board nucleo-f446re --scenario bmp180 \
  --controller-model tests/renode/models/EmelinoSTM32F4I2C.cs \
  --baseline-report out/stm32f4-bmp180-baseline/report.json
```

`--baseline-report` は比較対象とfirmware BINのSHA-256が一致することも必須にする。
差分はcontroller modelであり、センサー・アプリ・BSP・HALの成功値への差替えではない。
出力先にはELF/BIN、build log、Renode log、モデルのコピーとSHA-256、`report.json`を保存する。

構成変更前の2026-11-04のRenode 1.16.1で、修正版controllerでは `-Os` / `-O0` とも19項目が成功した。
同梱controllerのbaselineと修正版の `-Os` は同じfirmware BINであり、SHA-256は
`2b81088b5bb254768aff87f9ac42f800b555866f394457ae6d32a4d7369c03bc`。
Flashは16,356 / 39,888 bytes、静的RAMは16,984 / 16,976 bytes。
別にstack 16 KiBとguard 256 bytesを予約し、system allocatorはlinkしない。

## 検証範囲

BMP180の既存モデルは要求されたcountちょうどのバイトを返す。旧controller向けの
「`Read(1)`へ長いデータ列を返す」対応は、この試験にはない。
controller modelの修正は [EmelinoSTM32F4I2C.cs](../../../tests/renode/models/EmelinoSTM32F4I2C.cs) にあり、
デバイス固有のregister値やfirmwareのRAMを参照しない。

RenodeのBMP180は測定が即時に完了するため、変換所要時間は検査しない。
圧力は未補償値とOSSによる格納形式の確認であり、気圧への完全な補正式はこの試験の対象外。
電気的なopen drain、pull-up、SCL波形、F446実機のタイミングも検査範囲外とする。

GPIOのOTYPER、BMP180のreset register、HALがAFをclearするときのSR1未モデルbitについては、
特定の既知warningだけを許可してreportへ記録する。reset後の値とNACK処理は別途assertし、
それ以外のwarningは試験失敗にする。

参照元:

- [Renode 1.16.1に対応するBMP180実装](https://github.com/renode/renode-infrastructure/blob/add012af003a0f620d3da52828262676f374d121/src/Emulator/Peripherals/Peripherals/Sensors/BMP180.cs)
- [Renode公式のBMP180試験](https://github.com/renode/renode/blob/v1.16.1/tests/peripherals/BMP180.robot)
- [Bosch BMP180 datasheet、§3.4–3.5 / §4–5](https://media.digikey.com/pdf/Data%20Sheets/Bosch/BMP180.pdf)
