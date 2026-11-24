#!/bin/sh
# Prepare a release from develop (run it through `make release <version>`):
# bump Cargo.toml, Cargo.lock and CHANGELOG.md, run `make check`, commit on
# develop, merge develop into main with --no-ff and tag v<version> on main.
# Nothing is pushed: the push command is printed at the end.
#
# RELEASE_CHECK overrides the check command (the tests use `true`/`false`).
set -eu

die() {
    echo "release.sh: $*" >&2
    exit 1
}

new="${1:-}"
[ -n "$new" ] || die "usage: make release <version>"
check="${RELEASE_CHECK:-make check}"
tag="v$new"

echo "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ||
    die "version '$new' must be MAJOR.MINOR.PATCH, in 0.2.0 style"

name=$(sed -n 's/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
cur=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
highest=$(printf '%s\n%s\n' "$cur" "$new" | sort -V | tail -n 1)
if [ "$new" = "$cur" ] || [ "$highest" != "$new" ]; then
    die "version $new is not greater than $cur"
fi

# Repository state: on a clean develop, both branches in sync with origin,
# and main fully contained in develop.
[ "$(git branch --show-current)" = develop ] || die "run it on develop"
[ -z "$(git status --porcelain)" ] || die "the working tree has uncommitted changes"
git fetch --quiet --tags origin
for branch in develop main; do
    [ "$(git rev-parse "$branch")" = "$(git rev-parse "origin/$branch")" ] ||
        die "$branch differs from origin/$branch: pull or push it first"
done
git merge-base --is-ancestor main develop ||
    die "main has commits that are not in develop: merge main into develop first"
if git rev-parse --quiet --verify "refs/tags/$tag" >/dev/null; then
    die "tag $tag already exists"
fi

# Version and changelog.
unreleased=$(awk '/^## \[/ { in_section = ($0 ~ /^## \[Unreleased\]/); next } in_section && NF' CHANGELOG.md)
[ -n "$unreleased" ] || die "[Unreleased] in CHANGELOG.md is empty: nothing to release"

restore() {
    git checkout -- Cargo.toml Cargo.lock CHANGELOG.md
}

# First `version =` of Cargo.toml is the package version.
awk -v v="$new" '!done && /^version *= *"/ { sub(/"[^"]*"/, "\"" v "\""); done = 1 } 1' \
    Cargo.toml >Cargo.toml.new && mv Cargo.toml.new Cargo.toml
# In Cargo.lock, the version line right after the package's name line.
awk -v n="$name" -v v="$new" '
    found && /^version = "/ { sub(/"[^"]*"/, "\"" v "\""); found = 0 }
    { found = found || ($0 == "name = \"" n "\"") }
    { print }' Cargo.lock >Cargo.lock.new && mv Cargo.lock.new Cargo.lock
date=$(date +%F)
awk -v v="$new" -v d="$date" '
    /^## \[Unreleased\]/ && !done { print; print ""; print "## [" v "] - " d; done = 1; next } 1' \
    CHANGELOG.md >CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md

if ! sh -c "$check"; then
    restore
    die "make check failed: nothing was committed"
fi

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit --quiet -m "chore(release): $new"
git switch --quiet main
git merge --quiet --no-ff develop -m "Release $new"
git tag -a "$tag" -m "$name $new"
git switch --quiet develop

echo "Release $tag is ready on main. Publish it (this starts the deploy) with:"
echo "  git push --atomic origin develop main $tag"
