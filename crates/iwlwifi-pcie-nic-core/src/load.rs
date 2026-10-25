// SPDX-License-Identifier: GPL-2.0-only
//! The firmware image upload for the 8000/9000 families: sections of one image, split at the
//! CPU1/CPU2 separator, pushed in 128 KiB chunks through the flow handler's service channel into
//! device SRAM, with the per-section status word the secure loader watches. Ported from Linux
//! `pcie/gen1_2/trans.c:603-713` (`iwl_pcie_load_firmware_chunk_fh`, `iwl_pcie_load_firmware_chunk`,
//! `iwl_pcie_load_section`), `:715-783` (`iwl_pcie_load_cpu_sections_8000`), `:973-1008`
//! (`iwl_pcie_load_given_ucode_8000`), with `iwl-fh.h:326-327, :437-441, :465-502, :544-548`,
//! `iwl-prph.h:84-85, :243, :359, :422-424`, `fw/file.h:580-581`.
//!
//! Copyright (C) 2007-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! The debug-destination apply (`iwl_pcie_apply_destination`, trans.c:982-983) is a firmware-debug
//! feature that needs a DBGC TLV; it is not part of bring-up and is left out.
//!
//! DEVIATIONS, named. (1) Linux allocates the bounce buffer per section with `dma_alloc_coherent`
//! and falls back to PAGE_SIZE chunks if that fails (trans.c:663-673). CoralOS has no DMA free, so the
//! host allocates ONE bounce buffer of [`FH_MEM_TB_MAX_LENGTH`] once and reuses it ([`LoadEnv::stage`]);
//! there is no small-chunk fallback — an allocation that cannot give 128 KiB is a named refusal
//! upstream. (2) Linux sizes `chunk_sz = min(FH_MEM_TB_MAX_LENGTH, section->len)` per section; the port
//! does the same, so a short section uses a short chunk.

use crate::bus::{NicFault, NicIo};
use crate::regs::*;
use iwlwifi_fw_image_core::sections::Section;
use iwlwifi_pcie_trans_core::io::set_bits_prph;

/// `wait_event_timeout(…, 5 * HZ)`. // trans.c:647-648
pub const CHUNK_TIMEOUT_MS: u32 = 5_000;
/// The first section number written to FH_UCODE_LOAD_STATUS. // trans.c:721 (`sec_num = 0x1`)
pub const FIRST_SEC_NUM: u32 = 0x1;
/// The status written once all of CPU1's / CPU2's sections are in. // trans.c:773-779
pub const LOAD_STATUS_CPU1_DONE: u32 = 0xFFFF;
pub const LOAD_STATUS_CPU2_DONE: u32 = 0xFFFF_FFFF;
/// The host bounce buffer holds one full chunk. // trans.c:663
pub const FH_BOUNCE_BYTES: usize = FH_MEM_TB_MAX_LENGTH as usize;
/// `WFPM_GP2`'s pre-load value (debug only; Linux comments "no functional impact"). // trans.c:993
pub const WFPM_GP2_PRELOAD: u32 = 0x0101_0101;

/// What the host does that the registers cannot: copy bytes into DMA memory, wait for an interrupt.
pub trait LoadEnv: NicIo {
    /// Copy `chunk` into the host's coherent bounce buffer and return its BUS address. The buffer
    /// holds at least [`FH_MEM_TB_MAX_LENGTH`] bytes. // trans.c:663-678, :695
    fn stage(&mut self, chunk: &[u8]) -> Result<u64, LoadRefusal>;
    /// Block until the FH_TX interrupt reports the service channel done (`ucode_write_complete`,
    /// rx.c:2108-2115) or `timeout_ms` passes. `false` on timeout.
    fn wait_chunk_done(&mut self, timeout_ms: u32) -> bool;
    /// `iwl_enable_interrupts`: the full mask, after each CPU's sections. // trans.c:764
    fn enable_interrupts(&mut self);
}

