// SPDX-License-Identifier: GPL-2.0-only
//! Every constant of regs.rs against the Linux literal it was read from (file:line in a comment). EXPECTED values are written
//! out by hand; none is derived from the table under test (PORT_CONVENTIONS.md section 3).
use avs_fw_core::regs::*;

#[test]
fn adsp_general_registers() {
    assert_eq!(AVS_ADSP_REG_ADSPCS, 0x04); // registers.h:25
    assert_eq!(AVS_ADSP_REG_ADSPIC, 0x08); // registers.h:26
    assert_eq!(AVS_ADSP_REG_ADSPIS, 0x0C); // registers.h:27
    assert_eq!(AVS_ADSP_ADSPIC_IPC, 0x1); // registers.h:29
    assert_eq!(AVS_ADSP_ADSPIC_CLDMA, 0x2); // registers.h:30
    assert_eq!(AVS_ADSP_ADSPIS_IPC, 0x1); // registers.h:31
    assert_eq!(AVS_ADSP_ADSPIS_CLDMA, 0x2); // registers.h:32
    assert_eq!(AVS_ADSPCS_INTERVAL_US, 500); // registers.h:38
    assert_eq!(AVS_ADSPCS_TIMEOUT_US, 10000); // registers.h:39
    assert_eq!(AVS_MAIN_CORE_MASK, 1); // registers.h:40
    assert_eq!(AVS_ADSPCS_DELAY_US, 1000); // dsp.c:15
}

#[test]
fn adspcs_core_masks_shift_the_core_mask_into_each_field() {
    // registers.h:34-37; core mask 3 (two cores) shows the shift of every bit, not only bit 0
    assert_eq!(adspcs_crst_mask(1), 0x1);
    assert_eq!(adspcs_cstall_mask(1), 0x100);
    assert_eq!(adspcs_spa_mask(1), 0x1_0000);
    assert_eq!(adspcs_cpa_mask(1), 0x100_0000);
    assert_eq!(adspcs_crst_mask(3), 0x3);
    assert_eq!(adspcs_cstall_mask(3), 0x300);
    assert_eq!(adspcs_spa_mask(3), 0x3_0000);
    assert_eq!(adspcs_cpa_mask(3), 0x300_0000);
}

#[test]
fn hipc_and_sram_windows_of_the_skylake_row() {
    assert_eq!(AVS_ADSP_HIPCCTL_BUSY, 0x1); // registers.h:42
    assert_eq!(AVS_ADSP_HIPCCTL_DONE, 0x2); // registers.h:43
    assert_eq!(SKL_ADSP_REG_HIPCCTL, 0x50); // registers.h:46,51: base 0x40 + 0x10
    assert_eq!(SKL_ADSP_SRAM_BASE_OFFSET, 0x8000); // registers.h:107
    assert_eq!(SKL_ADSP_SRAM_WINDOW_SIZE, 0x2000); // registers.h:108
    assert_eq!(AVS_FW_REG_STATUS, 0x8000); // registers.h:116 with sts_offset = SKL_ADSP_SRAM_BASE_OFFSET (core.c:776)
    assert_eq!(AVS_FW_REG_ERROR, 0x8004); // registers.h:117
}

#[test]
fn rom_status_and_timeouts() {
    assert_eq!(AVS_ROM_STS_MASK, 0xFF); // loader.c:21
    assert_eq!(AVS_ROM_INIT_DONE, 0x1); // loader.c:22
    assert_eq!(SKL_ROM_BASEFW_ENTERED, 0xF); // loader.c:23
    assert_eq!(AVS_ROM_INIT_POLLING_US, 5); // loader.c:25
    assert_eq!(SKL_ROM_INIT_TIMEOUT_US, 1_000_000); // loader.c:26
    assert_eq!(AVS_FW_INIT_POLLING_US, 500); // loader.c:30
    assert_eq!(AVS_FW_INIT_TIMEOUT_MS, 3000); // loader.c:31
    assert_eq!(AVS_FW_INIT_TIMEOUT_US, 3_000_000); // loader.c:32
    assert_eq!(AVS_CLDMA_START_DELAY_MS, 100); // loader.c:34
}

#[test]
fn cldma_stream_descriptor() {
    assert_eq!(AZX_CL_SD_BASE, 0x80); // cldma.c:15
    assert_eq!(AZX_SD_CTL_STRM_MASK, 0x00F0_0000); // cldma.c:16 GENMASK(23, 20)
    assert_eq!(AZX_SD_CTL_STRM_SHIFT, 20); // cldma.c:17
    assert_eq!(AZX_SD_BDLPL_BDLPLBA_MASK, 0xFFFF_FF80); // cldma.c:18 GENMASK(31, 7)
    assert_eq!(AZX_CL_SPBFCS, 0x20); // cldma.c:22
    assert_eq!(AZX_REG_CL_SPBFCTL, 0x24); // cldma.c:23
    assert_eq!(AZX_REG_CL_SD_SPIB, 0x28); // cldma.c:24
    assert_eq!(AVS_CL_OP_INTERVAL_US, 3); // cldma.c:26
    assert_eq!(AVS_CL_OP_TIMEOUT_US, 300); // cldma.c:27
    assert_eq!(AVS_CL_IOC_TIMEOUT_MS, 300); // cldma.c:28
    assert_eq!(AVS_CL_STREAM_INDEX, 0); // cldma.c:29
    assert_eq!(AVS_CL_STREAM_TAG, 1); // cldma.c:55: AVS_CL_STREAM_INDEX + 1
    assert_eq!(AVS_CL_DEFAULT_BUFFER_SIZE, 131072); // cldma.h:13 SZ_128K
    assert_eq!(AZX_REG_SD_CTL, 0x00); // hda_register.h:81
    assert_eq!(AZX_REG_SD_STS, 0x03); // hda_register.h:83
    assert_eq!(AZX_REG_SD_CBL, 0x08); // hda_register.h:85
    assert_eq!(AZX_REG_SD_LVI, 0x0c); // hda_register.h:86
    assert_eq!(AZX_REG_SD_BDLPL, 0x18); // hda_register.h:91
    assert_eq!(AZX_REG_SD_BDLPU, 0x1c); // hda_register.h:92
}

