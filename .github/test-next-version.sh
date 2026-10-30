#!/usr/bin/env bash
# Checks next-version.sh against the rules in VERSIONING.md, in a scratch repository.
set -euo pipefail
next=$(cd "$(dirname "$0")" && pwd)/next-version.sh
cd "$(mktemp -d)"
git init -q
git config user.name t
git config user.email t@t
c() { echo "$1" >> "${2:-code}"; git add -A; git commit -qm "$1"; }
expect() {
    local got
    got=$("$next" "$1" "${2:-auto}" 2> /dev/null) || got=error
    [[ $got == "$3" ]] || { echo "FAIL: $1 ${2:-auto} after '$(git log -1 --format=%s)': got '$got', want '$3'" >&2; exit 1; }
    [[ -z $got || $got == error ]] || git tag "$got"
}

c "fix: first"
expect push auto ""                 # no tag yet: first release is by hand
expect nightly auto ""
expect manual major v1.0.0
c "docs: readme"
expect push auto ""                 # docs count for nothing
c "fix: typo in readme" README.md
expect push auto ""                 # only doc files
c "fix(core): stall"
expect push auto v1.0.1-beta.1
c "perf: faster"
expect push auto v1.0.1-beta.2
c "test: more" tests
expect push auto ""                 # nothing new since the beta
expect nightly auto v1.0.1          # the daily release of the betas
expect nightly auto ""              # nothing since that release
c "chore: release v9.9.9"
expect nightly auto ""              # release commits don't count
c "refactor: tidy"
c "feat(cli): new flag"
expect push auto v1.1.0             # feat releases right away
c "fix: x"
expect push auto v1.1.1-beta.1
c "feat!: drop old protocol"
expect push auto v2.0.0             # breaking, with the beta before it
c "fix: y"
expect manual patch v2.0.1
c "fix: z"
expect manual minor v2.1.0
c "fix: v"
c "chore: release v2.1.1"
git tag v2.1.1
git checkout -q HEAD~1              # the release commit, pushed by a workflow, starts no CI run
expect nightly auto ""              # so CI last tested the commit before it, already released
git tag v2.1.1-beta.98
c "fix: w"
expect push auto error              # a 99th beta has no build number
git tag v2.2.0-beta.1
expect manual patch error           # v2.1.1 is not above v2.2.0-beta.1
echo "next-version: all checks passed"
