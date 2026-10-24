# Embedded native playback probe

`examples/embedded_probe.rs` tests the real Rune → Windows child HWND → libmpv
adapter. It does not use a browser, the production UI, add-ons, or torrents.
No visual inspection is needed. It uses the installed libmpv runtime.

```powershell
cargo run --example embedded_probe -- target/playback-fixture.mp4 8 target/native-probe.jsonl
cargo run -- --analyze target/native-probe.jsonl
```

All three positional arguments are optional. The command above shows their
defaults. Duration must be finite and between 1 and 120 seconds. Sources are
local files, not URLs. The report directory must exist. Reports overwrite the
selected report file. They contain metrics, not source paths or URLs.

The local fixture used for validation is original synthetic media: 12 seconds of
FFmpeg `testsrc2`, 1280×720 at 24 fps, H.264 video and AAC sine audio. For example,
with FFmpeg available:

```powershell
ffmpeg -f lavfi -i "testsrc2=size=1280x720:rate=24" -f lavfi -i "sine=frequency=440:sample_rate=48000" -t 12 -c:v libx264 -pix_fmt yuv420p -c:a aac target/playback-fixture.mp4
```

**The probe sets mpv volume to zero before loading.** It still uses the native
audio output and audio clock; it does not play an audible test tone.

## What it checks

- Attaches `Host` through Rune `on_window` and places its child from element
  layout using the current display scale.
- Creates, uses, and drops `Player` on one worker. Its native pointer never
  crosses threads.
- Writes schema-1 `mode: native` JSONL with the shared telemetry tracker.
  Samples are taken once per second, plus startup/load and final samples.
- Uses actual observed progress to decide success. Load failure, player failure,
  or no measured progress returns a nonzero exit status and an error summary
  when the report can be written. Stopping before progress also records failure.
- Ends at the deadline, EOF, or Stop. EOF retains the last observed position if
  mpv clears it during unload; it never substitutes the advertised duration.
- Keeps Rune's parent message pump alive while the worker drops libmpv. Only
  then does it acknowledge completion and request exit. The UI never joins the
  worker. A bounded completion channel and Rune Waker replace idle polling.

On Windows, a small parent-window close gate converts `WM_CLOSE` into Stop after
`on_window` has run. It does not destroy or hide the parent during mpv teardown.
Rune `keep_running()` also protects the ordinary `on_close` path.

**Startup boundary:** an automated native close sent before Rune calls
`on_window` is outside this gate. An experiment that closed the newly created OS
window at that point left Rune startup stuck. Do not use the first nonzero
`Process.MainWindowHandle` as the readiness signal. For close-during-load smoke
tests, wait until the first report sample has been flushed. This establishes
that the host was attached and the native player initialized. The normal finite
completion path does not need an external close.

## Recorded validation

Windows, existing installed runtime, synthetic fixture, debug build:

| Check | Result |
| --- | --- |
| Muted 8-second run | 10 samples, 7.667 s measured advance, zero stalls/drops |
| Shared report analyzer | Accepted native schema and verified sample/summary agreement |
| Hardware decoder | `d3d11va` |
| EOF run (20-second limit, 12-second fixture) | 11.875 s measured advance; automatic exit; final position preserved |
| Close after first sample, during initialization/load | Exit in 504 ms; nonzero status because no progress yet |
| Close during playback | Exit in 260 ms; success summary |
| Missing local fixture | Generic error, nonzero exit, final error summary |
| Example unit test | Passed finite-duration validation |

These are observations, not performance thresholds. The 8-second report had
278.902 MiB peak resident memory, 551.230 MiB peak private memory, and 0.625
sampled CPU-seconds. Those are process-wide values, not decoder-only costs.

## Comparing later changes

Keep the media, duration, machine, runtime, and build profile fixed. Save reports
under distinct names, then use the existing analyzer:

```powershell
cargo run -- --analyze target/native-after.jsonl --baseline target/native-before.jsonl
```

Native and decode-only reports cannot be compared as baselines. Startup and
stalls are sampled at 1 Hz. Missing counters are unknown, not zero. Dedicated
GPU memory is not included. Reported progress and A/V counters do not prove that
pixels reached the display or that audio sounds correct. This example validates
the native adapter with local media; it does not validate streaming transport.
