# Decisions

The owner's rulings that govern this repository. Newest first. Each is recorded as given; where a ruling is
still being worked out, it says so and names what is pending rather than guessing.

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

*Pending:* the runtime crate and the exact system-call surface it exposes are being designed now. Until that design
lands (see "How CoralOS consumes these crates" in README.md), no runtime crate or `.device` crate exists here, and
CoralOS depends on nothing in this repository except `coralos-hal`.

**D5 — Consumption is pinned.** CoralOS builds consume this repository at a pinned revision, and a check in the
CoralOS build verifies the pin. *Pending:* designed together with D4's consumption path.

**D6 — Primary evidence over summaries.** A hardware fact is taken from primary evidence — the board's firmware or
recovery image, coreboot, the Linux source, a measurement on the machine — never from an external summary of it.
On 2026-10-11 a web summary of the Pixelbook Go (ATLAS) listed an Intel AC 7265, NVMe storage and a Synaptics
touchpad; all three were wrong against the board's own recovery image. A port's choice of which Linux driver to
port is grounded the same way.
