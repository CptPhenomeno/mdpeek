#!/bin/sh
# Fail unless a version tag (e.g. v0.1.0) matches the package version in
# Cargo.toml, so release assets and `mdpeek --version` agree with the tag.
set -eu

tag="${1:?usage: check-version.sh <tag>}"
ver=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)

if [ "${tag#v}" != "$ver" ]; then
    echo "check-version.sh: tag $tag does not match Cargo.toml version $ver" >&2
    exit 1
fi
