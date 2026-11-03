# STM32F4 BSP

最初の実機対象は **NUCLEO-F446RE / STM32F446RE**。生成された
`emela_gpio.h` / `emela_uart.h` / `emela_spi.h` / `emela_i2c.h` のABIを、
[STM32F4 port](../../../firmware/ports/stm32f4/)のdriverが **STM32CubeF4 の HAL / CMSIS** へ接続する。
SDK は同梱せず、build の `--sdk-dir` または `STM32CUBE_F4` にそのルートを指定する。
確認した SDK は STM32CubeF4 **1.28.3**。
SDK sourceは使用サービスに応じてGPIO・UART・SPI・I2CとHAL内部依存のDMAをlinkする。
DMA転送そのものは公開しない。NVIC / PRIMASK はCMSIS inline、clockはRCC macroを使う。
GPIOのみならSysTickは起動しない。UART・SPI・I2Cでは1 msのSysTickとcounter値を使い、
全操作の期限とSPIのµs単位CS待ち時間を管理する。`HAL_Init()`は呼ばない。

```text
firmware/
├── boards/nucleo-f446re/board.json # 基板の配線と MCU・port 選択
├── chips/stm32f446re.json       # MCU の CPU・容量・package pin・IRQ vector
├── build/                     # 共通の manifest・profile・ビルド処理
├── common/                    # SDK 非依存の SPI 状態・複合操作
└── ports/stm32f4/
    ├── gpio.c                 # GPIO と EXTI
    ├── uart.c / spi.c / i2c.c  # SDK による転送、内部 IRQ 処理、復旧
    ├── pins.c / bsp.h         # driver 間で共有する pin 所有権
    ├── time.c                 # SysTick と転送全体の deadline
    ├── platform.c             # Emela の起動、待機、critical section
    ├── registers.h            # CMSIS の CPU 操作と critical section
    ├── stm32f4xx_hal_conf.h    # 使用する HAL module の設定
    ├── startup.S              # vector table、FPU、data/bss 初期化
    ├── firmware.ld.in         # Flash / SRAM / stack 配置
    └── build.py               # STM32 固有のコンパイル・リンク・起動検証
```

別 SDK との接続実装は `firmware/ports/` 以下へ追加する。
配線はboard JSONに記録する。初期対応はUSART2（PA2/PA3・AF7）、SPI1（PA5/PA6/PA7・AF5）、
I2C1（PB8/PB9・AF4）。GPIO portは指定ピンから選択する。別controllerを増やす際は
driverのinstance / IRQ / clock対応も追加する。board定義だけで対応controllerを拡張することはできない。
Console / Clock effectのdriverはまだ提供しない。

## UART・SPI・I2C

詳細な対応範囲、エラー、Renodeの試験境界は [UART](uart.md)、[SPI](spi.md)、[I2C](i2c.md) を参照する。
各driverは設定から転送、公開されている割り込み制御・復旧操作まで実装する。
同期通信はmain contextでのみ使用し、割り込み禁止中の呼び出しも `InvalidState` とする。
待ち時間は正数の `1..UINT32_MAX` ms。転送の途中で期限をリセットしない。
GPIOと通信pinとSPI CSの予約は共有し、他用途へ上書きする要求は `ResourceConflict`。

生成bridgeではUART/SPIの要素を16 bit語、I2Cを8 bit語へ検査付きで変換する。
`peripheral_buffer_capacity` は語数の上限であり、SPI複合操作では操作数・入力総語数・出力総語数を
それぞれ制限する。結果は操作順のlistになり、Writeに対応する結果は空list。
SPIのC statusは `primary_spi` / `primary_gpio` / `cleanup_gpio` の3 fieldで両方の失敗を保持する。
callback経由のheap操作はGPIOのISR検査対象であり、通信APIをISR-safeとして扱わない。

## GPIO

論理 pin は `port * 16 + pin`。A=0、B=1、C=2…なので、PA5=5、PB0=16、
PC13=45。NUCLEO の LD2 は PA5。package に存在しない pin は拒否する。
実基板の SWD / oscillator / solder bridge の配線は、この package pin 情報と別に扱う。

- `configure` は Input / OutputPushPull / OutputOpenDrain と pull 設定に対応。
  出力の初期 level は output mode を有効にする前に設定する。GPIO speed は Low。
  Analog は NoPull に対応する。F4 の analog mode は pull を無効にするため、
  Analog(Up / Down) は `Unsupported` を返す。
- `read` は digital input の level を返す。`write` / `toggle` は設定済み出力だけに許可。
  `toggle` は output latch を読む。これら 3 操作は ISR からも使用できる。
