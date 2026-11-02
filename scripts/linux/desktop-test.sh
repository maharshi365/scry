#!/usr/bin/env bash
set -euo pipefail
artifacts="$1"
openbox > "$artifacts/window-manager.log" 2>&1 &
wm=$!
player=""
trap 'if [[ -n "$player" ]]; then kill "$player" 2>/dev/null || true; fi; kill "$wm" 2>/dev/null || true' EXIT

timeout 30s target/debug/examples/embedded_probe \
    "$artifacts/fixture.mkv" 3 "$artifacts/embedded.jsonl" \
    > "$artifacts/embedded.log" 2>&1

target/debug/scry --play "$artifacts/fixture.mkv" > "$artifacts/app.log" 2>&1 &
player=$!
window=$(timeout 20s xdotool search --sync --onlyvisible --name '^Scry$' | head -n 1)
xdotool windowactivate --sync "$window"
# Allow local media and the first rendered frame to arrive before input and capture.
sleep 3
kill -0 "$player"
import -window "$window" "$artifacts/windowed.png"
xdotool key --clearmodifiers f
for attempt in {1..50}; do
    if xprop -id "$window" _NET_WM_STATE | grep -q _NET_WM_STATE_FULLSCREEN; then break; fi
    sleep 0.1
done
xprop -id "$window" _NET_WM_STATE | grep -q _NET_WM_STATE_FULLSCREEN
sleep 1
import -window "$window" "$artifacts/fullscreen.png"
xdotool key --clearmodifiers Escape
for attempt in {1..50}; do
    if ! xprop -id "$window" _NET_WM_STATE | grep -q _NET_WM_STATE_FULLSCREEN; then break; fi
    sleep 0.1
done
if xprop -id "$window" _NET_WM_STATE | grep -q _NET_WM_STATE_FULLSCREEN; then
    echo "Fullscreen did not exit" >&2
    exit 1
fi
xdotool windowsize --sync "$window" 1000 700
xdotool key --clearmodifiers space Right space
sleep 2
kill -0 "$player"
import -window "$window" "$artifacts/resized.png"
xdotool key --clearmodifiers alt+F4
for attempt in {1..100}; do
    if ! kill -0 "$player" 2>/dev/null; then break; fi
    sleep 0.1
done
if kill -0 "$player" 2>/dev/null; then
    echo "Player did not shut down" >&2
    exit 1
fi
wait "$player"
player=""
echo "X11 playback, fullscreen, resize, and shutdown checks passed"
