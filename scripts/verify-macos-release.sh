#!/bin/bash
# Read-only checks on an already built Mac app. No upload, signing, or launch.
set -euo pipefail
app=${1:?Usage: verify-macos-release.sh /path/TableIQ.app [download.dmg]}
[[ -d "$app/Contents" ]] || { echo 'Not a Mac app bundle' >&2; exit 1; }
codesign --verify --deep --strict --verbose=2 "$app"
# A valid ad-hoc signature is insufficient for distribution.
signature=$(codesign -dv --verbose=4 "$app" 2>&1)
[[ "$signature" == *'Authority=Developer ID Application:'* ]] || { echo 'Developer ID Application signature required' >&2; exit 1; }
xcrun stapler validate "$app"
spctl --assess --type execute --verbose=2 "$app"
plist="$app/Contents/Info.plist"
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")
minimum=$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist")
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")
printf 'Version: %s\nMinimum macOS in bundle: %s\nArchitectures: ' "$version" "$minimum"
lipo -archs "$app/Contents/MacOS/$executable"
du -sk "$app"
if [[ $# -gt 1 ]]; then
  [[ -f "$2" ]] || { echo 'Download artifact missing' >&2; exit 1; }
  stat -f 'Download bytes: %z' "$2"
  shasum -a 256 "$2"
fi
printf '%s\n' 'Signature/notarization checks passed. Clean-Mac installation, update preservation, and runtime checks still require recorded evidence.'
