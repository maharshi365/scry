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
root="$staging/payload"
bundle="$root/Scry.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target/release/scry" "$bundle/Contents/MacOS/scry"
cp scripts/macos/Info.plist "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${version%%[-+]*}" "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion ${version%%[-+]*}" "$bundle/Contents/Info.plist"
cp crates/scry-app/assets/fonts/OFL.txt "$bundle/Contents/Resources/FONT-LICENSE.txt"
# Keep libmpv beside the executable, where Scry already looks for it. Bundle
# its transitive dependencies as well; recipients must not need Homebrew.
cp "$(brew --prefix mpv)/lib/libmpv.2.dylib" "$bundle/Contents/MacOS/libmpv.2.dylib"
chmod u+w "$bundle/Contents/MacOS/libmpv.2.dylib"
dylibbundler -b -of -x "$bundle/Contents/MacOS/scry" \
    -x "$bundle/Contents/MacOS/libmpv.2.dylib" \
    -d "$bundle/Contents/MacOS" -p '@loader_path/'
for library in "$bundle/Contents/MacOS/"*.dylib; do
    /usr/bin/codesign --force --sign - "$library"
done
# Local signature only. Developer ID signing and notarization are not configured.
/usr/bin/codesign --force --sign - "$bundle"
/usr/bin/codesign --verify --deep --strict "$bundle"
pkgbuild --analyze --root "$root" "$staging/components.plist"
python3 - "$staging/components.plist" <<'PY'
import plistlib
import sys

with open(sys.argv[1], 'rb') as source:
    components = plistlib.load(source)
for component in components:
    component['BundleIsRelocatable'] = False
    component['BundleOverwriteAction'] = 'upgrade'
with open(sys.argv[1], 'wb') as output:
    plistlib.dump(components, output)
PY
name="Scry-$version-macos-arm64.pkg"
# A fixed identifier and location let later packages replace the whole app,
# including obsolete bundled libraries, without touching user data.
pkgbuild --root "$root" --component-plist "$staging/components.plist" \
    --identifier app.scry.desktop --version "${version%%[-+]*}" \
    --install-location /Applications "$destination/$name"
(cd "$destination" && shasum -a 256 "$name" > "$name.sha256")
