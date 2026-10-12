// SPDX-License-Identifier: GPL-2.0-only
//! The capability chain: `snd_hdac_bus_parse_capabilities` (controller.c:406-483) and the multi-link list
//! `snd_hdac_ext_bus_get_ml_capabilities` (ext/controller.c:80-116), as coroutines the `.device` answers with register reads.
//!
//! Beyond Linux, every way the walk can go wrong has a NAME: Linux breaks out of the loop silently when the chain is too long
//! (controller.c:466-469), when a header reads as all ones (:424-427), and on an unknown id (:459-461, which also ends the chain and
//! hides every capability after it); a looping chain, a zero start, a misaligned pointer, or a link list that cannot fit the BAR are
//! refused here.
use crate::consts::*;
use crate::error::HdaError;
use crate::regs::*;
use alloc::vec::Vec;
use avs_fw_core::effect::{Op, OnFail, Reg, Space, Step, Width};

fn hda(off: u32, width: Width) -> Reg {
    Reg { space: Space::Hda, off, width }
}

/// Offsets (in BAR0) of the capability blocks the walk found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub ml: Option<u32>,
    pub gts: Option<u32>,
    pub pp: Option<u32>,
    pub spb: Option<u32>,
    pub drsm: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkOut {
    /// Read this register and answer [`WalkIn::Value`].
    Read(Step),
    Done(Caps),
    Failed(HdaError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkIn {
    Begin,
    Value(u32),
}

/// Walk the chain from `LLCH`.
#[derive(Clone, Copy, Debug)]
pub struct CapWalk {
    caps: Caps,
    at: Option<u32>,
    counter: u32,
    started: bool,
}

impl CapWalk {
    pub const fn new() -> Self {
        Self { caps: Caps { ml: None, gts: None, pp: None, spb: None, drsm: None }, at: None, counter: 0, started: false }
    }

    pub fn next(&mut self, input: WalkIn) -> WalkOut {
        match (self.started, input) {
            (false, WalkIn::Begin) => {
                self.started = true;
                WalkOut::Read(Step::new("llch", Op::Read { reg: hda(AZX_REG_LLCH, Width::W16) }, OnFail::Abort)) // controller.c:412
            }
            (true, WalkIn::Value(v)) => match self.at {
                None => {
                    // the first answer is LLCH: the offset of the first capability header
                    if v == 0 || v & 3 != 0 {
                        return WalkOut::Failed(HdaError::BadCapabilityStart { offset: v });
                    }
                    self.at = Some(v);
                    self.read_header(v)
                }
                Some(off) => self.header(off, v),
            },
            _ => WalkOut::Failed(HdaError::BadCapabilityStart { offset: 0 }),
        }
    }

    fn read_header(&self, off: u32) -> WalkOut {
        WalkOut::Read(Step::new("capability-header", Op::Read { reg: hda(off, Width::D32) }, OnFail::Abort)) // controller.c:414
    }

    fn header(&mut self, off: u32, cur: u32) -> WalkOut {
        if cur == 0xFFFF_FFFF {
            return WalkOut::Failed(HdaError::CapabilityReadAllOnes { offset: off }); // controller.c:424-427
        }
        let id = (cur & AZX_CAP_HDR_ID_MASK) >> AZX_CAP_HDR_ID_OFF; // controller.c:429
        let slot = match id {
            AZX_ML_CAP_ID => &mut self.caps.ml,       // controller.c:430
            AZX_GTS_CAP_ID => &mut self.caps.gts,     // controller.c:435
            AZX_PP_CAP_ID => &mut self.caps.pp,       // controller.c:440
            AZX_SPB_CAP_ID => &mut self.caps.spb,     // controller.c:446
            AZX_DRSM_CAP_ID => &mut self.caps.drsm,   // controller.c:452
            _ => return WalkOut::Failed(HdaError::UnknownCapability { id, offset: off }),
        };
        *slot = Some(off); // a repeated id overwrites, as in Linux; a chain that loops is stopped by the counter below
        self.counter += 1; // controller.c:463
        // Linux: `if (counter > HDAC_MAX_CAPS) break;` after counting (controller.c:466): the 11th header is processed, then it stops
        if self.counter > HDAC_MAX_CAPS {
            return WalkOut::Failed(HdaError::TooManyCapabilities { read: self.counter });
        }
        let next = cur & AZX_CAP_HDR_NXT_PTR_MASK; // controller.c:472
        if next == 0 {
            return WalkOut::Done(self.caps);
        }
        if next & 3 != 0 {
            return WalkOut::Failed(HdaError::MisalignedCapability { offset: next });
        }
        self.at = Some(next);
        self.read_header(next)
    }
}

impl Default for CapWalk {
    fn default() -> Self {
        Self::new()
    }
}

/// One link of the multi-link capability (`struct hdac_ext_link`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub index: u32,
    /// BAR0 offset of this link's register block (`mlcap + AZX_ML_BASE + AZX_ML_INTERVAL * index`).
    pub base: u32,
    pub lcaps: u32,
    /// `FIELD_GET(AZX_ML_HDA_LCAP_SLCOUNT, lcaps) + 1` (ext/controller.c:101).
    pub slcount: u32,
    /// `AZX_REG_ML_LEPTR_ID` when the link is "alt" (ext/controller.c:103-106).
    pub id: Option<u32>,
}

impl Link {
    pub const fn alt(&self) -> bool {
        self.lcaps & AZX_ML_HDA_LCAP_ALT != 0
    }
    /// This is the SSP (I2S) link the AVS driver reads the controller count from (loader.c:701, pcm.c:379).
    pub const fn is_ssp(&self) -> bool {
        matches!(self.id, Some(AZX_REG_ML_LEPTR_ID_INTEL_SSP))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MlOut {
    Read(Step),
    Done(Vec<Link>),
    Failed(HdaError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MlSt {
    Count,
    Lcap(u32),
    Lsdiid(u32),
    Leptr(u32),
}

/// Probe every link: `link_count = readl(mlcap + MLCD) + 1`, then per link `LCAP`, `LSDIID` (16-bit) and, for an alt link, `LEPTR`.
#[derive(Clone, Debug)]
pub struct MlProbe {
    mlcap: u32,
    bar_len: u32,
    st: MlSt,
    count: u32,
    links: Vec<Link>,
    lcaps: u32,
    started: bool,
}

impl MlProbe {
    /// `mlcap` is the capability's BAR0 offset ([`Caps::ml`]); `bar_len` is the length of BAR0, which bounds how many links can exist.
    pub fn new(mlcap: u32, bar_len: u32) -> Self {
        Self { mlcap, bar_len, st: MlSt::Count, count: 0, links: Vec::new(), lcaps: 0, started: false }
    }

    fn rd(label: &'static str, off: u32, w: Width) -> MlOut {
        MlOut::Read(Step::new(label, Op::Read { reg: hda(off, w) }, OnFail::Abort))
    }

    pub fn link_base(mlcap: u32, index: u32) -> u32 {
        mlcap + AZX_ML_BASE + AZX_ML_INTERVAL * index // ext/controller.c:97-98
    }

    pub fn next(&mut self, input: WalkIn) -> MlOut {
        if !self.started {
            self.started = true;
            return Self::rd("ml-link-count", self.mlcap + AZX_REG_ML_MLCD, Width::D32); // ext/controller.c:87
        }
        let WalkIn::Value(v) = input else { return MlOut::Failed(HdaError::BadCapabilityStart { offset: self.mlcap }) };
        match self.st {
            MlSt::Count => {
                self.count = v.wrapping_add(1);
                // the whole link array must fit in BAR0
                let need = (self.count as u64) * AZX_ML_INTERVAL as u64 + AZX_ML_BASE as u64 + self.mlcap as u64;
                if self.count == 0 || need > self.bar_len as u64 {
                    return MlOut::Failed(HdaError::LinkWindowPastBar { links: self.count, mlcap: self.mlcap, bar_len: self.bar_len });
                }
                if self.links.try_reserve_exact(self.count as usize).is_err() {
                    return MlOut::Failed(HdaError::OutOfMemory { what: "multi-link list" });
                }
                self.st = MlSt::Lcap(0);
                Self::rd("ml-lcap", Self::link_base(self.mlcap, 0) + AZX_REG_ML_LCAP, Width::D32)
            }
            MlSt::Lcap(i) => {
                self.lcaps = v;
                self.st = MlSt::Lsdiid(i);
                Self::rd("ml-lsdiid", Self::link_base(self.mlcap, i) + AZX_REG_ML_LSDIID, Width::W16) // ext/controller.c:100
            }
            MlSt::Lsdiid(i) => {
                if self.lcaps & AZX_ML_HDA_LCAP_ALT != 0 {
                    self.st = MlSt::Leptr(i);
                    return Self::rd("ml-leptr", Self::link_base(self.mlcap, i) + AZX_REG_ML_LEPTR, Width::D32); // ext/controller.c:104
                }
                self.push(i, None)
            }
            MlSt::Leptr(i) => {
                let id = (v & AZX_REG_ML_LEPTR_ID) >> AZX_REG_ML_LEPTR_ID_SHIFT; // FIELD_GET(AZX_REG_ML_LEPTR_ID, leptr)
                self.push(i, Some(id))
            }
        }
    }

    fn push(&mut self, i: u32, id: Option<u32>) -> MlOut {
        let slcount = ((self.lcaps & AZX_ML_HDA_LCAP_SLCOUNT) >> 20) + 1; // FIELD_GET(GENMASK(22, 20)) + 1, ext/controller.c:101
        self.links.push(Link { index: i, base: Self::link_base(self.mlcap, i), lcaps: self.lcaps, slcount, id });
        if i + 1 == self.count {
            return MlOut::Done(core::mem::take(&mut self.links));
        }
        self.st = MlSt::Lcap(i + 1);
        Self::rd("ml-lcap", Self::link_base(self.mlcap, i + 1) + AZX_REG_ML_LCAP, Width::D32)
    }
}
