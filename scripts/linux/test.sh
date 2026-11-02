#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_WARNINGS=deny
artifacts="$(pwd)/target/linux-check"
mkdir -p "$artifacts"
export XDG_CONFIG_HOME="$artifacts/config"
export XDG_DATA_HOME="$artifacts/data"
export XDG_CACHE_HOME="$artifacts/cache"
export XDG_RUNTIME_DIR="$artifacts/run"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i testsrc2=size=640x360:rate=24 \
    -t 20 -c:v mpeg4 "$artifacts/fixture.mkv"
export SCRY_SUBTITLE_TEST_VIDEO="$artifacts/fixture.mkv"

cargo fmt --all -- --check
for features in default all; do
    flags=()
    if [[ "$features" == all ]]; then flags+=(--all-features); fi
    cargo clippy --workspace --all-targets --locked "${flags[@]}"
    cargo test --workspace --all-targets --locked "${flags[@]}"
    cargo test -p scry-playback --test software_playback --locked "${flags[@]}" -- --ignored
    cargo test -p scry-app --test linux_video --locked "${flags[@]}" -- --ignored
    cargo test -p scry-app --bin scry --locked "${flags[@]}" \
        native_subtitle_adapter_supports_the_pinned_runtime_and_two_tracks -- --ignored
done
cargo build --workspace --examples --locked
cargo build --workspace --locked
target/debug/scry --self-test --seconds 3 --report "$artifacts/decode.jsonl"
xvfb-run -a -s "-screen 0 1440x1000x24" dbus-run-session -- \
    bash scripts/linux/desktop-test.sh "$artifacts"
