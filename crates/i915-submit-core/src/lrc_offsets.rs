// SPDX-License-Identifier: GPL-2.0-only
//! The per-platform register-state offset tables of gt/intel_lrc.c (`gen8_xcs_offsets`, `gen9_xcs_offsets`,
//! `gen8_rcs_offsets`, `gen9_rcs_offsets`, :108-413) as DATA, the decoder `set_offsets` (:49-106), `reg_offsets` (:643-677)
//! and the per-generation slot locators `lrc_ring_*` (:679-770). Linux packs each table into bytes (`NOP`, `LRI`, `REG`,
//! `REG16`); this port keeps the same tokens as an enum so the table reads like the original, and the test decodes both and
//! compares the resulting context-image dwords with what Linux's own `set_offsets` produces (evaluated by GCC).
//!
//! The Gen11, Gen12, DG2 and MTL tables are not carried: the target is Gen9.
use crate::error::SubmitError;
use crate::lrc_reg::GEN9_CTX_RING_MI_MODE;
use i915_gpu_commands_core::{mi_load_register_imm, MI_BATCH_BUFFER_END, MI_LRI_FORCE_POSTED, MI_LRI_LRM_CS_MMIO};

/// One token of a register-state table (intel_lrc.c:50-100).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// `NOP(x)`: skip `x` dwords of the image.
    Nop(u8),
    /// `LRI(count, flags)`: an `MI_LOAD_REGISTER_IMM(count)` header, `POSTED` adds `MI_LRI_FORCE_POSTED`; the next `count`
    /// items are [`Item::Reg`].
    Lri { count: u8, posted: bool },
    /// `REG(x)` / `REG16(x)`: a register offset relative to the engine's mmio base (Linux picks the one- or two-byte
    /// encoding from the magnitude; the value is the same).
    Reg(u16),
}

const fn nop(x: u8) -> Item {
    Item::Nop(x)
}
const fn lri(count: u8, posted: bool) -> Item {
    Item::Lri { count, posted }
}
const fn reg(x: u16) -> Item {
    Item::Reg(x)
}

/// `gen8_xcs_offsets` (intel_lrc.c:108-141).
pub static GEN8_XCS_OFFSETS: [Item; 28] = [
    nop(1), lri(11, false), reg(0x244), reg(0x034), reg(0x030), reg(0x038), reg(0x03c), reg(0x168), reg(0x140), reg(0x110),
    reg(0x11c), reg(0x114), reg(0x118), nop(9), lri(9, false), reg(0x3a8), reg(0x28c), reg(0x288), reg(0x284), reg(0x280),
    reg(0x27c), reg(0x278), reg(0x274), reg(0x270), nop(13), lri(2, false), reg(0x200), reg(0x028),
];

/// `gen9_xcs_offsets` (intel_lrc.c:143-225).
pub static GEN9_XCS_OFFSETS: [Item; 76] = [
    nop(1), lri(14, true), reg(0x244), reg(0x034), reg(0x030), reg(0x038), reg(0x03c), reg(0x168), reg(0x140), reg(0x110),
    reg(0x11c), reg(0x114), reg(0x118), reg(0x1c0), reg(0x1c4), reg(0x1c8), nop(3), lri(9, true), reg(0x3a8), reg(0x28c),
    reg(0x288), reg(0x284), reg(0x280), reg(0x27c), reg(0x278), reg(0x274), reg(0x270), nop(13), lri(1, true), reg(0x200),
    nop(13), lri(44, true), reg(0x028), reg(0x09c), reg(0x0c0), reg(0x178), reg(0x17c), reg(0x358), reg(0x170), reg(0x150),
    reg(0x154), reg(0x158), reg(0x41c), reg(0x600), reg(0x604), reg(0x608), reg(0x60c), reg(0x610), reg(0x614), reg(0x618),
    reg(0x61c), reg(0x620), reg(0x624), reg(0x628), reg(0x62c), reg(0x630), reg(0x634), reg(0x638), reg(0x63c), reg(0x640),
    reg(0x644), reg(0x648), reg(0x64c), reg(0x650), reg(0x654), reg(0x658), reg(0x65c), reg(0x660), reg(0x664), reg(0x668),
    reg(0x66c), reg(0x670), reg(0x674), reg(0x678), reg(0x67c), reg(0x068),
];

