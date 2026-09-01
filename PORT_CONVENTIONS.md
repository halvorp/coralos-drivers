# House rules for a CoralOS driver port

These are not style preferences. Each one exists because breaking it cost a debugging session.

## 1. The port is MECHANICAL, and every constant carries FILE and LINE

Extract register offsets, bit positions, masks and messages from the Linux source. Cite the FILE as
well as the line:

```rust
pub const CTRL: u32 = 0x00; // pwm-lpss.h:24
```

THE FILE MATTERS AS MUCH AS THE LINE. Linux moves code between files; a citation carrying only a
line number silently points into the wrong file after any re-sync.

A mistyped offset does not fail to compile. It drives a different register on real silicon and
reports nothing. That is why the tests below are not optional.

## 2. `no_std`, no hardware access, no I/O

The crate is `#![no_std]`. It contains register maps, encode/decode functions, and state machines.
It performs NO MMIO itself — the caller passes values in and gets values out, or drives a narrow
trait. A port that reads a register directly cannot be host-tested and is the wrong shape.

## 3. Tests are vectors with LINUX literals, and the expected values are written out LITERALLY

`tests/<module>.rs` per module. Expected values are Linux's own literals, with the file and line in
a comment.

**NEVER derive an expected list from the production table it is testing.** A list generated from
the table cannot detect a deletion from that table: the test case disappears together with the thing
it guards. Write the expected names/values out by hand, and assert the production table matches.

## 3a. THE FIVE WAYS A GREEN SUITE STILL LEAVES A CONSTANT UNGUARDED

Every one of these was found by mutating a crate whose tests passed, whose names read correctly, and
whose porting was right. Not one was caught by reading the tests. Check your own crate against all
five before you report it done.

**1. COMPOSITE.** Pinning a composite does not pin its components. Asserting `R2_FLAGS == 0x7` says
nothing about `RSP_136`, `RSP_CRC` or `RSP_OPCODE` unless the assertion is written as the OR of the
NAMED components. Otherwise a wrong component and a compensating wrong composite pass together.
*Fix:* assert `COMPOSITE == A | B | C` using the constants, and assert each of A, B, C by value.

**2. ZERO-VALUED.** A constant whose value is `0` cannot be covered by value: ORing zero changes
nothing, and a zero is indistinguishable from an absent entry. `KEY_RESERVED = 0` and `REP_DELAY = 0`
both survived mutation in a suite that named them. `REP_DELAY` is an INDEX — setting it to 1 collides
with `REP_PERIOD` and silently swaps a keyboard's repeat delay and period.
*Fix:* pin it by BEHAVIOUR — assert it is distinct from its siblings and that it selects the thing it
names.

**3. TYPE-PINNED — and this one is the INVERSE trap.** If a constant is load-bearing for a type, e.g.
`TRIP_TYPES: [TripType; TRIP_TYPE_COUNT]`, then a wrong value does not fail a test, it fails to
COMPILE. Zero `test result:` lines means the compiler pinned it, which is STRONGER than any test and
cannot be unpinned by deleting one. Do not "fix" it by adding a test; do not report it as a gap.
*Fix:* nothing. Recognise it, and say so in your report.

**4. RESTATED — the most dangerous, because from outside it is indistinguishable from coverage.** If
an assertion hardcodes the value instead of routing through the constant, the test and production
hold two independent copies that drift apart in silence. A test that hardcodes `0x80` does not notice
`DIRECTION_MASK` becoming `0x81`. A crate shipped with the right test names, sixteen passing tests,
and three unguarded masks for exactly this reason.
*Fix:* the EXPECTED side of an assertion is a Linux literal; the ACTUAL side must travel THROUGH the
constant that production code uses. If changing the constant cannot change the test result, the test
is testing itself.

**5. GUARDED BY CONVENIENCE.** The family member a test happened to exercise is pinned; its SIBLINGS
inherit nothing. `BASE_ADDRESS_MEM_TYPE_64` was caught because the 64-bit BAR walk drives it, while
`TYPE_32`, `TYPE_1M`, `TYPE_MASK` and `PREFETCH` were read only in decode branches no test drove.
`HEADER_TYPE_NORMAL` and `CARDBUS` were caught; `BRIDGE` and `MFD` were not. Nothing announces this:
a wrong `BASE_ADDRESS_MEM_TYPE_MASK` misclassifies every BAR on every device, and a wrong
`HEADER_TYPE_MFD` loses every function past zero — the machine enumerates the wrong hardware and
reports success.
*Fix:* for each ENUM-LIKE FAMILY, one test that drives a real input through the decode for EVERY
member and asserts the exact classification. A family is covered as a family or not at all.

## 4. Every test must be PROVEN able to fail

Before claiming a test works, mutate the source and confirm the test goes red. Classify any
zero-failure mutation as one of: never applied / test too weak / EQUIVALENT / did not compile / the
applied-probe itself was wrong. The discriminator for "did not compile" is whether a `test result:`
line exists at all — never grep for `^error`.

## 5. NAME the refusal

An error path says what refused and why. Never a bare `false`, never a silent clamp. If a value is
out of range, the error names the value and the bound.

## 6. Licence

Every file starts with `// SPDX-License-Identifier: GPL-2.0-only` and a module doc naming the Linux
source files it was ported from, plus the original copyright holders. This repo is the PUBLIC home
for GPL-derived ports. Do not copy code into any other tree.

## 7. What "done" means

- `cargo test` passes from `/tmp` with `--manifest-path <abs> --target x86_64-unknown-linux-gnu`
  (a bare `cargo test` in the crate dir picks up a bare-metal target and fails misleadingly).
- Every public function has at least one vector.
- Counts are pinned: if Linux defines N of something, a test asserts N and asserts the names.
- No `unsafe`. No `alloc` unless unavoidable and justified in a comment.

## 7a. The crate is a STANDALONE workspace, and that is load-bearing

Your `Cargo.toml` carries a bare `[workspace]` stanza. This is not boilerplate: it is what lets N
port workers hold N git worktrees and never contend on a shared root manifest, which is what §8 below
is asking of you.

The repo root's `Cargo.toml` therefore does NOT glob `crates/*`. When it did, cargo refused every
crate that had omitted the stanza with `multiple workspace roots found in the same workspace` — and
the error text names twenty OTHER crates, so it reads like somebody else's problem. The three crates
that omitted it were the three founding ports, and 181 of their tests stopped running with nothing
to say. `tools/check-crates-test.sh` now fails if any crate stops running its tests.

## 8. Scope discipline

Do ONLY the crate named in your task. Do not touch `Cargo.toml` at the workspace root, other
crates, or any file outside your crate directory. Another worker is editing those in parallel.
