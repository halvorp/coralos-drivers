// SPDX-License-Identifier: GPL-2.0-only
//! Can the cAVS interrupt reach this task? Read from the function's PCI configuration (read-only: the configuration authority owns the
//! writes), and the answer that decides whether the pump BLOCKS on the line or LOOKS at the causes on a bounded clock.
//!
//! WHY THIS EXISTS (Atlas, 2026-10-29, stick "golden maple glade" and the boot before it): the kernel allocates the cAVS function an MSI
//! route (`[irq-route] object=5 ... vector=0x42`) but nothing programs the function's MSI capability - the kernel writes no config
//! (coralos `kernel/src/arch/x86_64/audio_atlas.rs`: "it does not ... program the cAVS function's MSI capability ... a NAMED GAP"),
//! and the configuration authority serves only net/xhci/blk/gpu/wlan. The controller then signals on INTx, which nobody routes. The first
//! wait that needs the interrupt - the code loader's completion of the base firmware (`cldma_memcpy_work`'s
//! `wait_for_completion_timeout`, cldma.c:100, `AVS_CL_IOC_TIMEOUT_MS` 300 ms) - timed out: `[AVS-DEV FAIL step=cldma-ioc stage=base-firmware]`. The DSP never booted, so no
//! pipeline, no SSP clock and no amplifier enable ever followed, and the speakers stayed silent.
//!
//! Linux always has the interrupt (`avs_hdac_acquire_irq`, core.c:343-377: `pci_alloc_irq_vectors(pci, 1, 1, PCI_IRQ_MSI | PCI_IRQ_INTX)`).
//! This port cannot assume it, so it MEASURES it: when the MSI capability is present and not enabled, or no line was granted, the device
//! polls the same causes the interrupt handler reads (`ADSPIS`, the code loader's `SD_STS`, `INTSTS`) - the same shape as iwlwifi.device's
//! "No MSI route was granted: poll" and pciconfig's `mode=poll`. A wait that times out in line mode looks once more; a cause found pending
//! then is an interrupt that never arrived, and the device switches to polling for the rest of its life ([`crate::pump`]).
use crate::machine::{Machine, RegFault};
use crate::rig::Rig;
use avs_fw_core::effect::Space;
use pci_config_core::capability::find_capability;

// The MSI capability layout has one home: pci-config-core (re-exported for this device's tests).
pub use pci_config_core::msi::{
    PCI_CAP_ID_MSI, PCI_MSI_ADDRESS_LO, PCI_MSI_DATA_32, PCI_MSI_DATA_64, PCI_MSI_FLAGS, PCI_MSI_FLAGS_64BIT, PCI_MSI_FLAGS_ENABLE,
};

/// The standard configuration header plus the capability area: every capability pointer lands below 0x100.
pub const CONFIG_BYTES: usize = 256;

/// How often the pump looks at the causes while it waits in poll mode (a code-loader chunk, an IPC reply).
pub const WAIT_POLL_MS: u64 = 1;
/// How often a running stream is looked at in poll mode. A period is 1024 frames at 48 kHz (21 ms) and the ring is four of them.
pub const STREAM_POLL_MS: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollWhy {
    /// The function has an MSI capability and it is not enabled: the interrupt goes out as INTx, which this task does not hold.
    MsiDisabled,
    /// The launch row granted no interrupt line (or the kernel refused the wait on it).
    NoLine,
}

impl PollWhy {
    pub const fn name(&self) -> &'static str {
        match self {
            PollWhy::MsiDisabled => "msi-disabled",
            PollWhy::NoLine => "no-line",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqMode {
    /// Block on the interrupt.
    Line,
    Poll(PollWhy),
}

/// What the configuration header says about the function's interrupt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqProbe {
    pub command: u16,
    pub status: u16,
    /// The MSI capability's offset, if the list has one.
    pub msi_cap: Option<u8>,
    pub msi_flags: u16,
    pub msi_addr_lo: u32,
    pub msi_data: u16,
    /// The task holds an interrupt line it may wait on.
    pub line_granted: bool,
}

impl IrqProbe {
    /// Decode a configuration image (`cfg`, at least [`CONFIG_BYTES`]).
    pub fn decode(cfg: &[u8], line_granted: bool) -> Self {
        let u16_at = |o: usize| cfg.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| cfg.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let mut p = IrqProbe { command: u16_at(0x04), status: u16_at(0x06), line_granted, ..Default::default() };
        // a malformed list is "no capability found": the mode then follows the line alone
        if let Ok(Some(c)) = find_capability(cfg, PCI_CAP_ID_MSI) {
            let at = c.offset as usize;
            p.msi_cap = Some(c.offset);
            p.msi_flags = u16_at(at + PCI_MSI_FLAGS);
            p.msi_addr_lo = u32_at(at + PCI_MSI_ADDRESS_LO);
            p.msi_data = u16_at(at + if p.msi_flags & PCI_MSI_FLAGS_64BIT != 0 { PCI_MSI_DATA_64 } else { PCI_MSI_DATA_32 });
        }
        p
    }

