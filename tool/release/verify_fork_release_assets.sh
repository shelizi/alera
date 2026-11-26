#!/usr/bin/env bash
set -euo pipefail

directory="${1:?release asset directory is required}"
desktop="${2:?desktop version is required}"
mobile="${3:?mobile version is required}"
mode="${4:-manual}"
expected=(
  "alera-$desktop-windows-x64.zip"
  "alera-$desktop-macos-arm64.tar.gz"
  "alera-$mobile-android.apk"
)
metadata=()
if [[ "$mode" == classic ]]; then
  expected+=("alera-$desktop-macos-arm64.zip")
  metadata=(alera-classic-windows-x64-update-index.json alera-classic-windows-x64-release.json
    alera-classic-macos-arm64-update-index.json alera-classic-macos-arm64-release.json)
elif [[ "$mode" != manual ]]; then
  echo "::error::Unknown fork release mode." >&2
  exit 1
fi
for asset in "${expected[@]}"; do
  test -s "$directory/$asset" || { echo "::error::Missing $asset"; exit 1; }
  test -s "$directory/$asset.sha256" || { echo "::error::Missing $asset.sha256"; exit 1; }
  (cd "$directory" && sha256sum -c "$asset.sha256")
done
for asset in "${metadata[@]}"; do
  test -s "$directory/$asset" || { echo "::error::Missing $asset"; exit 1; }
done
expected_count=$((2 * ${#expected[@]} + ${#metadata[@]}))
count="$(find "$directory" -maxdepth 1 -type f | wc -l)"
[[ "$count" -eq "$expected_count" ]] || { echo "::error::Expected $expected_count release files, found $count."; exit 1; }
unzip -tq "$directory/${expected[0]}"
(cd "$directory" && tar -tzf "${expected[1]}" > /dev/null)
unzip -tq "$directory/${expected[2]}"
if [[ "$mode" == classic ]]; then
  unzip -tq "$directory/alera-$desktop-macos-arm64.zip"
fi
