"""Build, test, repeat HTTP/torrent runs, then verify the production boundary.

All trials use the current app and unmodified Cargo-managed Rune dependency.
Trials run sequentially; each repeat reverses transport order to reduce warm-up bias.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--cycles", type=int, default=3)
    parser.add_argument("--seconds", type=float, default=12)
    parser.add_argument("--repeats", type=int, default=2)
    args = parser.parse_args()
    if not 1 <= args.cycles <= 100 or not 1 <= args.seconds <= 3600 or not 1 <= args.repeats <= 10:
        parser.error("cycles must be 1–100, seconds 1–3600, and repeats 1–10")
    report = args.report.resolve()
    report.mkdir(parents=True, exist_ok=False)
    for tool in ("cargo", "ffmpeg"):
        if not shutil.which(tool):
            raise RuntimeError(f"Required tool is missing: {tool}")

    def run(command):
        subprocess.run(command, cwd=ROOT, check=True)

    run(["cargo", "test", "--workspace", "--all-targets", "--features", "profiling", "-q"])
    run(["cargo", "clippy", "--workspace", "--all-targets", "--features", "profiling", "--", "-D", "warnings"])
    run(["cargo", "build", "--release", "--features", "profiling"])
    executable = report / "profiled.exe"
    shutil.copy2(ROOT / "target/release/scry.exe", executable)

    results = []
    for repeat in range(args.repeats):
        order = ("http", "torrent") if repeat % 2 == 0 else ("torrent", "http")
        for transport in order:
            destination = report / f"{repeat + 1}-{transport}"
            command = [sys.executable, str(ROOT / "scripts/profile-app.py"),
                       "--report", str(destination), "--exe", str(executable),
                       "--cycles", str(args.cycles), "--seconds", str(args.seconds)]
            if transport == "torrent":
                command.append("--torrent")
            run(command)
            summary = json.loads((destination / "summary.json").read_text())
            streaming = [phase for name, phase in summary["phases"].items()
                         if name.startswith("cycle-") and not name.endswith("/stopped")]
            peak = max(phase["private_ws"]["peak"] for phase in streaming)
            results.append(dict(repeat=repeat + 1, transport=transport,
                                peak_streaming_private_bytes=peak, under_150_mb=peak < 150_000_000,
                                summary=str(destination.relative_to(report) / "summary.json")))

    run(["cargo", "build", "--release"])
    production = (ROOT / "target/release/scry.exe").read_bytes()
    for hook in (b"SCRY_PROFILE_ROOT", b"SCRY_PROFILE_CONTROL", b"SCRY_PROFILE_MPV_OPTIONS", b"SCRY_PROFILE_TORRENT_PEER"):
        if hook in production:
            raise RuntimeError(f"Profiling hook leaked into production: {hook.decode()}")
    output = dict(production_hooks_absent=True, results=results,
                  under_150_mb=all(row["under_150_mb"] for row in results))
    (report / "suite.json").write_text(json.dumps(output, indent=2))
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
