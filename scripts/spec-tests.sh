# Sourced by the check scripts that run spec tests (check-core.sh,
# check-hosts.sh, check-openai.sh, check-anthropic.sh, check-google.sh,
# check-vertex.sh, check-operations.sh). The caller sets `tests` to the test targets and `reqs`
# to the requirements, and may define `extra_check REQ`, which fails REQ when
# it returns nonzero.
# `run_spec_tests` prints one `sudus: REQ: pass|fail` line for each
# requirement and returns nonzero when one failed. Each requirement's tests
# are named after it (hst_001_..., agt_001_...). A requirement passes only
# when at least one of its tests ran and all of them passed. When the tests do
# not build, every requirement fails.

T=/home/shawn/.rustup/toolchains/1.95-x86_64-unknown-linux-gnu/bin
export PATH="$T:$PATH"
export CARGO_TARGET_DIR=/home/shawn/workspace2/scratchpads/targets/suprnova-ai-rs/w-spec
export RUSTC="$T/rustc" RUSTDOC="$T/rustdoc" RUSTUP_AUTO_INSTALL=0

# True when grep finds a match or cannot read a file; false only on a clean miss.
found() { grep "$@"; [ $? -ne 1 ]; }

# Prints REQ's result line; a failure sets the calling run_spec_tests' `failed`.
spec_report() {
  if [ "$2" -eq 0 ]; then echo "sudus: $1: pass"; else echo "sudus: $1: fail"; failed=1; fi
}

run_spec_tests() {
  local cargo=("$T/cargo" test --locked --all-features)
  local test req filter output code ran result failed=0
  for test in "${tests[@]}"; do cargo+=(--test "$test"); done

  if ! "${cargo[@]}" --no-run; then
    for req in "${reqs[@]}"; do spec_report "$req" 1; done
    return 1
  fi

  for req in "${reqs[@]}"; do
    echo "== $req"
    filter="$(printf '%s' "$req" | tr 'A-Z-' 'a-z_')_"
    output=$("${cargo[@]}" -- "$filter" 2>&1)
    code=$?
    printf '%s\n' "$output"
    # Tests that ran, passed or failed, across every test binary's summary line.
    ran=$(printf '%s\n' "$output" |
      sed -n 's/^test result: [A-Za-z]*\. \([0-9][0-9]*\) passed; \([0-9][0-9]*\) failed.*/\1 \2/p' |
      awk '{ total += $1 + $2 } END { print total + 0 }')
    result=0
    if [ "$code" -ne 0 ]; then result=1; fi
    if [ "$ran" -eq 0 ]; then
      echo "$req: no test named $filter* ran"
      result=1
    fi
    if declare -F extra_check >/dev/null && ! extra_check "$req"; then result=1; fi
    spec_report "$req" "$result"
  done
  return "$failed"
}
