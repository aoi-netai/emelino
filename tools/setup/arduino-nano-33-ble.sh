#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Install Nordic nrfx 3.2.0 and CMSIS 5.9.0 for Arduino Nano 33 BLE.'
# shellcheck source=tools/setup/lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"
require_command git

nrfx="$SETUP_PREFIX/embedded-sdk/nrfx-3.2.0"
cmsis="$SETUP_PREFIX/embedded-sdk/cmsis-5.9.0"
install_sdk https://github.com/NordicSemiconductor/nrfx.git v3.2.0 "$nrfx"
install_sdk https://github.com/ARM-software/CMSIS_5.git 5.9.0 "$cmsis"
require_file "$nrfx/mdk/nrf52840.h"
require_file "$nrfx/mdk/system_nrf52840.c"
require_file "$cmsis/CMSIS/Core/Include/core_cm4.h"
write_environment arduino-nano-33-ble NRFX_ROOT "$nrfx" CMSIS_ROOT "$cmsis"
setup_finished
