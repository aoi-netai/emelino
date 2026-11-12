#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Fetch pinned external ESP32-C3 Renode models (Renode itself is separate).'
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"
setup_init "$@"
require_command git
revision=9c9bdf6bb9c80d40efeaa8e4e398668e60c7007e
repository=https://github.com/mithro/renode-espemu.git
destination="$SETUP_PREFIX/renode-espemu"
if [[ -e "$destination" ]]; then
    [[ -d "$destination/.git" ]] || fail "unmanaged model directory: $destination"
    [[ $(git -C "$destination" rev-parse HEAD) == "$revision" ]] || fail "model checkout must be at $revision"
    [[ -z $(git -C "$destination" status --porcelain --untracked-files=no) ]] || fail 'models have tracked changes'
elif (( SETUP_DRY_RUN )); then
    run git init "$destination"
    run git -C "$destination" remote add origin "$repository"
    run git -C "$destination" fetch --depth 1 origin "$revision"
    run git -C "$destination" checkout --detach FETCH_HEAD
else
    mkdir -p -- "$SETUP_PREFIX"
    temporary=$(mktemp -d "$SETUP_PREFIX/.esp32c3-models-XXXXXX")
    trap 'rm -rf -- "$temporary"' EXIT
    run git init "$temporary/repo"
    run git -C "$temporary/repo" remote add origin "$repository"
    run git -C "$temporary/repo" fetch --depth 1 origin "$revision"
    run git -C "$temporary/repo" checkout --detach FETCH_HEAD
    mv -T -- "$temporary/repo" "$destination"
fi
write_environment esp32c3-renode ESP32C3_RENODE_MODELS "$destination"
setup_finished
