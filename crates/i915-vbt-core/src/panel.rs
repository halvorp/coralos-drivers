// SPDX-License-Identifier: GPL-2.0-only
//! LFP (panel) description: block 40 options, block 41 data-table pointers (validate + fix up), block 42 tables,
//! the panel DTD -> mode, the PnP id, the panel-type selection.
//! `parse_panel_options()` (intel_bios.c:792-838), `validate_lfp_data_ptrs()` / `fixup_lfp_data_ptrs()`
//! (intel_bios.c:223-345), `fill_detail_timing_data()` (intel_bios.c:531), `parse_lfp_panel_dtd()`
//! (intel_bios.c:839-878), `get_panel_type()` (intel_bios.c:722).

use crate::bdb::{Block, BlockId, Vbt};
use crate::child::{DEVICE_HANDLE_LFP2, ChildDevice};
use crate::error::VbtError;
use alloc::vec::Vec;

/// sizeof(struct bdb_edid_dtd) — intel_vbt_defs.h:55-90 (18 bytes).
pub const EDID_DTD_SIZE: usize = 18;
/// sizeof(struct bdb_edid_pnp_id) — intel_vbt_defs.h:43-49 (10 bytes).
pub const EDID_PNP_ID_SIZE: usize = 10;
/// sizeof(struct bdb_edid_product_name) — intel_vbt_defs.h:51-53 (13 bytes).
pub const EDID_PRODUCT_NAME_SIZE: usize = 13;
/// Panels per table: every `[16]` array in blocks 27/40-43.
pub const PANEL_COUNT: usize = 16;
/// sizeof(struct bdb_lfp_data_ptrs): u8 num_entries + 16 * 3 * (u16,u8) + panel_name (u16,u8) — intel_vbt_defs.h:1237.
pub const LFP_DATA_PTRS_SIZE: usize = 1 + PANEL_COUNT * 9 + 3;

/// DRRS type derived from block 40 (`enum drrs_support_type` values by name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrrsType {
    Static,
    Seamless,
    None,
}

/// Block 40 as parse_panel_options() reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LfpOptions {
    pub panel_type: u8,
    /// BDB 212+ (zero before; the zero pad).
    pub panel_type2: u8,
    /// `pixel_dither` — byte 2 bit 5, intel_vbt_defs.h:1196-1202.
    pub lvds_dither: bool,
    /// `None` when the block is shorter than 16 bytes (intel_bios.c:807-812: "return" before the DRRS decode).
    pub drrs_type: Option<DrrsType>,
}

/// `panel_bits()` — intel_bios.c:781: a `num_bits` field of the per-panel packed word.
pub const fn panel_bits(value: u32, panel_type: u8, num_bits: u32) -> u32 {
    (value >> (panel_type as u32 * num_bits)) & ((1u32 << num_bits) - 1)
}
/// `panel_bool()` — intel_bios.c:786.
pub const fn panel_bool(value: u32, panel_type: u8) -> bool {
    panel_bits(value, panel_type, 1) != 0
}

/// Block 40. `panel_type` is not applied here; see [`select_panel_type`].
pub fn parse_lfp_options(vbt: &Vbt, panel_type: u8) -> Result<Option<LfpOptions>, VbtError> {
    let Some(b) = vbt.find_block(BlockId::LfpOptions as u8)? else { return Ok(None) };
    // min_size = sizeof(struct bdb_lfp_options) = 34 (intel_vbt_defs.h:1192-1221); Linux zero-pads, so no refusal for
    // short blocks, but the first 4 bytes are needed to say anything at all.
    b.require(4)?;
    let drrs_type = if b.len() < 16 {
        None // "Empirical evidence indicates the block size can be either 4,14,16,24+ bytes" — intel_bios.c:807
    } else {
        // dps_panel_type_bits u32 @16 (4+4+2+2+2+2 after the 4 header bytes)
        Some(match panel_bits(b.u32_at(16), panel_type & 0xf, 2) {
            0 => DrrsType::Static,
            2 => DrrsType::Seamless,
            _ => DrrsType::None,
        })
    };
    Ok(Some(LfpOptions {
        panel_type: b.u8_at(0),
        panel_type2: b.u8_at(1),
        lvds_dither: b.u8_at(2) & (1 << 5) != 0,
        drrs_type,
    }))
}

