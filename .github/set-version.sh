#!/bin/sh
# Writes a release version into the project files (see VERSIONING.md).
# usage: .github/set-version.sh v1.2.3 | v1.2.3-beta.4
# ponytail: add the Flutter pubspec.yaml `version: X.Y.Z+BUILD` when the app exists.
set -eu
v=${1#v}
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$' || { echo "bad version: $1" >&2; exit 1; }
cd "$(dirname "$0")/../core"
sed -i "0,/^version = /s/^version = .*/version = \"$v\"/" Cargo.toml
sed -i "/^name = \"uft\"\$/{n;s/^version = .*/version = \"$v\"/}" Cargo.lock
