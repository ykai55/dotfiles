#!/usr/bin/env bash
set -euo pipefail

docker build -f Dockerfile.linux-tests -t clip-linux-tests .
docker run --rm clip-linux-tests
