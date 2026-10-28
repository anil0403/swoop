#!/bin/sh
# Writes a release version into the project files (see VERSIONING.md).
# usage: .github/set-version.sh v1.2.3 | v1.2.3-beta.4
set -eu
v=${1#v}
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$' || { echo "bad version: $1" >&2; exit 1; }
cd "$(dirname "$0")/../core"
sed -i "0,/^version = /s/^version = .*/version = \"$v\"/" Cargo.toml
sed -i "/^name = \"uft\"\$/{n;s/^version = .*/version = \"$v\"/}" Cargo.lock
# Flutter app: X.Y.Z+BUILD, build = M×1e6 + m×1e4 + p×100 + (beta number, or 99 for a release).
IFS=.- read -r M m p _ n <<EOF
$v
EOF
sed -i "s/^version: .*/version: $v+$((M * 1000000 + m * 10000 + p * 100 + ${n:-99}))/" ../app/pubspec.yaml
