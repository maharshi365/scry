#!/bin/bash
set -euo pipefail
[[ ${GITHUB_ACTIONS:-} == true ]] || { echo 'Installer smoke tests run only on disposable GitHub Actions runners.' >&2; exit 1; }
package=${1:?Usage: bash scripts/macos/test-package.sh INSTALLER}
bundle=/Applications/Scry.app
sudo installer -pkg "$package" -target /
test -f "$bundle/Contents/MacOS/scry"
test -f "$bundle/Contents/MacOS/libmpv.2.dylib"
test -f "$bundle/Contents/Resources/FONT-LICENSE.txt"
/usr/bin/codesign --verify --deep --strict "$bundle"
# No dependency may point back to the build runner's Homebrew installation.
python3 - "$bundle" <<'PY'
import pathlib
import subprocess
import sys

directory = pathlib.Path(sys.argv[1]) / 'Contents' / 'MacOS'
for binary in [directory / 'scry', *directory.glob('*.dylib')]:
    lines = subprocess.check_output(['otool', '-L', str(binary)], text=True).splitlines()
    # A dylib's first entry is its own install name, not a dependency.
    for line in lines[2 if binary.suffix == '.dylib' else 1:]:
        dependency = line.strip().split(' (', 1)[0]
        if dependency.startswith(('/usr/lib/', '/System/Library/')):
            continue
        if dependency.startswith('@loader_path/'):
            if (directory / dependency.removeprefix('@loader_path/')).is_file():
                continue
        raise SystemExit(f'Unbundled dependency in {binary.name}: {dependency}')
PY
# An upgrade must replace the whole bundle so obsolete runtime files go away.
sudo touch "$bundle/Contents/Resources/obsolete-installer-test"
sudo installer -pkg "$package" -target /
test ! -e "$bundle/Contents/Resources/obsolete-installer-test"
/usr/bin/codesign --verify --deep --strict "$bundle"
test "$(pkgutil --pkg-info app.scry.desktop | awk '/^version:/ {print $2}')" = "${SCRY_BUILD_VERSION%%[-+]*}"
