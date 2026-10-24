# UI performance checks

## First optimization pass

Release builds, Windows, no video playback. These are observations on one
machine, not RAM or latency guarantees.

The catalog regression test uses 1,000 distinct title records at 880×600. It
creates a real Rune runtime without a GPU window and settles five frames:

| Measurement | Before | After |
| --- | ---: | ---: |
| Full card trees | 1,000 | 12 |
| Runtime creation and settling | 121 ms | 61 ms |
| Process resident memory | 59 MiB | 27 MiB |
| Process private commit | 66 MiB | 26 MiB |

Run the check alone to avoid including other tests' allocations:

```powershell
cargo test --release --bin scry large_catalog_only_lays_out_nearby_cards -- --nocapture
```

Only nearby rows now build full cards. Distant rows keep fixed-height
placeholders, so scrolling, partial final rows, and history still work. Tests
check reaching the last title, opening it, returning to it, and fetching artwork
only for the viewport. This does not remove all O(n) work: row placeholders and
metadata still grow with catalog size.

The thumbnail cache retains the existing 32-image RAM bound. A second tier stores
at most 32 MiB / 256 thumbnails locally for seven days. A disk hit skips HTTP,
source-image decoding, and resizing, but Rune can still need to upload the image.
There can still be a brief placeholder after a large scroll jump. Cache keys are
hashed; the files contain only dimensions and small RGBA pixel buffers.

Already-sized thumbnails now use Rune's direct pixel path. The current card and
detail sizes do not sample the lower mip levels, so building and retaining a
second mip pyramid adds work without improving those views. Removing it reduces
the 32-thumbnail pixel budget from about 6.3 MiB to 4.7 MiB and removes the extra
mip worker. This is not a claim that the whole app uses only that much memory.

The native workload also exposed a request-scheduling bug: polling ran before
layout updated the viewport, and the window could go idle before it queued the
newly visible posters. Images then waited for another input or wake. Layout
callbacks now refresh the artwork queue directly. A regression test reproduces
the missing request while driving only the frames Rune actually needs.

## Measure the whole app

### Native click-through results

The final release build was exercised at 1280×820 with Cinemeta and the existing
stream add-on, a 49-title movie catalog, three full scroll/return passes, movie and
series details, Library, Add-ons, and search. App-only screenshots confirmed the
screens and restored posters. Search details contained 96 source cards; no source
was played. The disk thumbnail cache was warm. These are
sampled private-working-set peaks, not total resident memory:

| Step | Peak private RAM | Process CPU work during step |
| --- | ---: | ---: |
| Discover, settled | 90 MiB | 0 ms |
| Three full scroll/return passes | 107 MiB | 1,328 ms total |
| Movie details | 104 MiB | 141 ms |
| Library (empty watchlist) | 102 MiB | 63 ms |
| Add-ons | 99 MiB | 47 ms |
| Series catalog | 104 MiB | 94 ms |
| Series details | 104 MiB | 94 ms |
| Search | 109 MiB | 109 ms |
| Search details | 112 MiB | 141 ms |

CPU work is not click latency. Each ordinary step includes three seconds of
settling; detail steps use six, and remote catalog/search steps use eight.
Sampling is every 100 ms after input dispatch. One core can accumulate 1,000 ms
of CPU work per second. Windows CPU-time counters have coarse resolution.

An earlier baseline route reached 133 MiB and intermediate optimized runs reached
137 MiB. Other idle trials varied substantially. The final 112 MiB observation
is useful, but it is not enough to promise a stable 21 MiB reduction across
machines, driver/cache states, or longer sessions. The catalog stress test above
isolates the card-tree improvement more clearly. Baseline return-to-top captures
also contained blank posters after the settling window; the final captures did
not. The request-scheduling regression test isolates that correctness fix.

### Reproduce

See the commands under **Performance diagnostics** in `README.md`:

- `scripts/measure-idle.ps1`: warmed native-window memory and CPU.
- `scripts/measure-ui.ps1`: actual native click/scroll workload, sampled every
  100 ms, with CPU work and memory per step. Use app-only screenshots to confirm
  the intended screen was reached. The current Library fixture is empty; this
  does not establish populated-library performance.
- `scripts/measure-ui.ps1 -ScrollOnly -ScrollPasses 3`: repeated full-catalog
  scrolling and returns, which exercise RAM-cache eviction and disk reuse.

Keep viewport, installed providers, catalog, workload, and settling time fixed.
Run before/after several times. A warm thumbnail cache is a different case from
a cold one; do not present a warm-cache result as a first-run result. Network
responses and poster failures can vary between runs.

Use **private working set** when comparing Task Manager's usual Memory column.
Total resident memory includes shared code and driver pages. Private commit is
not the same as resident RAM. Initial idle trials varied enough that a single
sample does not establish a dependable idle-memory reduction.

## Remaining work

Playback and the torrent session are already lazy. Loading their code in the
executable is not the same as running a decoder or torrent engine. The native
window still initializes Rune/wgpu, font/glyph storage, image atlas pages, and
GPU-driver state even when no video is playing. NVIDIA driver modules were
present in the idle process; their mapped module sizes are not resident-memory
measurements and must not simply be added up as RAM usage.

Rune currently enables both Vulkan and DX12 on Windows, creates its default GPU
instance, and requests `MemoryHints::Performance`. A useful next renderer-level
experiment is to compare DX12-only initialization and memory-oriented allocation
on the same workload. Its public app API does not expose those settings at the
pinned revision. This pass does not vendor/fork Rune or silently change backend
support. Atlas fragmentation is another renderer-level item to profile during
long browsing sessions.

A Windows mimalloc trial increased memory use in this workload. It was removed,
not retained on the assumption that another allocator must be faster.
