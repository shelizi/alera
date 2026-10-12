#!/usr/bin/env bash
set -euo pipefail

directory="${1:?release asset directory is required}"
desktop="${2:?desktop version is required}"
mobile="${3:?mobile version is required}"
expected=(
  "alera-$desktop-windows-x64.zip"
  "alera-$desktop-macos-arm64.tar.gz"
  "alera-$mobile-android.apk"
)
for asset in "${expected[@]}"; do
  test -s "$directory/$asset" || { echo "::error::Missing $asset"; exit 1; }
  test -s "$directory/$asset.sha256" || { echo "::error::Missing $asset.sha256"; exit 1; }
  (cd "$directory" && sha256sum -c "$asset.sha256")
done
count="$(find "$directory" -maxdepth 1 -type f | wc -l)"
[[ "$count" -eq 6 ]] || { echo "::error::Expected 6 release files, found $count."; exit 1; }
unzip -tq "$directory/${expected[0]}"
(cd "$directory" && tar -tzf "${expected[1]}" > /dev/null)
unzip -tq "$directory/${expected[2]}"
