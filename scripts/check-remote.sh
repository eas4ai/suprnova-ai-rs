#!/usr/bin/env bash
# Checks PKG-005 in docs/spec/package.md: origin is the renamed repository,
# and GitHub answers there. Needs the network.
set -euo pipefail
cd "$(dirname "$0")/.."
want=https://github.com/eas4ai/suprnova-ai-rs.git
got=$(git remote get-url origin)
if [ "$got" != "$want" ]; then
  echo "origin is $got, expected $want"
  exit 1
fi
GIT_TERMINAL_PROMPT=0 timeout 60 git ls-remote --exit-code origin HEAD
