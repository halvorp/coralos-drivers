// SPDX-License-Identifier: GPL-2.0-only
//! The write allow-lists: what avs.device may change, written down register by register, in the spirit of i915.device's.
//!
//! * **cAVS BAR4 (the DSP)** and **BAR0 (the HD-audio controller)**: the registers the plans of avs-fw-core, avs-ipc-core, hda-ext-core
//!   and avs-boot-core write, each with the width it is written at, plus the downlink mailbox window (the IPC payload). The BAR0 list
//!   depends on the controller (how many streams it states, where its capability blocks are) and is built once discovery has read that;
//!   only the PLAYBACK stream's descriptor beyond its control and status bytes is writable.
//! * **PCI configuration**: `PGCTL` and `CGCTL` (power and clock gating, `avs-fw-core::programs::gating`) - two dwords.
//! * **I2C**: the codec's address and no other ([`I2cAllow`]). The speaker amplifiers' addresses are refused by NAME, whoever asks.
//! * **GPIO pads**: the speaker amplifiers' reset pad `PADCFG0`, to hold them in reset, and nothing else ([`PadAllow`]).
//!
//! The lists are the in-code half of the fence. The loader-facing half is the window grant, and the production machine builds the
//! grant's `WriteRange`s FROM these lists ([`Allow::window_ranges`]), so the two cannot disagree. `tests/allow.rs` RUNS every plan and
//! requires set equality: a register no plan writes cannot be granted, and a plan write that is not granted fails there, on the host,
//! before it can fail on silicon.
use crate::machine::{PadReg, RegFault};
use alloc::vec::Vec;
use avs_fw_core::effect::{Reg, Space, Width};
use avs_fw_core::regs as fw;
use avs_ipc_core::regs::{AVS_MAILBOX_SIZE, DOWNLINK_OFFSET, SKL_ADSP_REG_HIPCI, SKL_ADSP_REG_HIPCIE, SKL_ADSP_REG_HIPCT};
use hda_ext_core::regs as hda;
use hda_ext_core::stream::{HostStream, Streams};

/// One writable register: its offset in its window and the width the plans write it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub off: u32,
    pub width: Width,
}

/// A writable window of bytes (the IPC downlink payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Win {
    pub off: u32,
    pub len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowError {
    OutOfMemory,
}

/// What discovery learned about the controller that decides its write list.
#[derive(Clone, Debug)]
pub struct HdaLayout {
    pub streams: Streams,
    pub playback: HostStream,
    pub pp: Option<u32>,
    pub spb: Option<u32>,
    /// BAR0 offsets of the multi-link blocks (`Link::base`).
    pub link_bases: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Allow {
    dsp: Vec<Entry>,
    dsp_windows: Vec<Win>,
    hda: Vec<Entry>,
    cfg: Vec<Entry>,
}

fn e(off: u32, width: Width) -> Entry {
    Entry { off, width }
}

fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), AllowError> {
    v.try_reserve(1).map_err(|_| AllowError::OutOfMemory)?;
    v.push(x);
    Ok(())
}

/// The BAR4 registers (offsets are in the DSP window, not the HDA one).
fn dsp_entries(out: &mut Vec<Entry>) -> Result<(), AllowError> {
    use Width::{B8, D32};
    for x in [
        e(fw::AVS_ADSP_REG_ADSPCS, D32),                    // core power / reset / stall (programs::core_*)
        e(fw::AVS_ADSP_REG_ADSPIC, D32),                    // IPC and code-loader interrupt enables
        e(SKL_ADSP_REG_HIPCT, D32),                     // the ISR's "I accepted your message" (HIPCT BUSY, write 1 to clear)
        e(SKL_ADSP_REG_HIPCI, D32),                     // the request's primary dword: writing it with BUSY starts the DSP
        e(SKL_ADSP_REG_HIPCIE, D32),                    // the request's extension dword, and the ISR's ack of DONE
        e(fw::SKL_ADSP_REG_HIPCCTL, D32),                   // HIPC DONE / BUSY interrupt enables
        e(fw::cl_sd(fw::AZX_REG_SD_CTL), B8),                   // the code loader's stream: control (byte accesses)
        e(fw::cl_sd(fw::AZX_REG_SD_CTL), D32),                  //   ... and the stream tag (dword)
        e(fw::cl_sd(fw::AZX_REG_SD_CBL), D32),
        e(fw::cl_sd(fw::AZX_REG_SD_LVI), B8),
        e(fw::cl_sd(fw::AZX_REG_SD_BDLPL), D32),
        e(fw::cl_sd(fw::AZX_REG_SD_BDLPU), D32),
        e(fw::cl_sd(fw::AZX_REG_CL_SPBFCTL), D32),
        e(fw::cl_sd(fw::AZX_REG_CL_SD_SPIB), D32),
    ] {
        push(out, x)?;
    }
    Ok(())
}

