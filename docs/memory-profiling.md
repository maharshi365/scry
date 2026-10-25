# Full-app memory profiling

Windows, Python 3, FFmpeg, the Rust toolchain, and the configured libmpv runtime
are required. The native scroll workload needs an unlocked desktop and takes
foreground focus. No personal database or provider credentials are used.

For the complete unattended test/build/playback/production-verification sequence:

```powershell
python scripts/profile-suite.py --report target/perf-suite
```

This runs HTTP and controlled torrents using the current app, repeats with
reversed transport order, and rebuilds the normal production binary last.
Rune and rune-motion are downloaded by Cargo from the original pinned upstream
revision; there is no vendored framework or custom renderer configuration.
The suite records whether the runs meet 150 MB instead of treating a
successful playback test as proof of meeting the memory budget.

```powershell
python scripts/profile-app.py --build --report target/perf-run
python scripts/profile-app.py --torrent --report target/perf-torrent
```

The report directory must be new. The runner generates original 1080p H.264/AAC
media, serves it over byte-range HTTP on loopback, and seeds a separate 1,000-movie
library. It drives a 1,000-title catalog, native wheel scrolling, details and page
changes, then repeated play/pause/resume/seek/stop cycles in the full native app.
Default playback phases last 12 seconds each; `--seconds 1800 --cycles 1` provides
two 30-minute playback phases. Longer runs generate a longer fixture automatically;
allow enough disk space and encoding time. Playback failures, early EOF, lack of
post-seek progress, and steady-playback frame drops fail the run. Startup and
seek-transition drops are recorded separately. The seek phase includes a fixed
two-second settling interval; its memory peaks remain in the timeline and suite
budget check. Missing native cache/drop counters fail the measurement rather
than being counted as zero.
Quality-counter pairs in the JSON are `[renderer drops, decoder drops]`.

`--metadata-stress` retains 32 history snapshots of a 10,000-episode series before
playback. This checks the large-inventory case; normal app history remains bounded
to 32 entries. `--fullscreen` also drives F/Escape through native keyboard input.

The timeline samples private working set, private commit, total resident memory,
and process CPU time every 100 ms, including during native input. It joins those
samples with page, playback, artwork, and history state. Missing measurements are
not zeros. These are sampled peaks, not guarantees that no shorter spike occurred.
UI routes use the app's ordinary state methods; scrolling and pause/resume use
Windows native input. Page acknowledgements do not measure input-to-present latency.

`summary.json` contains per-phase results and executable/media hashes. The
timeline and app logs remain local. Default fixtures are synthetic; `--fixture`
can supply other authorized, sufficiently long media. `--torrent` uses the same
file via a loopback-only fixture peer, real torrent metadata/piece transfer, and
the app's normal torrent-to-HTTP bridge. DHT and trackers are disabled only for
this explicitly configured profiling session. Uploaded fixture bytes are recorded
to distinguish actual transfer from an empty test. This does not simulate a public
swarm or slow WAN. It does not establish physical audio/display quality.

`--map-memory` is a separate attribution pass: it records committed private,
mapped, and image regions and the largest private virtual allocations. These are
commit classifications, not resident-RAM attribution or allocation stack traces.

## Production boundary

Only the explicit, non-default `profiling` Cargo feature compiles the control
module, isolated-path override, diagnostic artwork counters, loopback HTTP artwork
support, loopback torrent-peer override, and mpv experiment overrides. A normal `cargo build --release` excludes
them. Merely setting the environment variables cannot enable them in production.

Both `SCRY_PROFILE_ROOT` and `SCRY_PROFILE_CONTROL` must be set to enable control.
Commands and status use local files rather than a publicly listening control
server. Keep the run directory private. Profiling files are bounded snapshots;
the external runner streams the timeline to disk.

For matched mpv experiments, use `--options` with a JSON object of string values.
This override exists only in profiling builds. Preserve a baseline executable
with `--exe`, run the same route and durations, and repeat sequentially. Do not
compile or run other benchmarks during measured trials. Check failures and
playback drops, not just the memory numbers.

