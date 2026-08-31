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

## 8. Scope discipline

Do ONLY the crate named in your task. Do not touch `Cargo.toml` at the workspace root, other
crates, or any file outside your crate directory. Another worker is editing those in parallel.
