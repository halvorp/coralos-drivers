// SPDX-License-Identifier: GPL-2.0-only
//! Register offsets, bit positions, timeouts and constants of the cAVS 1.5/1.8 DSP and its code-loader DMA, each with the
//! Linux FILE and LINE it was read from (PORT_CONVENTIONS.md section 1). Offsets are relative to BAR4 (`adev->dsp_ba`,
//! core.c:468) unless the name says otherwise. Linux v7.0-rc6, `sound/soc/intel/avs/` and `include/sound/`.

// ---- PCI configuration (core.c module params, registers.h) -------------------------------------------------------------
pub const AZX_PCIREG_PGCTL: u32 = 0x44; // registers.h:16
pub const AZX_PCIREG_CGCTL: u32 = 0x48; // registers.h:17
pub const AZX_PGCTL_LSRMD_MASK: u32 = 1 << 4; // registers.h:18
pub const AZX_CGCTL_MISCBDCGE_MASK: u32 = 1 << 6; // registers.h:19
/// `VS_EM2` lives in the HDA controller's own BAR0 window, not in the DSP BAR (hda_register.h:119, core.c:82).
pub const AZX_REG_VS_EM2: u32 = 0x1030; // hda_register.h:119
pub const AZX_VS_EM2_L1SEN: u32 = 1 << 13; // registers.h:20

// ---- DSP general registers ---------------------------------------------------------------------------------------------
pub const AVS_ADSP_REG_ADSPCS: u32 = 0x04; // registers.h:25
pub const AVS_ADSP_REG_ADSPIC: u32 = 0x08; // registers.h:26
pub const AVS_ADSP_REG_ADSPIS: u32 = 0x0C; // registers.h:27
pub const AVS_ADSP_ADSPIC_IPC: u32 = 0x1; // registers.h:29
pub const AVS_ADSP_ADSPIC_CLDMA: u32 = 1 << 1; // registers.h:30
pub const AVS_ADSP_ADSPIS_IPC: u32 = 0x1; // registers.h:31
pub const AVS_ADSP_ADSPIS_CLDMA: u32 = 1 << 1; // registers.h:32
/// `AVS_ADSPCS_INTERVAL_US` (registers.h:38).
pub const AVS_ADSPCS_INTERVAL_US: u32 = 500; // registers.h:38
/// `AVS_ADSPCS_TIMEOUT_US` (registers.h:39).
pub const AVS_ADSPCS_TIMEOUT_US: u32 = 10000; // registers.h:39
pub const AVS_MAIN_CORE_MASK: u32 = 0x1; // registers.h:40
/// `AVS_ADSPCS_DELAY_US` (dsp.c:15): `usleep_range(1000, 2000)` after SPA and after un-stalling; the lower bound is used.
pub const AVS_ADSPCS_DELAY_US: u32 = 1000; // dsp.c:15

/// `AVS_ADSPCS_CRST_MASK(cm)` (registers.h:34).
pub const fn adspcs_crst_mask(core_mask: u32) -> u32 {
    core_mask // registers.h:34
}
/// `AVS_ADSPCS_CSTALL_MASK(cm)` (registers.h:35).
pub const fn adspcs_cstall_mask(core_mask: u32) -> u32 {
    core_mask << 8 // registers.h:35
}
/// `AVS_ADSPCS_SPA_MASK(cm)` (registers.h:36).
pub const fn adspcs_spa_mask(core_mask: u32) -> u32 {
    core_mask << 16 // registers.h:36
}
/// `AVS_ADSPCS_CPA_MASK(cm)` (registers.h:37).
pub const fn adspcs_cpa_mask(core_mask: u32) -> u32 {
    core_mask << 24 // registers.h:37
}

// ---- Skylake-family HIPC (the Kaby Lake rows: skl_hipc_spec, core.c:767-776) -------------------------------------------
pub const AVS_ADSP_HIPCCTL_BUSY: u32 = 0x1; // registers.h:42
pub const AVS_ADSP_HIPCCTL_DONE: u32 = 1 << 1; // registers.h:43
pub const SKL_ADSP_REG_HIPCCTL: u32 = 0x40 + 0x10; // registers.h:51

// ---- SRAM windows (skl_sram_spec, core.c:752-755) ----------------------------------------------------------------------
pub const SKL_ADSP_SRAM_BASE_OFFSET: u32 = 0x8000; // registers.h:107
pub const SKL_ADSP_SRAM_WINDOW_SIZE: u32 = 0x2000; // registers.h:108
/// `AVS_FW_REG_STATUS(adev)` = `sts_offset + 0` and `sts_offset` is the SRAM base on Skylake (core.c:776, registers.h:116).
pub const AVS_FW_REG_STATUS: u32 = SKL_ADSP_SRAM_BASE_OFFSET; // registers.h:116
/// `AVS_FW_REG_ERROR(adev)` = `sts_offset + 4` (registers.h:117).
pub const AVS_FW_REG_ERROR: u32 = SKL_ADSP_SRAM_BASE_OFFSET + 4; // registers.h:117

