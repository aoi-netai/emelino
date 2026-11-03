# 共通のfirmware試験

```sh
python3 tools/test_embedded.py --board nucleo-f446re --scenario all
python3 tools/test_embedded.py --board nucleo-f446re --scenario all --optimization O0
python3 tools/test_embedded.py --scenario bmp180 --scenario mt25q
python3 tests/embedded/support/test_runner.py
python3 tests/embedded/ports/stm32f4/test_time.py
python3 tests/embedded/ports/stm32f4/test_events.py
python3 tests/embedded/ports/stm32f4/test_mapping.py
```

`ARM_GCC`、`STM32CUBE_F4`、`RENODE`の指定は
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
