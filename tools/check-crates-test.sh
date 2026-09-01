#!/usr/bin/env bash
# check-crates-test.sh — every crate under crates/ must actually RUN its tests.
#
# WHY THIS EXISTS. The repo's three founding ports (i2c-designware-core, r8169-core, sdhci-core)
# spent an unknown period completely untestable: the root Cargo.toml globbed `crates/*` while the
# other 36 crates declared their own `[workspace]`, so cargo refused those three with "multiple
# workspace roots found in the same workspace" — an error whose text names twenty OTHER crates.
# Nothing noticed, because every crate added after the convention landed carried its own stanza and
# tested fine. The failure was invisible precisely where it mattered most.
#
# THE LIST IS DERIVED, NEVER HAND-MAINTAINED. A hand-written list rots with the thing it guards; a
# crate added tomorrow is checked by this script the moment it exists.
#
# A crate that produces NO `test result:` line FAILS. That is the discriminator against the
# did-not-compile class: never grep for `^error`, which a passing run can also print.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
fail=0 n=0
for m in "$root"/crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$m")")
  n=$((n + 1))
  out=$(cd /tmp && cargo test --manifest-path "$m" --target x86_64-unknown-linux-gnu 2>&1)
  if ! printf '%s' "$out" | command grep -qaE '^test result:'; then
    echo "FAIL $crate — no 'test result:' line; it did not run. First line:"
    printf '%s\n' "$out" | head -1 | sed 's/^/      /'
    fail=$((fail + 1)); continue
  fi
  if printf '%s' "$out" | command grep -qaE '^test result: FAILED'; then
    echo "FAIL $crate — tests ran and went RED"; fail=$((fail + 1)); continue
  fi
  printf 'ok   %-34s %s passed\n' "$crate" "$(printf '%s' "$out" | command grep -aE '^test result:' | awk '{s+=$4} END{print s}')"
done
echo "── $((n - fail))/$n crates run their tests ──"
[ "$fail" -eq 0 ] || { echo "!! $fail crate(s) cannot be verified"; exit 1; }
