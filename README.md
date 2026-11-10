# emelino

emelinoは、マイコン向けのemela実装です。

emela本体が持つEffectシステムや関数型言語としての機能を可能な限り残したまま、マイコン上で動作するように設計されています。

現在はemela v0.26.1に対応しています。emelaのバージョンアップの少し後にemelinoも更新される予定です。

[emela](https://github.com/emela-lang/emela)

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
```

共通・ボード別セットアップの完了後、環境設定を読み込んでCLIをビルドします。

```sh
source .local/env.sh
cargo build --locked -p emelino-cli
```

各スクリプトは`--dry-run`で実行内容を確認できます。
インストール先の変更や再実行については[セットアップ手順](docs/setup.md)を参照してください。

センサーライブラリのサンプルとして、[LSM9DS1の6軸IMU読み出し](examples/lsm9ds1/README.md)を用意しています。
NUCLEO-F446REでI²CまたはSPIを使い、加速度3軸と角速度3軸をUARTへ出力します。
