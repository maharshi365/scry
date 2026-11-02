<p align="center">
  <img src="crates/scry-app/assets/scry.svg" width="120" height="120" alt="Scry logo">
</p>

<h1 align="center">Scry</h1>

<p align="center">A native streaming app for movies and TV, with Stremio add-on support and a built-in player.</p>

Scry is an early prototype. Save titles, track your progress, and watch without a browser or external player.

## Get started

Ask your coding agent:

> Clone https://github.com/maharshi365/scry and follow AGENTS.md to do a production install for my OS.

## OS support

| OS | Support |
| --- | --- |
| Windows (x64) | Supported |
| macOS (Apple Silicon / M-series only) | Supported |
| macOS (Intel) | Not supported |
| Linux (x64) | Experimental |

## Linux

Build from source with the pinned Rust toolchain, `pkg-config`, OpenSSL development
headers, and `libxkbcommon` development headers. Install libmpv **0.40 or newer**
(`libmpv.so.2`) and a Vulkan driver. File pickers require an XDG Desktop Portal
service and a desktop-specific portal backend.

```sh
cargo run --locked --bin scry
```

Video is composited with the app controls on X11 and Wayland. This initial frame
path caps rendering at 1280×720. Fullscreen works on X11; native Wayland fullscreen
needs an API that the pinned Rune version does not expose yet.

Validated in Debian 13 Docker with libmpv 0.40: default/all-feature checks, video
rendering and resizing, dual subtitles, X11 fullscreen and shutdown, and a headless
Weston Wayland playback probe. Real desktop audio output and hardware acceleration
still need testing on a Linux machine.

To repeat the automated checks and virtual X11 desktop smoke test:

```sh
docker build -f scripts/linux/Dockerfile -t scry-linux-test .
docker run --name scry-linux-check scry-linux-test
```

Reports and screenshots stay in the container under `/work/target/linux-check`.
