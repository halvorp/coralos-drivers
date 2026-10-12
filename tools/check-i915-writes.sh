#!/usr/bin/env bash
# check-i915-writes.sh — the i915 port family may WRITE the GPU in exactly the places docs/I915-CONTRACTS.md names.
#
# WHY THIS EXISTS. Increment 1 of the Kaby Lake i915 port is allowed three MMIO writes - the forcewake SET registers - and "read-only
# elsewhere" is only true while nothing quietly grows a w32. The allow-list is enforced at run time twice (rt `WriteWindow`,
# common-core `WriteGuard`); this check enforces that the CODE paths that can write are the reviewed ones, so a new write call in
# a port crate, or a second window opened with a different list, fails the build instead of reaching silicon.
#
#   * a register write call (`.w8(` `.w16(` `.w32(` `write32(` `write_volatile`) may appear only in:
#       crates/i915-uncore-core/src/forcewake.rs   (the forcewake protocol: the only register writer)
#       crates/i915-common-core/src/guard.rs        (the guard forwards writes it has allow-listed)
#       devices/i915/src/regio.rs                   (BarMmio forwards to the WriteWindow / latches refusals)
#       devices/i915/src/gt.rs                      (ByRef forwards to the guard's inner Mmio)
#       rt/src/mmio.rs                              (the WriteWindow itself)
#   * a `WriteWindow::open(` may appear only in devices/i915/src/service.rs, and must be handed `&BAR0_WRITE_RANGES`.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
allowed_writers='^(crates/i915-uncore-core/src/forcewake\.rs|crates/i915-common-core/src/guard\.rs|devices/i915/src/regio\.rs|devices/i915/src/gt\.rs|rt/src/mmio\.rs)$'
cd "$root" || exit 2
while IFS= read -r f; do
  rel="${f#./}"
  if printf '%s' "$rel" | command grep -qE "$allowed_writers"; then continue; fi
  # strip // comments before looking for a call
  hits=$(sed 's|//.*$||' "$f" | command grep -nE '\.w(8|16|32)\(|write32\(|write_volatile' || true)
  if [ -n "$hits" ]; then
    echo "FAIL $rel — a register write outside the reviewed writers:"; printf '%s\n' "$hits" | sed 's/^/      /'
    fail=$((fail + 1))
  fi
done < <(find crates/i915-*-core/src devices/i915/src -name '*.rs' 2>/dev/null | sort)
opens=$(command grep -rnE 'WriteWindow::open\(' devices crates --include='*.rs' 2>/dev/null | command grep -v '/tests/' || true)
while IFS= read -r l; do
  [ -n "$l" ] || continue
  case "$l" in
    devices/i915/src/service.rs:*'&BAR0_WRITE_RANGES'*) ;;
    *) echo "FAIL WriteWindow::open outside service.rs or without &BAR0_WRITE_RANGES: $l"; fail=$((fail + 1));;
  esac
done <<< "$opens"
if [ "$fail" -eq 0 ]; then echo "── i915 register writes confined to the reviewed writers ──"; exit 0; fi
echo "!! $fail violation(s) — a new writer needs a docs/I915-CONTRACTS.md change and a reviewed allow-list"; exit 1
