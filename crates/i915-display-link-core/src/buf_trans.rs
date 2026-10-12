// SPDX-License-Identifier: GPL-2.0-only
//! The DP / eDP buffer translation tables of Skylake and Kaby Lake (`struct hsw_ddi_buf_trans`: `trans1` = balance-leg
//! enable + de-emphasis level, `trans2` = vref select + vswing, `i_boost`), and the choice among them.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_ddi_buf_trans.c (v7.0-rc6): the tables `_skl_trans_dp` (:147) ..
//! `_skl_y_trans_edp` (:302), `skl_buf_trans_num_entries` (:1239-1246), `_skl_get_buf_trans_dp` (:1248-1256),
//! `skl_{y,u,}_get_buf_trans` (:1258-1297), `kbl_{y,u,}_get_buf_trans` (:1300-1341) and the platform choice in
//! `intel_ddi_buf_trans_init` (:1826-1846). Each row cites its own source line. Copyright (c) 2020 Intel Corporation.
//!
//! NOT PORTED (named): the HDMI tables (`_skl_trans_hdmi`, `_skl_y_trans_hdmi`; this milestone drives DP/eDP only) and
//! every other platform's tables. Kaby Lake H/S/U/Y SKUs use their own `kbl_*_trans_dp` for DP and the shared
//! `skl_*_trans_edp` for low-vswing eDP, exactly as `kbl_*_get_buf_trans` do.
use i915_devinfo_core::{Platform, PlatformDescriptor, SubPlatform};
use i915_kbl_display_core::Ddi;

/// `struct hsw_ddi_buf_trans` — intel_ddi_buf_trans.h:13-17.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HswBufTrans {
    /// "balance leg enable, de-emph level"
    pub trans1: u32,
    /// "vref sel, vswing"
    pub trans2: u32,
    /// "SKL: I_boost; valid: 0x0, 0x1, 0x3, 0x7"
    pub i_boost: u8,
}

const fn e(trans1: u32, trans2: u32, i_boost: u8) -> HswBufTrans {
    HswBufTrans { trans1, trans2, i_boost }
}

