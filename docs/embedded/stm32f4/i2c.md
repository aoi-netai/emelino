# STM32F4 I2C

NUCLEO-F446RE の I2C1 を STM32CubeF4 HAL へ接続する。初期 mapping は
PB8=SCL / PB9=SDA、AF4、open drain。実機では外付け pull-up が必要。
I2C2 / I2C3 は `Unsupported`。PB8 / PB9 を GPIO や別の peripheral が使用中なら
`ResourceConflict` を返す。

## 設定と転送

- APB1 は reset HSI の 16 MHz。`clock_speed_hz` は正数、最大 400 kHz。
  CCR の 12 bit に収まらない低速指定は `Unsupported`。速度と duty cycle は HAL に渡す。
- device address は shift 前の 7 bit 値。送信 list の要素は 0–255。
  `memory_write` の 16 bit address は上位 byte から送る。
- 全操作は同期呼出し。ISR 内や割り込み禁止中は `InvalidState`、同一 controller の
  再入は `Busy`。`timeout_ms` は 1–UINT32_MAX で、bus idle 待ち、書込み、読み取り、
  STOP 完了待ちが同じ deadline を共有する。
- `write_read` は任意長の command/register prefix を送ってから、STOP を挟まず
  repeated START で指定長を読む。prefix が空なら読み取りから開始する。
  成功時は 1 byte を含め要求した長さを返し、不完全な受信結果は返さない。
- 読み取り容量は生成された `EMELA_PERIPHERAL_BUFFER_CAPACITY` と C caller の
  out capacity の小さい方。送信は同じ固定 byte buffer に分割して marshal し、
  分割境界で START / STOP を追加しない。動的確保は行わない。
- `is_device_ready` は address-only transaction を実行する。NACK の場合は指定
  trials 回まで同じ deadline で再試行し、全て NACK なら `False`。
  Timeout や bus error は `I2cError`。HAL AF/BERR/OVR/ARLO をそれぞれ
  Nack/BusError/ArbitrationLost に変換する。

## HAL、割り込み、復旧

送受信は HAL の `HAL_I2C_Master_Seq_Transmit_IT` / `Receive_IT` を用いる。
通常は NVIC delivery を無効にし、main の待機 loop が pending EV/ER を見て HAL handler
を呼ぶ。`enable_interrupt` 後は同じ state machine を I2C1 の EV/ER IRQ から進める。
Low / Normal / High は NVIC priority 12 / 8 / 4。`disable_interrupt` 後も
同期送受信を続けられる。HAL completion callback は内部の完了 flag を更新するだけで、
ユーザー callback の登録機能ではない。

polling でも EV handler の前に SR1 を読む処理を置かない。STM32F4 は SR1→SR2 read で
ADDR を clear するため、wrapper が SR1 を先に読むと HAL の SR2→SR1 read が
本来処理すべき ADDR event を消してしまう。NVIC pending は delivery 無効時にも保持され、
SR1 を参照せずに EV/ER を選択できる。

HAL sequential option は非終端 `I2C_FIRST_AND_NEXT_FRAME`、終端 `I2C_LAST_FRAME`。
HAL の `PreviousState` に従って initial START、同方向の継続、方向変更時の repeated
START を選ぶ。先頭用 option に組み込まれた独立の 25 ms BUSY 待ちを避け、BSP の
deadline で idle を確認する。

1 byte 受信の終端にも HAL 固有の最大 5 ms STOP spin がある。
`i2c_events.h` は残り 1 byte かつ STOP pending の EV を一時保留し、EV / BUF のみを
mask して main の deadline loop へ戻す。STOP clear 後に再 enable して HAL を続行する。
ER はその間も有効。HAL 自身の `I2C_MasterReceive_RXNE` が
`I2C_WaitOnSTOPRequestThroughIT` → DR read の順に実行するため、DR read の前に STOP
を待つというハードウェア操作順序は変えない。`test_events.py` は実際の helper を
fake HAL/registers と組み合わせ、STOP 中の HAL 未呼出し、error IRQ の保持、STOP clear
後の再開、待機中の error completion を確認する。電気的な STOP timing の試験ではない。