/// The four panel-type sources of `get_panel_type()` (intel_bios.c:722-779). `None` = the source gave -1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PanelTypeSources {
    pub opregion: Option<u8>,
    pub vbt: Option<u8>,
    pub pnpid: Option<u8>,
}

/// `vbt_get_panel_type()` — intel_bios.c:632-656. `child` is the LFP child device (for LFP2 -> panel_type2).
pub fn vbt_panel_type(vbt: &Vbt, child: Option<&ChildDevice>) -> Result<Option<u8>, VbtError> {
    let Some(o) = parse_lfp_options(vbt, 0)? else { return Ok(None) };
    // `panel_type > 0xf && != 0xff` is invalid -> -1 (intel_bios.c:642)
    if o.panel_type > 0xf && o.panel_type != 0xff {
        return Ok(None);
    }
    if child.map_or(false, |c| c.handle() == DEVICE_HANDLE_LFP2) {
        return Ok(Some(o.panel_type2));
    }
    Ok(Some(o.panel_type))
}

/// The selection ladder of `get_panel_type()` (intel_bios.c:764-772): OpRegion wins; else PNPID when the VBT says
/// 0xff; else the VBT value; else the fallback (0 if `use_fallback`, else none).
pub fn select_panel_type(s: PanelTypeSources, use_fallback: bool) -> Option<u8> {
    if s.opregion.is_some() {
        s.opregion
    } else if s.vbt == Some(0xff) && s.pnpid.is_some() {
        s.pnpid
    } else if s.vbt.is_some() && s.vbt != Some(0xff) {
        s.vbt
    } else if use_fallback {
        Some(0)
    } else {
        None
    }
}

/// One `lfp_data_ptr_table`: offset (from the LFP data block after fix-up) and size — intel_vbt_defs.h:1225.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPtr {
    pub offset: u16,
    pub size: u8,
}

/// Validated, block-42-relative pointers (after `fixup_lfp_data_ptrs()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfpDataPtrs {
    pub fp_timing: [DataPtr; PANEL_COUNT],
    pub dvo_timing: [DataPtr; PANEL_COUNT],
    pub pnp_id: [DataPtr; PANEL_COUNT],
    pub panel_name: DataPtr,
    /// fp_timing size after the "6 unaccounted bytes" adjustment (intel_bios.c:282).
    pub fp_timing_size: usize,
}

fn ptr_at(b: &Block, off: usize) -> DataPtr {
    DataPtr { offset: b.u16_at(off), size: b.u8_at(off + 2) }
}

fn malformed(check: &'static str) -> VbtError {
    VbtError::MalformedLfpDataPtrs { check }
}

