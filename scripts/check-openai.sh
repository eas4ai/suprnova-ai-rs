#!/usr/bin/env bash
# Checks OAI-001 to OAI-027 in docs/spec/openai.md through
# scripts/spec-tests.sh, with the tests in tests/spec_openai.rs. The tests
# build or record each request the crate sends and answer it with a reply
# written in the test, so no request leaves the machine and no key is needed.
# A pass shows the crate sends what the AI SDK for TypeScript 7.0.148 sends,
# not that OpenAI still accepts it.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_openai)
reqs=(OAI-001 OAI-002 OAI-003 OAI-004 OAI-005 OAI-006 OAI-007 OAI-008 OAI-009
  OAI-010 OAI-011 OAI-012 OAI-013 OAI-014 OAI-015 OAI-016 OAI-017 OAI-018
  OAI-019 OAI-020 OAI-021 OAI-022 OAI-023 OAI-024 OAI-025 OAI-026 OAI-027)

run_spec_tests
