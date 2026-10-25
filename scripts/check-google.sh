#!/usr/bin/env bash
# Checks GGL-001 to GGL-032 in docs/spec/google.md through
# scripts/spec-tests.sh, with the tests in tests/spec_google.rs. The tests
# record each request the crate sends and answer it with a reply written in
# the test, so no request leaves the machine and no key is needed. A pass
# shows the crate sends what the AI SDK for TypeScript 7.0.148 sends, not that
# Google still accepts it.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_google)
reqs=(GGL-001 GGL-002 GGL-003 GGL-004 GGL-005 GGL-006 GGL-007 GGL-008 GGL-009
  GGL-010 GGL-011 GGL-012 GGL-013 GGL-014 GGL-015 GGL-016 GGL-017 GGL-018
  GGL-019 GGL-020 GGL-021 GGL-022 GGL-023 GGL-024 GGL-025 GGL-026 GGL-027
  GGL-028 GGL-029 GGL-030 GGL-031 GGL-032)

run_spec_tests
