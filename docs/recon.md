# Recon: ai-rs

Read on 2026-10-22 at commit `7f64f85` (branch `main`, clean tree) for the
Sudus existing-project intake, Path A: no `docs/spec/` existed. There were no
earlier recon findings to carry.

Each row carries one status:

- **Exists**: seen in the code or the repository, or shown by a check that ran
  and passed during this recon.
- **Documented**: stated in the repository docs; not checked further here.
- **Contradicted**: the docs and the code disagree.
- **Unverified**: the repository does not show whether it holds.

"check run" below means `./scripts/check.sh` as run on 2026-10-22 during this
recon. It exited 0: format check, both builds, all three test runs (676 test
results passed, 0 failed, 0 ignored, across 77 test binaries and doc-test
runs), clippy with `-D warnings`, and rustdoc with `-D warnings`.

## Identity and manifests

| # | Claim | Status | Citation |
|---|---|---|---|
| R1 | One library crate, `ai` 0.1.0, edition 2024, MSRV 1.95.0, Apache-2.0. No binaries and no workspace members. | Exists | `Cargo.toml:1-8` |
| R2 | The crate is a Rust port of AI SDK for Python 0.8.0 (`vercel-labs/ai-python`). `NOTICE` records the port and the upstream copyright. | Exists | `README.md:3`, `NOTICE`, `docs/PORT.md:3-5` |
| R3 | The toolchain is pinned to 1.95 with clippy and rustfmt. | Exists | `rust-toolchain.toml` |
| R4 | The default feature set is empty, so there is no HTTP client. The `reqwest` feature adds `ReqwestTransport` and a download transport. | Exists | `Cargo.toml:10-12`, `src/transport/mod.rs:21-27`, `tests/transport_reqwest.rs:15` |
| R5 | `serde_json` always has `preserve_order` and `float_roundtrip` enabled. | Exists | `Cargo.toml:23` |
| R6 | The crate forbids `unsafe` code. | Exists | `src/lib.rs:16` |
| R7 | The crate description still says "Inference core". It now also ships providers, agents and the UI adapter. | Exists (stale wording) | `Cargo.toml:6`, `src/lib.rs:18-25` |
| R8 | Whether the crate is published, or meant to be published, to crates.io. `Cargo.toml` has no `publish` key. | Unverified | `Cargo.toml` |

## Public surface

| # | Claim | Status | Citation |
|---|---|---|---|
| S1 | Top-level modules: `agents`, `errors`, `models`, `providers`, `transport`, `types`, `ui`, `util`. | Exists | `src/lib.rs:18-25` |
| S2 | Root re-exports: `Agent`, `AgentStream`, `AgentTool`, `Context`, `HookRegistry`, `RunOptions`, `Error`, `Model`, `StreamHandle`, `StreamOptions`, `stream`, `generate`, `experimental_generate`, `get_model`, `probe`, the provider traits and registry, and the message builders. | Exists | `src/lib.rs:27-34` |
| S3 | `providers::openai` gives `OpenAICompatibleProvider` with Responses and Chat Completions protocols. The OpenAI id selects Responses; xai, ollama and custom metadata select Chat Completions. | Contradicted since chat-hosts (2026-10-22): the provider chooses when it is built; `new` and metadata naming `@ai-sdk/openai` select Responses, metadata for a Chat Completions host gets that host's rules, and other metadata selects Chat Completions. Since responses-hosts (2026-10-22), metadata for xAI or Hugging Face, by package or id, gets that host's Responses rules first | `src/providers/openai/provider.rs` `from_modelsdev_provider`; `tests/spec_hosts.rs` `hst_002_*` |
| S4 | `providers::anthropic` gives `AnthropicCompatibleProvider` over the Messages API, with `register` for an explicit registry. | Exists | `src/providers/anthropic/mod.rs:16-56`; tests `tests/anthropic_*.rs` (34 tests, passed in check run) |
| S5 | `agents` runs the tool loop: stream, run function tools concurrently, append history, repeat until a final answer. It supports approval hooks, deferral, middleware and loop buffering. | Exists | `src/agents/README.md`, `src/agents/mod.rs:16-32`; tests `tests/agents_*.rs` (85 tests, passed in check run) |
| S6 | `ui::ai_sdk` converts Vercel AI SDK v6 UI requests to core messages, and core histories and events back to UI messages and SSE. | Exists | `README.md:12-14`, `src/ui/ai_sdk/mod.rs`; tests `tests/ui_*.rs` (67 tests, passed in check run) |

