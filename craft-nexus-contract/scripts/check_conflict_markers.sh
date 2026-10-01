#!/bin/bash
# Committed conflict-marker guard.
#
# Fails closed when git conflict markers are committed in tracked source.
# A merge can succeed and still leave conflict markers behind when the markers
# were committed on the source branch before the merge, so compilation alone
# is not sufficient evidence of a clean tree.
#
# Usage:
#   ./scripts/check_conflict_markers.sh [repo_root]
#
# Optional environment:
#   ALLOW_CONFLICT_MARKER_FILES  Space-separated glob patterns to exempt
#   MARKER_ALLOW_COMMENT        Opt-out comment honoured inside a file
#                                (default: contract-safety-gate: allow-conflict-markers)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="${1:-$(cd "${SCRIPT_DIR}/../.." && pwd)}"
cd "${ROOT}"

MARKER_ALLOW_COMMENT="${MARKER_ALLOW_COMMENT:-contract-safety-gate: allow-conflict-markers}"
ALLOW_CONFLICT_MARKER_FILES="${ALLOW_CONFLICT_MARKER_FILES:-}"

EXTENSIONS="rs toml yml yaml sh bash json sol md ts js tsx jsx py lock proto"

build_file_list() {
  local ext
  for ext in ${EXTENSIONS}; do
    git ls-files --cached --others --exclude-standard "*${ext}" 2>/dev/null || true
  done | LC_ALL=C sort -u
}

is_exempt() {
  local file="$1" pattern
  [ -n "${ALLOW_CONFLICT_MARKER_FILES}" ] || return 1
  for pattern in ${ALLOW_CONFLICT_MARKER_FILES}; do
    # shellcheck disable=SC2053
    [[ "${file}" == ${pattern} ]] && return 0
  done
  return 1
}

violations=0
scanned=0

while IFS= read -r file; do
  [ -f "${file}" ] || continue
  grep -Iq . "${file}" 2>/dev/null || continue
  is_exempt "${file}" && continue
  grep -qF "${MARKER_ALLOW_COMMENT}" "${file}" 2>/dev/null && continue
  scanned=$((scanned + 1))

  # A conflict start marker is anchored at column 0. Bare '=======' is
  # deliberately ignored because it is a valid Markdown setext heading.
  # An orphan end marker is reported too, so a partially resolved file with
  # only a trailing '>>>>>>>' is still caught.
  if hits=$(grep -nE '^(<{7}|>{7})( |$)' "${file}" 2>/dev/null); then
    violations=$((violations + 1))
    printf 'FAIL %s\n' "${file}" >&2
    printf '%s\n' "${hits}" | sed 's/^/       /' >&2
  fi
done < <(build_file_list)

if [ "${violations}" -gt 0 ]; then
  cat >&2 <<EOF

Committed conflict markers detected in ${violations} file(s) (${scanned} scanned).

Markers can reach main without Git reporting a conflict when they were already
committed on the source branch. Fix each file by completing the merge, then
re-run this check. To keep an intentional fixture, add the opt-out comment
"${MARKER_ALLOW_COMMENT}" on its own line, or set
ALLOW_CONFLICT_MARKER_FILES.
EOF
  exit 1
fi

printf 'conflict-marker guard: PASS (%d files scanned)\n' "${scanned}"
