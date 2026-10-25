Prefix: PKG

# Package identity

The names the crate and its repository carry, the credit to the upstream
project, and the check that the crate still builds and passes its tests.
The upstream Python names, such as the package `ai` and the module
`ai.types`, are not part of this identity and keep their spelling.

[PKG-001] `Cargo.toml` MUST name the package `suprnova-ai`, so that its
library target is `suprnova_ai`.
Falsifier: `cargo metadata` for this repository reports a root package named other than `suprnova-ai`, or a library target named other than `suprnova_ai`.
Mechanism: identity
Status: Agreed 2026-10-22

[PKG-002] Every file under `src/`, `tests/` and `scripts/`, and the files
`Cargo.toml`, `NOTICE`, `README.md` and `docs/PORT.md`, MUST name the Rust
crate and the repository only by their new names.
Falsifier: One of those files holds an `ai::` crate path or the old repository name `ai-rs` (outside `suprnova-ai-rs`); or the first line of `README.md` is not `# suprnova-ai`; or the Rust module column of the `docs/PORT.md` module map names the crate root `ai`.
Mechanism: identity
Rationale: Python names in `docs/PORT.md` and in the fixture scripts, such as `ai.types` and `ai-python`, name the upstream and stay; the identity script scans every file except itself, because it spells the old names to look for them.
Status: Agreed 2026-10-22

[PKG-003] `Cargo.toml` MUST set `repository` to
`https://github.com/eas4ai/suprnova-ai-rs`.
Falsifier: The `repository` value in `Cargo.toml` is any other string, such as the old URL `https://github.com/eas4ai/ai-rs`.
Mechanism: identity
Status: Agreed 2026-10-22

[PKG-004] `NOTICE` MUST start with the line `suprnova-ai-rs` and the line
`Copyright 2026 Shawn McAllister <@eas4ai>`, MUST state that the crate began
as a Rust port of the AI SDK for Python 0.8.0 and has changed from it in the
ways `docs/PORT.md` lists, and MUST keep the upstream source URLs, the Vercel
copyrights of the AI SDK for Python 0.8.0 and the AI SDK for TypeScript
7.0.148, and the Apache License 2.0.
Falsifier: The first two lines of `NOTICE` are not `suprnova-ai-rs` and `Copyright 2026 Shawn McAllister <@eas4ai>`; `NOTICE` still names the port `ai-rs`, or lacks any of `AI SDK for Python 0.8.0`, `https://github.com/vercel-labs/ai-python`, `Copyright 2026 Vercel, Inc.`, `AI SDK for TypeScript 7.0.148`, `https://github.com/vercel/ai`, `Copyright 2023 Vercel, Inc.`, `docs/PORT.md` or `Apache License, Version 2.0`.
Mechanism: identity
Rationale: The crate has departed from the port in many ways, and its own author holds the copyright of those changes; Shawn, 2026-10-23.
Status: Agreed 2026-10-23

[PKG-005] The origin remote of the development clone MUST be
`https://github.com/eas4ai/suprnova-ai-rs.git`, and GitHub MUST answer at
that URL.
Falsifier: `git remote get-url origin` prints any other URL, such as the old `https://github.com/eas4ai/ai-rs.git`, or `git ls-remote --exit-code origin HEAD` fails.
Mechanism: remote
Rationale: The remote URL lives in `.git/config`, which a mechanism cannot declare as an input, so changing it does not make a receipt stale; the check also needs the network.
Status: Agreed 2026-10-22

[PKG-006] `./scripts/check.sh` MUST exit 0: the format check, both builds,
all test runs, clippy and rustdoc pass with warnings denied.
Falsifier: `./scripts/check.sh` exits nonzero.
Mechanism: checks
Rationale: The script passed on 2026-10-22 at the last commit before the rename (676 test results, 0 failed); the rename must keep it passing, and it runs with `--locked`, so a stale `Cargo.lock` also fails it.
Status: Agreed 2026-10-22

[PKG-007] `LICENSE` MUST hold the full text of the Apache License, Version
2.0, and the files under `src/`, `tests/` and `scripts/` MUST NOT carry a
copyright or license header: the attribution is `NOTICE`'s. `Cargo.toml`
MUST list `Shawn McAllister <@eas4ai>` in `authors`, and the first paragraph
of `README.md` after its title MUST say that the crate began as a port of the
AI SDK for Python and follows the AI SDK for TypeScript, and has changed from
both, and that it is licensed under the Apache License, Version 2.0.
Falsifier: The `sha256sum` of `LICENSE` is not `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`, the hash of the text at `https://www.apache.org/licenses/LICENSE-2.0.txt`; a file under `src/`, `tests/` or `scripts/`, other than `scripts/check-identity.sh`, holds `Licensed under the Apache License`, an `SPDX-License-Identifier` or `SPDX-FileCopyrightText` line, or a line on which `Copyright` comes after one or more comment marks and nothing else but spaces; the `authors` of `Cargo.toml` lack `Shawn McAllister <@eas4ai>`; or the first paragraph of `README.md` after its title lacks `AI SDK for Python`, `AI SDK for TypeScript` or `Apache License, Version 2.0`.
Mechanism: identity
Rationale: Neither upstream repository puts a header in its source files; both keep the license and notice at the root, and Shawn asked for the same (2026-10-23).
Status: Agreed 2026-10-24
