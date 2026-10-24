# Decode-only diagnostics

```powershell
cargo run -- --self-test --seconds 5 --report self-test.jsonl
cargo run -- --probe "https://your-authorized-source/video" --seconds 30 --report probe.jsonl
cargo run -- --analyze probe.jsonl
cargo run -- --analyze probe.jsonl --baseline previous.jsonl
```

Use only media you have permission to play. The self-test generates original raw
Y4M video in a temporary file. It requires libmpv, but not ffmpeg, network access,
or downloaded media. It exercises demuxing, software decoding, and the player
clock. It is **not** a codec stress test. The fixture is removed when the probe
returns. See README.md for libmpv setup.

Probe durations must be finite and between 1 and 3600 seconds. The self-test
defaults to 5 seconds and has a 120-second limit. A source probe defaults to 30
seconds. A clean end of media can end the probe early. Failure, shutdown, or no
measured playback progress returns an error. The report is finalized with its
error flag even when player setup or loading fails, unless report I/O itself
fails. The deadline bounds the event loop, not a blocked native API call or
libmpv teardown.

The report starts before initialization. Its elapsed time includes setup and
load startup. Samples are written and flushed immediately before loading,
then once per second, plus loaded and final samples. At EOF, mpv can clear its
position; the final sample then uses the last position polled within the event
loop (up to 100 ms resolution). The JSONL contains a session record,
metric samples, and a final summary. It does not record source URLs, tokens, or
media titles. Command-line arguments can still be visible to the operating
system or shell history; protect private source links yourself. Report files
are overwritten, so use a distinct path for each run.

Analysis accepts complete schema-1 native playback and decode-only reports. It recalculates and
checks the summary against the samples. Failed sessions remain readable, but
analysis returns an error for them. Missing memory/drop values are printed as
unknown, not as proof of zero use or zero drops. Baseline deltas are descriptive,
not automatic regression thresholds. Compare matching media, builds, durations,
and machines. Native and decode-only reports cannot be compared to each other.
A baseline may have failed; inspect its error flag separately.

## Measurement limits

- Video output is null; hardware decoding is disabled; audio output is null.
- This does **not** measure rendering, GPU cost, display latency, audio latency,
  or the normal UI playback path.
- Startup is the first sample with position above 0.05 seconds, not the precise
  time of first decoded or displayed frame. Sampling resolution is one second.
- A stall is at least two seconds without measured position progress after
  startup, excluding paused intervals. Short stalls can be missed.
- Memory peaks are sampled, process-wide resident/private memory. They are not
  decoder allocations, GPU memory, or proof of a leak.
  On Windows, private bytes are committed memory, not the amount resident in RAM.
- CPU seconds are sampled process CPU deltas summed across cores, not wall-clock
  seconds or a CPU percentage. Setup before the first sample is excluded.
- Drop rates use observed counter maxima divided by elapsed session time.
  Missing counters remain unknown. These are not percentages of total frames;
  null output may not expose useful renderer drop counters.
- Local raw video validates the pipeline, not network buffering or compressed
  codec throughput. Use authorized representative media for those cases.
