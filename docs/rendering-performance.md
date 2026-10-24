# Native renderer memory comparison

## Result

Keep `gpu-next` as the default. Traditional `gpu` used about 5 MiB less
resident RAM in two matched 8-second runs, but used 8–10 MiB more private
memory. There is no clear overall memory win from changing the renderer.
This experiment does **not** establish a low-RAM target or a major RAM reduction.

`SCRY_VIDEO_RENDERER=gpu|gpu-next` selects the native renderer. The value is
validated once when `Player::open` creates a native player. An unset value uses
`gpu-next`; all other values return an error. Decode-only playback ignores it.
There is no automatic retry or silent renderer substitution. To test an
alternative renderer on a problem driver, set the other value and create a new
player. Windows explicitly uses `gpu-api=d3d11` and `gpu-context=d3d11` for both
renderers. `hwdec=auto-safe` remains enabled; software decoding is not selected
as a memory optimization.

## Matched measurements

Windows, same machine, installed libmpv, debug build, same example executable,
same original synthetic H.264 1280×720 24 fps / AAC 12-second fixture. Each run
started a new process. Audio output stayed enabled but volume was zero before
load. Run order was `gpu-next`, `gpu`, `gpu`, `gpu-next`; probes ran sequentially.

| 8-second report in `target/` | Peak resident MiB | Peak private MiB | Sampled CPU-seconds | Sampled startup s |
| --- | ---: | ---: | ---: | ---: |
| `native-gpunext.jsonl` | 277.340 | 545.270 | 0.969 | 1.579 |
| `native-gpu.jsonl` | 272.160 | 553.184 | 0.953 | 1.569 |
| `native-gpu-repeat.jsonl` | 272.258 | 556.062 | 0.609 | 1.568 |
| `native-gpunext-repeat.jsonl` | 277.188 | 546.301 | 0.766 | 1.651 |

All four reports used `d3d11va`, had zero observed renderer/decoder drops and
zero stalls, and measured 7.667–7.750 seconds of advancing playback. CPU results
vary enough that these short runs do not establish a CPU advantage. Startup is
the tracker's first sampled advance, not an exact first-frame measurement.
CPU-seconds cover the interval between samples, not all process startup work.

Before media load, samples already showed about 162 MiB resident and 383–384 MiB
private memory. That includes Rune, its GPU device, the native window, libmpv,
and process/runtime costs. It is not a measurement of Rune alone. Renderer
selection cannot remove this shared cost. Rune's GPU memory policy is not
exposed here; no dependency fork or custom compositor was added.

The older `native-probe.jsonl` baseline recorded 278.902 MiB resident and
551.230 MiB private with automatic GPU context selection. It is historical
context, not a matched proof that explicit D3D11 reduced memory.

### EOF and teardown

Both 20-second-limit runs reached actual EOF and exited automatically at about
12.8 seconds of report time. Both retained 11.917 seconds of observed progress,
used `d3d11va`, and had zero observed stalls or renderer/decoder drops:

| Report in `target/` | Peak resident MiB | Peak private MiB | Sampled CPU-seconds |
| --- | ---: | ---: | ---: |
| `native-gpunext-eof.jsonl` | 280.426 | 551.863 | 2.031 |
| `native-gpu-eof.jsonl` | 268.551 | 553.375 | 0.859 |

No probe process remained after testing. EOF measurements have a different
duration from the 8-second comparison and must not be mixed into its baseline.

## Reproduce

See [native-probe.md](native-probe.md) for fixture creation and probe limits.
Build once, then set the environment in each launching shell. No global
environment or Rust process environment mutation is needed.

```powershell
cargo build --example embedded_probe
$env:SCRY_VIDEO_RENDERER = 'gpu-next'
& .\target\debug\examples\embedded_probe.exe target/playback-fixture.mp4 8 target/native-gpunext.jsonl
$env:SCRY_VIDEO_RENDERER = 'gpu'
& .\target\debug\examples\embedded_probe.exe target/playback-fixture.mp4 8 target/native-gpu.jsonl
Remove-Item Env:SCRY_VIDEO_RENDERER
cargo run -- --analyze target/native-gpu.jsonl --baseline target/native-gpunext.jsonl
```

Repeat in reverse order with distinct report names. For EOF, use a 20-second
limit and `native-gpu-eof.jsonl` / `native-gpunext-eof.jsonl`. Do not run the
two renderer probes concurrently. Keep the media, build, runtime and machine
fixed. The tested files had these SHA-256 hashes:

- `runtime/libmpv-2.dll`: `04E4D27DC385DC18DDD93FBE69C75E635B864F3C6EC53A9122DF54C2F7842F4A`
- `target/playback-fixture.mp4`: `C5E6A0CD1ED990EECE6CC334FC1815C187772ECBE67E86E307C95CCF67CDF902`

Memory is process-wide. Resident memory is the working set; private memory is
private committed memory, not another amount to add to the working set.
Dedicated GPU memory is excluded. Samples do not prove display or audio quality.
The unchanged 32 MiB forward / 8 MiB backward demux limits constrain compressed
packet cache, not total process, decoded-frame, or GPU memory. No extra image
cache, quality profile, or decoder-buffer tuning was added.

## Release-build check

A separate optimized `embedded_probe` run using the same local fixture and the
default renderer recorded 263.164 MiB peak resident RAM and 520.887 MiB private
commit, 0.453 sampled CPU-seconds, 7.750 seconds of measured progress, and zero
observed stalls or drops. Report: `target/native-release.jsonl`.

This is the small native probe, not the full library/add-on application. Its
memory is **not** an app-wide RAM guarantee or evidence that Scry uses less
memory than Stremio. A substantial low-memory goal still needs work and matched
real-world benchmarks; the current Rune GPU allocation policy is a likely area
to investigate upstream rather than silently maintaining a framework fork.