/// `gen8_rcs_offsets` (intel_lrc.c:293-328).
pub static GEN8_RCS_OFFSETS: [Item; 30] = [
    nop(1), lri(14, true), reg(0x244), reg(0x034), reg(0x030), reg(0x038), reg(0x03c), reg(0x168), reg(0x140), reg(0x110),
    reg(0x11c), reg(0x114), reg(0x118), reg(0x1c0), reg(0x1c4), reg(0x1c8), nop(3), lri(9, true), reg(0x3a8), reg(0x28c),
    reg(0x288), reg(0x284), reg(0x280), reg(0x27c), reg(0x278), reg(0x274), reg(0x270), nop(13), lri(1, false), reg(0x0c8),
];

/// `gen9_rcs_offsets` (intel_lrc.c:330-412).
pub static GEN9_RCS_OFFSETS: [Item; 76] = [
    nop(1), lri(14, true), reg(0x244), reg(0x34), reg(0x30), reg(0x38), reg(0x3c), reg(0x168), reg(0x140), reg(0x110),
    reg(0x11c), reg(0x114), reg(0x118), reg(0x1c0), reg(0x1c4), reg(0x1c8), nop(3), lri(9, true), reg(0x3a8), reg(0x28c),
    reg(0x288), reg(0x284), reg(0x280), reg(0x27c), reg(0x278), reg(0x274), reg(0x270), nop(13), lri(1, false), reg(0xc8),
    nop(13), lri(44, true), reg(0x28), reg(0x9c), reg(0xc0), reg(0x178), reg(0x17c), reg(0x358), reg(0x170), reg(0x150),
    reg(0x154), reg(0x158), reg(0x41c), reg(0x600), reg(0x604), reg(0x608), reg(0x60c), reg(0x610), reg(0x614), reg(0x618),
    reg(0x61c), reg(0x620), reg(0x624), reg(0x628), reg(0x62c), reg(0x630), reg(0x634), reg(0x638), reg(0x63c), reg(0x640),
    reg(0x644), reg(0x648), reg(0x64c), reg(0x650), reg(0x654), reg(0x658), reg(0x65c), reg(0x660), reg(0x664), reg(0x668),
    reg(0x66c), reg(0x670), reg(0x674), reg(0x678), reg(0x67c), reg(0x68),
];

/// `set_offsets` (intel_lrc.c:49-106): write the `MI_LOAD_REGISTER_IMM` headers and register addresses of `items` into the
/// state page `regs`, relative to `mmio_base`; values are not filled. `close` ends the batch with `MI_BATCH_BUFFER_END`
/// (used when the whole page is freshly zeroed). `gen` >= 11 would also set `MI_LRI_LRM_CS_MMIO` (:80-81) and the close bit.
pub fn set_offsets(regs: &mut [u32], items: &[Item], name: &'static str, mmio_base: u32, gen: u8, close: bool) -> Result<(), SubmitError> {
    let limit = regs.len() as u32;
    let mut at: usize = 0;
    let mut i = 0;
    while i < items.len() {
        match items[i] {
            Item::Nop(n) => {
                at += n as usize; // intel_lrc.c:66-70
                i += 1;
            }
            Item::Lri { count, posted } => {
                if count == 0 || count >= 0x40 {
                    return Err(SubmitError::LrcTableMalformed { table: name, at: i as u32 }); // BUILD_BUG_ON_ZERO(count >= BIT(6)), GEM_BUG_ON(!count)
                }
                if at >= regs.len() {
                    return Err(SubmitError::LrcOverrun { index: at as u32, limit });
                }
                let mut hdr = mi_load_register_imm(count as u32); // intel_lrc.c:77
                if posted {
                    hdr |= MI_LRI_FORCE_POSTED;
                }
                if gen >= 11 {
                    hdr |= MI_LRI_LRM_CS_MMIO;
                }
                regs[at] = hdr;
                at += 1;
                i += 1;
                for _ in 0..count {
                    let off = match items.get(i) {
                        Some(Item::Reg(x)) if x % 4 == 0 => *x as u32,
                        _ => return Err(SubmitError::LrcTableMalformed { table: name, at: i as u32 }),
                    };
                    if at >= regs.len() {
                        return Err(SubmitError::LrcOverrun { index: at as u32, limit });
                    }
                    regs[at] = mmio_base + off; // intel_lrc.c:95: regs[0] = base + (offset << 2)
                    at += 2;
                    i += 1;
                }
            }
            Item::Reg(_) => return Err(SubmitError::LrcTableMalformed { table: name, at: i as u32 }),
        }
    }
    if close {
        if at >= regs.len() {
            return Err(SubmitError::LrcOverrun { index: at as u32, limit });
        }
        regs[at] = MI_BATCH_BUFFER_END; // intel_lrc.c:100-104 (+ BIT(0) on Gen11+)
        if gen >= 11 {
            regs[at] |= 1;
        }
    }
    Ok(())
}

