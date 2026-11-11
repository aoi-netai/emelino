#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Install ESP-IDF 5.4.1 and the ESP32-C3 RISC-V toolchain.'
# shellcheck source=tools/setup/lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"
require_command git
setup_apt=()
if (( EUID != 0 )); then setup_apt=(sudo); fi
run "${setup_apt[@]}" apt-get update
run "${setup_apt[@]}" apt-get install -y git cmake ninja-build build-essential python3 python3-pip python3-venv libusb-1.0-0
sdk="$SETUP_PREFIX/embedded-sdk/esp-idf-v5.4.1"
install_sdk https://github.com/espressif/esp-idf.git v5.4.1 "$sdk"
if (( ! SETUP_DRY_RUN )); then
    [[ $(git -C "$sdk" rev-parse HEAD) == 4c2820d377d1375e787bcef612f0c32c1427d183 ]] || fail 'unexpected ESP-IDF 5.4.1 revision'
fi
run git -C "$sdk" submodule update --init --recursive --depth 1
run env IDF_TOOLS_PATH="$SETUP_PREFIX/espressif" bash "$sdk/install.sh" esp32c3
write_environment esp-idf IDF_PATH "$sdk" IDF_TOOLS_PATH "$SETUP_PREFIX/espressif"
if (( ! SETUP_DRY_RUN )); then
    printf 'source %q\n' "$sdk/export.sh" >> "$SETUP_PREFIX/env.d/esp-idf.sh"
fi
setup_finished