/// The registers every stream of the controller has written (not just the playback one): the clear-all-interrupts and
/// disable-all-interrupts walks of `azx_int_clear` / `azx_int_disable` touch each stream's status and interrupt-enable bytes.
fn each_stream(streams: &Streams, out: &mut Vec<Entry>) -> Result<(), AllowError> {
    for i in 0..streams.total() {
        let sd = sd_base(i);
        push(out, e(sd + hda::AZX_REG_SD_CTL, Width::B8))?;
        push(out, e(sd + hda::AZX_REG_SD_STS, Width::B8))?;
    }
    Ok(())
}

const fn sd_base(index: u32) -> u32 {
    hda_ext_core::consts::SD_BASE + hda_ext_core::consts::SD_INTERVAL * index
}

impl Allow {
    /// Before discovery: nothing is writable in BAR0, and only the static DSP and configuration lists exist.
    pub fn read_only() -> Self {
        Self { dsp: Vec::new(), dsp_windows: Vec::new(), hda: Vec::new(), cfg: Vec::new() }
    }

    /// The write lists once discovery has read the controller's geometry.
    pub fn new(l: &HdaLayout) -> Result<Self, AllowError> {
        use Width::{B8, D32, W16};
        let mut dsp = Vec::new();
        dsp_entries(&mut dsp)?;
        let mut dsp_windows = Vec::new();
        push(&mut dsp_windows, Win { off: DOWNLINK_OFFSET, len: AVS_MAILBOX_SIZE as u32 })?;
        let mut cfg = Vec::new();
        push(&mut cfg, e(fw::AZX_PCIREG_PGCTL, D32))?;
        push(&mut cfg, e(fw::AZX_PCIREG_CGCTL, D32))?;

        let mut hda = Vec::new();
        for x in [
            e(hda::AZX_REG_GCTL, B8),         // link reset (byte exit, dword enter)
            e(hda::AZX_REG_GCTL, D32),
            e(hda::AZX_REG_STATESTS, W16),    // codec wake status, cleared by init
            e(hda::AZX_REG_INTCTL, D32),      // controller, global and per-stream interrupt enables
            e(hda::AZX_REG_INTSTS, D32),      // clear
            e(hda::AZX_REG_RIRBSTS, B8),      // cleared by azx_int_clear (the RIRB itself is never used)
            e(hda::AZX_REG_VS_EM2, D32),      // L1SEN and the DUM bit
        ] {
            push(&mut hda, x)?;
        }
        each_stream(&l.streams, &mut hda)?;
        let sd = sd_base(l.playback.index);
        for x in [
            e(sd + hda::AZX_REG_SD_CTL, D32),
            e(sd + hda::AZX_REG_SD_CBL, D32),
            e(sd + hda::AZX_REG_SD_LVI, W16),
            e(sd + hda::AZX_REG_SD_FORMAT, W16),
            e(sd + hda::AZX_REG_SD_BDLPL, D32),
            e(sd + hda::AZX_REG_SD_BDLPU, D32),
        ] {
            push(&mut hda, x)?;
        }
        if let Some(pp) = l.pp {
            push(&mut hda, e(pp + hda::AZX_REG_PP_PPCTL, D32))?;
        }
        if let Some(spb) = l.spb {
            push(&mut hda, e(spb + hda::AZX_REG_SPB_SPBFCCTL, D32))?;
            push(&mut hda, e(spb + hda::AZX_SPB_BASE + hda::AZX_SPB_INTERVAL * l.playback.index + hda::AZX_SPB_SPIB, D32))?;
        }
        for b in &l.link_bases {
            push(&mut hda, e(b + hda::AZX_REG_ML_LOSIDV, D32))?;
        }
        Ok(Self { dsp, dsp_windows, hda, cfg })
    }

    fn entries(&self, space: Space) -> &[Entry] {
        match space {
            Space::Dsp => &self.dsp,
            Space::Hda => &self.hda,
            Space::PciConfig => &self.cfg,
        }
    }

