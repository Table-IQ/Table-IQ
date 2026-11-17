#!/bin/bash
# Collect TableIQ's own license files plus every bundled dependency's license texts into
# one zip: the "notices" asset attached to each release. Needs node_modules (pnpm install)
# and the cargo registry (any cargo build/fetch) to be present.
set -euo pipefail
out=${1:?Usage: release-notices.sh <output.zip>}
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
list="$work/DEPENDENCY-LICENSES.txt"

copy_licenses() { # <package dir> <dest dir>; prints how many files it copied
  local n=0 f
  mkdir -p "$2"
  for f in "$1"/LICEN[CS]E* "$1"/licen[cs]e* "$1"/COPYING* "$1"/NOTICE* "$1"/UNLICENSE*; do
    [[ -f "$f" ]] && cp "$f" "$2/" && n=$((n + 1))
  done
  [[ $n -gt 0 ]] || rmdir "$2"
  echo $n
}

cp "$root"/{LICENSE,LICENSE-EXCEPTIONS.md,LICENSING.md,NOTICE} "$work/"
mkdir -p "$work/assets"
cp "$root"/docs/brand/licenses/*.txt "$root/src/assets/engines/LICENSE-devicon" "$work/assets/"

echo "# Third-party components bundled in TableIQ (ecosystem, name, version, declared license)" > "$list"

# Rust crates compiled into the Mac binary.
cargo metadata --format-version 1 --locked --manifest-path "$root/src-tauri/Cargo.toml" \
    --filter-platform aarch64-apple-darwin |
  jq -r '.packages[] | select(.source != null) | [.name, .version, (.license // "see files"), .manifest_path] | @tsv' |
  while IFS=$'\t' read -r name version license manifest; do
    echo "cargo  $name  $version  $license" >> "$list"
    [[ $(copy_licenses "$(dirname "$manifest")" "$work/cargo/$name-$version") -gt 0 ]] ||
      echo "  (no license file shipped by the crate; declared: $license)" >> "$list"
  done

# npm packages in the desktop UI bundle (production dependencies only).
(cd "$root" && pnpm licenses list --prod --json) |
  jq -r 'to_entries[] | .value[] | .name as $n | .license as $l | range(0; .versions | length) as $i |
         [$n, .versions[$i], $l, .paths[$i]] | @tsv' |
  while IFS=$'\t' read -r name version license path; do
    echo "npm    $name  $version  $license" >> "$list"
    [[ $(copy_licenses "$path" "$work/npm-desktop/${name//\//__}-$version") -gt 0 ]] ||
      echo "  (no license file shipped by the package; declared: $license)" >> "$list"
  done

rm -f "$out"
(cd "$work" && zip -qr -X "$out" .)
echo "Wrote $out ($(grep -c '^cargo' "$list") crates, $(grep -c '^npm' "$list") npm packages)"