## Behavior claims in README and PORT

| # | Claim | Status | Citation |
|---|---|---|---|
| B1 | The library never reads process environment variables. | Exists | No `std::env`, `env::var` or `env!` in `src/`; `src/models/mod.rs:185-188` |
| B2 | Model ids use `provider:model` and split at the first colon. An id with no colon routes to `gateway`. | Contradicted since core-hardening (2026-10-22): an id with no colon is an error (MOD-001) | `src/models/mod.rs` `get_model`; `tests/spec_models.rs` `mod_001_ids_without_a_provider_are_rejected` |
| B3 | `gateway`, `ai-gateway` and `vercel` share one registry entry. | Contradicted since core-hardening (2026-10-22): each handle is its own name (MOD-002) | `src/providers/base.rs` `ProviderRegistry::register`; `tests/spec_models.rs` `mod_002_gateway_handles_are_separate_names` |
| B4 | The provider registry starts empty. An unregistered id returns `UnsupportedProvider`. Because the gateway is not ported, an id with no colon fails unless the application registers its own `ai-gateway` provider. | Exists for the empty registry and `UnsupportedProvider`; the gateway clause is contradicted since core-hardening (2026-10-22), see B2 | `README.md:9`, `src/providers/base.rs` `get_provider` |
| B5 | A stream that ends without `StreamEnd` returns a retryable incomplete-response error. | Exists | `docs/PORT.md` (Wire format); `tests/models_core.rs:387`, `tests/errors.rs:92` |
| B6 | Event and part enums serialize with Python's `kind` discriminator and round-trip the upstream fixture. | Exists | `docs/PORT.md` (Wire format); `tests/python_compat.rs:36-78`, `tests/fixtures/python-0.8.0.json` |
| B7 | Media downloads check public addresses, revalidate and pin every redirect, bound DNS, limit body size and scope cookies. | Exists | `docs/PORT.md` (Rust differences); `tests/models_files.rs:121-377`, `tests/transport_reqwest.rs:73-97` |
| B8 | `to_sse` appends `data: [DONE]` only after a successful end. A source error ends the stream with no finish and no done marker. | Exists | `src/ui/ai_sdk/outbound_stream.rs:666-669`; `tests/ui_outbound_stream.rs:178`, `tests/ui_transport.rs:164` |
| B9 | UI conversion matches outputs captured from the unmodified Python tests. | Exists | `tests/ui_python_compat.rs:76-173`, `tests/fixtures/ui-python-0.8.0.json` |
| B10 | Debug output of HTTP requests and provider configuration hides credentials. | Exists for `HttpRequest`; Documented for provider configuration | `src/transport/mod.rs:41-48`; `docs/PORT.md` (Rust differences) |
| B11 | The local JSON Schema validator supports a stated subset, bounds nesting at 128 levels and decimals at 8192 bytes, and does not claim full conformance. | Documented | `docs/PORT.md:70-80`; validator in `src/types/tools.rs` |
| B12 | Provider errors do not trigger automatic retries in the agent loop. | Contradicted since core-hardening (2026-10-22): the loop itself still does not retry, but every model call retries a retryable error before its first event (MOD-005) | `src/models/call.rs`; `tests/agents_agent.rs` `provider_error_is_returned_without_an_agent_level_retry`; `tests/spec_models.rs` `mod_005_*` |
| B13 | The default agent loop buffer is unbounded, as in Python. The runtime side queue is also unbounded. | Documented | `src/agents/README.md:73-77` |
| B14 | The test suite needs no live provider credentials. Tests use scripted transports. | Exists | `docs/PORT.md:180`; B1 (no environment reads); check run passed |

