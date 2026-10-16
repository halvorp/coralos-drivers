// SPDX-License-Identifier: GPL-2.0-only
//! iwlwifi.device - the Intel Wireless-AC 9260 (PCI 8086:2526, "ThunderPeak2") of the Pixelbook Go (ATLAS) as a
//! ring-3 CoralOS `.device`.
//!
//! WHAT IT IS. The cores are PLANS and register programs (no effects): `iwlwifi-pcie-trans-core` powers the card,
//! `-nic-core` holds the NIC-access handshake, the interrupt masks and the firmware upload, `-ring-core` the RX and
//! host-command rings, `iwlwifi-fw-*-core` parse the ucode file and the ALIVE notification, `iwlwifi-nvm-parse-core`
//! the OTP, `iwlwifi-mvm-init-core` the INIT sequence as a state machine. This crate EXECUTES them over a
//! [`host::Host`] (BAR0, DMA memory, the MSI wait): [`transport`] (+ [`rxpath`], [`txpath`], [`irqloop`]) is the
//! gen1 PCIe transport, [`fwfile`] the verified firmware file, [`driver`] the INIT flow, [`scan`] the scan request
//! and the results it turns into [`event::Event`]s.
//!
//! MILESTONES, as built (see DECISIONS.md D-NEXT and the commit message for what is and is not proven):
//!  M1  power-up, firmware upload, ALIVE                  - [`driver::Driver::bring_up`] up to `Event::AliveOk`
//!  M2  NVM, antennas, PHY configuration, calibration     - the rest of `bring_up`
//!  M3  scan -> results shaped for wifi-manager           - [`rt_up`] restarts on the RT image (paging, post-ALIVE commands, MCC)
//!                                                          and [`scan`] runs one passive SCAN_REQ_UMAC; JOIN is not built
//!
//! THE WIRE TO wifi-manager. There is no wire crate here. wifi-manager.service speaks CoralOS's PRIVATE driver ABI
//! (`coral-wifi-abi::driver`), a closed crate; the closed wrapper (userland/coral-iwlwifi-driver) maps this crate's
//! neutral [`event::Event`] / [`event::Request`] onto it, field for field.
//!
//! THE FIRMWARE. Never embedded. The wrapper reads `System/Firmware/intel/iwlwifi-9260-th-b0-jf-b0-46.ucode` and hands
//! the bytes in with the digest the build baked; [`fwfile`] recomputes it and refuses a mismatch by name.
#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]

extern crate alloc;

pub mod driver;
pub mod event;
pub mod fwfile;
pub mod host;
pub mod irqloop;
#[cfg(target_arch = "x86_64")]
pub mod rt;
pub mod rt_up;
pub mod rxpath;
pub mod scan;
pub mod sha256;
pub mod transport;
pub mod txpath;

pub use driver::{Driver, DriverError, InitReport};
pub use host::Host;
