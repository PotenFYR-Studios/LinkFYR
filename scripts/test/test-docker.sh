#!/usr/bin/env bash
# Run the complete LinkFYR verification suite in Docker (no host
# toolchain, no tests on your machine). Requires a Docker engine.
set -euo pipefail
cd "$(dirname "$0")/../.."

echo "== LinkFYR Docker verification =="
fail=0

docker compose -f docker/compose.test.yml build rust && \
    docker compose -f docker/compose.test.yml run --rm rust || fail=1

if [ "$fail" -eq 0 ]; then
    docker compose -f docker/compose.test.yml build frontend && \
        docker compose -f docker/compose.test.yml run --rm frontend || fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "DOCKER VERIFICATION FAILED"
    exit 1
fi
echo "DOCKER VERIFICATION PASSED"
