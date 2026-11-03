# STM32F4 I2Cの通信試験用モデル

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

`RENODE`を指定して `python3 tools/test_renode_i2c_model.py` を実行する。
HAL・firmwareを介さずMMIO・IRQと相手への読出し回数を検査する。

## 範囲

これは使用したmaster通信経路を検査するモデルであり、STM32F446全体の再現ではない。
SCL周期、clock stretching、電気的なpull-upやopen drain、multi-master arbitration、
slave/10 bit/DMA/SMBusは検証しない。AF以外のバス異常も生成しない。
相手デバイスの変換・書込み待ち時間も、その標準モデルが実装する範囲に従う。
試験の合格はこの明示されたモデル上での結果であり、実機での電気的検証とは区別する。
