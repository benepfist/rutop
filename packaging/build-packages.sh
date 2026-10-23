#!/bin/sh
# Packages dist/rutop-linux-x86_64 as .deb and .rpm (into dist/) with nfpm in Docker.
# Run ./build.sh first. Optional: RPM_SIGNING_KEY_FILE=<armored private key, no passphrase> to sign the .rpm.
set -e
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
test -f dist/rutop-linux-x86_64 || { echo "dist/rutop-linux-x86_64 missing, run ./build.sh first" >&2; exit 1; }

key_mount=
key_env=
if [ -n "$RPM_SIGNING_KEY_FILE" ]; then
  # pwd -W: Windows path for Docker Desktop under Git Bash
  key_dir=$(cd "$(dirname "$RPM_SIGNING_KEY_FILE")" && { pwd -W 2>/dev/null || pwd; })
  key_mount="-v $key_dir/$(basename "$RPM_SIGNING_KEY_FILE"):/signing-key.asc:ro"
  key_env="-e RPM_SIGNING_KEY_FILE=/signing-key.asc"
fi

for packager in deb rpm; do
  # MSYS_NO_PATHCONV: keep Git Bash on Windows from rewriting the container paths
  # shellcheck disable=SC2086
  MSYS_NO_PATHCONV=1 docker run --rm -v "$(pwd):/src" -w /src -e VERSION="$VERSION" $key_mount $key_env \
    goreleaser/nfpm:v2.43.0 package --config packaging/nfpm.yaml --packager "$packager" --target dist/
done
ls -l dist