/// `fixup_lfp_data_ptrs()` + `validate_lfp_data_ptrs()` — intel_bios.c:223-345. Every `return false` is a named
/// `MalformedLfpDataPtrs` check.
pub fn parse_lfp_data_ptrs(vbt: &Vbt) -> Result<LfpDataPtrs, VbtError> {
    let Some(pb) = vbt.find_block(BlockId::LfpDataPtrs as u8)? else {
        return Err(VbtError::LfpDataPtrsMissing);
    };
    pb.require(LFP_DATA_PTRS_SIZE)?;
    let data = vbt.require_block(BlockId::LfpData as u8)?;
    let data_block_size = data.len() as i32;
    if data_block_size == 0 {
        return Err(malformed("empty LFP data block")); // intel_bios.c:236
    }
    // raw_block_offset(bdb, BDB_LFP_DATA) — intel_bios.c:133
    let base = data.bdb_offset;
    let mut fp = [DataPtr { offset: 0, size: 0 }; PANEL_COUNT];
    let mut dvo = fp;
    let mut pnp = fp;
    for i in 0..PANEL_COUNT {
        let o = 1 + i * 9;
        let (f, d, p) = (ptr_at(&pb, o), ptr_at(&pb, o + 3), ptr_at(&pb, o + 6));
        // fixup: pointers are BDB-relative on the wire; make them data-block-relative — intel_bios.c:317-343
        if u32::from(f.offset) < base || u32::from(d.offset) < base || u32::from(p.offset) < base {
            return Err(malformed("pointer below LFP data block start"));
        }
        fp[i] = DataPtr { offset: (u32::from(f.offset) - base) as u16, size: f.size };
        dvo[i] = DataPtr { offset: (u32::from(d.offset) - base) as u16, size: d.size };
        pnp[i] = DataPtr { offset: (u32::from(p.offset) - base) as u16, size: p.size };
    }
    let mut name = ptr_at(&pb, 1 + PANEL_COUNT * 9);
    if name.size != 0 {
        if u32::from(name.offset) < base {
            return Err(malformed("panel_name pointer below LFP data block start"));
        }
        name.offset = (u32::from(name.offset) - base) as u16;
    }
    // ---- validate_lfp_data_ptrs() ----
    // "always 3 indicating the presence of fp_timing+dvo_timing+panel_pnp_id" — intel_bios.c:240
    if pb.u8_at(0) != 3 {
        return Err(malformed("num_entries != 3"));
    }
    let mut fp_size = i32::from(fp[0].size);
    let dvo_size = i32::from(dvo[0].size);
    let pnp_size = i32::from(pnp[0].size);
    let name_size = i32::from(name.size);
    if fp_size < 32 || dvo_size != EDID_DTD_SIZE as i32 || pnp_size != EDID_PNP_ID_SIZE as i32 {
        return Err(malformed("table sizes (fp_timing < 32, dvo != 18, pnp != 10)"));
    }
    if name_size != 0 && name_size != EDID_PRODUCT_NAME_SIZE as i32 {
        return Err(malformed("panel_name size != 13"));
    }
    let lfp_data_size = i32::from(fp[1].offset) - i32::from(fp[0].offset);
    if 16 * lfp_data_size > data_block_size {
        return Err(malformed("16 entries exceed the data block"));
    }
    for i in 1..PANEL_COUNT {
        if i32::from(fp[i].size) != fp_size || i32::from(dvo[i].size) != dvo_size || i32::from(pnp[i].size) != pnp_size {
            return Err(malformed("non-uniform table sizes"));
        }
        let d = |t: &[DataPtr; PANEL_COUNT]| i32::from(t[i].offset) - i32::from(t[i - 1].offset);
        if d(&fp) != lfp_data_size || d(&dvo) != lfp_data_size || d(&pnp) != lfp_data_size {
            return Err(malformed("non-uniform entry stride"));
        }
    }
    // "all real VBTs seem to have 6 unaccounted bytes in the fp_timing table" — intel_bios.c:277-282
    if fp_size + 6 + dvo_size + pnp_size == lfp_data_size {
        fp_size += 6;
    }
    if fp_size + dvo_size + pnp_size != lfp_data_size {
        return Err(malformed("entry size != fp_timing + dvo + pnp"));
    }
    let (f0, d0, p0) = (i32::from(fp[0].offset), i32::from(dvo[0].offset), i32::from(pnp[0].offset));
    if f0 + fp_size != d0 || d0 + dvo_size != p0 || p0 + pnp_size != lfp_data_size {
        return Err(malformed("entry 0 tables are not contiguous"));
    }
    for i in 0..PANEL_COUNT {
        if i32::from(fp[i].offset) + fp_size > data_block_size
            || i32::from(dvo[i].offset) + dvo_size > data_block_size
            || i32::from(pnp[i].offset) + pnp_size > data_block_size
        {
            return Err(malformed("table outside the data block"));
        }
    }
    if i32::from(name.offset) + 16 * name_size > data_block_size {
        return Err(malformed("panel names outside the data block"));
    }
    for i in 0..PANEL_COUNT {
        if data.u16_at(fp[i].offset as usize + fp_size as usize - 2) != 0xffff {
            return Err(malformed("fp_timing terminator 0xffff missing"));
        }
    }
    Ok(LfpDataPtrs { fp_timing: fp, dvo_timing: dvo, pnp_id: pnp, panel_name: name, fp_timing_size: fp_size as usize })
}

