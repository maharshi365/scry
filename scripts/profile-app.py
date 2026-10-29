"""Unattended Windows full-app benchmark. Standard library + FFmpeg + Rust only.

All app data is isolated. Native wheel input needs an unlocked interactive desktop.
Reports contain synthetic fixture data only. Run variants sequentially, never in parallel.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import sqlite3
import struct
import subprocess
import sys
import threading
import time
import zlib
from profile_torrent import Seed

ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


class RangeServer(http.server.SimpleHTTPRequestHandler):
    allowed_file = None

    def log_message(self, *_):
        pass

    def send_head(self):
        if self.path.startswith("/poster/"):
            index = int(self.path.rsplit("/", 1)[-1].split(".")[0])
            def chunk(kind, data):
                return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))
            pixels = b"".join(b"\0" + bytes([(index * 13) % 256, y, (index * 7) % 256]) * 160 for y in range(240))
            data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 160, 240, 8, 2, 0, 0, 0))
                    + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return None
        path = Path(self.translate_path(self.path))
        if not path.is_file() or path.resolve() != self.allowed_file:
            self.send_error(404)
            return None
        stream = path.open("rb")
        size = path.stat().st_size
        start, end = 0, size - 1
        byte_range = self.headers.get("Range")
        if byte_range:
            left, right = byte_range.removeprefix("bytes=").split("-", 1)
            if not left:
                start = max(0, size - int(right))
            else:
                start = int(left)
                end = min(int(right) if right else end, end)
        if start > end:
            stream.close()
            self.send_error(416)
            return None
        self.send_response(206 if byte_range else 200)
        self.send_header("Content-Type", "video/mp4")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(end - start + 1))
        if byte_range:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.end_headers()
        stream.seek(start)
        self.remaining = end - start + 1
        return stream

    def copyfile(self, source, output):
        try:
            while self.remaining:
                data = source.read(min(self.remaining, 64 * 1024))
                if not data:
                    break
                output.write(data)
                self.remaining -= len(data)
        except (ConnectionError, OSError):
            pass


class Memory(c.Structure):
    _fields_ = [("cb", w.DWORD), ("faults", w.DWORD)] + [
        (name, c.c_size_t) for name in (
            "peak_ws", "ws", "peak_paged", "paged", "peak_nonpaged", "nonpaged",
            "pagefile", "peak_pagefile", "private", "private_ws", "shared_commit")]


class Region(c.Structure):
    _fields_ = [("base", c.c_void_p), ("allocation", c.c_void_p),
                ("allocation_protect", w.DWORD), ("partition", w.WORD),
                ("size", c.c_size_t), ("state", w.DWORD), ("protect", w.DWORD), ("kind", w.DWORD)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--exe", type=Path, default=ROOT / "target/release/scry.exe")
    parser.add_argument("--options", default="{}", help="Profiling-only mpv option overrides")
    parser.add_argument("--cycles", type=int, default=3)
    parser.add_argument("--seconds", type=float, default=12)
    parser.add_argument("--skip-ui", action="store_true")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--fixture", type=Path, help="Use an existing authorized media fixture instead of generating one")
    parser.add_argument("--map-memory", action="store_true", help="Separate diagnostic pass: classify committed virtual memory")
    parser.add_argument("--torrent", action="store_true", help="Stream the same fixture through a controlled loopback torrent peer")
    parser.add_argument("--fullscreen", action="store_true", help="Also exercise native fullscreen shortcuts during playback")
    parser.add_argument("--metadata-stress", action="store_true", help="Retain 32 history snapshots of a 10,000-episode series")
    args = parser.parse_args()
    if not 1 <= args.cycles <= 100 or not 1 <= args.seconds <= 3600:
        parser.error("cycles must be 1–100 and seconds must be 1–3600")
    if os.name != "nt":
        parser.error("This runner measures Windows native playback")
    report = args.report.resolve()
    report.mkdir(parents=True, exist_ok=False)
    options = json.loads(args.options)
    if args.build:
        subprocess.run(["cargo", "build", "--release", "--features", "profiling"], cwd=ROOT, check=True)
    fixture_dir = ROOT / "target/perf-fixtures"
    fixture_dir.mkdir(exist_ok=True)
    fixture_seconds = max(120, int(args.seconds) + 90)
    fixture = fixture_dir / f"1080p-h264-{fixture_seconds}s.mp4"
    if args.fixture:
        fixture = args.fixture.resolve()
        fixture_dir = fixture.parent
    elif not fixture.exists():
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=24",
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", str(fixture_seconds),
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "24", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-movflags", "+faststart", str(fixture),
        ], check=True)
    RangeServer.allowed_file = fixture.resolve()
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(RangeServer, directory=str(fixture_dir)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    control = report / "control"
    control.mkdir()
    data = report / "profile/data"
    data.mkdir(parents=True)
    with sqlite3.connect(data / "scry-v2.sqlite3") as db:
        db.executescript((ROOT / "crates/scry-library/src/schema.sql").read_text())
        db.execute("PRAGMA user_version=1")
        db.executemany("INSERT INTO media(media_id,namespace,kind,external_id,title) VALUES(?,'profile','movie',?,?)",
                       [(i + 1, f"profile-{i}", f"Benchmark movie {i}") for i in range(1000)])
        db.executemany("INSERT INTO watchlist(media_id) VALUES(?)", [(i + 1,) for i in range(1000)])
    env = dict(os.environ, SCRY_PROFILE_ROOT=str(report / "profile"),
               SCRY_PROFILE_CONTROL=str(control), SCRY_PROFILE_MPV_OPTIONS=json.dumps(options))
    env["Scry_MPV_LIBRARY"] = str(ROOT / "runtime/libmpv-2.dll")
    seed = Seed(fixture) if args.torrent else None
    if seed:
        env["SCRY_PROFILE_TORRENT_PEER"] = seed.address
    user = c.WinDLL("user32", use_last_error=True)
    kernel = c.WinDLL("kernel32", use_last_error=True)
    psapi = c.WinDLL("psapi", use_last_error=True)
    kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    kernel.OpenProcess.restype = w.HANDLE
    kernel.CloseHandle.argtypes = [w.HANDLE]
    psapi.GetProcessMemoryInfo.argtypes = [w.HANDLE, c.POINTER(Memory), w.DWORD]
    kernel.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
    kernel.VirtualQueryEx.argtypes = [w.HANDLE, c.c_void_p, c.POINTER(Region), c.c_size_t]
    kernel.VirtualQueryEx.restype = c.c_size_t
    user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
    user.IsWindowVisible.argtypes = [w.HWND]
    user.SetForegroundWindow.argtypes = [w.HWND]
    user.GetForegroundWindow.restype = w.HWND
    user.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    user.ClientToScreen.argtypes = [w.HWND, c.POINTER(w.POINT)]
    user.GetClientRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
    user.mouse_event.argtypes = [w.DWORD, w.DWORD, w.DWORD, w.DWORD, c.c_size_t]
    user.keybd_event.argtypes = [w.BYTE, w.BYTE, w.DWORD, c.c_size_t]
    phase = "startup"
    stopped = threading.Event()
    summaries = {}
    failures = []
    quality = []
    command_id = 0
    status = {}
    window_size = None
    with (report / "app.stdout").open("w") as stdout, (report / "app.stderr").open("w") as stderr:
        process = subprocess.Popen([str(args.exe.resolve())], cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
        handle = kernel.OpenProcess(0x410, False, process.pid)
        if not handle:
            raise c.WinError(c.get_last_error())
        started = time.perf_counter()

        def read_status():
            nonlocal status
            try:
                status = json.loads((control / "status.json").read_text())
            except (OSError, ValueError):
                pass
            return status

        def sample():
            previous_phase = None
            with (report / "timeline.jsonl").open("w") as output:
                while not stopped.is_set():
                    memory = Memory(cb=c.sizeof(Memory))
                    if not psapi.GetProcessMemoryInfo(handle, c.byref(memory), memory.cb):
                        if process.poll() is None and phase != "closing":
                            failures.append("Process memory sampling failed")
                        break
                    times = [w.FILETIME() for _ in range(4)]
                    cpu = None
                    if kernel.GetProcessTimes(handle, *(c.byref(t) for t in times)):
                        cpu = sum((t.dwHighDateTime << 32) | t.dwLowDateTime for t in times[2:]) / 1e7
                    row = dict(elapsed_s=time.perf_counter() - started, phase=phase,
                               private_ws=memory.private_ws, commit=memory.private, resident=memory.ws,
                               cpu_s=cpu, status=read_status())
                    entry = summaries.setdefault(row["phase"], dict(samples=0, first_cpu_s=cpu))
                    entry["samples"] += 1
                    entry["last_cpu_s"] = cpu
                    entry["last_playback_sample"] = row["status"].get("sample")
                    for metric in ("private_ws", "commit", "resident"):
                        counter = entry.setdefault(metric, dict(peak=0, final=0))
                        counter["peak"] = max(counter["peak"], row[metric])
                        counter["final"] = row[metric]
                    output.write(json.dumps(row) + "\n")
                    output.flush()
                    if args.map_memory and phase != previous_phase:
                        previous_phase = phase
                        address = 0
                        totals = {}
                        private = {}
                        while address < (1 << 47):
                            region = Region()
                            if not kernel.VirtualQueryEx(handle, address, c.byref(region), c.sizeof(region)):
                                break
                            if region.state == 0x1000:
                                key = {0x20000: "private", 0x40000: "mapped", 0x1000000: "image"}.get(region.kind, "other")
                                totals[key] = totals.get(key, 0) + region.size
                                if key == "private":
                                    base = hex(region.allocation or 0)
                                    private[base] = private.get(base, 0) + region.size
                            next_address = (region.base or 0) + region.size
                            if next_address <= address:
                                break
                            address = next_address
                        with (report / "virtual-memory.jsonl").open("a") as maps:
                            maps.write(json.dumps(dict(phase=previous_phase, committed=totals,
                                largest_private=sorted(private.items(), key=lambda item: item[1], reverse=True)[:30])) + "\n")
                    stopped.wait(0.1)

        sampler = threading.Thread(target=sample)
        sampler.start()

        def wait(predicate, timeout=30):
            deadline = time.perf_counter() + timeout
            while time.perf_counter() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"App exited with {process.returncode}")
                current = read_status()
                if current.get("error"):
                    raise RuntimeError(current["error"])
                if predicate(current):
                    return
                time.sleep(0.05)
            raise TimeoutError(f"Timed out in {phase}; status={status}")

        def command(action, **kwargs):
            nonlocal command_id
            command_id += 1
            temp = control / "command.tmp"
            temp.write_text(json.dumps(dict(id=command_id, action=action, **kwargs)))
            temp.replace(control / "command.json")
            wait(lambda s: s.get("command_id") == command_id)

        def rest(seconds):
            deadline = time.perf_counter() + seconds
            while time.perf_counter() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("App exited during measurement")
                current = read_status()
                if current.get("error"):
                    raise RuntimeError(current["error"])
                if phase.endswith(("/playing", "/post-seek", "/resumed", "/paused")) and not current.get("playing"):
                    raise RuntimeError("Playback stopped before the measurement finished")
                time.sleep(0.1)

        def drops():
            playback = read_status().get("sample", {})
            values = (playback.get("dropped_frames"), playback.get("decoder_drops"))
            if any(value is None for value in values):
                raise RuntimeError("Native frame-drop counters are unavailable")
            return values

        window = None
        callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)

        @callback_type
        def find_window(hwnd, _):
            nonlocal window
            pid = w.DWORD()
            user.GetWindowThreadProcessId(hwnd, c.byref(pid))
            if pid.value == process.pid and user.IsWindowVisible(hwnd):
                window = hwnd
                return False
            return True

        def wheel(delta):
            user.SetForegroundWindow(window)
            if user.GetForegroundWindow() != window:
                raise RuntimeError("App is not foreground; no native input sent")
            point = w.POINT(800, 550)
            user.ClientToScreen(window, c.byref(point))
            user.SetCursorPos(point.x, point.y)
            user.mouse_event(0x800, 0, 0, delta & 0xffffffff, 0)

        def key(code):
            user.SetForegroundWindow(window)
            if user.GetForegroundWindow() != window:
                raise RuntimeError("App is not foreground; no native key sent")
            user.keybd_event(code, 0, 0, 0)
            user.keybd_event(code, 0, 2, 0)

        try:
            wait(lambda s: "page" in s)
            user.EnumWindows(find_window, 0)
            if not window:
                raise RuntimeError("App window not found")
            rect = w.RECT()
            if user.GetClientRect(window, c.byref(rect)):
                window_size = [rect.right, rect.bottom]
            rest(3)
            if not args.skip_ui:
                phase = "catalog"
                command("catalog", count=1000, artwork_base=f"http://127.0.0.1:{server.server_port}/poster")
                rest(2)
                for index in range(3):
                    phase = f"scroll-{index}"
                    initial_row = read_status().get("first_visible_row")
                    for _ in range(20):
                        wheel(-1200)
                        rest(0.1)
                    final_row = read_status().get("first_visible_row")
                    if initial_row is not None and final_row is not None and final_row <= initial_row:
                        raise RuntimeError("Native wheel input did not scroll the catalog")
                    wheel(120000)
                    rest(1)
                phase = "detail"
                command("detail")
                rest(2)
                for page in ("addons", "library", "calendar"):
                    phase = page
                    command(page)
                    rest(2)
            if args.metadata_stress:
                phase = "metadata-history"
                command("metadata_stress")
                wait(lambda s: s.get("detail_episodes") == 10_000 and s.get("back_len") == 32)
                rest(3)
            for cycle in range(args.cycles):
                prefix = f"cycle-{cycle + 1}"
                phase = prefix + "/starting"
                if seed:
                    command("play_stream", stream={"infoHash": seed.info_hash.hex(), "fileIdx": 0})
                else:
                    command("play", source=f"http://127.0.0.1:{server.server_port}/{fixture.name}")
                wait(lambda s: s.get("ready") and (s.get("sample", {}).get("position_s") or 0) > 0.1)
                phase = prefix + "/playing"
                initial_drops = drops()
                rest(args.seconds)
                before_pause = drops()
                if before_pause != initial_drops:
                    raise RuntimeError("Playback dropped frames during steady playback")
                phase = prefix + "/paused"
                key(ord("K"))
                wait(lambda s: s.get("sample", {}).get("paused"))
                rest(2)
                phase = prefix + "/resumed"
                key(ord("K"))
                wait(lambda s: not s.get("sample", {}).get("paused", True))
                rest(2)
                if args.fullscreen:
                    phase = prefix + "/fullscreen"
                    key(ord("F"))
                    wait(lambda s: s.get("fullscreen"))
                    rest(2)
                    key(27)
                    wait(lambda s: not s.get("fullscreen", True))
                phase = prefix + "/seek"
                before_seek = drops()
                command("seek", seconds=60)
                wait(lambda s: (s.get("sample", {}).get("position_s") or 0) >= 60)
                # mpv can count the cancelled/late seek-transition frame as a
                # renderer drop. Keep these counts, but gate steady playback
                # separately after a fixed, recorded two-second settling period.
                rest(2)
                after_seek = drops()
                phase = prefix + "/post-seek"
                rest(args.seconds)
                playback = read_status().get("sample", {})
                if "cache_forward_bytes" in playback and playback["cache_forward_bytes"] is None:
                    raise RuntimeError("Native cache metrics are missing; adaptive cache sizing was not exercised")
                if (playback.get("position_s") or 0) < 60 + args.seconds * 0.7:
                    raise RuntimeError("Playback did not advance after seeking")
                final_drops = drops()
                quality.append(dict(cycle=cycle + 1, startup=initial_drops,
                                    steady_before_seek=[a - b for a, b in zip(before_pause, initial_drops)],
                                    other_transitions=[a - b for a, b in zip(before_seek, before_pause)],
                                    seek_transition=[a - b for a, b in zip(after_seek, before_seek)],
                                    steady_after_seek=[a - b for a, b in zip(final_drops, after_seek)]))
                if final_drops != after_seek:
                    raise RuntimeError("Playback dropped frames during steady post-seek playback")
                phase = prefix + "/stopped"
                command("stop")
                wait(lambda s: not s.get("playing"))
                rest(8)
        except Exception as error:
            failures.append(str(error))
        finally:
            phase = "closing"
            if window and process.poll() is None:
                user.PostMessageW(window, 0x10, 0, 0)
            try:
                process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                failures.append("App shutdown timed out; terminated owned process")
                process.terminate()
                process.wait(timeout=10)
            stopped.set()
            sampler.join()
            kernel.CloseHandle(handle)
            server.shutdown()
            if seed:
                seed.close()
    for entry in summaries.values():
        first, last = entry.pop("first_cpu_s"), entry.pop("last_cpu_s")
        entry["cpu_s"] = last - first if first is not None and last is not None else None
    if not summaries:
        failures.append("No process-counter samples were collected")
    summary = dict(success=not failures, failures=failures, phases=summaries,
                   quality=quality,
                   transport="torrent" if seed else "http", torrent_uploaded_bytes=seed.uploaded if seed else None,
                   client_pixels=window_size, windows_version=sys.getwindowsversion().build,
                   runtime_sha256=sha256(ROOT / "runtime/libmpv-2.dll"),
                   options=options, executable_sha256=sha256(args.exe),
                   fixture_sha256=sha256(fixture))
    (report / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(dict(success=not failures, failures=failures, phases={
        name: round(values["private_ws"]["peak"] / 1e6, 1) for name, values in summaries.items()}), indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