転送失敗時は、master を保持し arbitration を失っていない場合に STOP を要求してから
controller を reset し、保存している設定を再適用する。外部 device による SDA hold 等は
これだけで解消しないため、`recover` は peripheral を止め、SCL / SDA を GPIO open drain
へ変更し、SDA が low なら最大 9 clock、続いて STOP を試みる。SCL clock stretching と
全待機は同じ deadline に収める。成功・失敗の双方で AF4 と保存設定へ戻す。
失敗した元の通信を再実行する機能ではない。

## Renode で確認する範囲

`tools/test_embedded.py --board nucleo-f446re --scenario i2c` は Emela → 生成 C → SDK → firmware を build し、
raw BIN を Flash に置いて reset vector から起動する。標準 `STM32F4_I2C` controller と
試験用 I2C target を接続し、2 / 3 byte の読み取り、任意 prefix、memory address の
8 / 16 bit、polling / IRQ、pin conflict、Timeout、recover 後の再通信を確認する。
CR1 write の列から、combined transfer が START→START→STOP、memory write が
START→STOP であることも確認する。HAL library と production BSP に試験分岐はない。

Renode 1.16.1 の generic F4 model は STM32F446 silicon の完全なモデルではない。
以下を結果の `model_limitations` に残す。

- target に `Read(1)` を一度だけ呼ぶため、試験 target は register window を返す。
  ACK による実際の受信長制御、POS、電気的な open drain / pull-up は検証しない。
- **標準1.16.1モデルでは1 byte受信が完了しない。** モデルはADDRを立てるが、
  SR2だけを読むと解除してしまう。HALのSR2→SR1読み取りではADDRを観測できず、
  受信長によらず立つBTFが残る。1 byteだけを返すtargetでも同じ問題がある。
  HAL の `I2C_Master_ADDR` は `XferCount == 1` / `I2C_LAST_FRAME`
  で ACK を解除し、ADDR clear 後に STOP を出す。その後 `I2C_MasterReceive_RXNE`
  が最後の byte を取り出して完了する。model はこの ADDR 経路を飛ばして BTF handler
  へ入るため完了しない。通常成功の 19 項目とは別の 1 項目で、この既知制約が Timeout
  として止まることを確認する。1 byte を Timeout にする production 分岐はない。
  根拠となる SDK は STM32CubeF4 1.28.3 の
  `Drivers/STM32F4xx_HAL_Driver/Src/stm32f4xx_hal_i2c.c`、
  `HAL_I2C_EV_IRQHandler` / `I2C_Master_ADDR` / `I2C_MasterReceive_RXNE`。
- 未接続addressではAFを立てるが、元モデルはSR1 readでAFを解除するため、HALの
  エラー処理へ正しく届かない。旧試験のNACKとtrialsは試験ELFのlinker wrapperから
  HAL error callback(AF)を注入して確認する。実際のbus NACK試験ではない。
- target が空データを返す fault で受信を止め、実際の HAL 待機と SysTick deadline が
  Timeout に達することを確認する。この際の empty FIFO warning と GPIO OTYPER の
  未実装 warning は限定して許可・記録し、それ以外の warning は失敗とする。
- recover は controller reset と GPIO/AF 設定復元を確認する。device が実際に SDA を
  hold した場合の電気的な 9 clock 解放は実機で確認する必要がある。

host の service ABI 試験での exact 1 byte 成功、Renode の 2 / 3 byte 成功、上記の
single-byte model 制約は、それぞれ異なる検証範囲として扱う。

2026-11-03 の検証では `-Os` / `-O0` の双方で通常 19 項目と既知モデル制約 1 項目が成功。
結果は `out/stm32f4-i2c/report.json` / `out/stm32f4-i2c-o0/report.json` に保存する。
試験 firmware の Flash 使用量は 15,636 / 41,868 bytes、静的 RAM は 9,064 / 9,072 bytes。
別に stack 16 KiB と guard 256 bytes を予約し、system allocator は link していない。

## BMP180を接続した試験

2026-11-04に[標準BMP180モデルを相手とする試験](bmp180-test.md)を追加した。
こちらは[修正版コントローラーモデル](../../../tests/renode/models/README.md)を明示して実行し、
1 byteを含む受信、校正データ、温度・圧力のコマンド、polling/IRQ、未登録addressのNACKを検査する。
HAL/BSPの変更や返却値の注入は行わない。コントローラーのレジスター・IRQは
[独立したMMIO試験](../../../tests/renode/i2c/README.md)でも検査する。
標準モデルでの既知制約と、修正版モデルでの成功を分けて結果へ記録する。
