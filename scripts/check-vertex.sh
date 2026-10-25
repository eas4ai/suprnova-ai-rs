#!/usr/bin/env bash
# Checks VTX-001 to VTX-015 in docs/spec/vertex.md through
# scripts/spec-tests.sh, with the tests in tests/spec_vertex.rs. The tests
# record each request the crate sends and answer it with a reply written in
# the test, so no request leaves the machine and no key is needed. A pass
# shows the crate sends what the AI SDK for TypeScript 7.0.148 sends, not that
# Google still accepts it.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_vertex)
reqs=(VTX-001 VTX-002 VTX-003 VTX-004 VTX-005 VTX-006 VTX-007 VTX-008 VTX-009
  VTX-010 VTX-011 VTX-012 VTX-013 VTX-014 VTX-015)

run_spec_tests
