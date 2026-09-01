#!/usr/bin/env bash
# check-citations.sh — a cited constant must name a FILE **and a LINE**.
#
# WHY THIS EXISTS, and it is not style. Six constants in sdhci-core had WRONG VALUES: the eMMC
# driver's SDHCI_TRNS_AUTO_SEL, four host DMA flags, and MMC_RSP_BUSY. Every one of the six carried
# the comment `// sdhci.h (value from pinned header)` — a file with no line. None of the crate's
# correct constants did. The comment was the marker.
#
# A line-less citation cannot be re-checked, so a transcription slip survives review forever. The
# four host flags had been shifted by exactly two while their neighbours were right — the shape a
# hand-copied run of #defines takes when one line is skipped, and invisible without the line numbers.
#
# A MUTATION SWEEP CANNOT REPLACE THIS. sdhci-core measures 154/154 constants pinned, and that number
# would have been just as green with all six wrong values in place: the test and the constant hold
# the SAME value, so pinning proves nothing about truth. Coverage and correctness are different
# properties. This checker guards the second one.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
for m in "$root"/crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$m")")
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    echo "FAIL $crate — citation names a file but no line: ${line#"${line%%[![:space:]]*}"}"
    fail=$((fail + 1))
  done < <(command grep -rhE '^\s*pub const .*=.*;\s*//' "$(dirname "$m")"/src/*.rs 2>/dev/null \
           | command grep -E '\.(c|h)\b' | command grep -vE '\.(c|h):[0-9]+')
done
if [ "$fail" -eq 0 ]; then
  echo "── every cited constant carries a file AND a line ──"; exit 0
fi
echo "!! $fail constant(s) cite a file with no line — re-derive each from the source, do not guess"
exit 1
