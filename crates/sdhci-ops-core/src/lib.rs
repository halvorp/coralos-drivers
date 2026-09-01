// SPDX-License-Identifier: GPL-2.0-only
//! Pure SDHCI command and transfer setup.
//!
//! Ported mechanically from Linux:
//!   * `drivers/mmc/host/sdhci.c` — command flags (`sdhci_send_command`, :1709-:1725), block setup
//!     (`sdhci_set_block_info`, :1097-:1115), transfer mode (`sdhci_set_transfer_mode`,
//!     :1460-:1498), and timeout divisor (`sdhci_calc_timeout`, :969-:1025)
//!   * `drivers/mmc/host/sdhci.h` — field literals used by those operations
//!   * `include/linux/mmc/core.h`, `include/linux/mmc/mmc.h`, and `include/linux/mmc/sdio.h` — MMC
//!     input flags and opcodes consumed by `sdhci.c`
//!
//! Original copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved. Linux SDHCI authors and
//! JMicron are also acknowledged by the source driver.
//!
//! This crate performs no MMIO: callers provide descriptions and receive register words.

#![no_std]
#![forbid(unsafe_code)]

pub mod block;
pub mod command;
pub mod timeout;
pub mod transfer;
