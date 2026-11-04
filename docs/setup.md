# WSL / Ubuntu のセットアップ

Ubuntu 22.04以降と、WSL上のUbuntuを対象とする。以下のコマンドはUbuntuのBashで、
emelinoリポジトリのルートから実行する。WSLではLinux側のホームディレクトリ配下への
checkoutを推奨する。

## 共通ツール

```sh
bash tools/setup/common.sh
```

aptで以下を導入する。パッケージ操作には`sudo`を使用するため、必要に応じてパスワードを入力する。
スクリプト全体を`sudo`で実行する必要はない。

| ツール | 用途 |
| --- | --- |
| Git・curl・CA証明書 | 依存関係とSDKの取得 |
| build-essential | ホスト用Cコンパイラ・リンカー |
| Python 3（3.10以降） | ファームウェアのビルドと試験スクリプト |
| coreutils | `sha256sum`などの基本コマンド |
| gcc-arm-none-eabi・binutils-arm-none-eabi | ARM向けコンパイル、ELF/BIN生成、容量確認 |
| libnewlib-arm-none-eabi・libnewlib-dev | ARM向けCライブラリとnewlib-nano |

Rust/Cargoは[公式rustup](https://rust-lang.org/learn/get-started/)でstable toolchainを導入する。
インストール済みのrustupは再利用し、stable toolchainを最新に更新する。
`CARGO_HOME`と`RUSTUP_HOME`を設定済みの場合は、その配置を使用する。
ホストのC11コンパイルと、Cortex-M4F向けの`nano.specs` / `nosys.specs`を使ったlinkを
実行し、成功後に環境設定を保存する。ARMツールはUbuntuパッケージのバージョンを使用する。
既存の別toolchainを使う場合は、実行前に`ARM_GCC`へgcc実行ファイルを指定する。

## ボード別SDK

使用するボードのスクリプトを実行する。両方を実行して共存させることもできる。

| ボード | コマンド | 取得するSDK |
| --- | --- | --- |
| NUCLEO-F446RE | `bash tools/setup/nucleo-f446re.sh` | STM32CubeF4 **1.28.3**、必要なHALとCMSIS device submodule |
| Arduino Nano 33 BLE | `bash tools/setup/arduino-nano-33-ble.sh` | Nordic nrfx **3.2.0**、CMSIS **5.9.0** |

SDKは公式リポジトリの指定tagから取得する。
[STM32CubeF4](https://github.com/STMicroelectronics/STM32CubeF4/tree/v1.28.3)では
HALとCMSIS deviceを親リポジトリが指定するcommitへ揃え、使用しないBSPなどは取得しない。
[nrfx](https://github.com/NordicSemiconductor/nrfx/tree/v3.2.0)と
[CMSIS](https://github.com/ARM-software/CMSIS_5/tree/5.9.0)も固定したtagを使用する。
SDKのライセンスは各取得先に含まれるものを参照する。

## 環境設定とビルド

共通・ボード別セットアップの完了後に、同じターミナルで環境設定を読み込む。
新しいターミナルを開いたときも読み込む。

```sh
source .local/env.sh
cargo build --locked -p emelino-cli
```

`env.sh`は`ARM_GCC`、`RUSTUP_TOOLCHAIN=stable`、Cargoの`PATH`と、導入したボードの
`STM32CUBE_F4` / `NRFX_ROOT` / `CMSIS_ROOT`を設定する。
Rustの依存関係と固定したEmela本体のライブラリは、初回のCargoビルド時に取得する。
続きは[ファームウェアのビルド](embedded/README.md#ビルド)と
[Nano 33 BLEのビルド](embedded/nrf52/README.md#ビルド)を参照する。
そこで手動指定しているSDKの`export`は、この環境設定で済んでいる。

## 実行内容の確認・インストール先の変更

各スクリプトで`--dry-run`と`--prefix`を使用できる。
`--dry-run`ではapt・ダウンロード・ファイル作成を実行せず、予定している操作を表示する。

```sh
bash tools/setup/common.sh --dry-run
bash tools/setup/nucleo-f446re.sh --dry-run

# 配置を変える場合は、全スクリプトに同じprefixを指定する。
bash tools/setup/common.sh --prefix "$HOME/.local/share/emelino"
bash tools/setup/nucleo-f446re.sh --prefix "$HOME/.local/share/emelino"
source "$HOME/.local/share/emelino/env.sh"
```

既定の配置は次のとおり。セットアップはシェルの起動設定ファイルを編集しない。

```text
.local/
  env.sh
  env.d/
    common.sh
    nucleo-f446re.sh
    arduino-nano-33-ble.sh
  embedded-sdk/
    STM32CubeF4-1.28.3/
    nrfx-3.2.0/
    cmsis-5.9.0/
```

同じコマンドは再実行できる。SDKのtag・取得元・変更状態を確認して再利用する。
既存SDKの変更や、スクリプトが取得していない同名ディレクトリがある場合は停止するので、
別の`--prefix`を指定する。SDK取得が途中で失敗した場合は、一時ディレクトリを片付けて停止する。

Renodeによるシミュレーションには、別途[Renode](https://renode.readthedocs.io/en/latest/introduction/installing.html)
を導入し、必要に応じて`RENODE`へ実行ファイルを指定する。検証済み版は1.16.1。
導入後は[Renode試験](embedded/README.md#renode試験)を参照する。

セットアップスクリプトの回帰試験は次で実行する。aptとrustupはテスト用コマンドへ置換し、
SDKはローカルGitリポジトリから取得するため、システムの変更やネットワークアクセスは発生しない。

```sh
python3 tests/setup/test_setup.py
```
