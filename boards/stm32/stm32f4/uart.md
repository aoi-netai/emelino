# STM32F4 UART

NUCLEO-F446RE の `Uart2` を USART2 / PA2(TX) / PA3(RX) / AF7 に接続する。
対応ピンは board profile から `board_config.h` に生成する。GPIO、SPI、I2C と
ピンの所有権を共有し、競合時は `ResourceConflict` を返す。片方向の設定では
使用する側のピンだけを確保する。

設定は STM32CubeF4 の `HAL_UART_Init`、GPIO AF 設定は `HAL_GPIO_Init` を使う。
転送と IRQ の受信バッファは HAL のフラグ定義・制御マクロを用いる。
`List[Int]` の C 境界は全設定で `uint16_t` 配列なので、8-bit と 9-bit の両方で
同じ要素数・要素間隔を保つ。

## 対応設定

- データ幅は parity を除く。7-bit は Even/Odd、8-bit は NoParity/Even/Odd、
  9-bit は NoParity に対応する。
- stop bits は One / Two。Half / OneAndHalf はこの UART HAL 経路では `Unsupported`。
- Receive / Transmit / Both に対応する。
- oversampling は By8 / By16。Native は可能なら By16、高速設定では By8 を選ぶ。
  reset HSI / APB 16 MHz で表現できない baud rate は `Unsupported`。
- CTS/RTS の board pin 定義がないため、flow control は NoFlowControl のみ。
- `Uart0` や、ほかの未接続 UART ID は `Unsupported`。不正な引数は
  `InvalidArgument` を返す。

`configure` は検証とピン確保の後に設定を適用する。新設定の受信モードは
Polling、割り込みは無効、旧受信データと error latch は空になる。
設定が検証やピン競合で失敗した場合は、それまでの設定を維持する。

## 転送・受信モード

すべての API は main context 用。ISR 内や PRIMASK で割り込みを禁止した状態では
`InvalidState` を返す。実行中の再入呼び出しは `Busy`。

`transmit` は全 word の範囲を送信開始前に検査し、黙って切り捨てない。
最後の TC（最後の word の送出完了）まで待つ。空の送信は成功する。
`receive(size)` は正の size と完全一致する長さだけを成功として返す。
途中で失敗した際に、それまでの入力が消費されている場合がある。
`receive_some(max_size)` は最初の一語を待ち、その時点で取り出せる語を最大
max_size まで返す。新しい語が来るまで待って満杯にする処理は行わない。

タイムアウトは `1..UINT32_MAX` ms。SysTick の単一開始時刻を使い、全転送を制限する。
各 word ごとにタイムアウトを延長しない。

`set_receive_mode(InterruptReceive)` と `enable_interrupt` の両方を設定すると、
USART2 IRQ が 64-word ring buffer に受信データを保存する。設定順序は自由。
InterruptReceive のまま IRQ を無効にした状態で receive を呼ぶと `InvalidState`。
Polling へ戻す場合は既存の ring buffer を先に読み、以後は RXNE を polling する。
モード変更・割り込み無効化だけでは buffer を捨てない。

`enable_interrupt(..., notify_transmit=True)` は、送信中だけ TC 割り込みを有効にし、
IRQ が内部完了フラグを設定したことを同期 transmit で待つ。
現在のライブラリには UART のユーザー callback 登録 API がないため、`UartEvent`
をユーザー関数に配送する未定義の仕組みは追加していない。

## エラーと破棄

Parity / Framing / Overrun / Noise はそれぞれ Parity / Framing / Overrun / Other に
変換する。異常を検出した word は通常の受信データとして返さない。
IRQ 受信では error を保持し、次の receive で一度報告してから解除する。
複数のフラグが同時に立つ場合は Overrun、Parity、Framing、Other の順を優先する。

ring が満杯の場合は新しい word を破棄し、すでに入っている 64 word を維持する。
次の receive は Overrun を返す。その後は保存された word を読み出せる。
`discard_input(max_size)` は最大 max_size 語を待たずに破棄し、実際に捨てた数を返す。
同時にソフトウェア側の error latch を解除する。

## Renode 試験

```sh
python3 tools/test_stm32f4_uart.py \
  --renode /path/to/renode \
  --cc /path/to/arm-none-eabi-gcc \
  --sdk-dir /path/to/STM32CubeF4-1.28.3
```

`RENODE` / `ARM_GCC` / `STM32CUBE_F4` も使える。
`--optimization O0 --output-dir out/stm32f4-uart-o0` で最適化なしの試験も実行できる。
レポート、raw firmware、SDK hash、UART output、Renode log を出力先へ保存する。

Emela の 33 項目で polling と IRQ 受信、部分受信、TC 完了、timeout、discard、
方向制限、word 幅検査、ring overflow、error 後の再開を確認する。

Renode 1.16.1 の STM32 UART モデルは byte 単位であり、CR1.M、PEIE、CR3.EIE を
実装していない。9-bit 送信は DR への書込み値、受信 bit 8 は DR read hook で確認する。
PE/FE/NE/ORE も SR read hook で注入し、実際の RXNE IRQ からエラーを届ける経路を
確認する。モデル未実装項目の特定警告のみをレポートへ記録し、ほかの警告は失敗とする。
電気的な parity error、baud 精度、wire timing、9-bit 波形は実機検証が必要。