/// A display mode in the shape `fill_detail_timing_data()` fills (`struct drm_display_mode` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelMode {
    pub clock_khz: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub hsync_positive: bool,
    pub vsync_positive: bool,
    pub width_mm: u16,
    pub height_mm: u16,
}

/// `fill_detail_timing_data()` over an 18-byte `bdb_edid_dtd` — intel_bios.c:531-586. Bitfield order is LSB-first
/// per intel_vbt_defs.h:55-90.
pub fn decode_dtd(d: &[u8; EDID_DTD_SIZE]) -> PanelMode {
    let w = |hi: u8, lo: u8| (u16::from(hi) << 8) | u16::from(lo);
    let hactive_lo = d[2];
    let hblank_lo = d[3];
    let (hblank_hi, hactive_hi) = (d[4] & 0xf, d[4] >> 4);
    let vactive_lo = d[5];
    let vblank_lo = d[6];
    let (vblank_hi, vactive_hi) = (d[7] & 0xf, d[7] >> 4);
    let (hsync_off_lo, hsync_pw_lo) = (d[8], d[9]);
    let (vsync_pw_lo, vsync_off_lo) = (d[10] & 0xf, d[10] >> 4);
    let (vsync_pw_hi, vsync_off_hi, hsync_pw_hi, hsync_off_hi) = (d[11] & 3, (d[11] >> 2) & 3, (d[11] >> 4) & 3, d[11] >> 6);
    let (himage_lo, vimage_lo) = (d[12], d[13]);
    let (vimage_hi, himage_hi) = (d[14] & 0xf, d[14] >> 4);

    let hdisplay = w(hactive_hi, hactive_lo);
    let hsync_start = hdisplay + w(hsync_off_hi, hsync_off_lo);
    let mut hsync_end = hsync_start + w(hsync_pw_hi, hsync_pw_lo);
    let htotal = hdisplay + w(hblank_hi, hblank_lo);
    let vdisplay = w(vactive_hi, vactive_lo);
    let vsync_start = vdisplay + ((u16::from(vsync_off_hi) << 4) | u16::from(vsync_off_lo));
    let mut vsync_end = vsync_start + ((u16::from(vsync_pw_hi) << 4) | u16::from(vsync_pw_lo));
    let vtotal = vdisplay + w(vblank_hi, vblank_lo);
    // "Some VBTs have bogus h/vsync_end values" — intel_bios.c:566-579
    if hsync_end > htotal {
        hsync_end = htotal;
    }
    if vsync_end > vtotal {
        vsync_end = vtotal;
    }
    PanelMode {
        clock_khz: u32::from(u16::from_le_bytes([d[0], d[1]])) * 10,
        hdisplay,
        hsync_start,
        hsync_end,
        htotal,
        vdisplay,
        vsync_start,
        vsync_end,
        vtotal,
        vsync_positive: d[17] & (1 << 5) != 0,
        hsync_positive: d[17] & (1 << 6) != 0,
        width_mm: w(himage_hi, himage_lo),
        height_mm: w(vimage_hi, vimage_lo),
    }
}

/// The EDID product id a VBT stores per panel (`struct bdb_edid_pnp_id`, intel_vbt_defs.h:43), raw 10 bytes.
pub type PnpId = [u8; EDID_PNP_ID_SIZE];

