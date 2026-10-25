#!/usr/bin/env bash
# Checks OPS-001 to OPS-014 in docs/spec/operations.md through
# scripts/spec-tests.sh, with the tests in tests/spec_operations.rs. The tests
# run the front ends on scripted protocols and the WebSocket transport against
# a local server, so no request leaves the machine and no key is needed. A
# pass shows the front ends do what the AI SDK for TypeScript 7.0.148 core
# does.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_operations)
reqs=(OPS-001 OPS-002 OPS-003 OPS-004 OPS-005 OPS-006 OPS-007 OPS-008 OPS-009
  OPS-010 OPS-011 OPS-012 OPS-013 OPS-014)

run_spec_tests
