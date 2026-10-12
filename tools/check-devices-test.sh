#!/usr/bin/env bash
# check-devices-test.sh — every devices/* crate must actually RUN its tests, in every feature set it ships.
#
# WHY THIS EXISTS. tools/check-crates-test.sh walks crates/*/ and wire/*/ and has never looked at devices/. A device crate is where
# the effects live (the one crate that executes writes), and devices/i915's increment features are COMPILE-TIME grants: with `p2-memory`
# and `p3-exec` OFF the write allow-list is three registers and the uAPI service does not exist; with them ON a different list and a
# whole service are compiled in. A suite that only ran the default build would stay green while the p3-exec half stopped compiling,
# and the reverse. So each device is tested in its feature MATRIX, and a cell that produces no `test result:` line FAILS.
#
# THE MATRIX IS DERIVED, NEVER HAND-MAINTAINED (same rule as check-crates-test.sh): the cells are the crate's [features] table minus
# `entry` (which builds the standalone ELF, not the library under test) and `default` - the default build, each feature alone, and all of
# them together. devices/i915 therefore runs: default, p2-memory, p3-exec, p2-memory+p3-exec. A device added tomorrow is
# checked the moment it exists.
#
# A cell whose output has no `test result:` line (it did not compile or did not run), or any `test result: FAILED`, or zero passing tests
# across the cell, FAILS. Never grep for `^error`: a passing run can print one.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
# a runaway test once OOM-killed a whole session (2026-10-12): run under a memory cap whenever the host can give one
if [ -z "${CHECK_DEVICES_SCOPED:-}" ] && command -v systemd-run >/dev/null 2>&1; then
  exec env CHECK_DEVICES_SCOPED=1 systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 "$0" "$@"
fi
fail=0 n=0
matrix() { # print one feature set per line ("" = default) for the Cargo.toml in $1
  python3 -I - "$1" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    feats = [k for k in tomllib.load(f).get("features", {}) if k not in ("entry", "default")]
cells = [""] + feats
if len(feats) > 1:
    cells.append(",".join(feats))
seen = []
for c in cells:
    if c not in seen:
        seen.append(c)
        print(c)
PY
}
for m in "$root"/devices/*/Cargo.toml; do
  dev=$(basename "$(dirname "$m")")
  cells=$(matrix "$m") || { echo "FAIL $dev — could not read its [features]"; fail=$((fail + 1)); n=$((n + 1)); continue; }
  while IFS= read -r feat; do
    n=$((n + 1))
    label="$dev [${feat:-default}]"
    args=(--manifest-path "$m" --target x86_64-unknown-linux-gnu)
    [ -n "$feat" ] && args+=(--features "$feat")
    out=$(cd /tmp && cargo test "${args[@]}" 2>&1)
    if ! printf '%s' "$out" | command grep -qaE '^test result:'; then
      echo "FAIL $label — no 'test result:' line; it did not run. First error:"
      printf '%s\n' "$out" | command grep -aE '^error' | head -1 | sed 's/^/      /'
      fail=$((fail + 1)); continue
    fi
    if printf '%s' "$out" | command grep -qaE '^test result: FAILED'; then
      echo "FAIL $label — tests ran and went RED"; fail=$((fail + 1)); continue
    fi
    passed=$(printf '%s' "$out" | command grep -aE '^test result:' | awk '{s+=$4} END{print s+0}')
    if [ "$passed" -eq 0 ]; then
      echo "FAIL $label — 0 tests passed (nothing was checked)"; fail=$((fail + 1)); continue
    fi
    printf 'ok   %-44s %s passed\n' "$label" "$passed"
  done <<< "$cells"
done
echo "── $((n - fail))/$n device test cells run and pass ──"
[ "$fail" -eq 0 ] || { echo "!! $fail device cell(s) cannot be verified"; exit 1; }
