#!/bin/bash
set -euo pipefail
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { echo 'This script requires Apple Silicon macOS.' >&2; exit 1; }
source "$(dirname "$0")/env.sh"
destination=${1:?Usage: bash scripts/macos/package.sh DESTINATION}
mkdir -p "$destination"
destination=$(cd "$destination" && pwd)
cd "$(dirname "$0")/../.."
unset SCRY_DEV_ICON
cargo build -p scry-app --bin scry --release --locked
metadata=$(cargo metadata --no-deps --format-version 1 --locked)
target=$(printf '%s' "$metadata" | /usr/bin/plutil -extract target_directory raw -o - -)
version=$(printf '%s' "$metadata" | /usr/bin/python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "scry-app"))')
version=${SCRY_BUILD_VERSION:-$version}
staging=$(mktemp -d "${TMPDIR:-/tmp}/scry-package.XXXXXX")
trap 'rm -rf "$staging"' EXIT
bundle="$staging/Scry.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target/release/scry" "$bundle/Contents/MacOS/scry"
cp scripts/macos/Info.plist "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${version%%[-+]*}" "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion ${GITHUB_RUN_NUMBER:-1}" "$bundle/Contents/Info.plist"
cp crates/scry-app/assets/fonts/OFL.txt "$bundle/Contents/Resources/FONT-LICENSE.txt"
# Local signature only. Developer ID signing and notarization are not configured.
/usr/bin/codesign --force --sign - "$bundle"
/usr/bin/codesign --verify --deep --strict "$bundle"
ln -s /Applications "$staging/Applications"
name="Scry-$version-macos-arm64.dmg"
hdiutil create -quiet -ov -fs APFS -format ULFO -volname Scry -srcfolder "$staging" "$destination/$name"
(cd "$destination" && shasum -a 256 "$name" > "$name.sha256")