## App-side memory changes

- Metadata clones share descriptions and episode arrays. Editing a snapshot uses
  copy-on-write, so old history and worker snapshots keep their original values.
  Consuming uniquely owned metadata can still move its fields without copying.
- Pages with no visible artwork release their decoded RAM-cache entries. Poster
  thumbnails remain in the bounded disk cache; GPU atlas lifetime is separate.
- Native playback sizes compressed forward read-ahead from observed packet bytes
  per buffered second: ten seconds plus 25% headroom, rounded to MiB within
  8–32 MiB. Unknown/startup data keeps the original 32 MiB ceiling. Later bitrate
  bursts can increase the budget immediately; it does not oscillate down again.
  The backward packet cache is capped at 2 MiB instead of 8 MiB. Backward seeks
  outside that cache may need another range request. None of these limits bounds
  decoded frames, GPU-driver allocations, or total process RAM.

## Historical measurements with framework patches

**The framework patches have been removed.** The figures below describe the
earlier experiment, not the current unmodified Rune build. The custom GPU
allocation policy, Windows backend selection, and small-logo atlas policy no
longer ship. The app-side improvements above remain. Remeasure the current build
before making memory-reduction claims; the earlier percentages do not carry over.

2026-10-24, NVIDIA RTX 3070 Ti, release optimizations, original 1080p24 H.264/AAC
fixture, 1280×820 client area, 1,000 catalog titles and saved movies. Each process
browsed/scrolled first, then ran three play/pause/resume/seek/stop cycles. MB below
means 1,000,000 bytes, not MiB.

| Sampled measurement | Earlier build | Optimized pass |
| --- | ---: | ---: |
| Private working-set peaks during ordinary playback, three cycles | 259–260 MB | 223–230 MB |
| Largest private-commit sample during ordinary playback | 654 MB | 474 MB |
| Largest private-working-set sample around seeking | 336 MB | 297 MB |

Reports: `target/perf-policy-before` and `target/perf-compact-after`. The earlier
build already had the artwork-cache release fix, but preceded shared metadata,
adaptive packet sizing, and the low-memory renderer policy. The optimized pass
here preceded the final small-logo atlas change. These are single-process paired
runs, not statistical guarantees. Later short renderer-only controls varied in
private RAM, including a torrent case where the optimized run used more. Private
commit improved more clearly than resident RAM. Do not claim a universal RAM win
or add private commit to working set.

Both three-cycle runs used D3D11 hardware decoding and observed zero decoder or
renderer drops. CPU work varied; three scroll passes used about 2.1 CPU-seconds
before and 2.6 after. This is not click-to-paint latency or proof of faster UI.

A separate 4K24 10-bit HEVC test passed native pause/resume, F/Escape, seeking,
and 32 retained history snapshots of a 10,000-episode series. It observed zero
frame drops, but reached about 298 MB private working set during ordinary
playback and 514 MB around seeking. A controlled torrent run transferred over
400 MB from the local fixture peer and completed both playback cycles.
After the small-logo atlas change, a two-cycle 720p24 test with the full browsing
route and F/Escape observed zero frame drops. Ordinary-playback private RAM peaked
at 197–199 MB, with about 252 MB around seeking (`target/perf-720p-final`).
A final two-cycle 1080p24 run of the small-atlas build peaked at 220–223 MB
private RAM during ordinary playback, 475 MB private commit, and 293 MB private
RAM around seeking (`target/perf-1080p24-final`). A separate 1080p60 H.264 run
also passed native pause/resume, F/Escape, and seeking with zero observed drops
(`target/perf-1080p60-final`).

**The 150 MB streaming target is not met.** No working-set trimming, lower video
resolution, disabled hardware decoding, or null-output playback was used to make
these numbers smaller. GPU dedicated/shared memory and physical display latency
are not measured by this runner. Long-run automation is available, but these
short trials do not establish leak-free operation over a whole movie.