/// A table plus the name it is reported under.
#[derive(Clone, Copy, Debug)]
pub struct OffsetTable {
    pub name: &'static str,
    pub items: &'static [Item],
}

/// `reg_offsets` (intel_lrc.c:643-677) restricted to Gen8/Gen9: the render class uses the RCS table, every other engine the
/// XCS one. Gen10 and later are refused by name (their tables are not carried).
pub fn reg_offsets(gen: u8, render: bool) -> Result<OffsetTable, SubmitError> {
    Ok(match (gen, render) {
        (9, true) => OffsetTable { name: "gen9_rcs_offsets", items: &GEN9_RCS_OFFSETS }, // intel_lrc.c:664
        (8, true) => OffsetTable { name: "gen8_rcs_offsets", items: &GEN8_RCS_OFFSETS }, // intel_lrc.c:666
        (9, false) => OffsetTable { name: "gen9_xcs_offsets", items: &GEN9_XCS_OFFSETS }, // intel_lrc.c:673
        (8, false) => OffsetTable { name: "gen8_xcs_offsets", items: &GEN8_XCS_OFFSETS }, // intel_lrc.c:675
        _ => return Err(SubmitError::UnsupportedGeneration { gen }),
    })
}

/// `lrc_ring_mi_mode` (intel_lrc.c:679-691), Gen8/Gen9: the dword of `RING_MI_MODE`'s address slot, or `None` (-1).
pub fn lrc_ring_mi_mode(gen: u8, render: bool) -> Option<usize> {
    if gen >= 9 {
        Some(GEN9_CTX_RING_MI_MODE) // intel_lrc.c:686, intel_lrc_reg.h:31
    } else if render {
        Some(0x58) // intel_lrc.c:688
    } else {
        None
    }
}

/// `lrc_ring_bb_offset` (intel_lrc.c:693-706), Gen8/Gen9.
pub fn lrc_ring_bb_offset(gen: u8, render: bool) -> Option<usize> {
    if gen >= 9 {
        Some(0x64) // intel_lrc.c:700
    } else if gen >= 8 && render {
        Some(0xc4) // intel_lrc.c:702
    } else {
        None
    }
}

/// `lrc_ring_gpr0` (intel_lrc.c:708-720), Gen8/Gen9.
pub fn lrc_ring_gpr0(gen: u8, render: bool) -> Option<usize> {
    if gen >= 9 {
        Some(0x68) // intel_lrc.c:715
    } else if render {
        Some(0xd8) // intel_lrc.c:717
    } else {
        None
    }
}

/// `lrc_ring_wa_bb_per_ctx` (intel_lrc.c:722-730), Gen8/Gen9: the slot of `RING_BB_PER_CTX_PTR`.
pub fn lrc_ring_wa_bb_per_ctx(gen: u8, render: bool) -> Option<usize> {
    if gen >= 9 || render {
        Some(0x18) // intel_lrc.c:727
    } else {
        None
    }
}

/// `lrc_ring_indirect_ptr` (intel_lrc.c:732-741): two dwords after the per-context BB pointer.
pub fn lrc_ring_indirect_ptr(gen: u8, render: bool) -> Option<usize> {
    lrc_ring_wa_bb_per_ctx(gen, render).map(|x| x + 2)
}

/// `lrc_ring_indirect_offset` (intel_lrc.c:743-752): two dwords after the indirect pointer.
pub fn lrc_ring_indirect_offset(gen: u8, render: bool) -> Option<usize> {
    lrc_ring_indirect_ptr(gen, render).map(|x| x + 2)
}
