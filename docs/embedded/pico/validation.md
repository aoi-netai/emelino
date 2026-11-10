# Pico / Pico 2 検証結果

2026-11-10、`feat/pico-rp2040-rp2350` で検証。
今回の受入条件である公式Pico SDKによるELF生成を、両ボードで満たした。
実行結果は、成功した観測とエミュレータの未実装部分を分けて記録した。
実機での検証は行っていない。
[PR #20](https://github.com/aoi-netai/emelino/pull/20)の独立レビュー後も20構成と既存11シナリオを再実行し、
本書とJSONの測定値を修正後の結果へ更新した。

## 使用した環境

| 項目 | 固定版・検証環境 |
| --- | --- |
| Pico SDK | 2.3.1、`079c6f39023649b154152db30f1d781e884879bc` |
| picotool | 2.3.1、`2041936441b48a3cc53ae3da9e805229fe8f4e18`、USB機能なし |
| rp2350js | 2.0.1、`eefa1d62e82902c8f9986ab9293cf273bda30583`、上流への変更なし |
| Arm GCC / CMake | Ubuntu版 GCC 10.3.1 / CMake 3.22.1 |
| Node.js | 24.21.0 |
| 既存ボードの回帰 | Renode 1.16.1、STM32CubeF4 1.28.3、nrfx 3.2.0、CMSIS 5.9.0 |

SDK・engine・ツールの識別情報、firmwareのhash、メモリ使用量は各試験のreportと
`firmware/firmware.json` に保存する。[測定値の一覧](validation.json)も参照。

## ビルドと実行

5シナリオ × 2ボード × `-Os` / `-O0` の **20構成でELF・BIN・UF2生成が成功**。
CPU属性、Flash/RAM配置、entry/vector、16 KiB stackと256 bytesのguard予約、
未解決symbolとsystem allocatorの不在を検査した。
`examples/pico-gpio` と `examples/pico-peripherals` も、同じソースから両ボードへビルドできた。

rp2350jsは同梱bootromから正規SDK imageを起動する。PC/SP/VTORをテスト側で設定しない。
次の結果は `-Os` / `-O0` で共通。

| シナリオ | Pico / RP2040 | Pico 2 / RP2350 Arm |
| --- | --- | --- |
| GPIO | `passed`：出力・入力・実ISR callback | `model_limited`：GPIO出力とIRQ |
| UART | `model_limited`：IRQ受信のエラーレジスタ | 同左 |
| SPI | `passed`：8/16 bit往復、device CS、復旧 | `model_limited`：バス往復は成功、CS出力を確認できない |
| I2C | `model_limited`：controller disable状態 | 同左 |
| 4driver同時使用 | `model_limited`：UART IRQ受信で停止 | `model_limited`：GPIO制約とUART IRQ受信で停止 |

UARTでは通常送信・polling受信までは確認できた。IRQ受信で中断するため、
後続のIRQ送信・受信timeoutを実行成功に数えない。
I2Cは最初のSTARTに到達しないため、Repeated START・IRQ転送・復旧の実行成功は主張しない。
同時使用試験で停止後のSPI/I2Cは未実行であり、個別試験の結果と区別する。

## 制約の根拠

`tests/rp2350js/baseline/` はEmela生成CとBSPを使わない素のSDKプログラム。
両CPUで起動・UART送信・100 μs待機は成功し、次の不具合を再現した。

| 対象 | 素SDKでの観測と固定engineの原因 |
| --- | --- |
| UART、両CPU | UARTRSRがwrite-clear後も`0xffffffff`。`uart.ts`のread switchにoffset `0x04`がない |
| I2C、両CPU | ENABLEは0になるがIC_ENABLE_STATUSは`0xffffffff`。`i2c.ts`のread switchにoffset `0x9c`がない |
| RP2350 GPIO出力 | GPIO25の出力・出力許可がともにfalse。SDKが使うCP0のMCRR操作をengineが処理しない |
| RP2350 GPIO IRQ | 外部edgeに対するcallbackが0回。IO register groupのoffset計算が誤り、割り込み許可が反映されない |

[機械可読の再現記録](../../../tests/rp2350js/model-limitations.json)には
baselineのhash・観測値・engineの該当実装を記録した。
SDKやdriverの正当な状態確認を削除して通過させる変更は加えていない。
分類は固定SDK/engineと特定の失敗signatureに限定し、無関係なcheck失敗・defect・timeout・
engine未導入・古い実行reportは通常の失敗として扱う。

## 回帰と再現方法

次を確認した。

- `cargo test --offline`：既存compiler・frontend・生成Cの全試験が成功。
- firmwareのbuild境界、setup、共通runnerのPython試験が成功。
- GPIO・UART/SPI/I2C・共通SPIエンジンのhost契約試験が `-O0` / `-Os` とUBSanで成功。
- STM32の7シナリオ、nRF52の4シナリオが共通入口からRenodeで成功。

host契約試験はピン所有権、ISR文脈、IRQ bufferのoverflow、周波数上限、
operation全体のdeadlineと復旧を検査する。実CPUでの周辺機器試験とは区別する。

追加の回帰試験では、SPI mode 2/3で手動CSを保持した連続raw転送がSDK再初期化による余分なclockを
発生させないこと、clock変更時の分周再計算、recover時の強制resetを確認した。
SDK単独検証はコンパイラを変更して再実行するとbuild treeを作り直し、実際に選ばれたコンパイラを照合する。
両ボードのOs/O0で素SDKの既知制約も再確認し、100 μs以上の待機で生じる読取遅延は正常な差として扱う。
実行前のビルド失敗はmatrixでも `not_run` を維持し、固定fixtureが処理しないcontroller・配線の変更は拒否する。
ビルドの作業領域 `sdk-build` に入力コード・SDK・ツールが含まれる指定も事前拒否し、
再ビルドのcleanupで入力を削除しないことを確認した。

[セットアップ](README.md#セットアップとビルド)後に実行する。

```sh
for board in raspberry-pi-pico raspberry-pi-pico-2; do
  for optimization in Os O0; do
    python3 tools/test_embedded.py --board "$board" --scenario all \
      --optimization "$optimization"
  done
done
python3 tests/firmware/test_pico_gpio.py
python3 tests/firmware/test_pico_serial.py
python3 tests/firmware/test_spi_common.py
python3 -m unittest discover -s tests/firmware -p 'test_*.py'
python3 -m unittest discover -s tests/setup -p 'test_*.py'
python3 -m unittest discover -s tests/embedded/support -p 'test_*.py'
```

素SDKによる制約の再現方法は[rp2350js試験の説明](../../../tests/rp2350js/README.md)を参照。
`model_limited` は確認済みの制約として終了コード0になり、実行成功件数には含めない。
未判定の失敗は終了コード1になる。ELF生成だけを確認する場合は `--build-only` を付ける。
