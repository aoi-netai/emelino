# I2Cコントローラーモデルの独立試験

`tools/test_renode_i2c_model.py` は、firmware・HAL・BSPを使わず、RenodeのSystemBusから
コントローラーのレジスターを直接操作する。`--port nrf52`では
[nRF52840 TWIの114項目](../models/README.md#nrf52840-twi)を実行し、結果は
`out/renode-i2c-model-nrf52/`へ保存する。counting peerとrunnerは共通である。

以下は既定の`--port stm32f4`で操作する`EmelinoSTM32F4I2C`について示す。期待値はSTM32F446の
[RM0390](https://www.st.com/resource/en/reference_manual/dm00135183.pdf)の
controller receive、I2C_CR1、I2C_SR1、I2C_SR2の契約に基づく。

相手は `CountingPeer.cs` のcount厳守レジスターモデルで、読み取り要求数だけ返し、
自動incrementとWrite/Read/Finish回数を記録する。IRQは実際のGPIO接続で観測する。
観測器を別のMMIO領域へ配置し、コントローラーのprivate stateやHALのメモリーを読まない。

```sh
export RENODE=/path/to/renode
python3 tools/test_renode_i2c_model.py
```

2026-11-04、Renode 1.16.1で122項目が成功した。

- SR2単独readでADDRを消さず、SR1→SR2で解除する。
- AFはSR1を繰り返し読んでも残り、write 1で保持、write 0で解除する。
- 1 / 2 / 3 / 8 byte受信、ACK/POS、最後の3 byteの終端手順、RXNE/BTFの状態。
- STOP後に受信済みデータを読み出せ、追加の相手readを行わない。
- レジスター指定write→Repeated START→readで、prefixの到着順と途中STOPの不在。
- RXNEのEVT/BUF、BTF/ADDRのEVT、AFのERRというIRQ有効条件。
- PE解除とresetがIRQ・BUSY・FIFOを消し、再開後に古いデータを返さない。
- TXデータ書込み直後、schedulerを進めずresetした後も、完了flagや旧送信データが復活しない。

`out/renode-i2c-model/report.json` に各readの期待値・mask・実測値、モデルと相手のSHA-256を
記録する。`run.resc`、`renode.log`、`model.log`も保存する。warning/errorは失敗扱い。
これはregister/transactionモデルの試験で、SCL波形、電気的条件、厳密な時間を検証するものではない。

ADDR解除の競合（SR1読取り後、SR2読取り前に新たにADDRが立つ場合）や、DMA、10-bit address、
multi-master、target modeはこの試験の対象に含めない。
