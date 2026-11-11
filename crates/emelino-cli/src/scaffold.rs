use crate::firmware::Board;

pub fn manifest(name: &str, board: &Board) -> String {
    format!(
        "[project]\nname = {name:?}\nboard = {:?}\nentry = \"src/main.emel\"\n\n[resources]\nheap_bytes = 1024\n",
        board.board
    )
}

pub fn source(board: &Board) -> String {
    let pin = board
        .pins
        .get("led")
        .or_else(|| board.pins.get("led_green"))
        .expect("board LED");
    format!(
        "import Embedded.Board.{{BoardGpio}}\nimport Embedded.Gpio\n\n# {}: turn on the onboard LED (logical pin {pin}).\npub fn main() -> Int {{\n  with BoardGpio {{\n    gpio = use Gpio.Gpio\n    gpio.configure(Gpio.GpioPin({pin}), Gpio.OutputPushPull(initial: Gpio.High))\n    0\n  }} escape {{ Gpio.GpioError(_) -> 1 }}\n}}\n",
        board.physical_board
    )
}

pub fn readme(name: &str, board: &Board) -> String {
    format!(
        "# {name}\n\nEmelino application for {} (`{}`).\n\n## Build\n\n```sh\nemelino build\n```\n\nThe first build reports missing tools or SDKs and prints commands for the bundled\nUbuntu / WSL setup scripts. Run those commands and load `.local/env.sh` in each\nnew terminal before building. You can reuse an existing toolchain with `ARM_GCC`\nand the board SDK environment variables; see the\n[setup guide](https://github.com/aoi-netai/emelino/blob/main/docs/setup.md).\n\nGenerate C without Python, a cross compiler or an SDK:\n\n```sh\nemelino build --emit c\n```\n\n`src/main.emel` turns on the onboard LED. `pub fn main() -> Int` is the entry;\nEmelino selects its C export automatically. Add modules anywhere under `src/`;\nthey are collected automatically. Build settings live in `Emelino.toml`.\n\nGenerated C, headers and replay inputs are under\n`target/emelino/{}/generated/`. Firmware and build logs are under the adjacent\n`firmware/` directory. A successful firmware build publishes `firmware.json`.\nWriting the firmware to a physical board is a separate step.\n",
        board.physical_board, board.board, board.board
    )
}