// ---- ROM / base firmware status (loader.c) -----------------------------------------------------------------------------
pub const AVS_ROM_STS_MASK: u32 = 0xFF; // loader.c:21
pub const AVS_ROM_INIT_DONE: u32 = 0x1; // loader.c:22
pub const SKL_ROM_BASEFW_ENTERED: u32 = 0xF; // loader.c:23
pub const AVS_ROM_INIT_POLLING_US: u32 = 5; // loader.c:25
pub const SKL_ROM_INIT_TIMEOUT_US: u32 = 1_000_000; // loader.c:26
pub const AVS_FW_INIT_POLLING_US: u32 = 500; // loader.c:30
pub const AVS_FW_INIT_TIMEOUT_MS: u32 = 3000; // loader.c:31
pub const AVS_FW_INIT_TIMEOUT_US: u32 = AVS_FW_INIT_TIMEOUT_MS * 1000; // loader.c:32
pub const AVS_CLDMA_START_DELAY_MS: u32 = 100; // loader.c:34

// ---- CLDMA: stream-descriptor registers, relative to `sd_addr` = dsp_ba + AZX_CL_SD_BASE (cldma.c:15) --------------------
pub const AZX_CL_SD_BASE: u32 = 0x80; // cldma.c:15
pub const AZX_SD_CTL_STRM_SHIFT: u32 = 20; // cldma.c:17
pub const AZX_SD_CTL_STRM_MASK: u32 = 0xF << 20; // cldma.c:16
pub const AZX_SD_BDLPL_BDLPLBA_MASK: u32 = 0xFFFF_FF80; // cldma.c:18
pub const AZX_CL_SPBFCS: u32 = 0x20; // cldma.c:22
pub const AZX_REG_CL_SPBFCTL: u32 = AZX_CL_SPBFCS + 0x4; // cldma.c:23
pub const AZX_REG_CL_SD_SPIB: u32 = AZX_CL_SPBFCS + 0x8; // cldma.c:24
pub const AVS_CL_OP_INTERVAL_US: u32 = 3; // cldma.c:26
pub const AVS_CL_OP_TIMEOUT_US: u32 = 300; // cldma.c:27
pub const AVS_CL_IOC_TIMEOUT_MS: u32 = 300; // cldma.c:28
pub const AVS_CL_STREAM_INDEX: u32 = 0; // cldma.c:29
/// `.stream_tag = AVS_CL_STREAM_INDEX + 1` (cldma.c:55): stream tag 0 is reserved by the HDA spec, so the loader's tag is 1.
pub const AVS_CL_STREAM_TAG: u32 = AVS_CL_STREAM_INDEX + 1; // cldma.c:55
pub const AVS_CL_DEFAULT_BUFFER_SIZE: u32 = 128 * 1024; // cldma.h:13
pub const AZX_REG_SD_CTL: u32 = 0x00; // hda_register.h:81
pub const AZX_REG_SD_STS: u32 = 0x03; // hda_register.h:83
pub const AZX_REG_SD_CBL: u32 = 0x08; // hda_register.h:85
pub const AZX_REG_SD_LVI: u32 = 0x0c; // hda_register.h:86
pub const AZX_REG_SD_BDLPL: u32 = 0x18; // hda_register.h:91
pub const AZX_REG_SD_BDLPU: u32 = 0x1c; // hda_register.h:92
/// `BDL_SIZE` (hda_register.h:144): the BDL page `hda_cldma_init` allocates (cldma.c `snd_dma_alloc_pages(..., BDL_SIZE ...)`).
pub const BDL_SIZE: u32 = 4096; // hda_register.h:144
/// One BDL entry is four little-endian dwords: address low, address high, length, IOC (cldma.c `cldma_setup_bdle`).
pub const BDL_ENTRY_SIZE: u32 = 16; // hda_register.h:145
pub const AZX_MAX_BDL_ENTRIES: u32 = BDL_SIZE / BDL_ENTRY_SIZE; // hda_register.h:145
pub const SD_CTL_STREAM_RESET: u32 = 0x01; // hda_register.h:163
pub const SD_CTL_DMA_START: u32 = 0x02; // hda_register.h:164
pub const SD_INT_DESC_ERR: u32 = 0x10; // hda_register.h:172
pub const SD_INT_FIFO_ERR: u32 = 0x08; // hda_register.h:173
pub const SD_INT_COMPLETE: u32 = 0x04; // hda_register.h:174
pub const SD_INT_MASK: u32 = SD_INT_DESC_ERR | SD_INT_FIFO_ERR | SD_INT_COMPLETE; // hda_register.h:175

/// Absolute BAR4 offset of a stream-descriptor register of the code loader.
pub const fn cl_sd(reg: u32) -> u32 {
    AZX_CL_SD_BASE + reg
}

// ---- Firmware envelope (loader.c) --------------------------------------------------------------------------------------
pub const AVS_EXT_MANIFEST_MAGIC: u32 = 0x31454124; // loader.c:38
pub const SKL_MANIFEST_MAGIC: u32 = 0x0000_0006; // loader.c:39
pub const SKL_ADSPFW_OFFSET: u32 = 0x284; // loader.c:40
pub const APL_MANIFEST_MAGIC: u32 = 0x44504324; // loader.c:41
pub const APL_ADSPFW_OFFSET: u32 = 0x2000; // loader.c:42
pub const AVS_LIB_NAME_SIZE: usize = 8; // loader.c:49
/// `sizeof(struct avs_fw_manifest)` (loader.c:60).
pub const AVS_FW_MANIFEST_SIZE: usize = 36; // loader.c:60
/// `sizeof(struct avs_fw_ext_manifest)` (loader.c:69).
pub const AVS_FW_EXT_MANIFEST_SIZE: usize = 16; // loader.c:69
/// `skl_desc.min_fw_version` = `{ 9, 21, 0, 4732 }` (core.c:817).
pub const SKL_MIN_FW_VERSION: (u16, u16, u16, u16) = (9, 21, 0, 4732); // core.c:817