/// The host could not stage a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadRefusal {
    NoBounceBuffer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    /// `iwl_trans_grab_nic_access` failed. // trans.c:629-630
    NicAccess(NicFault),
    Stage(LoadRefusal),
    /// "Failed to load firmware chunk!" — the FH_TX interrupt never came. // trans.c:647-653
    ChunkTimeout { section: usize, dst: u32, offset: u32 },
    /// The bounce buffer's bus address does not fit the 36-bit TFDIB address. // iwl-fh.h:597, :620-623
    DmaAddrTooWide { phys: u64 },
}

/// What was uploaded, for the log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadReport {
    pub cpu1_sections: usize,
    pub cpu2_sections: usize,
    pub chunks: usize,
    pub bytes: usize,
}

/// `iwl_get_dma_hi_addr`: the 4 address bits above 32. // iwl-fh.h:620-623
pub fn dma_hi_addr(phys: u64) -> u32 {
    ((phys >> 32) & 0xF) as u32
}

/// `iwl_pcie_load_firmware_chunk_fh`: the six service-channel writes, in this order.
/// // trans.c:603-629
pub fn load_firmware_chunk_fh<I: NicIo>(io: &mut I, dst_addr: u32, phys: u64, byte_cnt: u32) {
    io.csr_write(fh_tcsr_chnl_tx_config_reg(FH_SRVC_CHNL), FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_PAUSE); // :607-608
    io.csr_write(fh_srvc_chnl_sram_addr_reg(FH_SRVC_CHNL), dst_addr); // :610-611
    io.csr_write(fh_tfdib_ctrl0_reg(FH_SRVC_CHNL), (phys as u32) & FH_MEM_TFDIB_DRAM_ADDR_LSB_MSK); // :613-614
    io.csr_write(
        fh_tfdib_ctrl1_reg(FH_SRVC_CHNL),
        (dma_hi_addr(phys) << FH_MEM_TFDIB_REG1_ADDR_BITSHIFT) | byte_cnt, // :616-618
    );
    io.csr_write(
        fh_tcsr_chnl_tx_buf_sts_reg(FH_SRVC_CHNL),
        (1 << FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_NUM) | (1 << FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_IDX) | FH_TCSR_CHNL_TX_BUF_STS_REG_VAL_TFDB_VALID, // :620-623
    );
    io.csr_write(
        fh_tcsr_chnl_tx_config_reg(FH_SRVC_CHNL),
        FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_ENABLE | FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_DISABLE | FH_TCSR_TX_CONFIG_REG_VAL_CIRQ_HOST_ENDTFD, // :625-628
    );
}

/// `iwl_pcie_load_firmware_chunk`: grab, program, release, wait. // trans.c:631-656
fn load_firmware_chunk<E: LoadEnv>(env: &mut E, section: usize, dst: u32, offset: u32, phys: u64, byte_cnt: u32) -> Result<(), LoadError> {
    if !env.grab() {
        return Err(LoadError::NicAccess(env.fault().unwrap_or(NicFault::AccessTimeout { gp_cntrl: 0 }))); // :640-641
    }
    load_firmware_chunk_fh(env, dst, phys, byte_cnt); // :643-644
    env.release(); // :645
    if !env.wait_chunk_done(CHUNK_TIMEOUT_MS) {
        return Err(LoadError::ChunkTimeout { section, dst, offset }); // :647-653
    }
    Ok(())
}

