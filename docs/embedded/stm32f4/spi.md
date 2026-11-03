# STM32F4 SPI

NUCLEO-F446RE の SPI1 を STM32CubeF4 HAL の同期 polling API に接続する。
基板定義の初期配線は SPI1 の PA5=SCK、PA6=MISO、PA7=MOSI、AF5。
SPI2–SPI6 は `Unsupported`。SPI1 のピンは共通の所有権管理で予約し、GPIO や
別の peripheral が使用中なら `ResourceConflict` を返す。

## 設定と転送

- Master の TwoLine / TwoLineReceiveOnly / OneLine に対応する。Slave は、この
  初期 mapping に NSS 入力がないため `Unsupported`。TwoLineReceiveOnly の書込みと
  同時送受信、OneLine の同時送受信は `InvalidState`。
- 8 / 16 bit、CPOL / CPHA、MSB / LSB first を HAL 設定へ渡す。
  8 bit 設定に 256 以上の word を渡した場合は、転送前に `InvalidArgument`。
  公開 C buffer は `uint16_t` で、HAL に渡すデータ幅との違いは driver が吸収する。
- HSI / APB2 は 16 MHz。2–256 の divider から `max_frequency_hz` 以下の
  最大 SCK を選ぶ。0 以下は `InvalidArgument`、62,500 Hz より低い上限は
  `Unsupported`。端数比較で上限を超える divider を選ばない。
- read は TwoLine では全 bit が 1 の dummy word を送信する。
  write でも TwoLine の受信を消費して overrun を防ぐ。
- `timeout_ms` は 1–UINT32_MAX。範囲外は `InvalidArgument`。転送中の各 word は同じ
  deadline を使う。HAL の millisecond tick と BSP の monotonic microsecond 時刻を使い、
  途中で timeout をリセットしない。失敗時の受信結果は返さない。
- 同期転送を ISR または割り込み禁止中に呼ぶと `InvalidState`、再入は `Busy`。

## Device と transaction

この処理は [共通SPI実装](../../../firmware/common/spi.c) が所有する。
STM32F4 portは設定・転送・復旧・排他・時刻・CS操作をhookで接続し、busの状態を保持する。
共通実装はHAL、NVIC、STM32のピン番号体系に依存しない。

`create_device` は最大 8 個の slot を確保する。device は自身の bus 設定、CS pin、
active level、setup/hold microseconds を保持する。CS の初期値は inactive。
既に所有されている CS は `GpioFailure(ResourceConflict)`。slot の解放 API はない。

transaction は device 設定を適用し、CS を assert、setup 時間を待ってから操作を順に
実行する。Write の結果は空 list、Read / Transfer の結果はそれぞれ受信 list となり、
操作数と同じ長さの外側 list を返す。全入力と容量を CS assertion 前に検証する。
setup/hold 時間も transaction 全体の deadline に含める。setup+hold だけで予算を
超える場合は CS を触る前に `Timeout`。転送用 deadline から hold 時間を先に予約する。

成功・失敗のどちらでも期限内で hold 時間を待ち、CS の解除を試みる。期限が切れた場合は
待機を続けず `Timeout` を保持して CS を解除する。最初の失敗を `primary`、解除の失敗を `cleanup` に
別々に保持する。解除だけが失敗した場合は `primary=None`。
異なる device を交互に使っても、直前の device の設定を引き継がない。
raw `configure` で設定した値も device 操作で上書きせず、raw 転送時に適用し直す。

転送中の HAL エラーまたは CS 解除失敗の後は `recover` が必要。
recover は SPI を停止・reset し、設定を再適用して、全 device の CS 解除を試みる。
未知の handle は `InvalidArgument`、未初期化 bus の recover は `NotConfigured`。

## C ABI と検証

単純操作の status は 0 が成功、非 0 は `SpiErrorKind` の index + 1。
複合操作は `EmelaSpiTransactionStatus { primary_spi, primary_gpio, cleanup_gpio }` を返す。
各 field の 0 は失敗なし、それ以外は対応する error kind の index + 1。
入力と出力 buffer は呼出し中だけ借用し、保持しない。

`tools/test_embedded.py --board nucleo-f446re --scenario spi` は Emela → 生成 C → HAL → firmware を build し、raw BIN を
Renode の Flash に置いて reset vector から起動する。SPI slave peer の実際の byte 往復、
混合 transaction、複数 device 設定、CS 遷移、setup/hold、引数・容量・状態エラーを調べる。
timeout と CS 解除の同時失敗は、試験 ELF にだけ適用する linker wrapper で注入する。
別に RXNE を Renode の bus read hook で抑え、実際の HAL polling と SysTick による
timeout、recover 後の通信再開を確認する。production driver に試験用分岐はない。

Renode 1.16.1 の STM32SPI は byte 通信だけをモデル化する。16 bit の HAL buffer と
DR 書込みは 0x1234 / 0xffff のまま観測するが、16 bit の wire 通信を実証する試験ではない。
DFF と HAL 初期化時の disabled CR1=0 に対する既知 warning は report に残す。
後者は初期の一度だけで、SPI を slave として有効にしていないことを CR1 書込みで確認する。
周波数・位相・bit order は register 設定の確認で、
電気的な信号品質や実機上の timing 精度は対象外。

2026-11-03 の検証では `-Os` / `-O0` とも 32 項目が成功。
最終確認結果は `out/stm32f4-spi/report.json` と
`out/stm32f4-spi-time-review/report.json`（共通時刻処理の修正後の O0 再確認）に保存した。
試験 firmware の Flash 使用量は 20,200 / 69,188 bytes、静的 RAM は 17,576 bytes。
別に stack 16 KiB と guard 256 bytes を予約し、system allocator は link していない。
