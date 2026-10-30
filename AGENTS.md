# Rust development rules

- On macOS, use `bash scripts/macos/setup.sh` for prerequisites,
  `bash scripts/macos/dev.sh` for isolated development (`--release` optional),
  and `bash scripts/macos/install.sh --launch` for the per-user production bundle.
  macOS scripts and native configuration stay under `scripts/macos/`.
  All PowerShell scripts stay under `scripts/windows/`.

- On Windows, use `pwsh -File scripts/windows/install.ps1 -Launch` to build and update the per-user
  production app outside the repo. Use `pwsh -File scripts/windows/dev.ps1` for a separate
  development copy with isolated data in `target/dev-data` (`-Release` optional).
  Both close only their own deployed app; use `-Force` only if normal shutdown fails.
- Prefer these scripts for dev and production; keep Cargo's existing build path.
  Dev/test builds use reduced symbols and no incremental cache; production keeps
  the release profile. Do not clean after each build. Never delete all of `target`:
  it holds development data. Review exact paths before selective cleanup.

- New workspace members must inherit `[lints] workspace = true`.
- Unless the user asks to skip validation, run `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked`, and
  `cargo test --workspace --all-targets --locked` before handoff.
  Repeat Clippy and tests with `--all-features`. Set `CARGO_BUILD_WARNINGS=deny`
  for these checks without changing compiler flags or build caches.
- Do not lower lint levels, add blanket suppressions, or remove tests to make a
  check pass. Use narrow `#[expect(..., reason = "...")]` only when the invariant
  is understood. Explain policy changes to the user.
- A safety comment must explain why the operation is sound. Do not add unsafe
  code to the add-on, library, or torrent crates.
- Add behavioral regression tests before fixing bugs where practical. Do not
  test Rust's type system or third-party implementation guarantees.
- Prefer bounded work, borrowed data, reused clients, and streaming. Do not
  introduce unsafe operations or unchecked indexing for speculative performance.
- Measure larger performance changes in matched release builds. Report what was
  measured and do not claim optimal performance from lint results alone.
- Keep private URLs and credentials out of errors and diagnostics.
- After dependency changes, run `cargo machete` and `cargo audit --file Cargo.lock`.
  Do not hide advisories or make incompatible dependency upgrades without review.