/// `iwl_pcie_load_section`. // trans.c:658-713
fn load_section<E: LoadEnv>(env: &mut E, section_num: usize, sec: &Section<'_>, report: &mut LoadReport) -> Result<(), LoadError> {
    let len = sec.data.len();
    let chunk_sz = (FH_MEM_TB_MAX_LENGTH as usize).min(len); // :663
    let mut offset = 0usize;
    while offset < len {
        let copy_size = chunk_sz.min(len - offset); // :684
        let dst_addr = sec.offset.wrapping_add(offset as u32); // :685
        let extended = (IWL_FW_MEM_EXTENDED_START..=IWL_FW_MEM_EXTENDED_END).contains(&dst_addr); // :687-689
        if extended {
            set_bits_prph(env, LMPM_CHICK, LMPM_CHICK_EXTENDED_ADDR_SPACE); // :691-693
        }
        let staged = env.stage(&sec.data[offset..offset + copy_size]); // memcpy, :695
        let result = match staged {
            Err(r) => Err(LoadError::Stage(r)),
            Ok(phys) if phys >> 36 != 0 => Err(LoadError::DmaAddrTooWide { phys }),
            Ok(phys) => load_firmware_chunk(env, section_num, dst_addr, offset as u32, phys, copy_size as u32), // :696-697
        };
        if extended {
            let v = env.prph_read(LMPM_CHICK);
            env.prph_write(LMPM_CHICK, v & !LMPM_CHICK_EXTENDED_ADDR_SPACE); // :699-701 (iwl_clear_bits_prph)
        }
        result?; // :703-708
        report.chunks += 1;
        report.bytes += copy_size;
        offset += chunk_sz; // :680 (`offset += chunk_sz`)
    }
    Ok(())
}

/// `iwl_pcie_load_cpu_sections_8000`. `first` carries `first_ucode_section` between the CPU1 and
/// CPU2 calls exactly as Linux's pointer does. Returns the number of sections loaded. // trans.c:715-783
fn load_cpu_sections_8000<E: LoadEnv>(env: &mut E, secs: &[Section<'_>], cpu: u32, first: &mut usize, report: &mut LoadReport) -> Result<usize, LoadError> {
    let shift_param = if cpu == 1 { 0 } else { 16 }; // :725, :728
    let mut loaded = 0usize;
    if cpu == 1 {
        *first = 0; // :726
    } else {
        *first += 1; // :729
    }
    let mut sec_num: u32 = FIRST_SEC_NUM; // :721
    let mut last_read_idx = 0usize; // :722
    let mut i = *first;
    while i < secs.len() {
        last_read_idx = i; // :733
        // CPU1_CPU2_SEPARATOR_SECTION / PAGING_SEPARATOR_SECTION, or no data. // :735-748
        // (`!image->sec[i].data` is a NULL pointer in Linux; a borrowed slice is never null, and a section
        // whose TLV carried only the offset word is an empty slice the loop below simply does not iterate.)
        if secs[i].is_separator() {
            break;
        }
        load_section(env, i, &secs[i], report)?; // :750-752
        // Notify ucode of loaded section number and status. // :755-759
        let val = env.read_direct32(FH_UCODE_LOAD_STATUS);
        env.write_direct32(FH_UCODE_LOAD_STATUS, val | (sec_num << shift_param));
        sec_num = (sec_num << 1) | 0x1; // :759
        loaded += 1;
        i += 1;
    }
    *first = last_read_idx; // :762
    env.enable_interrupts(); // :764
    // Not gen2: the DONE marker goes through FH_UCODE_LOAD_STATUS. // :766-779
    env.write_direct32(FH_UCODE_LOAD_STATUS, if cpu == 1 { LOAD_STATUS_CPU1_DONE } else { LOAD_STATUS_CPU2_DONE });
    Ok(loaded)
}

/// `iwl_pcie_load_given_ucode_8000` (the debug destination apply, trans.c:982-983, is not ported).
/// // trans.c:973-1008
pub fn load_given_ucode_8000<E: LoadEnv>(env: &mut E, secs: &[Section<'_>]) -> Result<LoadReport, LoadError> {
    let mut report = LoadReport::default();
    // Set default value. On resume reading the values that were zeroed can provide debug data… // trans.c:986-993
    env.prph_write(WFPM_GP2, WFPM_GP2_PRELOAD);
    // Configure the ucode to be ready to get the secured image: release CPU reset. // trans.c:995-998
    env.prph_write(RELEASE_CPU_RESET, RELEASE_CPU_RESET_BIT);
    let mut first = 0usize;
    report.cpu1_sections = load_cpu_sections_8000(env, secs, 1, &mut first, &mut report)?; // :1001-1005
    report.cpu2_sections = load_cpu_sections_8000(env, secs, 2, &mut first, &mut report)?; // :1007-1008
    Ok(report)
}
