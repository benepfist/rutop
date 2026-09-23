#!/bin/sh
# Builds the Linux and Windows binaries in Docker and writes them to ./dist
set -e
cd "$(dirname "$0")"
DOCKER_BUILDKIT=1 docker build -f docker/Dockerfile --target export --output type=local,dest=dist .
ls -l dist
