# Rust quality and performance policy

## Local checks

Run both configurations before handing off a change:

```powershell
pwsh -File scripts/check.ps1
pwsh -File scripts/check.ps1 -AllFeatures
```

The script checks lint inheritance for each workspace member, runs rustfmt,
then runs Clippy and tests for every crate and target with `--locked`.
`-AllFeatures` also checks the profiling-only code. No GitHub workflow is needed.
The pinned toolchain includes rustfmt and Clippy.

Warnings fail these checks through `CARGO_BUILD_WARNINGS=deny`. This is the
Cargo 1.97 mechanism recommended by Clippy. Unlike adding `-D warnings` to
compiler arguments, it does not invalidate compiler caches. The script restores
the previous environment setting when it exits.

For dependency checks, install the tools once:

```powershell
cargo install cargo-audit --locked --version 0.22.2
cargo install cargo-machete --locked --version 0.9.1
pwsh -File scripts/check.ps1 -AllFeatures -Dependencies
```

`cargo-machete` checks unused direct dependencies. The library's
`libsqlite3-sys` dependency is intentionally retained to enable bundled SQLite;
its package metadata records this exception. `cargo-audit` checks `Cargo.lock`
against RustSec. It needs network access to refresh advisories. Run it regularly,
not only after dependency changes. No advisories are silently ignored.

### Audit findings from this review

The 2026-10-24 audit failed on two high-severity advisories in `quick-xml 0.37.5`:
[RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194) (quadratic
duplicate-attribute checks) and
[RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195) (unbounded
namespace allocation). The dependency path is
`scry-torrents -> librqbit 8.1.1 -> librqbit-upnp 1.0.0 -> quick-xml`.
Both advisories require `quick-xml >=0.41.0`, outside the current parent's
compatible dependency range. The user approved accepting these two risks on
2026-10-24. `.cargo/audit.toml` ignores only these advisory IDs so local dependency
checks can pass. The vulnerabilities remain unresolved, and exposure has not
been ruled out. Remove the exceptions when an upstream update or reviewed
replacement provides a patched version. Do not force a lockfile-only incompatible
upgrade. Other advisories are not suppressed.

The audit also reports unmaintained `backoff`, `bincode` (both locked versions),
`crypto-hash`, and `instant`. These remain visible RustSec warnings, not proof
that those dependencies are exploitable. Dependency replacement and exposure
analysis need a separate review; a clean Clippy run is not a clean security audit.

## Lint choices

All crates inherit `Cargo.toml` workspace lints. Clippy's default groups already
include correctness, suspicious code, complexity, style, and performance checks.
The explicit `perf` entry makes that intent visible; it is not extra coverage.
`pedantic` adds API, conversion, and idiom checks. Selected restriction and nursery
lints add these checks:

| Area | Rules and purpose |
| --- | --- |
| Exceptions | `allow_attributes`, `allow_attributes_without_reason`, and `unfulfilled_lint_expectations`: prefer justified `#[expect]` attributes and reject stale expectations. |
| Panics | Existing unwrap/expect/panic/todo rules plus `unimplemented`, `unreachable`, `string_slice`, and `unchecked_time_subtraction`. These catch specific panic paths, not all possible panics. |
| Async | `let_underscore_future`, `await_holding_lock`, `await_holding_refcell_ref`, and `large_futures`: catch dropped work, retained synchronous guards, and oversized future values. |
| Results | `unused_must_use` and `unused_result_ok`: catch ignored results and bare `.ok()` statements. Explicit best-effort `let _ = ...` remains possible. |
| Unsafe | `unsafe_op_in_unsafe_fn`, `undocumented_unsafe_blocks`, and `unnecessary_safety_comment`: require explicit unsafe boundaries and relevant safety arguments. |
| Resources | `mem_forget`, `rc_mutex`, and `exit`: flag leaked destructors, misleading synchronization, and exits that skip cleanup. |
| Debug/release parity | `debug_assert_with_mut_call`: flag mutation that disappears in release builds. |
| Numeric values | Pedantic conversion checks plus `lossy_float_literal`: catch unintended narrowing and rounded literals. |
| Allocation costs | Default performance checks plus `redundant_clone` and `inefficient_to_string`: catch avoidable copies and conversions. |

