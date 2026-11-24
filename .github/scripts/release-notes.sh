#!/bin/sh
# Print the CHANGELOG.md section for a version tag (e.g. v0.1.0); fail when
# the changelog has no section for it.
set -eu

tag="${1:?usage: release-notes.sh <tag>}"
ver="${tag#v}" # strip the leading "v"

notes=$(awk -v v="$ver" '/^## \[/ { in_section = ($0 ~ "^## \\[" v "\\]") } in_section' CHANGELOG.md)
if [ -z "$notes" ]; then
    echo "release-notes.sh: no CHANGELOG.md section for $ver" >&2
    exit 1
fi
printf '%s\n\n' "$notes"
