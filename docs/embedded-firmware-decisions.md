# 組み込みインターフェースとfirmware接続の決定メモ

記録日：2026-11-03

GPIO・UART・SPI・I2Cの共通インターフェースと、生成Cを実機へ接続する方式についての決定を記録する。
決定した仕様、検討中の構成案、実装状況を分ける。ここに記載した機能がすべて実装・実機検証済みという意味ではない。

## 対象範囲と初期ボード

用途はホビーと一般的な産業用途とし、特殊なケースの網羅は目指さない。
次の4ボードを初期の対応・検証対象にする。

| 系統 | ボード | マイコン |
| --- | --- | --- |
| STM32 | NUCLEO-F446RE | STM32F446RE |
| Raspberry Pi Pico | 公式 Raspberry Pi Pico | RP2040 |
| ESP32 | ESP32-S3-DevKitC-1 | ESP32-S3 |
| Arduino | UNO R4 Minima | Renesas RA4M1 |

Arduinoはボード・開発環境の名前なので、初期対象はUNO R4 Minimaとする。
UNO R3などのAVR機種や、各系列の全機種への対応をこの決定に含めない。

## 周辺機器インターフェースの決定

### SPIの速度指定

- 分周比の指定をやめ、`SpiConfig.max_frequency_hz: Int`でHz単位の上限を指定する。
- Masterでは、BSPが指定上限以下で実現できる最大のSCK周波数を選ぶ。指定値を超えない。
- Masterの指定値が0以下なら`InvalidArgument`、上限以下の周波数を実現できなければ`Unsupported`。
- Slaveでは外部からSCKが供給されるため、この値を無視する。

宣言：[spi.emel](../crates/emelino-cli/lib/embedded/spi.emel)

### I2Cの読み取りを`write_read`へ統合

```text
write_read(id, address, write_data, read_size, timeout_ms)
```

- `write_data`は読み取り前に送るコマンド・レジスタアドレスのバイト列。
- 空配列なら書き込みフェーズを省略し、読み取りだけを行う。
- 空でなければ、送信後にSTOPを挟まずRepeated STARTで同じデバイスを読み、最後にSTOPを送る。
- レジスタアドレスの長さとバイト順は呼び出し側がバイト列で表す。
- 読み取り長は常に必須。1バイトの読み取りも`read_size = 1`として同じ操作を使う。
- `read_size`は正数とし、成功時は指定数ちょうどのバイトを返す。不完全な読み取りはエラーにする。
- `timeout_ms`は書き込みと読み取りを合わせた転送全体に適用する。
- 公開操作の`master_receive`と`memory_read`をこの操作へ統合する。`master_transmit`と`memory_write`は残す。

### I2Cの復旧操作

```text
recover(id, timeout_ms) -> () fails I2cError
```

- BSPが設定済みコントローラーとバスの復旧を試みる。
- タイムアウトは正数で、復旧全体に適用する。期限切れは`Timeout`。
- 別の操作がコントローラーを使用中なら`Busy`。固着したハードウェアのBUSY状態とは区別する。
- 成功時は既存の設定で次の通信を開始できる状態に戻す。失敗した通信の自動再送は行わない。
- 復旧機能自体が未対応なら`Unsupported`、検出したバス異常は`BusError`で表す。
- 操作種別に`Recover`を追加する。

宣言：[i2c.emel](../crates/emelino-cli/lib/embedded/i2c.emel)

## GPIO割り込みの接続と実行方式

### ピンごとのハンドラー登録

`Gpio.enable_interrupt(pin, edge, priority, on_event)`で、ピンごとにユーザー定義ハンドラーを登録する。
同じハンドラーを複数ピンに登録してよい。イベントは発生元のピンと、その時点で取得したレベルを持つ。

SDK側の入口が共通でも、内部で発生ピンを判別して登録先へ振り分ける。
ユーザーに全ピン分の割り込み振り分け処理を書かせる必要はない。

`InterruptPriority`はハードウェアIRQの優先度を表す。現在の宣言は`Low / Normal / High`。
物理IRQを共有するピンには独立したハードウェア優先度を設定できないため、
同じ優先度を要求するものとし、衝突は`ResourceConflict`にする。

### ライブラリの公開入口へSDKから接続する

最新の方針は、登録ハンドラーをISR内で直接実行する方式とする。

```text
HW割り込み
  → SDK/BSPの割り込みハンドラー（割り込み要因処理・発生ピン判定）
  → 組み込みライブラリが提供するC公開入口
  → そのピンに登録されたユーザーハンドラー
```

ライブラリ側に接続用の入口を用意し、ボードのSDKが定義するハンドラーから呼ぶ。
STM32F4実装では、生成headerの `emela_gpio_dispatch_interrupt(EmelaGpioPin, EmelaGpioPinState)` を入口とする。

