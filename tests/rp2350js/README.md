# Pico SDK / rp2350js 検証

`tools/test_embedded.py` から Pico と Pico 2 を同じ無料のローカルエンジンで検証する。
RP2040 は `RP2040`、RP2350 は `RP2350({coreArch: 'arm'})` を選ぶ。
`run.cjs` は UF2 をロードして、エンジン同梱の B1 / A2 bootrom から reset 起動する。
PC・SP・VTOR をアプリの入口へ書き換えず、Pico SDK の startup を実行する。

## 導入と実行

Ubuntu / WSL で共通開発環境と Node.js 18 以上・npm を用意してから実行する。

```bash
bash tools/setup/pico-sdk.sh
bash tools/setup/rp2350js.sh
source .local/env.sh
python3 tools/test_embedded.py --board raspberry-pi-pico
python3 tools/test_embedded.py --board raspberry-pi-pico-2
python3 tools/test_embedded.py --board raspberry-pi-pico --optimization O0
python3 tools/test_embedded.py --board raspberry-pi-pico-2 --optimization O0
```

`dependency.json` の Git revision と上流の `package-lock.json` を固定する。
`npm ci --ignore-scripts` で依存を展開し `npm run build` で JS を作る。
GUI 用 canvas の native addon、MCP server、背景 daemon は使用しない。
通常のテスト・ビルドでは download しない。Renode や有料アカウントは不要。
手動配置には `--rp2350js-dir` / `RP2350JS_DIR`、`--sdk-dir` / `PICO_SDK_PATH`、
`--cc` / `ARM_GCC` を使用できる。

`--scenario pico_gpio` / `pico_uart` / `pico_spi` / `pico_i2c` / `pico_peripherals`
で個別選択する。省略時は 5 シナリオを実行する。
`--build-only` は SDK ELF/BIN/UF2 の生成だけを行い、`runtime_status: not_run` とする。
`--timeout`（秒、既定 60）と `--max-instructions`（既定 10,000,000）が実行上限。

## 素の SDK との比較

```bash
python3 tests/rp2350js/baseline.py --board raspberry-pi-pico
python3 tests/rp2350js/baseline.py --board raspberry-pi-pico-2
```

`baseline/main.c` は Emela 生成 C や emelino BSP を使わず、公式 SDK のみで
UART 送信・100 µs 経過・GPIO 出力・外部入力による IRQ・UART RSR・I2C ENABLE_STATUS
を観測する。ELF/UF2、ビルドログ、`runner.json`、`runtime.json`、`report.json`
を `out/pico-baseline/<board>/<optimization>/` に保存する。

固定 revision `eefa1d62e82902c8f9986ab9293cf273bda30583` と SDK 2.3.1 の実測では：

| 項目 | RP2040 | RP2350 Arm |
| --- | --- | --- |
| bootrom → SDK startup、UART TX、単調時刻 | 実行確認済み | 実行確認済み |
| GPIO 入力・出力・実 ISR callback | 実行確認済み | 出力・IRQ にモデル制約 |
| SPI 8/16 bit echo、device CS | 実行確認済み | echo は観測、CS 出力にモデル制約 |
| UART polling RX | 実行確認済み | 実行確認済み |
| UART IRQ RX | RSR 未実装で Overrun | 同左 |
| I2C 転送 | ENABLE_STATUS 未実装で開始前に Timeout | 同左 |

UART RSR（offset `0x04`）と I2C ENABLE_STATUS（`0x9c`）はエンジンの read switch に
case がなく、既定値 `0xffffffff` を返す。素の SDK で clear / disable しても再現する。
M33 の GPIO coprocessor は SDK が生成する MCRR を処理せず、GPIO IRQ レジスタの
offset 計算にも不一致がある。`model-limitations.json` に基準値、UF2 hash、
エンジンの該当ソース、SDK / エンジン revision を記録した。

これらを避けるための BSP の変更、GPIO callback の polling 化、IRQ API の形だけの成功は行わない。
SPI peer は echo、I2C peer は address 80 の小さなメモリを実装する。
I2C peer の存在だけで転送成功とはしない。現 revision では START 前に止まるため
Repeated START・I2C IRQ・recover の実行保証は得られていない。

## 結果の扱い

ビルド結果と実行結果は分離する。`passed`、`failed`、`model_limited`、`not_run` を区別し、
制約がある matrix 全体も `model_limited` とする。既知の制約のみなら終了コード 0、
未知の失敗・engine 未導入・host timeout・instruction limit は非 0。

`limitations.py` は完了フラグ、実際の failed check ID、実行済み check 数、IRQ 文脈、
GPIO/CS、UART ログ、バス未開始を照合する。基準の SDK / engine revision、実行 JS 全体、
baseline・runner・診断した BSP / fixture の hash が一致する場合だけ制約へ分類する。
未知の追加失敗を既知の制約で隠さない。変更後は基準を再検証するまで通常の失敗になる。
`not_exercised` は制約により到達できなかった処理を示す。

`runtime.json` の firmware hash / CPU / boot 方式をビルド成果物と照合し、
新しい試行の前に古い runtime 記録を削除する。
ELF ができた事実と、実機を含む通信の動作保証は区別する。
