#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Install Pico SDK 2.3.1 and offline picotool 2.3.1 for Pico and Pico 2.'
# shellcheck source=tools/setup/lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"
require_command git
# picotool is a host tool built with its upstream precompiled embedded data.
# The cross compiler is checked by firmware builds, not by this SDK setup.
setup_apt=()
if (( EUID != 0 )); then setup_apt=(sudo); fi
run "${setup_apt[@]}" apt-get update
run "${setup_apt[@]}" apt-get install -y cmake ninja-build build-essential python3 libstdc++-arm-none-eabi-dev

sdk="$SETUP_PREFIX/embedded-sdk/pico-sdk-2.3.1"
picotool_source="$SETUP_PREFIX/embedded-sdk/picotool-2.3.1"
picotool_build="$SETUP_PREFIX/embedded-sdk/picotool-2.3.1-build"
picotool_install="$SETUP_PREFIX/picotool-2.3.1"
install_sdk https://github.com/raspberrypi/pico-sdk.git 2.3.1 "$sdk"
install_sdk https://github.com/raspberrypi/picotool.git 2.3.1 "$picotool_source"
if (( ! SETUP_DRY_RUN )); then
    [[ $(git -C "$sdk" rev-parse HEAD) == 079c6f39023649b154152db30f1d781e884879bc ]] || fail 'unexpected Pico SDK 2.3.1 revision'
    [[ $(git -C "$picotool_source" rev-parse HEAD) == 2041936441b48a3cc53ae3da9e805229fe8f4e18 ]] || fail 'unexpected picotool 2.3.1 revision'
fi
# USB flashing, signing and network peripherals are outside this port's scope;
# neither SDK submodules nor a runtime download are needed.
run cmake -S "$picotool_source" -B "$picotool_build" \
    -DPICO_SDK_PATH="$sdk" -DPICOTOOL_NO_LIBUSB=1 -DPICOTOOL_FLAT_INSTALL=1 \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$picotool_install"
run cmake --build "$picotool_build" --parallel 2
run cmake --install "$picotool_build"
require_file "$sdk/pico_sdk_init.cmake"
require_file "$picotool_install/picotool/picotoolConfig.cmake"
write_environment pico-sdk PICO_SDK_PATH "$sdk" picotool_DIR "$picotool_install/picotool"
setup_finished