/// `_skl_trans_dp` — Skylake H and S, DP — intel_ddi_buf_trans.c:147.
pub static SKL_TRANS_DP: [HswBufTrans; 9] = [
    e(0x00002016, 0x000000A0, 0x0), // intel_ddi_buf_trans.c:148
    e(0x00005012, 0x0000009B, 0x0), // intel_ddi_buf_trans.c:149
    e(0x00007011, 0x00000088, 0x0), // intel_ddi_buf_trans.c:150
    e(0x80009010, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:151
    e(0x00002016, 0x0000009B, 0x0), // intel_ddi_buf_trans.c:152
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:153
    e(0x80007011, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:154
    e(0x00002016, 0x000000DF, 0x0), // intel_ddi_buf_trans.c:155
    e(0x80005012, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:156
];

/// `_skl_u_trans_dp` — Skylake U, DP — intel_ddi_buf_trans.c:165.
pub static SKL_U_TRANS_DP: [HswBufTrans; 9] = [
    e(0x0000201B, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:166
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:167
    e(0x80007011, 0x000000CD, 0x1), // intel_ddi_buf_trans.c:168
    e(0x80009010, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:169
    e(0x0000201B, 0x0000009D, 0x0), // intel_ddi_buf_trans.c:170
    e(0x80005012, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:171
    e(0x80007011, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:172
    e(0x00002016, 0x00000088, 0x0), // intel_ddi_buf_trans.c:173
    e(0x80005012, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:174
];

/// `_skl_y_trans_dp` — Skylake Y, DP — intel_ddi_buf_trans.c:183.
pub static SKL_Y_TRANS_DP: [HswBufTrans; 9] = [
    e(0x00000018, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:184
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:185
    e(0x80007011, 0x000000CD, 0x3), // intel_ddi_buf_trans.c:186
    e(0x80009010, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:187
    e(0x00000018, 0x0000009D, 0x0), // intel_ddi_buf_trans.c:188
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:189
    e(0x80007011, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:190
    e(0x00000018, 0x00000088, 0x0), // intel_ddi_buf_trans.c:191
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:192
];

/// `_kbl_trans_dp` — Kabylake H and S, DP — intel_ddi_buf_trans.c:201.
pub static KBL_TRANS_DP: [HswBufTrans; 9] = [
    e(0x00002016, 0x000000A0, 0x0), // intel_ddi_buf_trans.c:202
    e(0x00005012, 0x0000009B, 0x0), // intel_ddi_buf_trans.c:203
    e(0x00007011, 0x00000088, 0x0), // intel_ddi_buf_trans.c:204
    e(0x80009010, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:205
    e(0x00002016, 0x0000009B, 0x0), // intel_ddi_buf_trans.c:206
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:207
    e(0x80007011, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:208
    e(0x00002016, 0x00000097, 0x0), // intel_ddi_buf_trans.c:209
    e(0x80005012, 0x000000C0, 0x1), // intel_ddi_buf_trans.c:210
];

/// `_kbl_u_trans_dp` — Kabylake U, DP — intel_ddi_buf_trans.c:219.
pub static KBL_U_TRANS_DP: [HswBufTrans; 9] = [
    e(0x0000201B, 0x000000A1, 0x0), // intel_ddi_buf_trans.c:220
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:221
    e(0x80007011, 0x000000CD, 0x3), // intel_ddi_buf_trans.c:222
    e(0x80009010, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:223
    e(0x0000201B, 0x0000009D, 0x0), // intel_ddi_buf_trans.c:224
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:225
    e(0x80007011, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:226
    e(0x00002016, 0x0000004F, 0x0), // intel_ddi_buf_trans.c:227
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:228
];

/// `_kbl_y_trans_dp` — Kabylake Y, DP — intel_ddi_buf_trans.c:237.
pub static KBL_Y_TRANS_DP: [HswBufTrans; 9] = [
    e(0x00001017, 0x000000A1, 0x0), // intel_ddi_buf_trans.c:238
    e(0x00005012, 0x00000088, 0x0), // intel_ddi_buf_trans.c:239
    e(0x80007011, 0x000000CD, 0x3), // intel_ddi_buf_trans.c:240
    e(0x8000800F, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:241
    e(0x00001017, 0x0000009D, 0x0), // intel_ddi_buf_trans.c:242
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:243
    e(0x80007011, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:244
    e(0x00001017, 0x0000004C, 0x0), // intel_ddi_buf_trans.c:245
    e(0x80005012, 0x000000C0, 0x3), // intel_ddi_buf_trans.c:246
];

/// `_skl_trans_edp` — Skylake/Kabylake H and S, eDP 1.4 low vswing — intel_ddi_buf_trans.c:258.
pub static SKL_TRANS_EDP: [HswBufTrans; 10] = [
    e(0x00000018, 0x000000A8, 0x0), // intel_ddi_buf_trans.c:259
    e(0x00004013, 0x000000A9, 0x0), // intel_ddi_buf_trans.c:260
    e(0x00007011, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:261
    e(0x00009010, 0x0000009C, 0x0), // intel_ddi_buf_trans.c:262
    e(0x00000018, 0x000000A9, 0x0), // intel_ddi_buf_trans.c:263
    e(0x00006013, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:264
    e(0x00007011, 0x000000A6, 0x0), // intel_ddi_buf_trans.c:265
    e(0x00000018, 0x000000AB, 0x0), // intel_ddi_buf_trans.c:266
    e(0x00007013, 0x0000009F, 0x0), // intel_ddi_buf_trans.c:267
    e(0x00000018, 0x000000DF, 0x0), // intel_ddi_buf_trans.c:268
];

/// `_skl_u_trans_edp` — Skylake/Kabylake U, eDP 1.4 low vswing — intel_ddi_buf_trans.c:280.
pub static SKL_U_TRANS_EDP: [HswBufTrans; 10] = [
    e(0x00000018, 0x000000A8, 0x0), // intel_ddi_buf_trans.c:281
    e(0x00004013, 0x000000A9, 0x0), // intel_ddi_buf_trans.c:282
    e(0x00007011, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:283
    e(0x00009010, 0x0000009C, 0x0), // intel_ddi_buf_trans.c:284
    e(0x00000018, 0x000000A9, 0x0), // intel_ddi_buf_trans.c:285
    e(0x00006013, 0x000000A2, 0x0), // intel_ddi_buf_trans.c:286
    e(0x00007011, 0x000000A6, 0x0), // intel_ddi_buf_trans.c:287
    e(0x00002016, 0x000000AB, 0x0), // intel_ddi_buf_trans.c:288
    e(0x00005013, 0x0000009F, 0x0), // intel_ddi_buf_trans.c:289
    e(0x00000018, 0x000000DF, 0x0), // intel_ddi_buf_trans.c:290
];

/// `_skl_y_trans_edp` — Skylake/Kabylake Y, eDP 1.4 low vswing — intel_ddi_buf_trans.c:302.
pub static SKL_Y_TRANS_EDP: [HswBufTrans; 10] = [
    e(0x00000018, 0x000000A8, 0x0), // intel_ddi_buf_trans.c:303
    e(0x00004013, 0x000000AB, 0x0), // intel_ddi_buf_trans.c:304
    e(0x00007011, 0x000000A4, 0x0), // intel_ddi_buf_trans.c:305
    e(0x00009010, 0x000000DF, 0x0), // intel_ddi_buf_trans.c:306
    e(0x00000018, 0x000000AA, 0x0), // intel_ddi_buf_trans.c:307
    e(0x00006013, 0x000000A4, 0x0), // intel_ddi_buf_trans.c:308
    e(0x00007011, 0x0000009D, 0x0), // intel_ddi_buf_trans.c:309
    e(0x00000018, 0x000000A0, 0x0), // intel_ddi_buf_trans.c:310
    e(0x00006012, 0x000000DF, 0x0), // intel_ddi_buf_trans.c:311
    e(0x00000018, 0x0000008A, 0x0), // intel_ddi_buf_trans.c:312
];

/// Which Kaby Lake / Skylake DDI buffer SKU: `intel_ddi_buf_trans_init` (intel_ddi_buf_trans.c:1826-1846) picks by platform
/// and ULX / ULT sub-platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sku {
    SklHs,
    SklU,
    SklY,
    KblHs,
    KblU,
    KblY,
}

/// The platform is not one this crate has translation tables for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufTransError {
    /// Cherryview (Gen8) or anything else: `MISSING_CASE(pdev->device)` (intel_ddi_buf_trans.c:1848-1850).
    UnsupportedPlatform { devid: u16 },
    /// A signal level beyond the table: Linux `drm_WARN_ON`s and clamps (intel_ddi.c:1524-1527); a caller that does not clamp gets this.
    NoSuchEntry { level: usize, n_entries: usize },
}

impl BufTransError {
    pub const fn name(&self) -> &'static str {
        match self {
            BufTransError::UnsupportedPlatform { .. } => "BUF-TRANS-UNSUPPORTED-PLATFORM",
            BufTransError::NoSuchEntry { .. } => "BUF-TRANS-NO-SUCH-ENTRY",
        }
    }
}

impl Sku {
    /// `kabylake_ulx` -> Y, `kabylake_ult` -> U, else H/S; likewise Skylake (intel_ddi_buf_trans.c:1828-1844). The Pixelbook
    /// Go's 8086:591C is a Kaby Lake ULX part, so it selects [`Sku::KblY`].
    pub fn from_descriptor(d: &PlatformDescriptor) -> Result<Sku, BufTransError> {
        match (d.platform(), d.sub) {
            (Platform::Kabylake, SubPlatform::Ulx) => Ok(Sku::KblY),
            (Platform::Kabylake, SubPlatform::Ult) => Ok(Sku::KblU),
            (Platform::Kabylake, SubPlatform::None) => Ok(Sku::KblHs),
            (Platform::Skylake, SubPlatform::Ulx) => Ok(Sku::SklY),
            (Platform::Skylake, SubPlatform::Ult) => Ok(Sku::SklU),
            (Platform::Skylake, SubPlatform::None) => Ok(Sku::SklHs),
            (Platform::Cherryview, _) => Err(BufTransError::UnsupportedPlatform { devid: d.devid }),
        }
    }
}

/// The chosen table and how many of its entries the port may use (`*n_entries`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufTrans {
    pub entries: &'static [HswBufTrans],
    pub n_entries: usize,
}

/// `skl_buf_trans_num_entries()` — intel_ddi_buf_trans.c:1239-1246: "Only DDIA and DDIE can select the 10th register with
/// DP".
pub const fn skl_buf_trans_num_entries(port: Ddi, n_entries: usize) -> usize {
    let cap = match port {
        Ddi::A | Ddi::E => 10,
        _ => 9,
    };
    if n_entries < cap {
        n_entries
    } else {
        cap
    }
}

/// `{skl,kbl}_{y,u,}_get_buf_trans()` for a DP or eDP output: the eDP low-vswing tables when `edp_low_vswing` (the VBT's
/// `edp.low_vswing`, `use_edp_low_vswing()` :1195-1201) and the output is eDP, else the DP table of the SKU.
pub fn get_buf_trans(sku: Sku, is_edp: bool, edp_low_vswing: bool, port: Ddi) -> BufTrans {
    let low = is_edp && edp_low_vswing;
    let entries: &'static [HswBufTrans] = match (sku, low) {
        (Sku::SklHs, true) | (Sku::KblHs, true) => &SKL_TRANS_EDP,
        (Sku::SklU, true) | (Sku::KblU, true) => &SKL_U_TRANS_EDP,
        (Sku::SklY, true) | (Sku::KblY, true) => &SKL_Y_TRANS_EDP,
        (Sku::SklHs, false) => &SKL_TRANS_DP,
        (Sku::SklU, false) => &SKL_U_TRANS_DP,
        (Sku::SklY, false) => &SKL_Y_TRANS_DP,
        (Sku::KblHs, false) => &KBL_TRANS_DP,
        (Sku::KblU, false) => &KBL_U_TRANS_DP,
        (Sku::KblY, false) => &KBL_Y_TRANS_DP,
    };
    BufTrans { entries, n_entries: skl_buf_trans_num_entries(port, entries.len()) }
}

impl BufTrans {
    /// `trans->entries[level]`, refused beyond `n_entries` (Linux clamps to `n_entries - 1` after a `drm_WARN`,
    /// intel_ddi.c:1524-1527; callers here clamp explicitly with [`BufTrans::clamp_level`]).
    pub fn entry(&self, level: usize) -> Result<HswBufTrans, BufTransError> {
        if level < self.n_entries {
            Ok(self.entries[level])
        } else {
            Err(BufTransError::NoSuchEntry { level, n_entries: self.n_entries })
        }
    }
    /// `if (level >= n_entries) level = n_entries - 1` — intel_ddi.c:1524-1527.
    pub const fn clamp_level(&self, level: usize) -> usize {
        if level >= self.n_entries {
            self.n_entries - 1
        } else {
            level
        }
    }
}
