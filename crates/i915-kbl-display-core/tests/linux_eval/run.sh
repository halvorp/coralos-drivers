#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Regenerates the Linux-evaluated tables behind tests/skl_wm_linux.rs and tests/mode_write_linux.rs (link M/N): cuts the named
# functions VERBATIM out of the pinned Linux tree (v7.0-rc6) by line range, compiles them with the stub types in wm_shim.h /
# mn_shim.h, and prints what Linux computes. Usage: LINUX_REF=~/Projects/references/coralos/linux-ref bash run.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
ref=${LINUX_REF:-$HOME/Projects/references/coralos/linux-ref}
D=$ref/drivers/gpu/drm/i915/display
W=$D/skl_watermark.c
out=$(mktemp -d)
mkdir -p "$out/inc/linux"; : > "$out/inc/linux/bug.h"; for h in kernel math64 types; do : > "$out/inc/linux/$h.h"; done
cp "$D/intel_fixed.h" "$out/inc/"
cp "$here"/*.h "$here"/*.c "$here/scen.txt" "$out/"
{ sed -n 60,76p "$W"; sed -n 398,405p "$W"; sed -n 94,97p "$W"; sed -n 596,608p "$W"; sed -n 610,631p "$W"; sed -n 632,668p "$W"
  sed -n 1352,1358p "$W"; sed -n 1359,1368p "$W"; sed -n 1370,1386p "$W"; sed -n 1388,1419p "$W"; sed -n 1266,1277p "$W"
  sed -n 1294,1313p "$W"; sed -n 1613,1745p "$W"; sed -n 1769,1955p "$W"; sed -n 1420,1605p "$W"; sed -n 3146,3258p "$W"; } > "$out/wm_fns.inc"
gcc -w -I"$out/inc" -include stdlib.h -o "$out/wm_main" "$out/wm_main.c"
while read -r n rest; do echo "## $n $rest"; "$out/wm_main" $rest; done < "$out/scen.txt"
R=$ref/drivers/gpu/drm; H=$ref/drivers/gpu/drm/display
{ echo '#include "mn_shim.h"'; sed -n 4827,4839p "$H/drm_dp_helper.c"; sed -n 4860,4868p "$H/drm_dp_helper.c"
  sed -n 155,158p "$D/intel_dp.c"; sed -n 167,170p "$D/intel_dp.c"; sed -n 476,481p "$D/intel_dp.c"; sed -n 2499,2547p "$D/intel_display.c"; } > "$out/mn_fns.inc"
gcc -w -o "$out/mn_main" "$out/mn_main.c"
for a in "24 4 148500 270000" "24 2 148500 162000" "24 4 594000 540000"; do echo "MN $a => $("$out/mn_main" $a)"; done
