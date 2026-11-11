# アプリケーションプロジェクト

`emelino new`は、新しいディレクトリにボード別のアプリを作成する。
SDK、Python、ネットワーク接続はプロジェクト作成に不要。既存ディレクトリは空でも変更しない。

```sh
emelino boards
emelino new my-device --board nucleo-f446re
cd my-device
emelino build
```

対応するboard IDは`nucleo-f446re`、`arduino-nano-33-ble`、
`raspberry-pi-pico`、`raspberry-pi-pico-2`。
名前はディレクトリ名から取り、先頭は英字、以降は英数字・`_`・`-`を使う。
`--name my-device`を指定すると、空白を含むディレクトリ名などにも作成できる。

```text
my-device/
├── Emelino.toml
├── src/
│   └── main.emel
├── README.md
└── .gitignore
```

`src/main.emel`は基板LEDを点灯する最小アプリであり、点滅や追加配線は必要としない。
Emelinoの論理pin IDを使用する。Nano 33 BLEはNordic bare-metal portを使用する。
実機への書き込みは別の手順で行う。

## プロジェクト設定

```toml
[project]
name = "my-device"
board = "nucleo-f446re"
entry = "src/main.emel"

[resources]
heap_bytes = 1024
```

`project.name`と`project.board`は必須。`project.entry`の既定は`src/main.emel`。
entryは`src/`以下の`.emel`ファイルを指定し、起動関数には引数のない`pub fn main`を使う。
CLIは`main`のC公開名を`app_main`にし、起動用の設定を補完する。
ソース側の`@export(c)`や、手書きの`exports`設定は不要。

`src/`以下の`.emel`を再帰的に収集する。例えば`src/math.emel`は`import Math`で使い、
`src/device/sensor.emel`は`import Device.Sensor`で使う。
entryを`src/app/main.emel`に変えてもモジュールの基準は`src/`のまま。
ソースツリー内のシンボリックリンクは、循環や外部ソースの意図しない取込みを避けるため拒否する。
未知の設定項目、絶対entryパス、`..`を含むパスも拒否する。

`[resources]`には既存compiler設定と同じ容量指定を使用できる。
`heap_bytes`、`string_capacity`、`peripheral_buffer_capacity`、`const_diagnostic_bytes`、
`fiber_capacity`、`frame_capacity`、`frame_bytes`、`task_capacity`、`event_capacity`、
`deferred_capacity`を受け付ける。省略時はcompilerの既定値を使う。
契約と容量の扱いは[構成と機能契約](architecture.md)を参照する。

ネイティブCやlink引数を追加する場合は`[firmware]`を使用する。

```toml
[firmware]
optimization = "Os"
stack_bytes = 16384
guard_bytes = 256
sources = ["native/device.c"]
link_args = ["-Wl,--defsym=example=1"]
```

上記の最適化・stack・guardの値は既定値。`sources`と`link_args`の既定は空のリスト。
最適化は`O0`・`O1`・`O2`・`Os`を使用できる。stackは正、guardは32 bytes以上とし、
どちらも8 bytesの倍数を指定する。RAM容量などのボード固有制限は既存SDK adapterも検査する。
`sources`はプロジェクトルートからの相対パス。追加Cは生成ヘッダーのABIに従う。

## ビルドと環境構築

`emelino build`はカレントディレクトリから親を辿り、最初の`Emelino.toml`を読む。
`emelino build /path/to/my-device`のようにディレクトリを指定することもできる。

```sh
# ソース → C → ボードのファームウェア
emelino build

# ソース → C。Python・クロスコンパイラ・SDKは不要
emelino build --emit c

# 手元のクロスコンパイラやSDKを使う
emelino build --cc /path/to/arm-none-eabi-gcc --sdk-dir /path/to/sdk
```

`--cc`と`--sdk-dir`の相対パスは、コマンドを呼び出したディレクトリから解決する。
`--cc arm-none-eabi-gcc`のようなコマンド名はPATHから探す。
環境変数`ARM_GCC`、`STM32CUBE_F4`、`NRFX_ROOT`、`CMSIS_ROOT`、
`PICO_SDK_PATH`、`picotool_DIR`も既存ビルダーと同じ方法で使用できる。
Pythonは`python3`を使い、`EMELINO_PYTHON`で実行ファイルを変更できる。

ファームウェアビルダー、Cのport、ボード・chip定義、Ubuntu / WSL用のsetupスクリプトは
CLIビルド時に埋め込む。ファームウェアビルド時に`target/emelino/support-<内容ID>/`へ展開し、
Pythonから呼び出す。元のcheckoutやCLIバイナリの配置には依存しない。
SDKそのものは埋め込まず、通常ビルドは追加ダウンロードやシステムへのインストールを行わない。

初回にツールやSDKが不足している場合は、失敗理由と同梱setupスクリプトのコマンドを表示する。
表示された共通・ボード別のコマンドを実行し、`.local/env.sh`を読み込む。
スクリプトは既存の[セットアップ手順](setup.md)と同じもので、apt操作などはユーザーが
そのコマンドを実行したときに行う。CLIはスクリプトを自動実行しない。

## 生成物

```text
target/emelino/<board>/
├── generated/
│   ├── emela_embedded.c
│   ├── emela_embedded.h
│   ├── emela_board.h
│   ├── emela_embedded_exports.h
│   ├── emela_artifact.json
│   ├── program.backend.json
│   ├── program.config.json
│   └── program.flow.txt
└── firmware/
    ├── firmware.elf
    ├── firmware.bin
    ├── firmware.uf2              # Pico / Pico 2のみ
    ├── firmware.map
    ├── firmware.json
    └── …                         # portごとのログ・容量・symbol情報
```

ビルド開始時に、選択ボードの古い`firmware.json`を無効化する。
ソースやSDKのエラーがあった場合に、以前の成功を今回の成功として残さない。
`--emit c`も古いfirmwareの成功を無効化する。既存のBINなどは成功markerなしで残る場合がある。
出力ディレクトリやその配下のシンボリックリンクは拒否する。

既存の明示入力コマンドは引き続きC生成専用として使える。
独自の公開関数・C名・adapter設定を使うライブラリや、保存入力の再生成に使用する。

```sh
emelino build src/main.emel --config config.json --out-dir generated
emelino flow src/main.emel --config config.json --out-dir flow
emelino build --input generated/program.backend.json \
  --config generated/program.config.json --out-dir replay
```
