# Fetches publicly-available Game Boy test ROMs into tests/roms/.
# These ROMs are NOT committed to this repository.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'tests\roms'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

function Clone-OrUpdate($url, $folder) {
    $target = Join-Path $dest $folder
    if (Test-Path $target) {
        Write-Host "Updating $folder..."
        git -C $target pull --ff-only
    } else {
        Write-Host "Cloning $folder..."
        git clone --depth 1 $url $target
    }
}

Clone-OrUpdate 'https://github.com/retrio/gb-test-roms.git'              'blargg'
Clone-OrUpdate 'https://github.com/Gekkio/mooneye-test-suite.git'        'mooneye'
Clone-OrUpdate 'https://github.com/mattcurrie/dmg-acid2.git'             'dmg-acid2'
Clone-OrUpdate 'https://github.com/mattcurrie/mealybug-tearoom-tests.git' 'mealybug'

Write-Host "`nDone. Test ROMs are in $dest"
