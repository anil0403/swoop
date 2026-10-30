#!/usr/bin/env bash
# Prints the next tag for HEAD from the commit types, or nothing when there is nothing to release (see VERSIONING.md).
# usage: .github/next-version.sh push|nightly|manual [auto|major|minor|patch]
set -euo pipefail
event=$1 bump=${2:-auto}

tag() { git describe --tags --abbrev=0 --match 'v[0-9]*' "$@" 2> /dev/null || true; }

# A release commit is pushed by a workflow, which starts no CI run: until the next push, the commit CI last tested
# is the one before it, already released.
released=$(git tag --contains HEAD --list 'v[0-9]*' --sort=v:refname | grep -v -- - | head -n1 || true)
if [[ -n $released ]]; then
    echo "already released in $released" >&2
    exit 0
fi
last=$(tag --exclude '*-*')
newest=$(tag)

if [[ -z $last && $bump == auto ]]; then
    echo "no release yet: tag v1.0.0 by hand, or run by hand with bump: major" >&2
    exit 0
fi

# Subjects since $1 that count: not docs/ci/test, not a release commit, not a commit with only doc files.
counted() {
    local sha subject
    git log --format='%H %s' "${1:+$1..}HEAD" | while read -r sha subject; do
        [[ $subject =~ ^(docs|ci|test)(\(.*\))?!?: || $subject == "chore: release v"* ]] && continue
        git diff-tree --no-commit-id --name-only -r --root "$sha" |
            grep -qvE '^(docs/|bench/|\.github/|LICENSE$)|\.md$' || continue
        echo "$subject"
    done
}

if [[ $bump == auto ]]; then
    subjects=$(counted "$last")
    if [[ -z $subjects ]]; then
        echo "nothing to release since $last" >&2
        exit 0
    fi
    if [[ $event == push && -z $(counted "$newest") ]]; then
        echo "nothing new since $newest" >&2
        exit 0
    fi
    if grep -qE '^[a-z]+(\(.*\))?!:|BREAKING' <<< "$subjects"; then
        bump=major
    elif grep -qE '^feat(\(.*\))?:' <<< "$subjects"; then
        bump=minor
    elif [[ $event == push ]]; then
        bump=beta
    else
        bump=patch
    fi
fi

IFS=. read -r M m p <<< "${last#v}"
M=${M:-0} m=${m:-0} p=${p:-0}
case $bump in
    major) v=$((M + 1)).0.0 ;;
    minor) v=$M.$((m + 1)).0 ;;
    patch | beta) v=$M.$m.$((p + 1)) ;;
    *) echo "bad bump: $bump" >&2; exit 1 ;;
esac
n=99
if [[ $bump == beta ]]; then
    n=$(git tag -l "v$v-beta.*" | sed 's/.*-beta\.//' | sort -n | tail -n1)
    n=$((${n:-0} + 1))
    v=$v-beta.$n
fi

# Android build number: MAJOR×1,000,000 + MINOR×10,000 + PATCH×100 + N. It keeps SemVer order, so it also
# checks that versions only go up.
build() {
    local x=${1#v} N=99 a b c
    [[ $x == *-beta.* ]] && N=${x##*.}
    IFS=. read -r a b c <<< "${x%%-*}"
    echo $((a * 1000000 + b * 10000 + c * 100 + N))
}
IFS=. read -r _ m p <<< "${v%%-*}"
if [[ $bump == beta ]] && ((n > 98)) || ((m > 99 || p > 99)); then
    echo "v$v has no build number (at most 98 betas, MINOR and PATCH below 100)" >&2
    exit 1
fi
if [[ -n $newest ]] && (($(build "v$v") <= $(build "$newest"))); then
    echo "v$v is not above $newest" >&2
    exit 1
fi
echo "v$v"
