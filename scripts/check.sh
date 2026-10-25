#!/usr/bin/env bash
set -euo pipefail
T=/home/shawn/.rustup/toolchains/1.95-x86_64-unknown-linux-gnu/bin
export PATH="$T:$PATH"
export CARGO_TARGET_DIR=/home/shawn/workspace2/scratchpads/targets/suprnova-ai-rs/w-core
export RUSTC="$T/rustc" RUSTDOC="$T/rustdoc" RUSTUP_AUTO_INSTALL=0
"$T/cargo" fmt --all -- --check
"$T/cargo" build --locked --no-default-features
"$T/cargo" build --locked --all-features
"$T/cargo" test --locked --no-default-features
"$T/cargo" test --locked --all-features
"$T/cargo" test --locked --all-features --features serde_json/preserve_order --test models_core
"$T/cargo" clippy --locked --all-targets --no-default-features -- -D warnings
"$T/cargo" clippy --locked --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" "$T/cargo" doc --locked --all-features --no-deps
