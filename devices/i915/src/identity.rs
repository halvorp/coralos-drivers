// SPDX-License-Identifier: GPL-2.0-only
//! PCI identity from the configuration page: who is this function, and is it one this device serves?
//!
//! The header layout is PCI's (vendor id at 0x00, device id at 0x02, revision id at 0x08, class code at 0x09-0x0b,
//! subsystem ids at 0x2c/0x2e); the table is Linux's (see i915-kbl-display-core `platform`).
use crate::regio::{RegRead, CFG_HEADER_BYTES};
use coralos_rt::mmio::MmioError;
use i915_kbl_display_core::platform::{identify, KblDevice, PlatformError};

/// PCI base class 0x03: display controller. Linux binds i915 with `INTEL_VGA_DEVICE`, which carries
/// `.class = PCI_BASE_CLASS_DISPLAY << 16, .class_mask = 0xff << 16` (include/drm/intel/pciids.h:34-38).
pub const PCI_BASE_CLASS_DISPLAY: u8 = 0x03;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciIdentity {
    pub vendor: u16,
    pub device: u16,
    pub revision: u8,
    pub base_class: u8,
    pub sub_class: u8,
    pub subsystem_vendor: u16,
    pub subsystem_device: u16,
}

/// A named reason the device will not drive this function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The configuration page could not be read.
    ConfigUnreadable(MmioError),
    /// The page reads all-ones in the vendor id: nothing answers (a dead link or a function that is not there).
    ConfigAllOnes,
    /// Not a display controller (`INTEL_VGA_DEVICE`'s class test).
    NotDisplayClass { base_class: u8 },
    /// Not a Kaby Lake id; see the inner reason.
    Platform(PlatformError),
}

impl Refusal {
    pub fn name(&self) -> &'static str {
        match self {
            Refusal::ConfigUnreadable(_) => "config-unreadable",
            Refusal::ConfigAllOnes => "config-all-ones",
            Refusal::NotDisplayClass { .. } => "not-display-class",
            Refusal::Platform(p) => p.name(),
        }
    }
}

/// Read the 64-byte header's identity fields.
pub fn read_identity<C: RegRead>(cfg: &mut C) -> Result<PciIdentity, Refusal> {
    if cfg.len() < CFG_HEADER_BYTES {
        return Err(Refusal::ConfigUnreadable(MmioError::TooShort { slot: crate::regio::CFG_SLOT, len: cfg.len(), need: CFG_HEADER_BYTES }));
    }
    let mut rd = |off: u32| cfg.read32(off).map_err(Refusal::ConfigUnreadable);
    let id = rd(0x00)?; // device id << 16 | vendor id
    if id & 0xffff == 0xffff {
        return Err(Refusal::ConfigAllOnes);
    }
    let class_rev = rd(0x08)?; // class code << 8 | revision id
    let subsys = rd(0x2c)?; // subsystem id << 16 | subsystem vendor id
    Ok(PciIdentity {
        vendor: id as u16,
        device: (id >> 16) as u16,
        revision: class_rev as u8,
        base_class: (class_rev >> 24) as u8,
        sub_class: (class_rev >> 16) as u8,
        subsystem_vendor: subsys as u16,
        subsystem_device: (subsys >> 16) as u16,
    })
}

/// The identity check the device makes before it touches BAR0.
pub fn check(id: &PciIdentity) -> Result<KblDevice, Refusal> {
    let dev = identify(id.vendor, id.device, id.revision).map_err(Refusal::Platform)?;
    if id.base_class != PCI_BASE_CLASS_DISPLAY {
        return Err(Refusal::NotDisplayClass { base_class: id.base_class });
    }
    Ok(dev)
}
