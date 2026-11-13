#!/usr/bin/env bash
# check-i915-waits.sh — a wait in i915.device must be able to END: its predicate has to be something that can change while it waits,
# and whoever waits has to drive the thing that changes it.
#
# WHY THIS EXISTS. i915.device is single-threaded and has no engine interrupt: completion is found ONLY by servicing the engine (CSB +
# seqno read + retire, `UapiService::poll` / `EngineRig::service`). A loop that yields while it waits for "the GPU is done" and does NOT
# service the engine waits for something that cannot happen until it returns. Atlas 2026-10-28: every GEM_CLOSE that unbound an idle
# object while ANY other request was in flight ran `Executor::run_bind`'s AwaitRetire loop with the predicate `|_| !busy` - a bool
# captured once, before the loop - spun 10,000 scheduler yields, answered `AWAIT-RETIRE-TIMEOUT` (EBUSY) and blocked every client of
# the device for the duration (`[coral-drm-shim] WIRE-WAIT SLOW`, 16 frames latched in 3 minutes). The fix moved the wait to the uAPI
# sink, where it can call the engine's service between asks; this check keeps the hole closed.
#
#   A. Every `run_bind(` / `run_bind_step(` call in devices/i915/src passes as its retirement predicate either `|_| true` (a plan of the
#      device's own objects, or one whose victims were proven retired) or a closure that calls `retired_now(` (the sink's per-VMA
#      predicate over the request seqnos the hardware wrote). Anything else - in particular a closure over a captured value - FAILS.
#   B. A `.relax()` call (a yield inside a wait loop) may appear only in a reviewed function whose loop re-reads state the HARDWARE
#      changes on its own (an MMIO poll) or that services the engine itself; the list is below with the reason for each. The executor's
#      bind path is NOT on it: it cannot make the GPU progress, so it never waits.
#   C. The handlers that answer a GEM_WAIT, SYNCOBJ_WAIT or SYNC_FILE_WAIT advance the device (`self.poll(hw)`) before they evaluate
#      completion, so a Mesa poll (timeout 0) reads the seqno the GPU wrote now, not the one the last 4 ms timer saw.
#
# Exit 0 = clean; 1 = a violation (each printed as FAIL file:line - reason).
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 2
fail=0

# ---- A ----------------------------------------------------------------------------------------------------------------------------
while IFS= read -r hit; do
  [ -n "$hit" ] || continue
  file="${hit%%:*}"; rest="${hit#*:}"; line="${rest%%:*}"; text="${rest#*:}"
  # the predicate is the last argument: take the text from the last `&mut |` to the end of the call (one line in this tree)
  pred="${text##*&mut |}"
  # run_bind_step forwards its caller's predicate to run_bind unchanged: the caller is what this rule checks
  if [ "$file" = devices/i915/src/exec/memory.rs ] && printf '%s' "$text" | command grep -qE 'run_bind\(gm, plan, retired\)'; then continue; fi
  if [ "$pred" = "$text" ]; then
    echo "FAIL $file:$line - run_bind without an inline predicate closure (pass |_| true or a closure calling retired_now): $text"
    fail=$((fail + 1)); continue
  fi
  body="${pred#*|}"
  case "$body" in
    ' true)'*|' true).'*|' true);'*|' true)?'*) ;;
    *retired_now\(*) ;;
    *) echo "FAIL $file:$line - AwaitRetire predicate is neither |_| true nor the per-VMA retired_now(): |$pred"
       fail=$((fail + 1));;
  esac
done < <(command grep -rnE '\.run_bind(_step)?\(' devices/i915/src | command grep -v '^\s*//' | command grep -vE 'pub fn run_bind')

