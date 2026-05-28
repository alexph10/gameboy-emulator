#!/usr/bin/env bash
# Fetches publicly-available Game Boy test ROMs into tests/roms/.
# These ROMs are NOT committed to this repository.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/tests/roms"
mkdir -p "$DEST"

clone_or_update() {
    local url="$1" folder="$2" target="$DEST/$2"
    if [ -d "$target" ]; then
        echo "Updating $folder..."
        git -C "$target" pull --ff-only
    else
        echo "Cloning $folder..."
        git clone --depth 1 "$url" "$target"
    fi
}

clone_or_update https://github.com/retrio/gb-test-roms.git               blargg
clone_or_update https://github.com/Gekkio/mooneye-test-suite.git         mooneye
clone_or_update https://github.com/mattcurrie/dmg-acid2.git              dmg-acid2
clone_or_update https://github.com/mattcurrie/mealybug-tearoom-tests.git mealybug

echo ""
echo "Done. Test ROMs are in $DEST"