#[test]
fn cl_sd_is_the_stream_base_plus_the_register() {
    assert_eq!(cl_sd(AZX_REG_SD_CTL), 0x80); // the zero-valued register: pinned by where it lands, not by its value
    assert_eq!(cl_sd(AZX_REG_SD_STS), 0x83);
    assert_eq!(cl_sd(AZX_REG_SD_CBL), 0x88);
    assert_eq!(cl_sd(AZX_REG_SD_LVI), 0x8c);
    assert_eq!(cl_sd(AZX_REG_SD_BDLPL), 0x98);
    assert_eq!(cl_sd(AZX_REG_SD_BDLPU), 0x9c);
    assert_eq!(cl_sd(AZX_REG_CL_SPBFCTL), 0xa4);
    assert_eq!(cl_sd(AZX_REG_CL_SD_SPIB), 0xa8);
    // SD_CTL is the one stream register at offset 0; it must not collide with any other register of the family
    for other in [AZX_REG_SD_STS, AZX_REG_SD_CBL, AZX_REG_SD_LVI, AZX_REG_SD_BDLPL, AZX_REG_SD_BDLPU, AZX_REG_CL_SPBFCTL, AZX_REG_CL_SD_SPIB] {
        assert_ne!(AZX_REG_SD_CTL, other);
    }
}

#[test]
fn sd_ctl_and_status_bits() {
    assert_eq!(SD_CTL_STREAM_RESET, 0x01); // hda_register.h:163
    assert_eq!(SD_CTL_DMA_START, 0x02); // hda_register.h:164
    assert_eq!(SD_INT_DESC_ERR, 0x10); // hda_register.h:172
    assert_eq!(SD_INT_FIFO_ERR, 0x08); // hda_register.h:173
    assert_eq!(SD_INT_COMPLETE, 0x04); // hda_register.h:174
    // composite pinned as the OR of its NAMED components (PORT_CONVENTIONS 3a.1)
    assert_eq!(SD_INT_MASK, SD_INT_DESC_ERR | SD_INT_FIFO_ERR | SD_INT_COMPLETE);
    assert_eq!(SD_INT_MASK, 0x1C);
}

#[test]
fn bdl_page() {
    assert_eq!(BDL_SIZE, 4096); // hda_register.h:144
    assert_eq!(BDL_ENTRY_SIZE, 16); // hda_register.h:145 (BDL_SIZE / 16)
    assert_eq!(AZX_MAX_BDL_ENTRIES, 256); // hda_register.h:145
}

#[test]
fn pci_gating_registers() {
    assert_eq!(AZX_PCIREG_PGCTL, 0x44); // registers.h:16
    assert_eq!(AZX_PCIREG_CGCTL, 0x48); // registers.h:17
    assert_eq!(AZX_PGCTL_LSRMD_MASK, 0x10); // registers.h:18 BIT(4)
    assert_eq!(AZX_CGCTL_MISCBDCGE_MASK, 0x40); // registers.h:19 BIT(6)
    assert_eq!(AZX_VS_EM2_L1SEN, 0x2000); // registers.h:20 BIT(13)
    assert_eq!(AZX_REG_VS_EM2, 0x1030); // hda_register.h:119
}

#[test]
fn firmware_envelope_constants() {
    assert_eq!(AVS_EXT_MANIFEST_MAGIC, 0x31454124); // loader.c:38
    assert_eq!(SKL_MANIFEST_MAGIC, 0x0000_0006); // loader.c:39
    assert_eq!(SKL_ADSPFW_OFFSET, 0x284); // loader.c:40
    assert_eq!(APL_MANIFEST_MAGIC, 0x44504324); // loader.c:41
    assert_eq!(APL_ADSPFW_OFFSET, 0x2000); // loader.c:42
    assert_eq!(AVS_LIB_NAME_SIZE, 8); // loader.c:49
    assert_eq!(AVS_FW_MANIFEST_SIZE, 36); // loader.c:60
    assert_eq!(AVS_FW_EXT_MANIFEST_SIZE, 16); // loader.c:69
    assert_eq!(SKL_MIN_FW_VERSION, (9, 21, 0, 4732)); // core.c:817
}

#[test]
fn a_sixteen_bit_register_is_a_distinct_access_width() {
    // hda-ext-core programs SD_FORMAT / SD_LVI / STATESTS with readw/writew; the three widths must not compare equal
    use avs_fw_core::effect::Width;
    assert_ne!(Width::W16, Width::B8);
    assert_ne!(Width::W16, Width::D32);
}