# ---- B ----------------------------------------------------------------------------------------------------------------------------
# file:function - why its loop may yield
allowed_relax='
devices/i915/src/exec/plans.rs:reset_loop        # reset handshake: polls RESET_CTL / GDRST, which the hardware clears
devices/i915/src/exec/plans.rs:clock_plan_awake  # display-clock PollPlan: polls a status register the hardware sets
devices/i915/src/exec/plans.rs:pcode             # pcode mailbox: polls GEN6_PCODE_MAILBOX READY, cleared by the PCU
devices/i915/src/exec/core.rs:poll               # Executor::poll: an MMIO PollPlan, bounded by its own deadline
devices/i915/src/exec/engine.rs:wait_idle        # calls EngineRig::service (CSB + seqno + retire) every pass
crates/i915-uncore-core/src/forcewake.rs:poll    # forcewake ACK register, set by the hardware
devices/i915/src/exec/power_chv.rs:wait_for_pw_status # Cherryview VLV_GTLC_PW_STATUS: the GUnit sets ALLOWWAKEACK / the well bits; 3 ms deadline (vlv_suspend.c:295)
'
allowed_list=$(printf '%s\n' "$allowed_relax" | sed 's/#.*$//; s/[[:space:]]*$//' | command grep -v '^$')
while IFS= read -r f; do
  awk -v F="$f" '
    /^[[:space:]]*\/\// { next }
    /fn [a-z_0-9]+/ { match($0, /fn [a-z_0-9]+/); fn = substr($0, RSTART + 3, RLENGTH - 3) }
    /\.relax\(\)/ && fn != "relax" && fn != "park_ms" { print F ":" fn ":" NR }  # relax/park_ms bodies are the forwarders
  ' "$f"
done < <(find devices/i915/src crates/i915-*-core/src -name '*.rs' 2>/dev/null | sort) | while IFS= read -r site; do
  key="${site%:*}"; line="${site##*:}"
  if ! printf '%s\n' "$allowed_list" | command grep -qxF "$key"; then
    echo "FAIL ${key%%:*}:$line - .relax() in fn ${key##*:}: a yield in a wait loop outside the reviewed hardware polls (a wait on GPU completion must service the engine, not yield)"
    echo X >> "${TMPDIR:-/tmp}/check-i915-waits.$$"
  fi
done
if [ -f "${TMPDIR:-/tmp}/check-i915-waits.$$" ]; then
  fail=$((fail + $(wc -l < "${TMPDIR:-/tmp}/check-i915-waits.$$")))
  rm -f "${TMPDIR:-/tmp}/check-i915-waits.$$"
fi

# ---- C ----------------------------------------------------------------------------------------------------------------------------
check_poll_before() { # file, ERE of the evaluation, label
  local f="$1" re="$2" label="$3" n
  n=$(command grep -cE "$re" "$f")
  if [ "$n" -eq 0 ]; then echo "FAIL $f - the $label evaluation ($re) is gone: this check no longer sees it"; fail=$((fail + 1)); return; fi
  while IFS= read -r ln; do
    local from=$((ln > 12 ? ln - 12 : 1))
    if ! sed -n "${from},${ln}p" "$f" | command grep -q 'self\.poll(hw)'; then
      echo "FAIL $f:$ln - the $label handler evaluates completion without advancing the device first (self.poll(hw))"
      fail=$((fail + 1))
    fi
  done < <(command grep -nE "$re" "$f" | cut -d: -f1)
}
check_poll_before devices/i915/src/uapi/calls_gem.rs 'self\.gem\.wait_plan\(sid, r\.handle' 'GEM_WAIT'
check_poll_before devices/i915/src/uapi/calls_sync.rs 'self\.sync\.wait_eval\(inc, &handles, points\.as_deref\(\), r\.head\.flags, timeline, None\)' 'SYNCOBJ_WAIT'
check_poll_before devices/i915/src/uapi/calls_sync.rs 'match self\.sync\.file_status\(inc, r\.token\)' 'SYNC_FILE_WAIT'

if [ "$fail" -eq 0 ]; then echo "── i915 waits: every wait can end (predicates are live, waiters drive progress) ──"; exit 0; fi
echo "!! $fail wait violation(s) - see the header of tools/check-i915-waits.sh"
exit 1
