// SPDX-License-Identifier: GPL-2.0-only
//! `struct child_device_config` (intel_vbt_defs.h:474-566): one display device / DDI port description, decoded from
//! its packed bytes with the same zero-padded tail Linux uses (parse_general_definitions copies
//! `min(child_dev_size, sizeof)` bytes into a zeroed struct, intel_bios.c:2942-2944).
//!
//! Field offsets below are derived from the packed layout (u16 handle @0, u16 device_type @2, 10-byte union @4,
//! u16 addin_offset @14, dvo_port @16, i2c_pin @17, target_addr @18, ddc_pin @19, u16 edid_ptr @20, dvo_cfg @22,
//! 4-byte union @23, caps @27, dvo_wiring @28, dvo2_wiring @29, u16 extended_type @30, dvo_function @32, flags
//! @33, dp_gpio_index @34, u16 dp_gpio_pin_num @35, iboost @37, dp_max_link_rate @38, efp_index @39,
//! u32 edp_data_rate_override @40) and pinned by `tests/child.rs`.

// ---- device types, intel_vbt_defs.h:312-323 ------------------------------------------------------------------
/// `DEVICE_TYPE_INTERNAL_CONNECTOR (1 << 12)` — intel_vbt_defs.h:312.
pub const DEVICE_TYPE_INTERNAL_CONNECTOR: u16 = 1 << 12;
/// `DEVICE_TYPE_NOT_HDMI_OUTPUT (1 << 11)` — intel_vbt_defs.h:313.
pub const DEVICE_TYPE_NOT_HDMI_OUTPUT: u16 = 1 << 11;
/// `DEVICE_TYPE_MIPI_OUTPUT (1 << 10)` — intel_vbt_defs.h:314.
pub const DEVICE_TYPE_MIPI_OUTPUT: u16 = 1 << 10;
/// `DEVICE_TYPE_TMDS_DVI_SIGNALING (1 << 4)` — intel_vbt_defs.h:319.
pub const DEVICE_TYPE_TMDS_DVI_SIGNALING: u16 = 1 << 4;
/// `DEVICE_TYPE_DISPLAYPORT_OUTPUT (1 << 2)` — intel_vbt_defs.h:321.
pub const DEVICE_TYPE_DISPLAYPORT_OUTPUT: u16 = 1 << 2;
/// `DEVICE_TYPE_ANALOG_OUTPUT (1 << 0)` — intel_vbt_defs.h:323.
pub const DEVICE_TYPE_ANALOG_OUTPUT: u16 = 1 << 0;

// ---- device handles, intel_vbt_defs.h:262-272 ----------------------------------------------------------------
/// `DEVICE_HANDLE_LFP1 0x0008` — intel_vbt_defs.h:270.
pub const DEVICE_HANDLE_LFP1: u16 = 0x0008;
/// `DEVICE_HANDLE_LFP2 0x0080` — intel_vbt_defs.h:271.
pub const DEVICE_HANDLE_LFP2: u16 = 0x0080;

// ---- dvo_port (BDB 155+), intel_vbt_defs.h:351-375 -----------------------------------------------------------
/// `DVO_PORT_HDMIA 0` — intel_vbt_defs.h:351.
pub const DVO_PORT_HDMIA: u8 = 0;
/// `DVO_PORT_HDMIB 1` — intel_vbt_defs.h:352.
pub const DVO_PORT_HDMIB: u8 = 1;
/// `DVO_PORT_HDMIC 2` — intel_vbt_defs.h:353.
pub const DVO_PORT_HDMIC: u8 = 2;
/// `DVO_PORT_HDMID 3` — intel_vbt_defs.h:354.
pub const DVO_PORT_HDMID: u8 = 3;
/// `DVO_PORT_LVDS 4` — intel_vbt_defs.h:355.
pub const DVO_PORT_LVDS: u8 = 4;
/// `DVO_PORT_TV 5` — intel_vbt_defs.h:356.
pub const DVO_PORT_TV: u8 = 5;
/// `DVO_PORT_CRT 6` — intel_vbt_defs.h:357.
pub const DVO_PORT_CRT: u8 = 6;
/// `DVO_PORT_DPB 7` — intel_vbt_defs.h:358.
pub const DVO_PORT_DPB: u8 = 7;
/// `DVO_PORT_DPC 8` — intel_vbt_defs.h:359.
pub const DVO_PORT_DPC: u8 = 8;
/// `DVO_PORT_DPD 9` — intel_vbt_defs.h:360.
pub const DVO_PORT_DPD: u8 = 9;
/// `DVO_PORT_DPA 10` — intel_vbt_defs.h:361.
pub const DVO_PORT_DPA: u8 = 10;
/// `DVO_PORT_DPE 11` — intel_vbt_defs.h:362.
pub const DVO_PORT_DPE: u8 = 11;
/// `DVO_PORT_HDMIE 12` — intel_vbt_defs.h:363.
pub const DVO_PORT_HDMIE: u8 = 12;

