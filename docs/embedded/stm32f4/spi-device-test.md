# RenodeのMicron MT25Qを相手にしたSPI通信試験

Emelaアプリから生成C、STM32F4 BSP、STM32CubeF4 HAL、Renode標準の`STM32SPI`を通し、
標準の`SPI.Micron_MT25Q`モデルへ通信する。相手モデルもコントローラーモデルも変更せず、
HAL関数の置換や読み取り結果の注入も使わない。

8 MiBのMT25QモデルをSPI1へ、CSをGPIO PA4から接続する。JEDEC IDの期待値は
メーカー`0x20`、モデルの1.8 V品種`0xBB`、容量`0x17`。
根拠はRenode 1.16.1の
[Micron_MT25Q](https://github.com/renode/renode-infrastructure/blob/add012af003a0f620d3da52828262676f374d121/src/Emulator/Peripherals/Peripherals/SPI/Micron_MT25Q.cs)と
[GenericSpiFlash](https://github.com/renode/renode-infrastructure/blob/add012af003a0f620d3da52828262676f374d121/src/Emulator/Peripherals/Peripherals/SPI/GenericSpiFlash.cs)。

## 検査内容

| コマンド・操作 | 確認内容 |
| --- | --- |
| READ ID `0x9F` | `20 BB 17`を取得し、1 byteで終了した次のtransactionでも先頭から読み直せる |
| READ `0x03` | 初期値`FF`、書込み後`00 55 AA FF`、消去後`FF`を取得 |
| READ STATUS `0x05` | WELが初期0、WREN後1、Program後0、WRDI後0、Erase後0 |
| WREN `0x06` / WRDI `0x04` | CSで区切った独立commandとしてwrite enable latchを切り替える |
| PAGE PROGRAM `0x02` | command＋addressとpayloadを別々の`Spi.Write`に分け、1回のCS内で送信 |
| FAST READ `0x0B` | addressに続くdummy byteを送ってからdataを取得 |
| `Spi.Transfer` | command/addressとdummy clocksを1配列で送り、応答位置を含む全結果を比較 |
| 4 KiB ERASE `0x20` | 書込み場所を消去し、読み出し値と保存メモリーの両方で確認 |

firmware内の14項目に加え、19個のCS区間と各区間の全77送信byteを比較する。
相手の保存メモリーは観測専用に`0x90000000`へも割り当て、アプリが待機した時点で
Renode側から読み取る。firmwareはこのアドレスへアクセスせず、書込みも読み出しもSPI経由で行う。
probe Cは判定結果・待機stageの記録だけを担う。

## 再実行

[共通手順](../README.md#renode試験)の`ARM_GCC`、`STM32CUBE_F4`、`RENODE`を設定して実行する。

```sh
python3 tools/test_embedded.py --board nucleo-f446re --scenario mt25q
python3 tools/test_embedded.py --board nucleo-f446re --scenario mt25q --optimization O0 --output-dir out/stm32f4-spi-device-o0
```

生成C、ELF/BIN、model log、送信command列と判定を含む`report.json`を出力する。
モデルの初期CS解除に伴う空transaction warningと、HAL初期化時のSPI無効化warningは
最初の送信より前に限って記録・許可する。通信開始後のwarningと全errorは失敗にする。

## 検証範囲

この試験は実デバイスのコマンド状態を持つ標準モデルとの通信確認であり、実機の電気的検証ではない。
STM32SPIモデルの通信単位は8 bit。周波数・CPOL/CPHA・セットアップ時間の電気的な成否は検証しない。
flashモデルのProgram/Eraseは即時完了するため、WIPの持続時間・耐久性・物理NORのbit遷移制約は
主張しない。試験の書込みは1ページ内に収める。

構成変更前の2026-11-04にRenode 1.16.1、STM32CubeF4 1.28.3、ARM GCC 13.2.1で、`-Os`と`-O0`の
両方が14項目・19 CS区間・77 byteの比較に成功した。Flashは19,956 / 62,332 bytes、
静的RAMは17,568 / 17,560 bytes（固定heap 16 KiBを含む）。別にstack 16 KiBを予約する。
