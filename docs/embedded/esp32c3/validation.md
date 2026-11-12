# ESP32-C3-DevKitM-1-N4Xの検証記録

2026-11-11に、ESP-IDF 5.4.1、riscv32-esp-elf GCC 14.2.0、Renode 1.16.1で実行した。
対象は`feat/esp32c3-devkitm-1-n4x`。SDK revision、モデルrevision、ROM ELFと成果物の
SHA-256、ソースのSHA-256は[validation.json](validation.json)に記録している。

| 検査 | 結果 |
| --- | --- |
| `emelino new` → 通常の `emelino build` | 外付けLEDテンプレートからELF/BIN作成に成功。SDKとC exportの衝突なし |
| 4driverサンプルの実SDKビルド | Os / O0でELF、bootloader、partition、結合BIN作成に成功 |
| Renode smoke | Os / O0で3チェック、GPIO7の出力・出力有効、UART `EMELINO`、SDK task起動、正常終了を確認 |
| `--build-only` | ビルド成功と`runtime_status: not_run`を確認。Renodeへの依存なし |
| SDK初期化のpanic | 両Renode実行でpanic pointerが0、board defectが0 |
| SDK build境界の回帰 | ESP32-C3を含む35件成功 |
| driverのホスト試験 | 10ケース × O0/Os、計20実行がUBSan付きで成功 |
| 組み込み試験runnerの回帰 | 12件成功 |
| setupの回帰 | 13件成功。新setupのdry runを含む |
| CLI project workflow | 8件成功。ESP32-C3のテンプレート、C export、依存不足の診断を含む |

ホスト試験ではピンの部分予約が残らないこと、IRQ受信の64/65ワード境界と復旧、
16bit SPIのMSB/LSBのbyte順、timeout後のCS解除・転送記述子の寿命、SPI初期化失敗時の解放、
I2Cのprefix/repeated START・NACK・timeout・確保失敗・buffer解放を確認した。

Renodeの起動は`rom-init-elf-bootstrap`。ROM CRT0を実行し、ROM mainからELF entryへ
フックで移動する。CLIC dispatch、cycle counter CSR、WFIのNOP化も使用している。
フラッシュbootloaderからの通常起動を確認した結果ではない。
外部モデルのRTC BiasConf+4に関する警告は、モデル付属のHello Worldでも発生する。
smokeの各観測値とこれらの警告は実行reportへ保存している。

実機への書き込みと、SPI/I2Cの実デバイスとの通信は実施していない。
外部Renodeモデルの汎用SPIとI2Cはスタブであり、これらの通信をRenodeで検証したとは扱わない。
GPIOの外部入力・callbackとUART RX/IRQも、今回のRenode smokeの対象外。

## main統合後の確認（2026-11-12）

LSP対応を含む最新mainと統合し、テンプレートにESP32-C3の外付けLED・toolchainの説明と
LSPのエディター設定を残した。`cargo test --locked --offline -p emelino-cli --test project --test lsp`
でprojectの8件と実stdioのLSP統合試験1件が成功し、`cargo fmt --all -- --check`も成功した。
MCU portの実装は変更していない。上記のSDK/Renode実行と`validation.json`は
`2fcf3f3`で記録した2026-11-11の検証結果を示す。
