"""Resolve Cargo's existing output location without changing its configuration."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def build_directory():
    result = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
                            cwd=ROOT, check=True, capture_output=True, text=True)
    return Path(json.loads(result.stdout)["target_directory"])


def release_executable():
    return build_directory() / "release" / "scry.exe"