以前検討した「GPIOイベントをキューへ入れ、main側のスケジューラーでハンドラーを呼ぶ」方式は採用しない。
このGPIO接続のためにイベントキューやスケジューラーを追加する必要はない。
既存の非同期task完了通知と、そのmain側での再開は別の契約として扱う。

### 登録インターフェースを起点にコンパイラがISR検査する

関数を登録するインターフェースはコンパイラが認識できるため、
`enable_interrupt`の`on_event`へ渡される関数をISR検査対象として扱う。

1. 登録される関数と、その関数から到達する処理に既存のISR検査を適用する。
2. 関数変数、引数による受け渡し、条件分岐などを経由しても、登録され得るすべての候補を検査する。
3. 候補のいずれかが制約に違反する場合、または候補を特定できない場合はコンパイルエラーにする。
4. ユーザーが各登録ハンドラーを手動で`isr_entries`へ列挙することは要求しない。

既存の`isr_entries`と`isr_safe_externals`による検査を再利用する。
現在の検査は、到達するsuspend、heapを伴う値や処理、再帰、実行時const初期化、
FiberのHold/Unhold、未許可の外部C呼び出しなどを拒否する。
登録先の追跡と既存検査への接続を実装した。callback本文内に残る未解決の間接呼び出しは引き続き拒否する。

言語の`R: Immediate`は、effect集合が言語上のsuspendを含まないことを制約する。
`Immediate`や`fails {} use {}`だけでISR安全性が保証されるわけではなく、上記の到達性検査も使う。
「ISRで許可するeffect名の一覧」を新設する方針は決めていない。

キャプチャなしの関数だけに限定する決定はしていない。
生成側が登録中のcallback環境を保持し、置換・解除時に解放する。STM32F4では割り込み禁止区間で登録変更を排他する。
ISRのdispatchはborrowのみで、scalar captureに対応する。ISR内で参照管理を要するcaptureは検査で拒否する。
未登録ピンへの通知は無視し、BSPのGPIO read/write/toggleをISR-safe操作として提供する。
マルチコアのBSPでは全dispatch実行主体との排他を別途実装する。

関連実装：[ISR検査](../crates/emelino-compiler/src/backend/plan/interrupts.rs)、
[CLI設定](../crates/emelino-cli/src/config.rs)、
[GPIO宣言](../crates/emelino-cli/lib/embedded/gpio.emel)

## firmware生成の構成案

生成CとSDKを接続する境界として、生成されたCヘッダーとmanifestを使う。
通常の周辺機器操作は次の経路になる。

```text
EmelaアプリのEmbedded.*操作
  → Boardハンドラー
  → 生成されたC bridge
  → BSP
  → SDK/HALのドライバー
```

ビルドは、アプリとランタイムを含む生成Cに、BSP・配線設定・SDKのドライバー・startup・リンカ設定を加えて行う。
過去の実装を参考に、生成物一覧、ABI版、必要サービス、ビルド結果と入力の対応を管理する。

STM32F4は以下の構成で実装した。残り3ボードのSDK・ビルド方式は検討案。

| ボード | SDK・ビルドの候補 | 接続案 |
| --- | --- | --- |
| NUCLEO-F446RE | STM32CubeF4 1.28.3 HAL GPIO/UART/SPI/I2C・CMSIS、Python build + ARM GCC | BSPの`main()`から生成側の入口を呼ぶ。startup/linkerはBSPが提供 |
| Raspberry Pi Pico | Pico C/C++ SDK、CMake | `PICO_BOARD=pico`で生成CとBSPを組み込む |
| ESP32-S3-DevKitC-1 | ESP-IDF | 生成CとBSPをcomponentにし、`app_main()`から起動する |
| UNO R4 Minima | ArduinoCore-renesas、arduino-cli | Cでコンパイルする生成物と、Arduino APIを呼ぶC++の接続層を組み合わせる |

STM32F4では生成側が通常処理のスケジューラーを所有し、setupのreturn後もBSPはWFIで待機してGPIO割り込みを受ける。
ほかのSDKの実行主体・起動・待機との接続は今後詰める。
GPIOのISR直接呼び出しを理由に、常駐イベントループを必須にはしない。

## 記録時点の実装状況と残作業

| 項目 | 状態 |
| --- | --- |
| SPIのHz指定 | STM32F4 BSPで上限以下の最速分周を選択。Renodeで設定値を検証 |
| I2Cの`write_read`・`recover` | HAL sequential転送とバス復旧を実装。Repeated START・全体timeoutをRenodeで検証。1バイト受信は修正版コントローラーモデル＋標準BMP180でpolling/IRQとも成功 |
| GPIOのピン別ハンドラー引数 | C bridge・登録表・所有権管理を実装。Renodeでcapture寿命・置換・解除を検証 |
| GPIOのISR直接呼び出し | 宣言コメントも同期し、HALのEXTI callbackからの直接呼び出しを実装・検証 |
| 登録先すべてへのISR検査 | 候補追跡と既存検査への接続を実装。未知候補やunsafe候補の拒否を回帰試験 |
| `Embedded.Interrupt` | `bundled.rs`へ登録済み |
| Boardハンドラー全体 | GPIO・UART・SPI・I2Cの生成CをSTM32F4 HALへ接続。初期配線はUSART2・SPI1・I2C1 |
| 新しい型のC表現 | GPIOのtagged config、SPI操作配列・nested list結果・主エラーとcleanupエラーの保持に対応 |
| 4ボードのfirmware | STM32F4の4 peripheralをSDKとlinkしRenodeで検証。ほか3ボード、書き込み・実機検証は未実施 |

