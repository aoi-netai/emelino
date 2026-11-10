# ペリフェラルの省略引数

`Embedded.Gpio`・`Embedded.Uart`・`Embedded.I2c`・`Embedded.Spi` の一部の呼び出しは、
以下の引数を省略できる。省略分を呼び出し側で補完し、既存の型検査・C生成へ渡す。
明示した値、引数の順番、エラー処理、同期転送・タイムアウトの意味、BSPのC ABIは変わらない。

```emel
import Embedded.Board.{BoardUart, BoardI2c, BoardSpi, BoardGpio}
import Embedded.Uart
import Embedded.I2c
import Embedded.Spi
import Embedded.Gpio

pub fn setup() -> () {
  with BoardUart, BoardI2c, BoardSpi, BoardGpio {
    (use Uart.Uart).configure(Uart.Uart2, Uart.UartConfig(115200))
    (use I2c.I2c).configure(I2c.I2c1)
    (use Spi.Spi).configure(Spi.Spi1)
    (use Gpio.Gpio).configure(Gpio.GpioPin(4), Gpio.OutputPushPull(initial: Gpio.High))
  } escape {
    Uart.UartError(_) -> ()
    I2c.I2cError(_) -> ()
    Spi.SpiError(_) -> ()
    Gpio.GpioError(_) -> ()
  }
}
```

これは呼び出し表記の例であり、同時使用できるピン・コントローラーはボード定義に従う。
明示指定と同じボードの対応可否・リソース競合の検査を受ける。

## 設定の既定値

| コンストラクター | 必須の引数 | 省略時の値 |
| --- | --- | --- |
| `Gpio.Input` | なし | `pull: NoPull` |
| `Gpio.OutputPushPull` | なし | `pull: NoPull, initial: Low` |
| `Gpio.OutputOpenDrain` | なし | `pull: NoPull, initial: High` |
| `Gpio.Analog` | なし | `pull: NoPull` |
| `Uart.UartConfig` | `baud_rate` | `data_bits: Bits8, stop_bits: One, parity: NoParity, direction: Both, flow_control: NoFlowControl, oversampling: Native` |
| `I2c.I2cConfig` | なし | `clock_speed_hz: 100000, duty_cycle: Standard` |
| `Spi.SpiConfig` | なし | `mode: Master, direction: TwoLine, data_size: Bits8, polarity: IdleLow, phase: FirstEdge, max_frequency_hz: 4000000, bit_order: MostSignificantFirst` |
| `Spi.SpiDeviceConfig` | `bus, cs` | `config: SpiConfig(), active: Low, setup_us: 0, hold_us: 0` |

表内のenum値は対応するmoduleの定義を指す。ユーザー側で既定値の名前をimportする必要はない。
SPIの周波数は既存仕様どおり上限であり、BSPが上限以下の実現可能な周波数を選ぶ。
Open drainの初期値 `High` は出力解放を表す。必要な初期値や通信設定は明示して上書きできる。

## 操作の既定値

| effect操作 | 省略できる引数と値 |
| --- | --- |
| `Uart.transmit / receive / receive_some` | `timeout_ms: 1000` |
| `Uart.enable_interrupt` | `priority: Normal, notify_transmit: False` |
| `I2c.configure` | `config: I2cConfig()` |
| `I2c.master_transmit` | `timeout_ms: 1000` |
| `I2c.write_read` | `write_data: [], timeout_ms: 1000` |
| `I2c.memory_write` | `address_size: Bits8, timeout_ms: 1000` |
| `I2c.is_device_ready` | `trials: 1, timeout_ms: 1000` |
| `I2c.recover` | `timeout_ms: 1000` |
| `I2c.enable_interrupt` | `priority: Normal` |
| `Spi.configure` | `config: SpiConfig()` |
| `Spi.transmit / receive / transfer / transaction` | `timeout_ms: 1000` |
| `Gpio.enable_interrupt` | `priority: Normal` |

`timeout_ms` は `Int64` のミリ秒値。省略しても操作全体の期限という既存の意味は同じ。
UARTは設定後にPollingで受信し、割り込みは明示的に有効化する。自動リトライや自動復旧を追加しない。

ピン・コントローラーID、デバイスアドレス、送信データ、読み取り長、GPIOモード、
割り込みエッジとコールバックなど、表で省略可能としたもの以外は必須。
`Gpio.configure` の `config` と `Uart.configure` の `config` も必須。
Clockの待機時間とConsoleの文字列は引き続き明示する。

## 位置引数と名前付き引数

位置引数の順番は変更しない。途中の引数を省略する場合は、残りを名前付きで渡す。

```emel
Uart.UartConfig(9600, parity: Uart.Even)
Spi.SpiConfig(max_frequency_hz: 1000000)
Spi.SpiDeviceConfig(Spi.Spi1, cs: Gpio.GpioPin(4))
i2c.write_read(I2c.I2c1, 80, read_size: 2)
i2c.memory_write(I2c.I2c1, 80, 3, data: [42])
gpio.enable_interrupt(button, Gpio.Falling, on_event: callback)
```

名前付き引数の略記 (`cs:`)、importしたコンストラクターの直接呼び出し、
別名へ束縛したcapabilityの操作、pipeにも対応する。
書いた引数は既存の規則どおり、書いた順に一度ずつ評価する。
record更新の `..config` は既存どおり更新元の値を引き継ぎ、既定値では上書きしない。

```emel
uart = use Uart.Uart
uart.configure(Uart.Uart2, Uart.UartConfig(115200))
config = 115200 |> Uart.UartConfig
changed = Uart.UartConfig(parity: Uart.Even, ..config)
```

この省略表記はemelinoが所有する上記のAPIに限定する。
同名のユーザー定義関数・型・effectには適用しない。
関数値の型と引数の個数も変えないため、`make = Uart.UartConfig` とした
`make(...)` や、`send = uart.transmit` とした `send(...)` には全引数を渡す。
必須引数の不足、未知・重複した名前、余分な引数、型不一致は引き続き診断する。

## 実装境界

現在の本体依存はEmela v0.27.0である。名前解決済みHIRのコンストラクターIDと、
本体の型検査が解決したeffect操作IDに基づき、CLIが省略分だけを追加する。
操作引数を追加したときは型検査を再実行し、その結果だけをCore/Flowへlowerする。
一般のデフォルト引数宣言構文、本体crateのpatch、Core関数本体の書換えは追加しない。
保存backend入力には補完済みの値が入り、再生成時にこの処理を必要としない。

実装: `crates/emelino-cli/src/peripheral_defaults.rs`。
検証: `crates/emelino-cli/tests/peripheral_defaults.rs`。
