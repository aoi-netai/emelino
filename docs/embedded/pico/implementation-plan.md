# Raspberry Pi Pico / Pico 2 対応計画

作成日：2026-11-10。実装前の計画であり、ビルド・実行成功の報告ではない。
実装後の記録：[接続仕様](README.md)、[検証結果](validation.md)。以下の受入チェックは検証後に更新した。
作業ブランチ：`feat/pico-rp2040-rp2350`。
調査時の基点：`547f551540ba65d17116728dae61d10a86442c18`。

## 採用方針と完了の基準

Pico SDKを使い、Emelaソースから生成したCとBSPをリンクして、2ボードのELFを作れるようにする。
実行検証には無料のOSSである[c1570/rp2350js](https://github.com/c1570/rp2350js)を使用する。
利用者が有料ライセンスやクラウド実行環境を用意する必要のない構成にする。

今回のボード対応では、ユーザーの指定に従って**正規のPico SDKビルドによるELF生成を主要な受入条件**とする。
GPIO・UART・SPI・I2Cを使用する代表アプリと、その同時使用を両ボードでコンパイル・リンクする。
SDKによる起動処理、必要なシンボル、メモリ配置、成果物を確認し、rp2350jsで実行できる範囲を追加検証する。
ELF生成の成功と実行確認の成功は、結果にそれぞれ記録する。

エミュレータ未実装部分の完成や実機検証を、このボード追加の必須条件にはしない。
一方、実装したBSPのエラーや根拠不明の実行失敗は修正対象とし、エミュレータ制約へ自動的に分類しない。
既存コンパイラ機能・STM32・nRF52の検証方針は継続する。本書の受入条件は今回のPico対応に限定する。

## 対象ボードと初期範囲

| emelinoのボードID | 公式ボード | MCU / CPU | SDK設定 | Flash / SRAM |
| --- | --- | --- | --- | --- |
| `raspberry-pi-pico` | Raspberry Pi Pico | RP2040 / Cortex-M0+ | `PICO_BOARD=pico`, `PICO_PLATFORM=rp2040` | 2 MiB / 264 KiB |
| `raspberry-pi-pico-2` | Raspberry Pi Pico 2 | RP2350A / Cortex-M33 | `PICO_BOARD=pico2`, `PICO_PLATFORM=rp2350-arm-s` | 4 MiB / 520 KiB |

両方ともcore 0を使用する。RP2350はArm版を初期対象とし、RISC-V向けtoolchainを追加しない。
Pico W / Pico 2 W、マルチコア、USB CDC、Wi-Fi、PIO・DMA・PWM・ADCの公開API追加、
実機への書込み機能は後続作業とする。Analogの既存GPIO設定はADC変換機能とは区別する。

公開APIとC ABIは既存の`BoardGpio` / `BoardUart` / `BoardSpi` / `BoardI2c`を使用する。
GPIO callbackの登録表、captureの寿命管理、ISR到達性検査をPico用に複製しない。
初期実装は各通信種別1コントローラーとし、対応外の公開IDには`Unsupported`を返す。

| 公開ID・用途 | ハードウェア | 初期配線 |
| --- | --- | --- |
| `GpioPin(n)` | GPIO n | ヘッダー上のGP0〜GP22・GP26〜GP28と、基板LEDのGP25 |
| LED | SIO GPIO | GP25、active-high |
| `Uart0` | UART0 | TX GP0 / RX GP1 |
| `Spi1` | SPI0 | SCK GP18 / MOSI GP19 / MISO GP16、CSの例 GP17 |
| `I2c1` | I2C0 | SDA GP4 / SCL GP5 |
| 外部入力の例 | GPIO | GP2 / GP3 |

`Spi1` / `I2c1`は公開APIの先頭IDであり、SDKの`spi0` / `i2c0`へ接続する。
C ABIは公開enumのordinalを使うため、既存と同じくSPI/I2Cとも先頭の値は0になる。
GP番号とヘッダーの物理ピン番号を区別し、GP23・GP24・GP29など基板内部用ピンを公開対象から外す。
外付けFlash容量はboardの情報として持ち、MCUに内蔵されたFlash容量とは扱わない。

## 既存実装との接続

| 現在の構成 | 今回の変更 |
| --- | --- |
| `firmware/build/cli.py`がboardのportを選択 | 同じ入口から共通の`pico_sdk` portを選択する |
| `firmware/build/artifact.py`が生成C manifestを検証 | ABI 2の入力・source一覧・追加source指定をそのまま利用する |
| `firmware/build/arm_gcc.py`と`common/cortex_m4f/`がM4F起動・リンクを所有 | PicoはSDKのCMake・startup・linkerを使用する |
| `firmware/common/spi.c`がdevice・CS・複合操作を所有 | Picoにも流用し、転送・GPIO・時刻・排他のhookを実装する |
| `tools/test_embedded.py`が`renode.py`を直接読み込む | simulatorを選べる入口へ拡張する |
| `support/common.py`がビルド前にRenodeの存在を必須確認 | 生成C/firmwareビルドをsimulatorの探索・実行から分ける |
| target設定の`renode_port`と固定のboot情報 | simulator別の選択、起動方式、実行結果を表せるようにする |
| `tools/setup/lib.sh`の環境読込み対象が固定 | Pico SDK・rp2350js用の環境fragmentも読み込む |

M0+向けにM4F用startupを拡張したり、M33の起動処理を独自に作ったりしない。
SDKが選ぶCPU/FPU/float ABIと起動用メタデータを使用する。
Emela本体の固定revisionやfrontendを、このボード追加のために変更しない。

## 実装順序

### 1. SDKとエミュレータの基準を固定する

Pico SDKは調査時の安定版[2.3.1](https://github.com/raspberrypi/pico-sdk/releases/tag/2.3.1)を初期候補とする。
両ボードの素のCサンプルをビルドし、同じSDK版をrp2350jsに接続して、起動・GPIO・UART・時刻の到達範囲を確認する。
この段階はSDKとエミュレータの組合せを確認するもので、emelino対応完了とは扱わない。

- Pico SDK、必要なsubmodule、ELF/UF2後処理に使うpicotool、rp2350jsのrevisionを固定する。
- rp2350jsは`main`追従やnpmの範囲指定を避け、採用したcommitまたは固定パッケージとlockfileを記録する。
- 使用するNode.js/npm、Arm GNU Toolchain、CMake/Ninjaの版と必要条件を記録する。
- RP2040/M0+とRP2350/M33を明示選択し、両方を同じrp2350js由来のengineで動かす。
- 公開CLIで必要な観測ができれば利用し、足りないGPIO入力・UART・バスの接続には小さなNode runnerを追加する。
- 配布済みJSを使うかsourceからビルドするかを固定し、テストに不要なGUI・MCP server・背景daemonを起動しない。
- bootromの供給方法・revisionと、reset起動かimage直接ロードかを記録する。直接ロードはその方式として表示する。

成果物：依存固定情報、再現コマンド、2ボードの基準ログ、実行可能な機能と既知の制約の一覧。
SDKのCサンプルでも失敗する項目は、最小再現とエミュレータのソース・上流報告を調べて分類する。
全周辺機器のエミュレータ修正へ作業範囲を広げず、BSPの実装とELF生成を進める。

### 2. 2ボード共通のPico SDKビルドを追加する

- `firmware/boards/raspberry-pi-pico/board.json`と`raspberry-pi-pico-2/board.json`を追加する。
- `firmware/chips/rp2040.json`と`rp2350a.json`を追加し、両方から`port: "pico_sdk"`を参照する。
- `firmware/ports/pico_sdk/build.py`とCMake設定を追加し、SDKの`pico_sdk_init()`と通常のcompile/linkを使用する。
- `--sdk-dir` / `PICO_SDK_PATH`でSDKを選択し、`--cc` / `ARM_GCC`、最適化、追加C source・link引数も接続する。
- 生成Cのsource一覧はmanifestから取得する。CMake内で未申告Cをglobして追加しない。
- ビルド時にSDK・ツールを暗黙downloadしない。setup段階で用意し、欠けていれば理由を示して失敗する。
- board・platform・SDK・最適化が違うビルドのCMake cacheを使い回さない。
- `firmware.elf` / `firmware.bin` / `firmware.uf2` / `firmware.map`、symbol・resource情報とbuild logを出力する。
- `firmware.json`は全必須成果物の存在と検査成功後だけ発行する。失敗時に前回の成功記録を残さない。

Pico用の成果物検査では、ELFのarchitecture、配置、entry/vector、未解決symbol、Flash/RAMの収まりを確認する。
RP2040のboot2やRP2350のimage情報を含むため、BINの先頭8bytesをSP/reset vectorとする既存M4F検査は流用しない。
RAM容量全体と実際にSDKがアプリへ割り当てる領域も区別する。
外付けFlash容量はboard設定とSDK設定の一致を検査する。

既存のstack予約量・guard予約量をSDKのlinker設定へ反映し、容量検査に含める。
guardは領域予約であり、MCUによる実行時メモリ保護を保証するものとはしない。
SDKの不要なstdio、USB、動的確保を持ち込む機能を有効にせず、system allocatorの混入検査を継続する。
必要に応じて、SDKに依存しないsymbol/size集計だけを共通helperへ抽出する。

完了条件：最小のEmelaアプリが両ボードでELF/BIN/UF2へなり、`-Os` / `-O0`の4ビルドが成功する。

### 3. runtime・ピン所有権・GPIOを接続する

`firmware/ports/pico_sdk/`にplatform、pins、time、GPIOの実装を追加する。
SDKの`main()`から既存の`emela_embedded_start()`へ接続し、初期化hook、defect、idle、event lockを提供する。
data/BSS初期化はSDKへ任せ、アプリ入口の終了後もGPIO購読と割り込みを維持する。

- 時刻はSDKの単調増加microsecond counterを利用する。M4F用SysTick実装は持ち込まない。
- core 0の割り込み状態を保存・復元できるcritical sectionで、pin所有権とcallback登録変更を保護する。
- GPIOのinput/output、pull、read/write/toggleと、可能な既存OpenDrain/Analog設定を接続する。
- GPIO ISRから既存の`emela_gpio_dispatch_interrupt`へ実際のpinとlevelを渡す。
- 共有IRQの優先度競合、Rising/Falling/Both、解除・再登録、capture保持の契約を維持する。
- ISR中の設定変更・通信など、既存で拒否する操作を同じ型付きエラーで拒否する。

完了条件：GPIOアプリが両ボードでビルドでき、engineが対応する範囲でLED出力・入力・callbackを確認できる。
GPIO割り込みを観測できない場合も、pollingへの置換でcallback成功と報告しない。

### 4. UART・SPI・I2Cを接続する

| 機能 | 実装する内容 |
| --- | --- |
| UART | configure、送受信、exact/some/discard、timeout、IRQ受信buffer、IRQ送信通知の接続 |
| SPI | Masterの8/16 bit、mode 0〜3、転送、deviceごとの設定切替、CS、複合操作、recover |
| I2C | 7 bit Master、送信、任意prefix＋Repeated STARTの`write_read`、pure read、memory_write、IRQ設定、recover |
| 共通 | ピン競合、未設定、未対応の設定・ID、main/ISR文脈、失敗後の設定と所有権の維持 |

SDKのblocking APIが既存のtimeout・エラー・割り込み契約を満たすかを操作ごとに確認する。
UART/SPIで期限のない待機に入らず、I2Cもprefixとreadを合わせた一つのdeadlineを使用する。
割り込みAPIは名前だけの成功にせず、実際のIRQ設定と必要な完了処理へ接続する。
SDKの高水準APIだけで足りない処理は、SDKのレジスタ定義を使った限定的な接続を実装する。

SPIの実現SCKは指定上限以下とする。SDKの丸め・最小速度で上限を超える場合は`Unsupported`を返す。
LSB-firstやSlaveなど、今回接続しない設定も明示的に`Unsupported`を返す。
I2Cのaddress-only ready確認は、SDKの長さ0送信で実現できると仮定しない。
初期portで正しいaddress-only動作を提供できなければ`is_device_ready`は`Unsupported`とし、
ダミーwrite/readを相手へ送って代用しない。対応表とサンプルにもこの制限を記載する。
復旧ではCS解除、controller再初期化、必要な期限付きSCL/STOP処理を実装する。

完了条件：4driverの個別・同時使用アプリが両ボードでELFへなり、未対応操作の境界が明文化されている。
SPI共通層は既存実装を使用し、Pico側にdevice表やCS cleanupロジックを複製しない。

### 5. rp2350jsを共通試験入口へ接続する

`tools/test_embedded.py --board <board> --scenario <scenario>`を共通入口のまま利用する。
targetにsimulator選択を追加し、既存targetの`renode_port`は互換読込みする。
生成C/SDKビルドとsimulator探索を分け、Pico試験にRenodeを必須にしない。
必要な変更は薄いadapterに留め、各simulatorのデバイスモデルAPI全体を共通化しない。

- `tests/embedded/ports/pico_sdk/`と`tests/rp2350js/`にboard接続・Node runner・試験用peerを配置する。
- CPU modelをboardから選び、RP2350をRP2040 modelで代用しない。
- 正規SDKビルドの成果物をロードし、試験のためにBSPや生成Cの処理を別実装へ差し替えない。
- ELFのsymbolを使う観測と、実行用UF2/BINのhashを対応させる。
- 時刻・GPIO入力・UART受信・I2C/SPI peerの操作と、命令数/host時間の上限・終了条件を実装する。
- 既存のEmelaアプリ・`Test.Target`・C probeを可能な範囲で再利用する。
  Renode固有のPython観測コードはPico向けadapterへ分ける。
- 最初は起動、GPIO出力、UART送信。続いてGPIO callback、UART受信、SPI/I2C転送、timeout・競合を確認する。
- BMP180/MT25Qは必要なpeerを移植できる場合に追加し、最初からRenodeモデル全体を移植しない。

reportには少なくともboard/CPU、SDK/toolchain/engineの版、firmware hash、boot方式、
`build_status`、`runtime_status`、観測値、ログ、モデル制約を保存する。
実行結果は`passed` / `failed` / `model_limited` / `not_run`を区別し、ビルドのみの成功を実行成功へ変換しない。
`model_limited`には素のSDKサンプルによる再現などの根拠と対象機能を付ける。
engine未導入・設定誤り・host timeoutだけでは`model_limited`にせず、試験環境または実行の失敗として扱う。
実行可能と確認したシナリオの失敗は共通runnerの非0終了コードへ反映する。

完了条件：両board targetがrp2350jsを選択でき、再現可能な実行結果または根拠付きの制約を出力する。
共通runnerの変更後もSTM32/nRF52のRenode試験が同じコマンドで動く。

### 6. setup・サンプル・回帰確認を仕上げる

- `tools/setup/pico-sdk.sh`でSDK・CMake/Ninja・picotoolを準備し、`--dry-run`と既存checkout検査を用意する。
- rp2350js用setupで固定依存を準備する。共通setupの利用者全員にNode.jsを必須にしない。
- `.local/env.sh`から`PICO_SDK_PATH`とengineの配置を読み込めるようにする。
- 同じEmelaアプリを2ボードへビルドできるGPIOサンプルと4driver同時使用サンプルを追加する。
- README、setup手順、embedded索引、Picoの接続仕様・対応表・検証結果を更新する。
- 初期ボードを列挙する`docs/embedded-firmware-decisions.md`にもPico 2と今回の採用方針を追記する。
- ビルド入力拒否、board/platform不一致、未対応ID、失敗後の古い成功記録、target誤選択を確認する。
- 既存のcompiler・firmware契約試験とRenodeシナリオを回帰確認し、Picoの`-Os` / `-O0`を記録する。

## 変更する主な場所

| 追加・更新先 | 内容 |
| --- | --- |
| `firmware/boards/raspberry-pi-pico{,-2}/board.json` | SDK board選択、Flash容量、利用可能pin、配線 |
| `firmware/chips/rp2040.json`, `rp2350a.json` | CPU/RAM・MCU識別情報 |
| `firmware/ports/pico_sdk/` | CMake/build adapter、platform、pins/time、4driver |
| `firmware/build/` | 必要な成果物集計の共通化。M4F専用検査は維持 |
| `tests/embedded/support/`, `tools/test_embedded.py` | ビルドとsimulator実行の分離、結果記録 |
| `tests/embedded/targets/raspberry-pi-pico{,-2}.json` | 2ボードの配線・シナリオ・engine選択 |
| `tests/embedded/ports/pico_sdk/`, `tests/rp2350js/` | engine接続、観測、peer、依存固定情報 |
| `tests/firmware/`, `tools/setup/`, `examples/`, `docs/embedded/pico/` | 必要な回帰試験、導入、利用例、接続仕様と実測結果 |

表の`{,-2}`は2つの予定pathを短く表したもので、実ファイル名には含めない。

## 受入チェック

- [x] 2ボードが同じ`pico_sdk` portを使用し、生成C ABI 2からビルドできる。
- [x] GPIO/UART/SPI/I2Cと同時使用の代表アプリが、両ボードの`-Os` / `-O0`でELF/BIN/UF2へなる。
- [x] SDKの起動・リンク設定を使い、M0+/M33・メモリ配置・必要symbol・system allocator混入を検査する。
- [x] スタック・予約領域とFlash/RAM容量を確認し、各成果物のhash・SDK/toolchain版を保存する。
- [x] 両ボードをrp2350jsで検証し、実行成功とモデル制約を個別に記録する。
- [x] 実行失敗の原因が未判定のまま、ビルド成功だけで実行成功を報告しない。
- [x] 未対応の設定・ID・I2C readyなどが正しい型付きエラーとなり、対応表に記載されている。
- [x] setupから再現でき、通常のビルド・試験実行で有料契約・クラウドサービスを要求しない。
- [x] 共通部分を変更した範囲の既存試験と、STM32/nRF52のRenode回帰を確認する。

## 実装時の参照先

- [Pico SDK 2.3.1](https://github.com/raspberrypi/pico-sdk/tree/2.3.1)
- [公式Pico board定義](https://github.com/raspberrypi/pico-sdk/blob/2.3.1/src/boards/include/boards/pico.h)
- [公式Pico 2 board定義](https://github.com/raspberrypi/pico-sdk/blob/2.3.1/src/boards/include/boards/pico2.h)
- [rp2350js README・既知の未実装項目](https://github.com/c1570/rp2350js#readme)
- [rp2350js package設定・公開入口](https://github.com/c1570/rp2350js/blob/main/package.json)
- [現在のfirmware構成](../README.md)
- [既存C生成・検証方針](../../implementation-plan.md)

rp2350jsのREADMEにある未実装一覧は調査時点の情報であり、個々のアプリの失敗原因を確定するものではない。
実装では固定したrevisionに対して確認し、変更したエミュレータを使う場合もrevisionと差分を記録する。