    /// Block on the line only when the line exists and the function can signal on it.
    pub fn mode(&self) -> IrqMode {
        if !self.line_granted {
            return IrqMode::Poll(PollWhy::NoLine);
        }
        match self.msi_cap {
            Some(_) if self.msi_flags & PCI_MSI_FLAGS_ENABLE == 0 => IrqMode::Poll(PollWhy::MsiDisabled),
            _ => IrqMode::Line,
        }
    }
}

/// Read the configuration header and capability area (dword reads; the window is the function's config page). A window shorter than
/// [`CONFIG_BYTES`] is read as far as it goes; the rest reads as zero (a capability pointer into it then ends the walk, `mode` follows the line).
pub fn read_probe<M: Machine>(rig: &mut Rig<M>) -> Result<IrqProbe, RegFault> {
    let mut cfg = [0u8; CONFIG_BYTES];
    let n = (rig.m.window_len(Space::PciConfig) as usize).min(CONFIG_BYTES) / 4;
    for i in 0..n {
        let v = rig.read32(Space::PciConfig, (i * 4) as u32)?;
        cfg[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    Ok(IrqProbe::decode(&cfg, rig.m.avs_irq_granted()))
}

/// Probe, decide, and set the rig's mode. A mode already forced to polling (a lost interrupt was found) stays polling.
pub fn settle<M: Machine>(rig: &mut Rig<M>) -> Result<(IrqProbe, IrqMode), RegFault> {
    let p = read_probe(rig)?;
    let mode = p.mode();
    if let IrqMode::Poll(_) = mode {
        rig.irq_poll = true;
    }
    Ok((p, mode))
}

/// The registers that say why a wait for the cAVS interrupt ended without it, read (never written) and put on ONE ring line, at the
/// moment a wait times out and before any clean-up touches them:
/// - `INTCTL`/`INTSTS` (BAR0 0x20/0x24, hda_register.h:36-37): are the controller's enables on, and does it think it is interrupting?
/// - `ADSPCS`/`ADSPIC`/`ADSPIS` (BAR4 0x04/0x08/0x0C, registers.h:25-27): DSP core state, which DSP causes are enabled, which are pending;
/// - the code loader's `SD_CTL`+`SD_STS` dword, `LPIB` and `SPIB` (BAR4 0x80/0x84/0xA8, cldma.c:15-24): did the transfer run, and finish?
/// - the firmware's status and error words (SRAM base, registers.h:116-117): where the ROM / firmware got to;
/// - PCI `COMMAND`/`STATUS` and the MSI flags: `STATUS` bit 3 (pci_regs.h:55) set with MSI off is the controller asserting INTx nobody holds.
///
/// A cause pending in `ADSPIS` with `sd_sts` showing completion means the hardware did its part and only the delivery failed; an idle
/// `lpib` means the DMA never moved.
pub fn snapshot<M: Machine>(rig: &mut Rig<M>, step: &str) {
    use avs_fw_core::regs::{cl_sd, AVS_ADSP_REG_ADSPCS, AVS_ADSP_REG_ADSPIC, AVS_ADSP_REG_ADSPIS, AVS_FW_REG_ERROR, AVS_FW_REG_STATUS, AZX_REG_CL_SD_SPIB, AZX_REG_SD_CTL};
    use hda_ext_core::regs::{AZX_REG_INTCTL, AZX_REG_INTSTS, AZX_REG_SD_LPIB};
    let mut r = |space: Space, off: u32| rig.read32(space, off).unwrap_or(0xdead_dead);
    let intctl = r(Space::Hda, AZX_REG_INTCTL);
    let intsts = r(Space::Hda, AZX_REG_INTSTS);
    let adspcs = r(Space::Dsp, AVS_ADSP_REG_ADSPCS);
    let adspic = r(Space::Dsp, AVS_ADSP_REG_ADSPIC);
    let adspis = r(Space::Dsp, AVS_ADSP_REG_ADSPIS);
    let cl_ctlsts = r(Space::Dsp, cl_sd(AZX_REG_SD_CTL));
    let cl_lpib = r(Space::Dsp, cl_sd(AZX_REG_SD_LPIB));
    let cl_spib = r(Space::Dsp, cl_sd(AZX_REG_CL_SD_SPIB));
    let fw_sts = r(Space::Dsp, AVS_FW_REG_STATUS);
    let fw_err = r(Space::Dsp, AVS_FW_REG_ERROR);
    let cmdsts = r(Space::PciConfig, 0x04);
    let msi = read_probe(rig).map(|p| p.msi_flags as u32).unwrap_or(0xdead);
    crate::log::emit(
        &mut rig.m,
        format_args!(
            "[AVS-DEV IRQ-SNAPSHOT step={} intctl={:#x} intsts={:#x} adspcs={:#x} adspic={:#x} adspis={:#x} cl_sd={:#x} cl_lpib={:#x} cl_spib={:#x} fw_sts={:#x} fw_err={:#x} pci={:#x} msi={:#x} poll={}]",
            step, intctl, intsts, adspcs, adspic, adspis, cl_ctlsts, cl_lpib, cl_spib, fw_sts, fw_err, cmdsts, msi, rig.irq_poll
        ),
    );
}
