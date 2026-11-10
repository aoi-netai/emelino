#!/usr/bin/env bash
set -euo pipefail
SETUP_DESCRIPTION='Build the fixed, free rp2350js engine for Pico embedded tests (Node.js 18+ required).'
source "$(dirname -- "$0")/lib.sh"
setup_init "$@"
require_command git
require_command python3
if (( ! SETUP_DRY_RUN )); then
    for command in node npm; do
        command -v "$command" >/dev/null || fail "missing $command; install Node.js 18+ and npm before this optional setup"
    done
    node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 18 ? 0 : 1)' || fail 'Node.js 18 or later is required'
fi
dependency="$SETUP_ROOT/tests/rp2350js/dependency.json"
revision=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' "$dependency")
url=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["repository"])' "$dependency")
destination="$SETUP_PREFIX/rp2350js-${revision:0:8}"
if [[ -e "$destination" ]]; then
    [[ -d "$destination/.git" ]] || fail "unmanaged engine directory: $destination"
    [[ $(git -C "$destination" config --get remote.origin.url) == "$url" ]] || fail 'unexpected rp2350js origin'
    [[ $(git -C "$destination" rev-parse HEAD) == "$revision" ]] || fail 'unexpected rp2350js revision'
    [[ -z $(git -C "$destination" status --porcelain --untracked-files=no) ]] || fail 'rp2350js has local source changes'
else
    run mkdir -p -- "$SETUP_PREFIX"
    if (( SETUP_DRY_RUN )); then
        run git clone --no-checkout -- "$url" "$destination"
        run git -C "$destination" checkout --detach "$revision"
    else
        temporary=$(mktemp -d "$SETUP_PREFIX/.rp2350js-XXXXXX")
        trap 'rm -rf -- "$temporary"' EXIT
        run git clone --no-checkout -- "$url" "$temporary/engine"
        run git -C "$temporary/engine" checkout --detach "$revision"
        mv -T -- "$temporary/engine" "$destination"
    fi
fi
# The engine's committed package-lock.json fixes all transitive dependencies.
# Ignore install scripts: headless tests do not use the canvas native GUI addon.
run npm --prefix "$destination" ci --ignore-scripts --no-audit --no-fund
run npm --prefix "$destination" run build
require_file "$destination/dist/cjs/index.js"
write_environment rp2350js RP2350JS_DIR "$destination"
setup_finished
