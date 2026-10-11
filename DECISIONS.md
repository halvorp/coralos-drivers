# Decisions

The owner's rulings that govern this repository. Newest first. Each is recorded as given; where a ruling is
still being worked out, it says so and names what is pending rather than guessing.

## 2026-10-11 — Architect (Astra), keyboard ruling

**D7 — rt extended with capability-scoped port I/O and the physical key-event frame.** *Exposing an operation is not
granting it.* `rt/` gains `PORT_IO` (system call 52: ops IN8/OUT8/IN16/OUT16/IN32/OUT32 = 0..5, QUERY = 6, refusal
`u64::MAX`, rights IN/OUT/W8/W16/W32 = 1/2/4/8/16, 4 slots) behind an opaque `PortRange` handle, and the key frame
`[0x90, HID usage, state]` on endpoint 12 (state RELEASE 0 / PRESS 1 / REPEAT 2 / ALL_UP 3). A driver never names a raw port;
the loader grants `i8042.device` byte access to 0x60 and 0x64 as two ranges, plus IRQ 1. `devices/i8042` is the first
public GPL `.device`: it links only rt, hal and the three cores (i8042-core, atkbd-scancode-core, hid-input-keyboard-core).
It emits PHYSICAL key events; layout, modifiers, CapsLock, text and the ChromeOS top-row policy stay in the input layer.

## 2026-10-11 — Peter (owner)

**D1 — Linux drivers are ported, not reinvented.** In the owner's words: *"we have the open source repo for the GPL
derived code, so you can take all the hardware drivers from Linux and rewrite them in Rust, adjusted to work with
CoralOS."* A port is mechanical: every register offset, mask, sequence and message is taken from the Linux source
and cited by file and line (PORT_CONVENTIONS.md §1). BSD and Haiku drivers are second opinions, read when the Linux
source is ambiguous; they are not the port source.

**D2 — This repository is the only home of GPL-derived CoralOS code.** One standalone `no_std` crate per port,
`crates/<name>-core`, with its own `[workspace]`, `license` naming the Linux file's licence (normally
`GPL-2.0-only`) and an SPDX line on every file (PORT_CONVENTIONS.md §6, §7a). The CoralOS operating system tree is
closed source and does not contain GPL-derived code; code written there from public specifications may cite Linux
for facts, but ported logic belongs here.

**D3 — No broad HAL.** `hal/` (`coralos-hal`, MIT OR Apache-2.0) is the `Mmio` trait plus the cooperative-wait
hooks, mirrored verbatim from CoralOS's own seam. It grows only when a real driver needs something, and only by
what that driver needs. The next additions are `Dma` and `Irq` traits, for the Intel Wireless 9260 (iwlwifi) port.
Ports stay pure: values in, values out, no MMIO of their own (PORT_CONVENTIONS.md §2).

**D4 — Licence shape of a driver binary.** A `.device` driver binary that links a GPL port is GPL. It links only
GPL-compatible code: the port crates, `coralos-hal`, and a minimal permissive driver runtime (the system-call shim
for MMIO, DMA, IRQ, IPC and logging) that will live in this repository. The `.device` driver crates themselves live
here too. CoralOS uses a driver only as a separate process, across its process / capability boundary — never by
linking it into its own binaries.

The runtime is `rt/` (`coralos-rt`, MIT OR Apache-2.0), published with the owner's approval on 2026-10-11 ("yes,
publish rt/ to the drivers repo"). Its header lists every CoralOS ABI fact it makes public: the `syscall` calling
convention, sixteen system-call numbers, the return words, the DMA out-record and cache argument, and the `_start(role)`
entry. Anything a driver needs beyond that list is added here deliberately, with the same review — never by linking a
proprietary CoralOS crate. Still to come: logging/formatting and a heap usable by a GPL `.device`, per-device slot
numbers, the network and Wi-Fi frame formats, and a kernel call that frees DMA memory.

**D5 — Consumption is pinned.** CoralOS builds consume this repository at a pinned revision, and a check in the
CoralOS build verifies the pin: a driver crate names this repository as a cargo git dependency with an exact `rev`,
pinned once in CoralOS's `tools/drivers.lock`, and CoralOS's `tools/check-drivers-pin.sh` fails the build on a branch,
another rev, a local path, a stale Cargo.lock, or a rev not on this repository's `main`.

**D6 — Primary evidence over summaries.** A hardware fact is taken from primary evidence — the board's firmware or
recovery image, coreboot, the Linux source, a measurement on the machine — never from an external summary of it.
On 2026-10-11 a web summary of the Pixelbook Go (ATLAS) listed an Intel AC 7265, NVMe storage and a Synaptics
touchpad; all three were wrong against the board's own recovery image. A port's choice of which Linux driver to
port is grounded the same way.