各peripheralのC statusは成功0、失敗は各 `ErrorKind` のindex + 1とする。
SPI複合エラーはC structの `primary_spi` / `primary_gpio` / `cleanup_gpio` で表し、
言語側のprimaryとcleanupへ変換する。生成bridgeは入力範囲・出力長を検証する。
STM32F4のpin所有権をdriver間で共有し、同期通信はmain context・IRQ有効時に限定する。
UART・I2Cの割り込み設定は内部処理を切り替え、公開宣言にないユーザーイベント配送は追加しない。
SPI SlaveとUART RTS/CTSは初期board配線に含めず `Unsupported` を返す。
実装・実行手順は [ボード向けfirmware](embedded/README.md) を参照する。
UARTの送信完了条件など、候補として挙げただけの追加変更を決定済みとして扱わない。

2026-11-04追記：相手デバイスを含むRenode試験として、標準BMP180とMicron MT25Qを接続した。
BMP180は旧STM32モデルのADDR/AF/受信FIFO動作を修正したモデルを使用し、HAL/BSPは変更していない。
標準モデルでの制約、修正版モデルでの結果、実機の電気的検証を区別して記録する。
詳しくは[BMP180試験](embedded/stm32f4/bmp180-test.md)と
[SPI flash試験](embedded/stm32f4/spi-device-test.md)を参照する。

## ボード追加に向けた構成の分離（2026-11-04）

ボードごとにdriver・ビルド・通信試験一式を複製しない構成を採用する。

- `firmware/boards/<board>/board.json` はMCU・接続portの参照、配線、クロック方針を持つ。
  基板固有の初期化がある場合だけ `init.c` を置く。
- `firmware/chips/` はCPU/FPU、メモリ容量・アドレス、CMSIS define、package pin、IRQ表を持つ。
- `firmware/ports/<port>/` はSDKとのC ABI接続と起動・時刻・排他・SDK固有ビルドを持つ。
  同じ接続実装を複数基板から共有する。未実装controllerは定義の追加だけで対応済みにはしない。
- `firmware/build/` と `tools/build_firmware.py` はmanifest検証・実行・成果物記録と共通入口を担う。
  各SDKのビルド方式をportへ委ねる。
- `firmware/common/` はSDK非依存の処理を持つ。SPIのdevice管理・操作列・CS cleanup・複合エラーを
  ここへ抽出する。割り込み・pin所有権・時刻取得は当面port側に残す。
- `tests/embedded/scenarios/` は通信手順と期待値、`targets/` は試験配線と適用シナリオ、
  `support/` は実行基盤、`ports/` はSDK固有の検査・注入を持つ。Renodeモデルは `tests/renode/` に置く。
- 接続仕様・実行手順・結果は `docs/embedded/` にまとめる。

BMP180・MT25Qの試験はtargetからバス番号・CS設定を受け取り、ボード追加でアプリを複製しない。
共通入口は `tools/test_embedded.py --board <board> --scenario <scenario>` とする。
STM32F4固有のレジスタ・HAL検査と、別MCUへ流用できる通信手順の適用範囲を区別する。

実行手順と配置は [組み込みfirmware](embedded/README.md) を参照する。

## 過去実装の参照先

参照元は兄弟リポジトリ`../emela-codegen-emb-cli`を中心にする。

- [ボード構成・検証の説明](../../emela-codegen-emb-cli/crates/emela-codegen-emb/docs/boards.md)
- [生成物とplatformの接続契約](../../emela-codegen-emb-cli/crates/emela-codegen-emb/docs/artifact-contract.md)
- [NUCLEO-F446REの定義](../../emela-codegen-emb-cli/crates/emela-codegen-emb/boards/stm32/stm32f4/boards/nucleo-f446re.json)
- [組み込みターゲットのISR設定](../../emela-codegen-emb-cli/spec/targets/c-embedded.md)
- [Immediate制約](../../emela-codegen-emb-cli/spec/language/09-types.md)

旧STM32実装には直接MMIOのGPIO・固定Console・Clock、startup、vector、linker、build/flash処理がある。
I2C/SPIや汎用UART転送の完成済みBSPとしては扱わない。
旧ESP32対象はC3であり、S3のstartup・命令・レジスタ設定へそのまま流用しない。
旧実績はF4のRenodeとC3のQEMUによる検証を含むが、今回の4ボードの実機検証とは分ける。