## Tests, checks and CI

| # | Claim | Status | Citation |
|---|---|---|---|
| T1 | 36 integration test files with 328 `#[test]`/`#[tokio::test]` functions, plus doc tests. No `#[cfg(test)]` unit tests in `src/`. | Exists | `tests/*.rs`; `rg '#\[cfg\(test\)\]' src` finds none |
| T2 | `scripts/check.sh` runs fmt, builds with no features and all features, tests both ways, clippy `-D warnings`, and rustdoc `-D warnings`. All passed. | Exists | `scripts/check.sh`; check run |
| T3 | `scripts/check.sh` hard-codes `/home/shawn/.rustup/...` and a `CARGO_TARGET_DIR` under `/home/shawn/workspace2/scratchpads`. It runs only on this workstation. | Exists | `scripts/check.sh:3-6` |
| T4 | The extra `--features serde_json/preserve_order` test run repeats a feature `Cargo.toml` already enables. | Exists | `scripts/check.sh:12`, `Cargo.toml:23` |
| T5 | There is no CI configuration. | Exists | no `.github/` or other CI files in the tree |
| T6 | The PORT test map covers the core, transport and UI tests only. It does not list the OpenAI, Anthropic or agent test files. | Exists | `docs/PORT.md` (Test map) |

## Fixtures and data

| # | Claim | Status | Citation |
|---|---|---|---|
| D1 | `tests/fixtures/python-0.8.0.json` comes from `scripts/generate_python_fixtures.py` run against the upstream source. | Documented | `docs/PORT.md` (Test map, last paragraphs) |
| D2 | `tests/fixtures/ui-python-0.8.0.json` comes from `scripts/generate_ui_fixtures.py`, which runs all upstream `tests/ui` tests. | Documented | `docs/PORT.md` (Test map, last paragraphs) |
| D3 | Where `tests/fixtures/openai-streams.json` and `tests/fixtures/body.gz` came from. No script or doc names their source. | Unverified | `tests/providers_openai.rs:104` |
| D4 | The upstream Python 0.8.0 source is checked out next to this repo at `/home/shawn/workspace2/ai-python-0.8.0`. | Exists | that directory |
| D5 | The crate stores no persistent state: no database, files or caches. | Exists | no `std::fs`, `tokio::fs` or file creation in `src/` |

## Gaps against upstream Python 0.8.0

| # | Claim | Status | Citation |
|---|---|---|---|
| G1 | Not ported, by decision: Vercel AI Gateway protocol and catalog, OIDC, telemetry spans, the MCP client, durable workflow integrations, and environment-loading factories. | Documented | `README.md:10`, `docs/PORT.md:200-205` |
| G2 | Upstream `ai.ops` has front ends for images, video, audio, embeddings, reranking and transcription. Rust has only the `Provider` trait methods (default `UnsupportedOperation`) and payload types. | Exists | upstream `src/ai/ops/__init__.py`; `src/providers/base.rs:154-213`, `src/providers/operations.rs` |
| G3 | Upstream `ai.ops.experimental.evaluation` has no Rust counterpart, and the docs do not mention it. | Exists | upstream `src/ai/ops/experimental/evaluation.py` |
| G4 | Upstream `ai.providers.history_utils` is public (`repair`, `validate`, `inspect`). In Rust, each adapter has its own private `repair`. Applications cannot call a shared validate or repair. The docs do not mention this. | Exists | upstream `src/ai/providers/history_utils.py:63-265`; `src/providers/openai/messages.rs:132`, `src/providers/anthropic/messages.rs:141` |

## Doc drift

