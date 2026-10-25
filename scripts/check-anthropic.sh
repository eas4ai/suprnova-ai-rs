#!/usr/bin/env bash
# Checks ANT-001 to ANT-020 in docs/spec/anthropic.md through
# scripts/spec-tests.sh, with the tests in tests/spec_anthropic.rs. The tests
# record each request the crate sends and answer it with a reply written in
# the test, so no request leaves the machine and no key is needed. A pass
# shows the crate sends what the AI SDK for TypeScript 7.0.148 sends, not
# that Anthropic still accepts it.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_anthropic)
reqs=(ANT-001 ANT-002 ANT-003 ANT-004 ANT-005 ANT-006 ANT-007 ANT-008 ANT-009
  ANT-010 ANT-011 ANT-012 ANT-013 ANT-014 ANT-015 ANT-016 ANT-017 ANT-018
  ANT-019 ANT-020)

run_spec_tests
