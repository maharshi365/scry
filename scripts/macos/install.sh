#!/bin/bash
set -euo pipefail
[[ $(uname -s) == Darwin ]] || { echo 'This script requires macOS.' >&2; exit 1; }
source "$(dirname "$0")/env.sh"
cd "$(dirname "$0")/../.."
unset SCRY_DEV_ICON
cargo build -p scry-app --bin scry --release --locked
target=$(cargo metadata --no-deps --format-version 1 --locked | /usr/bin/plutil -extract target_directory raw -o - -)
bundle="$HOME/Applications/Scry.app"
# Never replace a running app or close an unrelated development copy.
if [[ -e "$bundle/Contents/MacOS/scry" ]] && /usr/sbin/lsof -t "$bundle/Contents/MacOS/scry" >/dev/null 2>&1; then
    echo 'Close the installed Scry app before updating it.' >&2
    exit 1
fi
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target/release/scry" "$bundle/Contents/MacOS/scry"
cp scripts/macos/Info.plist "$bundle/Contents/Info.plist"
cp crates/scry-app/assets/fonts/OFL.txt "$bundle/Contents/Resources/FONT-LICENSE.txt"
/usr/bin/codesign --force --sign - "$bundle"
echo "Installed $bundle (local, ad-hoc signed; Homebrew mpv is required)."
if [[ ${1:-} == --launch ]]; then open "$bundle"; fi
