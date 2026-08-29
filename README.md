# coralos-drivers

Open-source hardware drivers for CoralOS, written as **pure Rust ports of proven
open-source drivers** — primarily the Linux kernel's, with BSD/Haiku implementations
where they are cleaner.

## Why ports, not rewrites

A mature driver encodes decades of errata: tuning retries, error-recovery state machines,
PHY init tables, quirk lists. Reverse-engineering rediscovers those the hard way, one
hang at a time. Porting keeps the hard-won sequences **verbatim and cited** (every
non-obvious register sequence carries a comment naming its source file and line), while
the Rust rewrite brings memory safety and `no_std` portability.

## Licensing

Each crate keeps the license of the driver it ports — a port of a GPL-2.0 Linux driver
is GPL-2.0, clearly marked in its `Cargo.toml` and crate root. Firmware blobs, where a
driver needs them, are included only under their redistribution licenses. CoralOS itself
consumes these drivers as **separate driver processes across an IPC/capability
boundary** (microkernel `.device` artifacts), so this repository stands alone.

## Structure

```
crates/
  sdhci/      SD/eMMC host controller (port of Linux drivers/mmc/host/sdhci*)
  r8169/      Realtek gigabit ethernet (port of Linux drivers/net/ethernet/realtek)
  ...
hal/          The minimal MMIO/DMA/IRQ trait surface a host OS provides
```

Each crate is `no_std`, takes its hardware access through the `hal` traits, and carries
its own tests where hardware behavior can be modeled.

## Status

Early. First fronts: SDHCI/eMMC error-recovery hardening, r8169.
