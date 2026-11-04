#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Install STM32CubeF4 1.28.3 for NUCLEO-F446RE.'
# shellcheck source=tools/setup/lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"
require_command git

sdk="$SETUP_PREFIX/embedded-sdk/STM32CubeF4-1.28.3"
install_sdk https://github.com/STMicroelectronics/STM32CubeF4.git v1.28.3 "$sdk" \
    Drivers/STM32F4xx_HAL_Driver Drivers/CMSIS/Device/ST/STM32F4xx
require_file "$sdk/Drivers/STM32F4xx_HAL_Driver/Src/stm32f4xx_hal_gpio.c"
require_file "$sdk/Drivers/CMSIS/Include/core_cm4.h"
require_file "$sdk/Drivers/CMSIS/Device/ST/STM32F4xx/Include/stm32f446xx.h"
write_environment nucleo-f446re STM32CUBE_F4 "$sdk"
setup_finished
