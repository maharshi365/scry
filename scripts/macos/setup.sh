#!/bin/bash
set -euo pipefail
[[ $(uname -s) == Darwin ]] || { echo 'This script requires macOS.' >&2; exit 1; }
command -v brew >/dev/null || { echo 'Install Homebrew from https://brew.sh first.' >&2; exit 1; }
xcode-select -p >/dev/null || { echo 'Run xcode-select --install first.' >&2; exit 1; }
export HOMEBREW_NO_INSTALL_CLEANUP=1
source "$(dirname "$0")/env.sh"
if ! command -v rustup >/dev/null; then
    brew install rustup
fi
source "$(dirname "$0")/env.sh"
brew install mpv powershell
rustup update stable
rustup default stable
cd "$(dirname "$0")/../.."
rustup show
echo 'Put $HOME/.cargo/bin before Homebrew in your shell PATH (or the rustup formula bin directory).'
cargo --version
rustc --version
