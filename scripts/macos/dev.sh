#!/bin/bash
set -euo pipefail
[[ $(uname -s) == Darwin ]] || { echo 'This script requires macOS.' >&2; exit 1; }
source "$(dirname "$0")/env.sh"
cd "$(dirname "$0")/../.."
profile=dev
directory=debug
if [[ ${1:-} == --release ]]; then profile=release; directory=release; shift; fi
export SCRY_DEV_ICON=1
cargo build -p scry-app --bin scry --locked --profile "$profile"
target=$(cargo metadata --no-deps --format-version 1 --locked | /usr/bin/plutil -extract target_directory raw -o - -)
export SCRY_DATA_ROOT="$PWD/target/dev-data"
exec "$target/$directory/scry" "$@"
