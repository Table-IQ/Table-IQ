#!/bin/bash
# Set the app version everywhere it is declared. Merging the result to main publishes
# that version as a release (.github/workflows/developer-preview.yml).
set -euo pipefail
new=${1:?Usage: bump-version.sh <x.y.z>}
[[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Not a x.y.z version: $new" >&2; exit 1; }
cd "$(dirname "$0")/.."
old=$(jq -r .version package.json)
[[ $old != "$new" ]] || { echo "Already $new" >&2; exit 1; }

jq --arg v "$new" '.version = $v' package.json > package.json.tmp && mv package.json.tmp package.json
jq --arg v "$new" '.version = $v' src-tauri/tauri.conf.json > tauri.tmp && mv tauri.tmp src-tauri/tauri.conf.json
perl -0pi -e "s/^version = \"\Q$old\E\"/version = \"$new\"/m" src-tauri/Cargo.toml
perl -0pi -e "s/(name = \"tableiq\"\nversion = )\"\Q$old\E\"/\$1\"$new\"/" src-tauri/Cargo.lock
git diff --stat
echo "$old -> $new. Add a CHANGELOG entry, then open a PR; merging it publishes the release."
