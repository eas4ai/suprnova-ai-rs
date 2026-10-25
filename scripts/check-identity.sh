#!/usr/bin/env bash
# Checks PKG-001 to PKG-004 and PKG-007 in docs/spec/package.md and prints
# one `sudus: PKG-nnn: pass|fail` line for each. A requirement whose check
# cannot run (no cargo or jq) gets no line, so Sudus records it unverified.
set -uo pipefail
cd "$(dirname "$0")/.."

failed=0
report() {
  if [ "$2" -eq 0 ]; then echo "sudus: $1: pass"; else echo "sudus: $1: fail"; failed=1; fi
}
# Old crate paths, matched so that `ai_sdk::` and `openai::` do not count.
old_path='\bai::'
files=(src tests scripts Cargo.toml NOTICE README.md docs/PORT.md)
# True when grep finds a match or cannot read a file; false only on a clean miss.
found() { grep "$@"; [ $? -ne 1 ]; }
# Prints every old repository name: each word ending in `ai-rs`, in any case,
# that is not `suprnova-ai-rs`. A missing file already fails the ai:: scan.
old_repo() {
  grep -rnIoiE --exclude=check-identity.sh '[[:alnum:]_.-]*ai-rs' "$@" | grep -vi ':suprnova-ai-rs$'
}
# Prints every row of the docs/PORT.md module map whose Rust module cell
# lists the crate root `ai`, with or without backticks, and fails when the map
# is missing.
port_ai() {
  awk -F'|' '
    /^\|[[:space:]]*Python module[[:space:]]*\|[[:space:]]*Rust module[[:space:]]*\|/ { map = 1; seen = 1; next }
    map && !/^\|/ { map = 0 }
    map {
      count = split($3, entries, ",")
      for (i = 1; i <= count; i++) {
        entry = entries[i]
        gsub(/[`[:space:]]/, "", entry)
        if (entry == "ai") { print "docs/PORT.md:" NR ": " $0; break }
      }
    }
    END { if (!seen) print "docs/PORT.md: no module map" }
  ' docs/PORT.md
}

if meta=$(cargo metadata --no-deps --offline --format-version 1) &&
  pkg=$(jq -r '.packages[0].name' <<<"$meta") &&
  lib=$(jq -r '.packages[0].targets[] | select(.kind | index("lib")) | .name' <<<"$meta") &&
  repo=$(jq -r '.packages[0].repository' <<<"$meta") &&
  authors=$(jq -r '.packages[0].authors[]' <<<"$meta"); then
  meta_ok=1
  n=0
  [ "$pkg" = suprnova-ai ] || { echo "package is $pkg"; n=1; }
  [ "$lib" = suprnova_ai ] || { echo "library target is $lib"; n=1; }
  report PKG-001 "$n"
  n=0
  [ "$repo" = https://github.com/eas4ai/suprnova-ai-rs ] || { echo "repository is $repo"; n=1; }
  report PKG-003 "$n"
else
  echo "cargo metadata or jq failed; PKG-001, PKG-003 and PKG-007 not checked"
  meta_ok=
fi

n=0
found -rnIE --exclude=check-identity.sh "$old_path" "${files[@]}" && n=1
old_repo "${files[@]}" && n=1
[ "$(head -n 1 README.md)" = '# suprnova-ai' ] || { echo "README.md:1: $(head -n 1 README.md)"; n=1; }
[ -z "$(port_ai)" ] || { port_ai; n=1; }
report PKG-002 "$n"

n=0
holder='Shawn McAllister <@eas4ai>'
[ "$(sed -n 1p NOTICE)" = suprnova-ai-rs ] || { echo "NOTICE:1: $(sed -n 1p NOTICE)"; n=1; }
[ "$(sed -n 2p NOTICE)" = "Copyright 2026 $holder" ] || { echo "NOTICE:2: $(sed -n 2p NOTICE)"; n=1; }
old_repo NOTICE && n=1
for credit in 'AI SDK for Python 0.8.0' 'https://github.com/vercel-labs/ai-python' \
  'Copyright 2026 Vercel, Inc.' 'AI SDK for TypeScript 7.0.148' 'https://github.com/vercel/ai' \
  'Copyright 2023 Vercel, Inc.' 'docs/PORT.md' 'Apache License, Version 2.0'; do
  grep -qF "$credit" NOTICE || { echo "NOTICE lacks: $credit"; n=1; }
done
report PKG-004 "$n"

# The hash of https://www.apache.org/licenses/LICENSE-2.0.txt.
apache=cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30
if [ -n "$meta_ok" ]; then
  n=0
  [ "$(sha256sum LICENSE | cut -d' ' -f1)" = "$apache" ] || { echo "LICENSE is not the Apache License 2.0 text"; n=1; }
  # A license header: the Apache boilerplate, an SPDX line, or a line whose
  # comment punctuation (`//`, `//!`, `///`, `/*!`, `#`, `"""`, `<!--`, ...),
  # in one or more runs such as `// //`, comes before `Copyright`.
  found -rnIE --exclude=check-identity.sh \
    -e 'Licensed under the Apache License' -e 'SPDX-(License-Identifier|FileCopyrightText)' \
    -e '^[[:space:]]*([^[:alnum:][:space:]]+[[:space:]]*)+Copyright' \
    src tests scripts && n=1
  grep -qxF "$holder" <<<"$authors" || { echo "Cargo.toml authors lack $holder"; n=1; }
  # The first paragraph after the title line.
  intro=$(awk 'NR == 1 { next } /^[[:space:]]*$/ { if (text) exit; next } { text = text " " $0 } END { print text }' README.md)
  for phrase in 'AI SDK for Python' 'AI SDK for TypeScript' 'Apache License, Version 2.0'; do
    grep -qF "$phrase" <<<"$intro" || { echo "README.md opening lacks: $phrase"; n=1; }
  done
  report PKG-007 "$n"
fi

exit "$failed"