Add-on, storage, and torrent libraries use `#![forbid(unsafe_code)]`. Native
Win32 and libmpv bindings remain in the app and playback crates. A safety comment
must explain pointer validity, lifetime, ownership, or ABI requirements. Merely
adding the word "SAFETY" is not a safety review.

These are guardrails for humans and coding agents, not a security sandbox. An
agent can still edit lint configuration or write a poor exception. Review such
changes as policy changes. Do not disable a rule just to pass a check.

## Deliberate exclusions

- Do not enable the entire `restriction` group. Clippy explicitly warns against
  this, and some of its rules conflict. Do not enable all of `nursery` merely to
  increase the number of checks.
- `indexing_slicing` and `arithmetic_side_effects` are not enabled globally.
  Fixed-size UI geometry and checked numeric operations would need many noisy
  exceptions. Validate external indices and sizes at their boundaries. The
  narrower `string_slice` rule guards UTF-8 boundaries.
- `let_underscore_must_use` is not enabled globally. Thumbnail caching, shutdown
  notification, and report cleanup have intentional best-effort paths. A blanket
  ban would encourage meaningless error-handling boilerplate.
- `map_err_ignore` is not enabled globally. Remote errors can contain private
  URLs and tokens, so several boundaries intentionally replace their messages.
- `multiple_unsafe_ops_per_block` is not enabled. Splitting each native call into
  a separate unsafe block does not prove its shared lifetime/ownership invariant.
- `clone_on_ref_ptr` is not enabled. Changing `arc.clone()` to `Arc::clone(&arc)`
  does not improve runtime performance.
- Do not enforce arbitrary license allowlists or update the whole dependency
  tree automatically. Dependency policy and incompatible upgrades need review.

## Performance

Clippy cannot prove that code is the fastest possible implementation. Prefer
bounded work, reused HTTP clients, borrowed data, and streaming over buffering
entire media files. Keep blocking work off the UI thread. Test observable
behavior, bounds, ordering, and cancellation before changing algorithms.

This review removed owned search deduplication keys, computed search ranks once
per result instead of once per sort comparison, bounded query-length scanning,
and loaded thumbnail pixels without copying them into a second allocation.
These reduce known work; no end-to-end speedup percentage is claimed.

Keep the existing release profile (thin LTO and one codegen unit). Do not add
`target-cpu=native`, unchecked indexing, or unsafe code as speculative speedups.
Use the existing [UI measurements](ui-performance.md) and
[memory profiling tools](memory-profiling.md) for matched release-build comparisons.
Measure CPU, memory, and latency separately. Dynamic libmpv binaries are outside
Cargo's advisory scan and need their own runtime review.

## Research

Reviewed on 2026-10-24:

- [Official Clippy usage](https://doc.rust-lang.org/clippy/usage.html): groups,
  selective restriction rules, and Cargo's warning enforcement.
- [Cargo warning configuration](https://doc.rust-lang.org/cargo/reference/config.html#buildwarnings).
- [Evan Schwartz: Your Clippy Config Should Be Stricter](https://emschwartz.me/your-clippy-config-should-be-stricter/):
  concrete panic/async failure modes and coding-agent guardrails.
- [Billy Levin: Your Clippy Config Should Be Stricter-er](https://billylevin.dev/posts/clippy-config/):
  the opposing blanket-opt-in approach. We retain selective restriction rules.
- [rust-magic-linter](https://github.com/vicnaum/rust-magic-linter):
  AI-assisted Rust presets. The previous configuration was close to its standard
  preset; we assessed additional rules rather than copying a maximum preset.
- [RustSec cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit)
  and [cargo-machete](https://github.com/bnjbvr/cargo-machete) for dependency checks.
