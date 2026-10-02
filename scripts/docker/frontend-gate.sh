#!/usr/bin/env bash
# Full frontend gate, run inside docker/frontend.Dockerfile.
set -euo pipefail
cd /app

echo "== pnpm install =="
pnpm install --frozen-lockfile --prefer-offline

echo "== typecheck =="
pnpm typecheck

echo "== lint =="
pnpm lint

echo "== test =="
pnpm test

echo "== build =="
pnpm build

echo "== mobile: typecheck + build =="
pnpm --filter linkfyr-mobile-ui typecheck
pnpm --filter linkfyr-mobile-ui build

echo "FRONTEND GATE PASSED"
