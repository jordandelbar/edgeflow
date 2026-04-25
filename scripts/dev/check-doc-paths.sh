#!/usr/bin/env bash
# Verify every `examples/...` and `deploy/...` path mentioned in README
# and docs/ actually exists. Catches stale paths after renames/moves.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

mapfile -t files < <(
  find docs -name "_build" -prune -o -type f \( -name "*.rst" -o -name "*.md" \) -print 2>/dev/null
  echo README.md
)

# Two sources of valid paths:
#  1. Standalone mentions like `deploy/quickstart.yaml`. Negative lookbehind
#     keeps `targets/.../deploy/ack` (MQTT topic) out.
#  2. GitHub raw URLs - the lookbehind would reject these too, so match the
#     suffix after `/<ref>/` explicitly with \K.
paths=$( {
  grep -hoP '(?<![/\w-])(examples|deploy)/[a-zA-Z0-9_./-]+' "${files[@]}"
  grep -hoP 'raw\.githubusercontent\.com/[^/]+/[^/]+/[^/]+/\K(examples|deploy)/[a-zA-Z0-9_./-]+' "${files[@]}"
} 2>/dev/null | sed 's/[.,;:)`]*$//' | sort -u | grep -v '^$' || true)

missing=0
while IFS= read -r path; do
  [ -z "$path" ] && continue
  if [ ! -e "$path" ]; then
    echo "missing: $path"
    missing=1
  fi
done <<< "$paths"

if [ $missing -ne 0 ]; then
  echo
  echo "Some doc paths do not exist. Update the docs or restore the files."
  exit 1
fi

count=$(printf '%s\n' "$paths" | grep -c . || true)
echo "ok: all $count referenced doc paths exist"