- `configure` / `enable_interrupt` / `disable_interrupt` は main context で実行する。
  IRQ を登録中の pin を再設定すると `Busy`。先に登録を解除する。
- `enable_interrupt` は設定済み Input に対して Rising / Falling / Both を受け付ける。
  EXTI の同じ line を PA0 と PB0 のように複数 port で同時使用する場合は
  `ResourceConflict`。EXTI5–9 / EXTI10–15 の共有 IRQ では priority を一致させる。
- priority は Low=12、Normal=8、High=4 の NVIC priority に対応する。
  shared IRQ 内で同時に pending した line は番号順に処理する。
- EXTI handler は `HAL_GPIO_EXTI_IRQHandler` を呼ぶ。HAL は pending bit を
  acknowledge し、BSP の `HAL_GPIO_EXTI_Callback` が
  `emela_gpio_dispatch_interrupt(pin, level)` を直接呼ぶ。callback の保有と dispatch は
  生成 library の責務。main scheduler へのイベント queue は使わない。
- 再登録は古い pending bit を破棄する。検証失敗時は元の登録を維持する。
  `disable_interrupt` は未登録 pin に対しても成功し、同じ line の別 port の登録には触れない。

C status は成功が 0、失敗が `GpioErrorKind` の variant index + 1。
不正 pin / enum 値は `InvalidArgument`、未設定 pin の入出力は `NotConfigured`、
mode に合わない操作や ISR 内の設定変更は `InvalidState` を返す。

## 起動と検証範囲

reset HSI 16 MHz のまま Flash から起動し、生成 library の
`emela_embedded_start()` を呼ぶ。通常実行の scheduler は生成側が所有する。
setup が return した後も platform は割り込みを有効にして待機し、GPIO 登録を維持する。
`emela_board_completed` / `emela_board_failures` / `emela_board_boot_initialized`
は debugger と Renode から観測できる。

F446RE の Flash 512 KiB / 通常 SRAM 128 KiB と、16 KiB の stack を使用する。
linker は静的 RAM と stack reservation の衝突を拒否する。

Renode の generic STM32F4 platform は F446RE 実機そのものではない。
GPIO / EXTI / NVIC / reset の検証結果と、実機の electrical timing や pin routing の
確認は区別する。特に generic platform の port-to-EXTI 接続は SYSCFG の切り替えを
モデル化していないため、入力試験は PA port を使用する。

2026-11-03の検証では、UART 33項目、SPI 32項目、I2C 19項目と既知モデル制約1項目、
4 driver同時使用9項目が `-Os` / `-O0` の両方で成功した。
GPIOの16項目・7回のISR callbackも回帰確認した。
標準Renode 1.16.1のI2CモデルにはADDR解除・BTF遷移の問題があり、旧試験では1バイト受信が
Timeoutへ終了することを別項目で検査した。
UARTの9 bit、SPIの16 bit、I2CのNACKなど、模型で再現できない部分は各driverの説明に
注入・register観測の範囲を記録する。

2026-11-04には、相手デバイスを含めた試験を追加した。
[BMP180](bmp180-test.md)は19項目・30 transaction、[Micron MT25Q](spi-device-test.md)は
14項目・19 CS区間が `-Os` / `-O0` の両方で成功した。
BMP180は修正版I2Cコントローラーモデルを明示して使用し、1バイト受信のpolling/IRQ、
温度24.0℃→32.0℃、校正データ、NACKを確認した。旧モデルと同じfirmware BINで比較している。
修正版はfirmwareを使わない122項目のレジスター・IRQ試験も通した。
SPI側は標準のコントローラーとデバイスモデルで書込み・読み戻し・消去を確認している。
HAL/BSPへの試験分岐や返却値の注入は追加していない。

再実行は [共通手順](../README.md#renode試験) のコマンドを使う。
共通待ち時間の期限超過は `python3 tests/embedded/ports/stm32f4/test_time.py` でも検査する。
各 `out/embedded/<board>/<optimization>/<scenario>/report.json` は判定・使用SDK・実行log・容量情報を持ち、
ELF/BINとともにローカルへ出力される。

register / IRQ 定義の参照元:

- [ST の STM32F446 CMSIS header](https://github.com/STMicroelectronics/cmsis-device-f4/blob/master/Include/stm32f446xx.h)
- [ST の GPIO HAL driver](https://github.com/STMicroelectronics/stm32f4xx-hal-driver/blob/master/Src/stm32f4xx_hal_gpio.c)
- [STM32F446 reference manual RM0390](https://www.st.com/resource/en/reference_manual/dm00135183.pdf)

startup / linker と MCU / board metadata は、既存 `emela-codegen-emb-cli` の
STM32F4 package を基にしている。
