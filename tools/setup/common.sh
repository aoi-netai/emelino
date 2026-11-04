#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Install common emelino build tools for Ubuntu / WSL Ubuntu.'
# shellcheck source=tools/setup/lib.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"

if (( EUID == 0 )); then
    apt_command=(apt-get)
else
    require_command sudo
    apt_command=(sudo apt-get)
fi
run "${apt_command[@]}" update
run "${apt_command[@]}" install -y --no-install-recommends \
    ca-certificates curl git build-essential python3 coreutils \
    gcc-arm-none-eabi binutils-arm-none-eabi libnewlib-arm-none-eabi libnewlib-dev

export CARGO_HOME RUSTUP_HOME
CARGO_HOME=$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")
RUSTUP_HOME=$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")
SETUP_CARGO_BIN="$CARGO_HOME/bin"
export PATH="$SETUP_CARGO_BIN:$PATH"
if (( ! SETUP_DRY_RUN )); then
    temporary=$(mktemp -d)
    trap 'rm -rf -- "$temporary"' EXIT
fi
if ! command -v rustup >/dev/null; then
    if (( SETUP_DRY_RUN )); then
        printf 'Would download https://sh.rustup.rs and install rustup for this user.\n'
    else
        run curl --proto '=https' --tlsv1.2 -fsSL --retry 3 \
            https://sh.rustup.rs -o "$temporary/rustup.sh"
        run sh "$temporary/rustup.sh" -y --profile minimal --no-modify-path --default-toolchain none
    fi
fi
run rustup toolchain install stable --profile minimal

if (( SETUP_DRY_RUN )); then
    arm_cc=${ARM_GCC:-arm-none-eabi-gcc}
    printf 'Would verify Rust/Cargo, Python >= 3.10, C11 and Cortex-M4F newlib linking.\n'
else
    run rustup run stable rustc --version
    run rustup run stable cargo --version
    run python3 -c 'import sys; assert sys.version_info >= (3, 10), "Python 3.10 or later is required"'
    arm_cc=$(command -v "${ARM_GCC:-arm-none-eabi-gcc}") || fail 'ARM GCC not found'
    arm_cc="$(cd -- "$(dirname -- "$arm_cc")" && pwd)/$(basename -- "$arm_cc")"
    [[ "$arm_cc" == *gcc ]] || fail 'ARM_GCC must point to an arm-none-eabi-gcc executable'
    for tool in gcc nm objcopy size; do
        require_command "${arm_cc%gcc}$tool"
    done
    printf '#include <stdint.h>\nint main(void) { return (int)(uint32_t)0; }\n' > "$temporary/check.c"
    run cc -std=c11 "$temporary/check.c" -o "$temporary/host"
    run "$arm_cc" -std=c11 -mcpu=cortex-m4 -mthumb -mfpu=fpv4-sp-d16 -mfloat-abi=hard \
        -nostartfiles --specs=nano.specs --specs=nosys.specs -Wl,-e,main \
        "$temporary/check.c" -lm -o "$temporary/arm.elf"
fi
write_environment common ARM_GCC "$arm_cc" RUSTUP_TOOLCHAIN stable \
    CARGO_HOME "$CARGO_HOME" RUSTUP_HOME "$RUSTUP_HOME"
setup_finished