| # | Claim | Status | Citation |
|---|---|---|---|
| C1 | PORT says tool execution and approval "belong to the later agent module". That module now exists. | Contradicted | `docs/PORT.md:67-68` vs `src/agents/` |
| C2 | PORT says Python queues, task groups and exception-group teardown "belong to the later agent runtime". That runtime now exists. | Contradicted | `docs/PORT.md:117-118` vs `src/agents/runtime.rs` |
| C3 | PORT says adapter HTTP paths and vendor errors "are deferred with the adapters". Both adapters now exist with tests. | Contradicted | `docs/PORT.md:178-180` vs `src/providers/openai/errors.rs`, `src/providers/anthropic/errors.rs` |

## Recent history

| # | Claim | Status | Citation |
|---|---|---|---|
| H1 | The repository has 11 commits, all dated 2026-10-21, by one author. Order: core port, Anthropic, OpenAI, UI, agents, then README and PORT updates. | Exists | `git log` |
| H2 | Each later module was built on its own branch and merged (`port-ui`, `port-agents`, the OpenAI port). | Exists | merge commits `3342a3c`, `3e5ca08`, `1a47c93` |

## Local workspace

| # | Claim | Status | Citation |
|---|---|---|---|
| W1 | `.claude/`, `.agents/` and `.remember/` exist but are ignored by Git. `.claude/skills` and `.agents/skills` hold the same five Rust skills. | Exists | `git status --ignored`; `diff -rq .claude/skills .agents/skills` |
| W2 | `.sudus/settings.json` was created by `sudus init` during this intake (authority remote `origin`). It is not committed yet. | Exists | `.sudus/settings.json` |

## Blast radius: rename to suprnova-ai

Traced on 2026-10-22, after the local folder and the GitHub repository were
renamed, for the first commitment (`rename-to-suprnova-ai`, spec in
`docs/spec/package.md`).

| # | Claim | Status | Citation |
|---|---|---|---|
| X1 | 225 lines in 42 files hold an `ai::` crate path: 38 files under `tests/` (36 test files and 2 support modules), `README.md` (10), `src/agents/README.md` (2), and the module docs of `src/providers/openai/mod.rs` and `src/providers/anthropic/mod.rs` (3 each). Both READMEs compile as doc tests through `include_str!`. | Exists | `rg -c '\bai::'`; `src/lib.rs:15`, `src/agents/mod.rs:15` |
| X2 | The old name appears in `Cargo.toml:8` (repository URL), `NOTICE:1` and `scripts/check.sh:5` (`CARGO_TARGET_DIR`). `README.md:1` titles the crate `# ai`. `docs/PORT.md:11` maps `ai.__init__` to the Rust crate root `ai`. | Exists | those lines; `scripts/check-identity.sh` run on this tree reports PKG-001 to PKG-004 as fail |
| X3 | Python names must stay: the left column of the `docs/PORT.md` module map (`ai.types`, ...) and the imports of the Python package `ai` in the fixture scripts. | Exists | `docs/PORT.md:11-25`; `scripts/generate_ui_fixtures.py:33-36`, `scripts/generate_python_fixtures.py:14-22` |
| X4 | suprchat depends on this crate as `ai = { git = "https://github.com/eas4ai/ai-rs.git", rev = "1a47c93..." }`. That revision predates the rename and GitHub redirects the old URL, so suprchat keeps building as it is. Moving its pin past the rename needs `package = "suprnova-ai"` (or rewriting its `ai::` paths) and the new URL. This commitment does not change suprchat. | Exists (pin); Unverified (no suprchat build ran) | the `ai =` line of `/home/shawn/workspace2/suprchat/Cargo.toml` (line 56 on 2026-10-22) |
| X5 | The local folder is `/home/shawn/workspace2/suprnova-ai-rs`. `origin` is `https://github.com/eas4ai/suprnova-ai-rs.git`, and `git ls-remote origin HEAD` answered `7f64f85`. | Exists | `git remote -v`; `scripts/check-remote.sh` run on 2026-10-22 |
| X6 | No public API changes apart from the crate name. No module, type or function is renamed. | Exists (plan) | `docs/spec/roadmap.md` |
