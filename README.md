# emelino

emelinoは、マイコン向けのemela実装です。

emela本体が持つEffectシステムや関数型言語としての機能を可能な限り残したまま、マイコン上で動作するように設計されています。

現在はemela v0.27.0に対応しています。emelaのバージョンアップの少し後にemelinoも更新される予定です。

[emela](https://github.com/emela-lang/emela)

## プロジェクトを作る

ビルドした`emelino`をPATHへ追加すると、リポジトリ外にアプリを作成できます。

```sh
cargo build --locked -p emelino-cli
export PATH="$PWD/target/debug:$PATH"
emelino boards
emelino new my-device --board raspberry-pi-pico
cd my-device
emelino build
```

`Emelino.toml`、`src/main.emel`、READMEと`.gitignore`を生成します。
サンプルは指定ボードのLEDを点灯します。`build`は設定を読み、`src/`以下のソースから
Cを生成し、ボードのSDKでファームウェアを作ります。ツールやSDKが不足している場合は、
同梱セットアップスクリプトの実行方法を表示します。導入後は`.local/env.sh`を読み込んでください。

SDKを導入する前にC生成だけを試す場合は、`emelino build --emit c`を使用できます。
ファームウェアビルダーとボード定義はCLIに同梱されるため、ビルド済みバイナリの移動後も
元のリポジトリを参照しません。[設定と生成物の詳細](docs/projects.md)を参照してください。

## Setup

ここではemelino本体を動かすために必要な共通ツールと各マイコンボードに対応したセットアップ手順を説明します。


### 共通ツール

WSL上のUbuntu、またはUbuntu 22.04以降で実行してください。

```sh
bash tools/setup/common.sh
```

Rust/Cargo、Cコンパイラ、Python 3、Arm GNU Toolchain、newlib/newlib-nanoを導入します。


### マイコンボード

使用するボードのSDKをセットアップします。

```sh
# NUCLEO-F446RE: STM32CubeF4 1.28.3
bash tools/setup/nucleo-f446re.sh

# Arduino Nano 33 BLE: Nordic nrfx 3.2.0 + CMSIS 5.9.0
bash tools/setup/arduino-nano-33-ble.sh

# Raspberry Pi Pico / Pico 2: Pico SDK 2.3.1 + picotool
bash tools/setup/pico-sdk.sh

# ESP32-C3-DevKitM-1-N4X: ESP-IDF 5.4.1 + RISC-V toolchain
bash tools/setup/esp32-c3-devkitm-1-n4x.sh
```

共通・ボード別セットアップの完了後、環境設定を読み込んでCLIをビルドします。

```sh
source .local/env.sh
cargo build --locked -p emelino-cli
```

各スクリプトは`--dry-run`で実行内容を確認できます。
インストール先の変更や再実行については[セットアップ手順](docs/setup.md)を参照してください。

[Pico / Pico 2の手順](docs/embedded/pico/README.md)では、同じEmelaアプリから
RP2040・RP2350向けのELF/BIN/UF2を作れます。任意の実行検証には無料のローカルOSSである
rp2350jsを使用します。GPIO・4driver同時使用サンプルと、ビルドのみの検証入口も用意しています。

[ESP32-C3の手順](docs/embedded/esp32c3/README.md)では、ESP-IDF向けELFとフラッシュ用BIN、
GPIO・UART・SPI・I2Cのサンプル、外部Renodeモデルによる起動・GPIO・UARTのsmoke試験を用意しています。

センサーライブラリのサンプルとして、[LSM9DS1の6軸IMU読み出し](examples/lsm9ds1/README.md)を用意しています。
NUCLEO-F446REでI²CまたはSPIを使い、加速度3軸と角速度3軸をUARTへ出力します。
