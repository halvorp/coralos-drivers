# coralos-drivers

Open-source hardware drivers for CoralOS, written as **pure Rust ports of proven
open-source drivers** — the Linux kernel's, with BSD/Haiku implementations read as
second opinions where the Linux source is ambiguous. The owner's rulings that govern
this repository are in [DECISIONS.md](DECISIONS.md).

## Why ports, not rewrites

A mature driver encodes decades of errata: tuning retries, error-recovery state machines,
PHY init tables, quirk lists. Reverse-engineering rediscovers those the hard way, one
hang at a time. Porting keeps the hard-won sequences **verbatim and cited** (every
non-obvious register sequence carries a comment naming its source file and line), while
the Rust rewrite brings memory safety and `no_std` portability. How a port is written is
in [PORT_CONVENTIONS.md](PORT_CONVENTIONS.md).

## Licensing

Each crate keeps the license of the driver it ports — a port of a GPL-2.0 Linux driver
is GPL-2.0, clearly marked in its `Cargo.toml` and on every file's SPDX line. Firmware
blobs, where a driver needs them, are included only under their redistribution licenses.

This repository is the **only** home of GPL-derived CoralOS code (DECISIONS.md D2).
CoralOS itself is closed source and never links a GPL crate into its own binaries.
The licence shape (D4):

| Piece | Lives | Licence |
|---|---|---|
| `crates/<name>-core` — a port | here | the Linux file's (normally `GPL-2.0-only`) |
| `hal/` — `coralos-hal`, the hardware-access traits | here | MIT OR Apache-2.0 |
| the driver runtime — system-call shim for MMIO / DMA / IRQ / IPC / log | here (*pending*) | permissive |
| a `.device` driver binary linking ports + `coralos-hal` + the runtime | here (*pending*) | GPL (it links GPL ports) |
| the CoralOS kernel, services and desktop | the closed CoralOS tree | proprietary |

A `.device` links only GPL-compatible code. CoralOS uses it as a **separate driver
process across its IPC / capability boundary** (a microkernel `.device` artifact),
never as a linked library.

## The HAL

`hal/` is deliberately small (D3): the `Mmio` register trait and the cooperative-wait
hooks, mirrored verbatim from CoralOS's own seam. It grows only when a real driver needs
more, and only by what that driver needs — the next additions are `Dma` and `Irq` traits
for the Intel Wireless 9260 (iwlwifi) port. Port crates stay pure: they take values in and
give values out, or drive a narrow trait; they do no MMIO of their own.

## How CoralOS consumes these crates

*Pending — being designed (DECISIONS.md D4, D5).* This section is filled in when the
design lands. It will say:

- the permissive driver runtime crate: its name, licence and system-call surface;
- where `.device` driver crates live in this repository and how one is built;
- how a CoralOS build pins a revision of this repository and how the pin is verified;
- how a built `.device` reaches a CoralOS system image.

Until then, CoralOS depends on nothing here except `coralos-hal`, and nothing here
depends on CoralOS.

## Structure

```
crates/
  <name>-core/   one Linux port per crate, standalone [workspace], no_std
                 (e.g. sdhci-core, r8169-core, xhci-ring-core, pinctrl-cherryview-core)
hal/             coralos-hal: the minimal MMIO + cooperative-wait trait surface
tools/
  check-citations.sh    every cited constant names a file AND a line
  check-crates-test.sh  every crate still runs its tests
```

Each crate is `no_std`, takes its hardware access through the `hal` traits or as plain
values, and carries its own tests where hardware behavior can be modeled.

## Status

Early. 45 port crates, host-tested. First fronts: SDHCI/eMMC error-recovery hardening,
r8169; next: the Intel Wireless 9260 (iwlwifi), which brings the `Dma` and `Irq` traits.