// ---- DP AUX channel codes, intel_vbt_defs.h:414-422 ----------------------------------------------------------
/// `DP_AUX_A 0x40` — intel_vbt_defs.h:414.
pub const DP_AUX_A: u8 = 0x40;
/// `DP_AUX_B 0x10` — intel_vbt_defs.h:415.
pub const DP_AUX_B: u8 = 0x10;
/// `DP_AUX_C 0x20` — intel_vbt_defs.h:416.
pub const DP_AUX_C: u8 = 0x20;
/// `DP_AUX_D 0x30` — intel_vbt_defs.h:417.
pub const DP_AUX_D: u8 = 0x30;
/// `DP_AUX_E 0x50` — intel_vbt_defs.h:418.
pub const DP_AUX_E: u8 = 0x50;

/// sizeof(struct child_device_config) as laid out above (BUILD_BUG_ON(>= 40) at intel_bios.c:2835; 44 with the
/// BDB 263+ `edp_data_rate_override`).
pub const CHILD_DEVICE_CONFIG_SIZE: usize = 44;
/// `LEGACY_CHILD_DEVICE_CONFIG_SIZE 33` — intel_vbt_defs.h:384.
pub const LEGACY_CHILD_DEVICE_CONFIG_SIZE: usize = 33;

/// The DDI port a `dvo_port` names (the generic `port_mapping[]` of `dvo_port_to_port()`, intel_bios.c:2344-2352,
/// valid for Kaby Lake / display version 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Port {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
}

/// AUX channel (`enum aux_ch`) decoded from the VBT `aux_channel` byte via `direct_aux_ch_map` (intel_bios.c:3675).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AuxCh {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
}

/// `dvo_port_to_port()` for display ver < 11 non-RKL/ADL (intel_bios.c:2344-2352). Port A..E only: the F..I rows of
/// the C table do not exist on Kaby Lake and return `None`, like an unknown value (`PORT_NONE`).
/// Ordering is the C loop's: ports ascending, first hit wins — so DVO_PORT_CRT maps to E (intel_bios.c:2348).
pub const fn dvo_port_to_port(dvo_port: u8) -> Option<Port> {
    match dvo_port {
        DVO_PORT_HDMIA | DVO_PORT_DPA => Some(Port::A),
        DVO_PORT_HDMIB | DVO_PORT_DPB => Some(Port::B),
        DVO_PORT_HDMIC | DVO_PORT_DPC => Some(Port::C),
        DVO_PORT_HDMID | DVO_PORT_DPD => Some(Port::D),
        DVO_PORT_HDMIE | DVO_PORT_DPE | DVO_PORT_CRT => Some(Port::E),
        _ => None,
    }
}

/// `direct_aux_ch_map[]` reverse lookup, used by `map_aux_ch()` for every platform this crate covers
/// (intel_bios.c:3675-3705). `0` and unknown codes are `None` (AUX_CH_NONE, "Ignoring alternate AUX CH").
pub const fn map_aux_ch(aux_channel: u8) -> Option<AuxCh> {
    match aux_channel {
        DP_AUX_A => Some(AuxCh::A),
        DP_AUX_B => Some(AuxCh::B),
        DP_AUX_C => Some(AuxCh::C),
        DP_AUX_D => Some(AuxCh::D),
        DP_AUX_E => Some(AuxCh::E),
        _ => None,
    }
}