    /// May a write of `width` at `off` in `space` happen?
    pub fn permits(&self, space: Space, off: u32, width: Width) -> bool {
        if self.entries(space).iter().any(|x| x.off == off && x.width == width) {
            return true;
        }
        space == Space::Dsp && width == Width::D32 && off.is_multiple_of(4) && self.dsp_windows.iter().any(|w| off >= w.off && off + 4 <= w.off + w.len)
    }

    pub fn check(&self, reg: Reg) -> Result<(), RegFault> {
        if self.permits(reg.space, reg.off, reg.width) {
            Ok(())
        } else {
            Err(RegFault::NotAllowed { space: reg.space, off: reg.off })
        }
    }

    /// The exact registers of one space (for the equality test and the launch-row report).
    pub fn registers(&self, space: Space) -> &[Entry] {
        self.entries(space)
    }
    pub fn windows(&self) -> &[Win] {
        &self.dsp_windows
    }

    /// The dword ranges a loader grant must make writable for `space`: every allowed register's containing dword, merged, and the
    /// mailbox window. A byte or halfword register costs its whole dword at the window; THIS list is the finer fence.
    pub fn window_ranges(&self, space: Space) -> Result<Vec<(u32, u32)>, AllowError> {
        let mut r: Vec<(u32, u32)> = Vec::new();
        for x in self.entries(space) {
            let d = x.off & !3;
            if !r.iter().any(|(o, l)| d >= *o && d + 4 <= o + l) {
                push(&mut r, (d, 4))?;
            }
        }
        if space == Space::Dsp {
            for w in &self.dsp_windows {
                push(&mut r, (w.off, w.len))?;
            }
        }
        r.sort_unstable();
        // merge neighbours
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (o, l) in r {
            match merged.last_mut() {
                Some((mo, ml)) if o <= *mo + *ml => {
                    let end = (o + l).max(*mo + *ml);
                    *ml = end - *mo;
                }
                _ => push(&mut merged, (o, l))?,
            }
        }
        Ok(merged)
    }
}

/// The I2C addresses the device may transfer to: the codec's, and no other. The speaker amplifiers' are refused by name even if the
/// codec's address were configured equal to one (the board record is refused at load in that case, [`crate::board::Board::check`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cAllow {
    codec: Option<u16>,
    amps: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cRefusal {
    SpeakerAmplifier { addr: u16 },
    NotTheCodec { addr: u16 },
}

impl I2cAllow {
    /// No board yet: every address is refused.
    pub const fn none() -> Self {
        Self { codec: None, amps: Vec::new() }
    }
    pub fn new(codec: u16, amps: &[u16]) -> Result<Self, AllowError> {
        let mut a = Vec::new();
        a.try_reserve_exact(amps.len()).map_err(|_| AllowError::OutOfMemory)?;
        a.extend_from_slice(amps);
        Ok(Self { codec: Some(codec), amps: a })
    }
    pub fn codec(&self) -> Option<u16> {
        self.codec
    }
    pub fn check(&self, addr: u16) -> Result<(), I2cRefusal> {
        if self.amps.contains(&addr) {
            return Err(I2cRefusal::SpeakerAmplifier { addr });
        }
        if Some(addr) != self.codec {
            return Err(I2cRefusal::NotTheCodec { addr });
        }
        Ok(())
    }
}

/// The pad registers the device may WRITE: the speaker-reset pad's `PADCFG0`. Everything else in a pad window is read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadAllow {
    /// Resolved once the pad window's `PADBAR` has been read; `None` refuses every write.
    pub spkr_rst: Option<PadReg>,
}

impl PadAllow {
    pub const fn none() -> Self {
        Self { spkr_rst: None }
    }
    pub fn permits_write(&self, reg: PadReg) -> bool {
        self.spkr_rst == Some(reg)
    }
}

/// The DesignWare I2C controller's WRITABLE registers (the transfer in [`crate::i2c`] writes these and no others; the interrupt clears are
/// READS of `CLR_*`, which a read-only grant allows). Offsets from `i2c-designware-core::regs::off`.
pub const I2C_WRITE_REGS: [u32; 8] = [
    i2c_designware_core::regs::off::CON,
    i2c_designware_core::regs::off::TAR,
    i2c_designware_core::regs::off::DATA_CMD,
    i2c_designware_core::regs::off::FS_SCL_HCNT,
    i2c_designware_core::regs::off::FS_SCL_LCNT,
    i2c_designware_core::regs::off::INTR_MASK,
    i2c_designware_core::regs::off::ENABLE,
    i2c_designware_core::regs::off::SDA_HOLD,
];
