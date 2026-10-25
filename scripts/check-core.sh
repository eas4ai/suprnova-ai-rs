#!/usr/bin/env bash
# Checks MOD-001 to MOD-009 in docs/spec/models.md and AGT-001 to AGT-006 in
# docs/spec/agents.md through scripts/spec-tests.sh, with the tests in
# tests/spec_models.rs and tests/spec_agents.rs. MOD-002 also needs the
# gateway names gone from the sources and the README.
set -uo pipefail
cd "$(dirname "$0")/.."
. scripts/spec-tests.sh

tests=(spec_models spec_agents)
reqs=(MOD-001 MOD-002 MOD-003 MOD-004 MOD-005 MOD-006 MOD-007 MOD-008 MOD-009
  AGT-001 AGT-002 AGT-003 AGT-004 AGT-005 AGT-006)

# MOD-002 names that only the Vercel AI Gateway routing used.
gateway_names='canonical_provider_id|RoutingParams|RoutingTarget|ProviderRankingStrategy|GeoRegion|CloudRegion|routing_target'
gateway_free() {
  local params
  if found -rnE "$gateway_names" src tests README.md; then return 1; fi
  if found -n '`gateway`' README.md; then return 1; fi
  params=$(awk '/^pub struct InferenceRequestParams /,/^}/' src/models/params.rs)
  if [ -z "$params" ]; then
    echo "src/models/params.rs: InferenceRequestParams not found"
    return 1
  fi
  if printf '%s\n' "$params" | grep -nE '^\s*pub (routing|tags):'; then return 1; fi
  return 0
}

extra_check() {
  if [ "$1" = MOD-002 ]; then gateway_free; fi
}

run_spec_tests