/// `parse_bdb_230_dp_max_link_rate()` (intel_bios.c:2441-2462): kHz-ish port clock in units of 10 kHz... exactly
/// Linux's number (162000 = 1.62 Gbps). 0 means "platform default".
pub const fn dp_max_link_rate_230(v: u8) -> u32 {
    match v {
        7 => 2000000, // UHBR20, intel_vbt_defs.h:437
        6 => 1350000, // UHBR13P5, :436
        5 => 1000000, // UHBR10, :435
        4 => 810000,  // HBR3, :434
        3 => 540000,  // HBR2, :433
        2 => 270000,  // HBR, :432
        1 => 162000,  // LBR, :431 (DEF = 0 at :430 and every unknown value return 0)
        _ => 0,
    }
}

/// `parse_bdb_216_dp_max_link_rate()` (intel_bios.c:2464-2477); unknown values fall to HBR3.
pub const fn dp_max_link_rate_216(v: u8) -> u32 {
    match v {
        1 => 540000, // HBR2, intel_vbt_defs.h:426
        2 => 270000, // HBR, :427
        3 => 162000, // LBR, :428
        _ => 810000, // HBR3 (0, :425) and `default:`
    }
}

/// A decoded child device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildDevice {
    /// `display->vbt.version` (BDB version) the accessors gate on.
    pub bdb_version: u16,
    raw: [u8; CHILD_DEVICE_CONFIG_SIZE],
}

