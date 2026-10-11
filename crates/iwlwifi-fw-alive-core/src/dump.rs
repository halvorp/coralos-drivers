// SPDX-License-Identifier: GPL-2.0-only
//! One-line diagnostic markers for a driver with no serial console: the firmware error dump
//! (`iwl_fwrt_dump_lmac_error_log` + `iwl_fwrt_dump_umac_error_log`, Linux
//! `drivers/net/wireless/intel/iwlwifi/fw/dump.c:99-230`) condensed to its decisive fields, and the
//! ALIVE-timeout register snapshot of `iwl_mvm_load_ucode_wait_alive` / `iwl_mvm_print_pd_notification`
//! (`drivers/net/wireless/intel/iwlwifi/mvm/fw.c:263-391`) for the 9000 family, with the register
//! addresses from `drivers/net/wireless/intel/iwlwifi/iwl-prph.h:400-413`.
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! Linux prints one line per field; a ring buffer read over HDMI/QR wants ONE line per event. The
//! fields kept are the ones Linux's own triage reads first: the assert id and its name, the program
//! counter, the branch / interrupt links, the three data words, the last host command.

use core::fmt;

use crate::error_table::*;

/// `MVM_UCODE_ALIVE_TIMEOUT` (2 * HZ), in ms. // mvm/fw.c:28
pub const MVM_UCODE_ALIVE_TIMEOUT_MS: u32 = 2000; // mvm/fw.c:28

pub const SB_CPU_1_STATUS: u32 = 0xA0_1E30; // iwl-prph.h:400
pub const SB_CPU_2_STATUS: u32 = 0xA0_1E34; // iwl-prph.h:401
pub const UREG_UMAC_CURRENT_PC: u32 = 0xA0_5C18; // iwl-prph.h:405
pub const UREG_LMAC1_CURRENT_PC: u32 = 0xA0_5C1C; // iwl-prph.h:406
pub const WFPM_ARC1_PD_NOTIFICATION: u32 = 0xA0_3044; // iwl-prph.h:410
pub const HPM_SECONDARY_DEVICE_STATE: u32 = 0xA0_3404; // iwl-prph.h:411
pub const WFPM_MAC_OTP_CFG7_ADDR: u32 = 0xA0_3338; // iwl-prph.h:412
pub const WFPM_MAC_OTP_CFG7_DATA: u32 = 0xA0_333C; // iwl-prph.h:413

/// The periphery registers Linux reads on an ALIVE timeout for 8000 <= family < 22000, in its print
/// order: SecBoot status (mvm/fw.c:355-360), the PD notification block for family <= 9000
/// (:263-284), the UMAC / LMAC program counters for family >= 9000, not CDB (:374-381).
pub const ALIVE_TIMEOUT_PRPH_REGS: [(&str, u32); 8] = [
    ("sb_cpu1", SB_CPU_1_STATUS),
    ("sb_cpu2", SB_CPU_2_STATUS),
    ("pd_notif", WFPM_ARC1_PD_NOTIFICATION),
    ("hpm_sec_state", HPM_SECONDARY_DEVICE_STATE),
    ("otp_cfg7_addr", WFPM_MAC_OTP_CFG7_ADDR),
    ("otp_cfg7_data", WFPM_MAC_OTP_CFG7_DATA),
    ("umac_pc", UREG_UMAC_CURRENT_PC),
    ("lmac_pc", UREG_LMAC1_CURRENT_PC),
];

/// Which image was running. // `fwrt->cur_fw_img`, dump.c:152
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Image {
    Init,
    Regular,
}

impl Image {
    fn name(self) -> &'static str {
        match self {
            Image::Init => "INIT",
            Image::Regular => "RT",
        }
    }
}

/// `[IWL-ALIVE-TIMEOUT img=… sb_cpu1=0x… … lmac_pc=0x…]`: `values[i]` is the read of
/// `ALIVE_TIMEOUT_PRPH_REGS[i]`.
pub struct AliveTimeoutMarker {
    pub image: Image,
    pub values: [u32; 8],
}

impl fmt::Display for AliveTimeoutMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[IWL-ALIVE-TIMEOUT img={}", self.image.name())?;
        for ((name, _), v) in ALIVE_TIMEOUT_PRPH_REGS.iter().zip(self.values) {
            write!(f, " {name}=0x{v:08x}")?;
        }
        f.write_str("]")
    }
}

/// `[IWL-FW-ERROR img=… id=0x… desc=… pc=0x… blink1=0x… blink2=0x… ilink1=0x… ilink2=0x…
/// data1=0x… data2=0x… data3=0x… hcmd=0x… last_cmd=0x… fw=major.minor(hex, as iwl-drv.c:1084 prints it) umac_id=0x… umac_desc=…
/// umac_data1=0x… umac_cmd=0x…]`. `blink1` exists only in the UMAC table (dump.c:81); every other
/// unprefixed field is the LMAC table's. With no UMAC table (pointer zero, dump.c:105-108) the
/// UMAC fields print as `umac=none`.
pub struct FwErrorMarker<'a> {
    pub image: Image,
    pub lmac: &'a LmacErrorTable,
    pub umac: Option<&'a UmacErrorTable>,
}

impl fmt::Display for FwErrorMarker<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let l = |i| self.lmac.get(i);
        let id = l(LMAC_ERROR_ID);
        write!(f, "[IWL-FW-ERROR img={} valid={} id=0x{id:08x} desc={}", self.image.name(), l(LMAC_VALID), lookup_assert_desc(id))?;
        write!(f, " pc=0x{:08x}", l(LMAC_LOG_PC))?; // dump.c:44 log_pc
        match self.umac {
            Some(u) => write!(f, " blink1=0x{:08x}", u.get(UMAC_BLINK1))?, // dump.c:131
            None => f.write_str(" blink1=none")?,
        }
        write!(
            f,
            " blink2=0x{:08x} ilink1=0x{:08x} ilink2=0x{:08x} data1=0x{:08x} data2=0x{:08x} data3=0x{:08x}",
            l(LMAC_BLINK2),
            l(LMAC_ILINK1),
            l(LMAC_ILINK2),
            l(LMAC_DATA1),
            l(LMAC_DATA2),
            l(LMAC_DATA3)
        )?; // dump.c:205-210
        write!(f, " hcmd=0x{:08x} last_cmd=0x{:08x}", l(LMAC_HCMD), l(LMAC_LAST_CMD_ID))?; // dump.c:221, :227
        write!(f, " fw={}.{:08x}", l(LMAC_MAJOR), l(LMAC_MINOR))?; // dump.c:217-218
        match self.umac {
            Some(u) => {
                let uid = u.get(UMAC_ERROR_ID);
                write!(
                    f,
                    " umac_id=0x{uid:08x} umac_desc={} umac_data1=0x{:08x} umac_cmd=0x{:08x}]",
                    lookup_assert_desc(uid),
                    u.get(UMAC_DATA1),
                    u.get(UMAC_CMD_HEADER)
                ) // dump.c:129-135, :142
            }
            None => f.write_str(" umac=none]"),
        }
    }
}

/// `[IWL-FW-ERROR img=… hw_error_value=0x… base=0x…]`: the first table word read as a bus error, so
/// the table must not be trusted until the NIC is SW-reset and reactivated. // dump.c:169-184
pub struct HwErrorValueMarker {
    pub image: Image,
    pub base: u32,
    pub value: u32,
}

impl fmt::Display for HwErrorValueMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[IWL-FW-ERROR img={} hw_error_value=0x{:08x} base=0x{:08x}]", self.image.name(), self.value, self.base)
    }
}
