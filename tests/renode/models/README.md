# I2Cの通信試験用モデル

## nRF52840 TWI

`EmelinoNRF52840I2C.cs` はNordic legacy TWIのmaster・7 bit通信を対象とする。
[Renode公式NRF52840_I2C](https://github.com/renode/renode-infrastructure/blob/76fa7d720e52377ae1986c22fae84965c2aa72e9/src/Emulator/Peripherals/Peripherals/I2C/NRF52840_I2C.cs)
を基に、使用するtask/eventの処理を整理した。著作権表示とMITライセンスを保持する。
動作はnrfx v3.2.0の
[legacy TWI driver](https://github.com/NordicSemiconductor/nrfx/blob/v3.2.0/drivers/src/nrfx_twi.c)
と[nRF52840 Product Specification](https://docs.nordicsemi.com/bundle/ps_nrf52840/page/twi.html)
のtask/event・SHORTS・ERRORSRCの契約に合わせる。

- `TASKS_STARTTX/STARTRX` がアドレス段階を実行する。未登録アドレスはANACKとERRORを立てる。
  address-only STARTTX/STOPでも検出し、データを読み書きせずreadyを確認できる。
- 必要なバイトを`Read(1)`で取得する。`BB_SUSPEND`はバスを保持して次の受信を止め、
  `RESUME`で再開する。`BB_STOP`は最後のRXDを保持したままSTOPPEDを立てる。
- SHORTSを使わない場合も、RXD読出しとRXDREADY解除のどちらの順序でも受信を継続する。
- SUSPENDまたはRepeated STARTでwrite prefixを相手へ渡し、最終STOPだけで
  `FinishTransmission()`を呼ぶ。read中にセンサーの値やfirmware内の状態を参照しない。
- ERRORSRCはlatched・write-one-to-clear、ERROR eventは別途解除する。
  INTENSET/INTENCLRと全イベント状態からIRQを生成する。
- disable/reset後は中断した処理を再開せず、余分なRead/Write/STOPを発生させない。

独立試験は既存の`CountingPeer.cs`と共通runnerを使用する。

```sh
python3 tools/test_renode_i2c_model.py --port nrf52 --output-dir out/nano-model
python3 tools/test_embedded.py --board arduino-nano-33-ble --scenario bmp180
```

独立MMIO試験114項目では、1/2/8バイト受信・連続pure read・prefix・NACK・IRQ mask・
RXD/event解除順序・STOP・disable/resetを検証した。モデルのhashはreportへ保存する。
Nanoのtargetは修正版を既定とし、`--stock-controller`で同梱モデルとの比較が可能。
実機firmwareはモデルの有無で変更しない。
同一BINでBMP180の19項目を比較すると、Renode 1.16.1同梱モデルは存在しないアドレスへの
`is_device_ready`だけが誤って成功し、修正版では全項目が成功する。

SCL周期、電気的open drain、clock stretching、multi-master、DMA/TWIM、slaveは対象外。
データの進行はMMIO操作を境界とし、実際のwire時間を再現しない。
指定アドレスの有無によるANACKを検証し、任意のDNACK・電気的バス異常は生成しない。

## STM32F4 I2C

`EmelinoSTM32F4I2C.cs` は、Renode 1.16.1に含まれる
[STM32F4_I2C](https://github.com/renode/renode-infrastructure/blob/add012af003a0f620d3da52828262676f374d121/src/Emulator/Peripherals/Peripherals/I2C/STM32F4_I2C.cs)
を基にした、master・7 bit I2Cの試験用修正版である。元ソースの著作権表示と
[MITライセンス](LICENSE.renode-MIT)を保持する。インストール済みRenodeやfirmwareのHAL/BSPは変更しない。

## 修正したレジスター動作

基準はSTM32F446の[RM0390](https://www.st.com/resource/en/reference_manual/dm00135183.pdf)にある
master receiverとI2C_SR1/SR2の説明。

- ADDRの解除をSR1→SR2の順序で扱う。元モデルはSR2だけで解除するため、
  HALのイベントハンドラーが行うSR2→SR1読み取りでADDRを失っていた。
  STARTとアドレス/データのDR書込みでは、前フェーズのread sequenceを破棄する。
- AFは読み取りで消さず、write-zero-to-clearまたはPE=0で解除する。
  元モデルにも未登録アドレスでAFを立てる処理はあるが、read-to-clearになっていた。
- 受信はADDR解除後に開始し、DRとshift registerを2バイトのFIFOで表す。
  RXNEは1バイト以上、BTFは2バイトが保持されている場合に立てる。
- ACK/POSから、1バイト・2バイト受信と3バイト以上の終端を扱う。
  相手には必要な分だけ`Read(1)`を繰り返し、要求長を超える応答を前提にしない。
  STOP後は受信済みデータを保持し、新しいデータを取り込まない。
- Repeated STARTでは送信prefixを相手に渡し、最後のSTOPで`FinishTransmission()`を呼ぶ。
  BUSY/MSLとIRQ出力も状態に合わせて更新する。
- フラグはMMIO操作内で更新する。このモデルにはwire時間のスケジューラーを置かず、
  reset後に遅延した旧TX処理がフラグを復活させることも防ぐ。

相手デバイスのIDや測定値、HALのメモリー、firmwareの試験番号には依存しない。
BMP180のモデルはRenode標準のものをそのまま使用する。

## 検証

```sh
# docs/embedded/README.mdと同じARM_GCC / STM32CUBE_F4 / RENODEを指定する。
python3 tools/test_renode_i2c_model.py
python3 tools/test_embedded.py --scenario bmp180 \
  --controller-model tests/renode/models/EmelinoSTM32F4I2C.cs
python3 tools/test_embedded.py --scenario bmp180 \
  --controller-model tests/renode/models/EmelinoSTM32F4I2C.cs \
  --optimization O0 --output-dir out/stm32f4-bmp180-o0
```

独立モデル試験はHALやfirmwareを使わず、MMIOを直接操作する。ADDR、AF、IRQ mask、
1/2/3/8バイトの受信、実際の相手への読出し回数、Repeated START、STOP、PE/resetを検査する。
BMP180試験は同じモデルへ生成firmwareを接続し、デバイスの状態・データを照合する。
結果にはモデルソースのSHA-256を記録する。

NUCLEO-F446REのtest targetはこの修正版モデルを既定で選択する。
`--stock-controller`を指定すれば、標準Renode 1.16.1での比較試験になる。
その場合、現状はBMP180の最初の1バイトID読み取りがTimeoutとなり、スクリプトは失敗を返す。
失敗を正常通信として数えない。`tools/test_embedded.py --scenario i2c`は、元モデルの制約を別項目で
確認する試験として残す。

## 範囲

これは使用したmaster通信経路を検査するモデルであり、STM32F446全体の再現ではない。
SCL周期、clock stretching、電気的なpull-upやopen drain、multi-master arbitration、
slave/10 bit/DMA/SMBusは検証しない。AF以外のバス異常も生成しない。
相手デバイスの変換・書込み待ち時間も、その標準モデルが実装する範囲に従う。
試験の合格はこの明示されたモデル上での結果であり、実機での電気的検証とは区別する。