impl ChildDevice {
    /// Copy `min(child_dev_size, 44)` bytes into a zeroed struct — intel_bios.c:2942-2944.
    pub fn from_bytes(bdb_version: u16, bytes: &[u8]) -> ChildDevice {
        let mut raw = [0u8; CHILD_DEVICE_CONFIG_SIZE];
        let n = bytes.len().min(CHILD_DEVICE_CONFIG_SIZE);
        raw[..n].copy_from_slice(&bytes[..n]);
        ChildDevice { bdb_version, raw }
    }
    fn b(&self, o: usize) -> u8 {
        self.raw[o]
    }
    fn w(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.raw[o], self.raw[o + 1]])
    }
    pub fn raw(&self) -> &[u8; CHILD_DEVICE_CONFIG_SIZE] {
        &self.raw
    }
    pub fn handle(&self) -> u16 {
        self.w(0)
    }
    pub fn device_type(&self) -> u16 {
        self.w(2)
    }
    /// 4-byte-union-independent fields:
    pub fn dvo_port(&self) -> u8 {
        self.b(16)
    }
    pub fn i2c_pin(&self) -> u8 {
        self.b(17)
    }
    pub fn target_addr(&self) -> u8 {
        self.b(18)
    }
    /// VBT `ddc_pin`. On Kaby Lake with an SPT/KBP PCH `map_ddc_pin()` is the identity ("Assuming direct map",
    /// intel_bios.c:2243-2282); `intel_bios_hdmi_ddc_pin()` returns 0 for 0 (intel_bios.c:3763-3768).
    pub fn ddc_pin(&self) -> u8 {
        self.b(19)
    }
    pub fn dvo_cfg(&self) -> u8 {
        self.b(22)
    }
    /// `efp_routed` — BDB 158+, byte 23 bit 0.
    pub fn efp_routed(&self) -> bool {
        self.b(23) & 1 != 0
    }
    /// `lane_reversal` — BDB 184+, byte 23 bit 1.
    pub fn lane_reversal(&self) -> bool {
        self.b(23) & (1 << 1) != 0
    }
    /// `lspcon` — BDB 192+, byte 23 bit 2.
    pub fn lspcon(&self) -> bool {
        self.b(23) & (1 << 2) != 0
    }
    /// `iboost` — BDB 196+, byte 23 bit 3.
    pub fn iboost(&self) -> bool {
        self.b(23) & (1 << 3) != 0
    }
    /// `hpd_invert` — BDB 196+, byte 23 bit 4.
    pub fn hpd_invert(&self) -> bool {
        self.b(23) & (1 << 4) != 0
    }
    /// `use_vbt_vswing` — BDB 218+, byte 23 bit 5.
    pub fn use_vbt_vswing(&self) -> bool {
        self.b(23) & (1 << 5) != 0
    }
    /// raw `dp_max_lane_count` field — BDB 244+, byte 23 bits 6-7.
    pub fn dp_max_lane_count_field(&self) -> u8 {
        self.b(23) >> 6
    }
    /// `hdmi_support` / `dp_support` / `tmds_support` — BDB 158+, byte 24 bits 0..2.
    pub fn hdmi_support(&self) -> bool {
        self.b(24) & 1 != 0
    }
    pub fn dp_support(&self) -> bool {
        self.b(24) & (1 << 1) != 0
    }
    pub fn tmds_support(&self) -> bool {
        self.b(24) & (1 << 2) != 0
    }
    /// raw `aux_channel` byte (@25).
    pub fn aux_channel(&self) -> u8 {
        self.b(25)
    }
    /// `intel_bios_dp_aux_ch()` — intel_bios.c:3718-3724: 0 is AUX_CH_NONE, else `map_aux_ch()`.
    pub fn aux_ch(&self) -> Option<AuxCh> {
        if self.aux_channel() == 0 {
            None
        } else {
            map_aux_ch(self.aux_channel())
        }
    }
    pub fn dvo_wiring(&self) -> u8 {
        self.b(28)
    }
    /// `dp_usb_type_c` (195+), `tbt` (209+), `dedicated_external` (264+), `dyn_port_over_tc` (264+): byte 33 bits 0..3.
    pub fn dp_usb_type_c(&self) -> bool {
        self.b(33) & 1 != 0
    }
    pub fn tbt(&self) -> bool {
        self.b(33) & (1 << 1) != 0
    }
    pub fn dedicated_external(&self) -> bool {
        self.b(33) & (1 << 2) != 0
    }
    pub fn dyn_port_over_tc(&self) -> bool {
        self.b(33) & (1 << 3) != 0
    }
    /// raw `dp_iboost_level` (byte 37 low nibble, 196+) and `hdmi_iboost_level` (high nibble).
    pub fn dp_iboost_level(&self) -> u8 {
        self.b(37) & 0xf
    }
    pub fn hdmi_iboost_level(&self) -> u8 {
        self.b(37) >> 4
    }
    /// raw `dp_max_link_rate` (byte 38 bits 0-2, 216+).
    pub fn dp_max_link_rate_field(&self) -> u8 {
        self.b(38) & 7
    }
    /// `edp_data_rate_override` (BDB 263+): 12 bits at byte 40.
    pub fn edp_data_rate_override(&self) -> u16 {
        self.w(40) & 0x0fff
    }
    /// `hdmi_level_shifter_value` — byte 7 bits 0-4 (158+).
    pub fn hdmi_level_shifter_value_raw(&self) -> u8 {
        self.b(7) & 0x1f
    }
    /// raw `hdmi_max_data_rate` — byte 7 bits 5-7 (204+).
    pub fn hdmi_max_data_rate_field(&self) -> u8 {
        self.b(7) >> 5
    }

    // ---- accessors that Linux gates on the BDB version ----------------------------------------------------

    /// `intel_bios_encoder_supports_crt()` — intel_bios.c:2621.
    pub fn supports_crt(&self) -> bool {
        self.device_type() & DEVICE_TYPE_ANALOG_OUTPUT != 0
    }
    /// `intel_bios_encoder_supports_dvi()` — intel_bios.c:2627.
    pub fn supports_dvi(&self) -> bool {
        self.device_type() & DEVICE_TYPE_TMDS_DVI_SIGNALING != 0
    }
    /// `intel_bios_encoder_supports_hdmi()` — intel_bios.c:2633.
    pub fn supports_hdmi(&self) -> bool {
        self.supports_dvi() && self.device_type() & DEVICE_TYPE_NOT_HDMI_OUTPUT == 0
    }
    /// `intel_bios_encoder_supports_dp()` — intel_bios.c:2640.
    pub fn supports_dp(&self) -> bool {
        self.device_type() & DEVICE_TYPE_DISPLAYPORT_OUTPUT != 0
    }
    /// `intel_bios_encoder_supports_edp()` — intel_bios.c:2646.
    pub fn supports_edp(&self) -> bool {
        self.supports_dp() && self.device_type() & DEVICE_TYPE_INTERNAL_CONNECTOR != 0
    }
    /// `intel_bios_encoder_supports_dsi()` — intel_bios.c:2653.
    pub fn supports_dsi(&self) -> bool {
        self.device_type() & DEVICE_TYPE_MIPI_OUTPUT != 0
    }
    /// The DDI port (`intel_bios_encoder_port()`, Kaby Lake map).
    pub fn port(&self) -> Option<Port> {
        dvo_port_to_port(self.dvo_port())
    }
    /// `sanitize_device_type()` (intel_bios.c:2578-2597) for display ver < 12: port A cannot do DVI/HDMI.
    pub fn sanitize_device_type(&mut self) {
        if self.port() != Some(Port::A) || !self.supports_dvi() {
            return;
        }
        let t = (self.device_type() & !DEVICE_TYPE_TMDS_DVI_SIGNALING) | DEVICE_TYPE_NOT_HDMI_OUTPUT;
        self.raw[2..4].copy_from_slice(&t.to_le_bytes());
    }
    /// `intel_bios_hdmi_level_shift()` — intel_bios.c:2666: -1 below BDB 158 (display ver < 14).
    pub fn hdmi_level_shift(&self) -> i32 {
        if self.bdb_version < 158 {
            -1
        } else {
            i32::from(self.hdmi_level_shifter_value_raw())
        }
    }
    /// `intel_bios_hdmi_max_tmds_clock()` — intel_bios.c:2675-2697 (kHz; 0 = platform limit).
    pub fn hdmi_max_tmds_clock(&self) -> u32 {
        if self.bdb_version < 204 {
            return 0;
        }
        match self.hdmi_max_data_rate_field() {
            3 => 594000, // HDMI_MAX_DATA_RATE_594, intel_vbt_defs.h:380
            4 => 340000, // _340, :381
            5 => 300000, // _300, :382
            1 => 297000, // _297, :378
            2 => 165000, // _165, :379
            _ => 0,      // _PLATFORM (0, :377) and `default:`
        }
    }
    /// `intel_bios_dp_max_link_rate()` — intel_bios.c:2498-2507. 0 = platform default.
    pub fn dp_max_link_rate(&self) -> u32 {
        if self.bdb_version < 216 {
            0
        } else if self.bdb_version >= 230 {
            dp_max_link_rate_230(self.dp_max_link_rate_field())
        } else {
            dp_max_link_rate_216(self.dp_max_link_rate_field())
        }
    }
    /// `intel_bios_dp_max_lane_count()` — intel_bios.c:2509-2515. 0 = unspecified.
    pub fn dp_max_lane_count(&self) -> u8 {
        if self.bdb_version < 244 {
            0
        } else {
            self.dp_max_lane_count_field() + 1
        }
    }
    /// `translate_iboost()` (intel_bios.c:2181-2191): the VBT level index -> boost value 1/3/7; out of range -> 0.
    pub const fn translate_iboost(v: u8) -> u8 {
        match v {
            0 => 1,
            1 => 3,
            2 => 7,
            _ => 0,
        }
    }
    /// `intel_bios_dp_boost_level()` — intel_bios.c:3747-3753.
    pub fn dp_boost_level(&self) -> u8 {
        if self.bdb_version < 196 || !self.iboost() {
            0
        } else {
            Self::translate_iboost(self.dp_iboost_level())
        }
    }
    /// `intel_bios_hdmi_boost_level()` — intel_bios.c:3755-3761.
    pub fn hdmi_boost_level(&self) -> u8 {
        if self.bdb_version < 196 || !self.iboost() {
            0
        } else {
            Self::translate_iboost(self.hdmi_iboost_level())
        }
    }
}