/// `pnpid_get_panel_type()` — intel_bios.c:658-706: index of the VBT PnP id equal to the EDID product id
/// (a full match wins; else the first match with date zeroed — week @8, year @9).
pub fn pnpid_panel_type(vbt: &Vbt, edid_product_id: &PnpId) -> Result<Option<u8>, VbtError> {
    let ptrs = parse_lfp_data_ptrs(vbt)?;
    let data = vbt.require_block(BlockId::LfpData as u8)?;
    let mut nodate = *edid_product_id;
    nodate[8] = 0;
    nodate[9] = 0;
    let mut best = None;
    for i in 0..PANEL_COUNT {
        let id = lfp_pnp_id(&data, &ptrs, i);
        if id == *edid_product_id {
            return Ok(Some(i as u8));
        }
        if best.is_none() && id == nodate {
            best = Some(i as u8);
        }
    }
    Ok(best)
}

fn lfp_pnp_id(data: &Block, ptrs: &LfpDataPtrs, i: usize) -> PnpId {
    let o = ptrs.pnp_id[i].offset as usize;
    let mut out = [0u8; EDID_PNP_ID_SIZE];
    for (k, b) in out.iter_mut().enumerate() {
        *b = data.u8_at(o + k);
    }
    out
}

/// Everything about the chosen panel from blocks 41/42.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfpPanel {
    pub panel_type: u8,
    pub mode: PanelMode,
    /// fp_timing.x_res / y_res (u16 @0 / @2).
    pub fp_x_res: u16,
    pub fp_y_res: u16,
    /// `bios_lvds_val` = fp_timing.lvds_reg_val (u32 @8), only when x/y match the DTD (intel_bios.c:870-876).
    pub bios_lvds_val: Option<u32>,
    pub pnp_id: PnpId,
    /// 13-byte panel name (BDB 156+ tail), if the pointer table has one.
    pub panel_name: Option<[u8; EDID_PRODUCT_NAME_SIZE]>,
}

/// `parse_lfp_panel_dtd()` (intel_bios.c:839-878) for `panel_type`.
pub fn parse_lfp_panel(vbt: &Vbt, panel_type: u8) -> Result<LfpPanel, VbtError> {
    if usize::from(panel_type) >= PANEL_COUNT {
        return Err(VbtError::PanelTypeOutOfRange { panel_type });
    }
    let ptrs = parse_lfp_data_ptrs(vbt)?;
    let data = vbt.require_block(BlockId::LfpData as u8)?;
    let i = usize::from(panel_type);
    let mut dtd = [0u8; EDID_DTD_SIZE];
    let o = ptrs.dvo_timing[i].offset as usize;
    for (k, b) in dtd.iter_mut().enumerate() {
        *b = data.u8_at(o + k);
    }
    let mode = decode_dtd(&dtd);
    let f = ptrs.fp_timing[i].offset as usize;
    let (x, y) = (data.u16_at(f), data.u16_at(f + 2));
    let bios_lvds_val = if x == mode.hdisplay && y == mode.vdisplay { Some(data.u32_at(f + 8)) } else { None };
    let panel_name = if ptrs.panel_name.size != 0 {
        let mut n = [0u8; EDID_PRODUCT_NAME_SIZE];
        let no = ptrs.panel_name.offset as usize + i * EDID_PRODUCT_NAME_SIZE;
        for (k, b) in n.iter_mut().enumerate() {
            *b = data.u8_at(no + k);
        }
        Some(n)
    } else {
        None
    };
    Ok(LfpPanel { panel_type, mode, fp_x_res: x, fp_y_res: y, bios_lvds_val, pnp_id: lfp_pnp_id(&data, &ptrs, i), panel_name })
}

/// Used by tests/callers that want all of a block 42's entries decoded: the raw DTD bytes of every panel.
pub fn all_dtds(vbt: &Vbt) -> Result<Vec<PanelMode>, VbtError> {
    let ptrs = parse_lfp_data_ptrs(vbt)?;
    let data = vbt.require_block(BlockId::LfpData as u8)?;
    let mut v = Vec::new();
    for i in 0..PANEL_COUNT {
        let mut dtd = [0u8; EDID_DTD_SIZE];
        for (k, b) in dtd.iter_mut().enumerate() {
            *b = data.u8_at(ptrs.dvo_timing[i].offset as usize + k);
        }
        v.push(decode_dtd(&dtd));
    }
    Ok(v)
}
