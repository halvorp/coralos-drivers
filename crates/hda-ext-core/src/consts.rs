// SPDX-License-Identifier: GPL-2.0-only
//! Constants that are not in `hda_register.h` (those are in [`crate::regs`]), each with the Linux FILE and LINE it was read from.

/// The cAVS audio controller of Kaby Lake-LP: `PCI_DEVICE_ID_INTEL_HDA_KBL_LP`, bound to `skl_desc` (avs/core.c:895).
pub const PCI_VENDOR_INTEL: u32 = 0x8086; // include/linux/pci_ids.h:2688
pub const PCI_DEVICE_HDA_KBL_LP: u32 = 0x9d71; // include/linux/pci_ids.h:3134

/// `HDAC_MAX_CAPS` (controller.c:399): how many capability headers the chain walk follows.
pub const HDAC_MAX_CAPS: u32 = 10; // controller.c:399
/// `HDA_MAX_CODECS` (hdaudio.h:262): codec addresses on the link (also the width of `STATESTS_INT_MASK`).
pub const HDA_MAX_CODECS: u32 = 8; // hdaudio.h:262

/// Stream descriptor `n` lives at `0x80 + 0x20 * n` (stream.c:98-99: "SDI0=0x80, SDI1=0xa0, ... SDO3=0x160").
pub const SD_BASE: u32 = 0x80; // stream.c:98
pub const SD_INTERVAL: u32 = 0x20; // stream.c:98
/// The stream tag is a 4-bit field (`SD_CTL_STREAM_TAG_MASK = 0xf << 20`, hda_register.h:168); tag 0 is reserved by the HDA spec.
pub const STREAM_TAG_MAX: u32 = 0xf; // hda_register.h:168

/// `AZX_VS_EM2_DUM` (avs/registers.h:21): set after the controller leaves reset "to address incorrect position reporting for capture
/// streams" (avs/core.c:121-125).
pub const AZX_VS_EM2_DUM: u32 = 1 << 23; // avs/registers.h:21

/// Link reset waits: 100 ms in 500 us steps (controller.c:497-500, :516-518); the settle delays after them (controller.c:538, :544).
pub const LINK_RESET_POLL_INTERVAL_US: u32 = 500; // controller.c:500
pub const LINK_RESET_TIMEOUT_US: u32 = 100_000; // controller.c:497
pub const LINK_RESET_ENTER_SETTLE_US: u32 = 500; // controller.c:538 (usleep_range(500, 1000): the lower bound)
pub const LINK_RESET_EXIT_SETTLE_US: u32 = 1000; // controller.c:544 (usleep_range(1000, 1200))
/// Stream reset waits: `snd_hdac_stream_readb_poll(..., 3, 300)` (stream.c:236, :244).
pub const STREAM_RESET_POLL_INTERVAL_US: u32 = 3; // stream.c:236
pub const STREAM_RESET_TIMEOUT_US: u32 = 300; // stream.c:236

// ---- stream format word (SDnFMT), include/sound/hda_verbs.h:211-229 -----------------------------------------------------
pub const AC_FMT_CHAN_MASK: u32 = 0x0f; // hda_verbs.h:212
pub const AC_FMT_BITS_SHIFT: u32 = 4; // hda_verbs.h:213
pub const AC_FMT_BITS_8: u32 = 0 << 4; // hda_verbs.h:215
pub const AC_FMT_BITS_16: u32 = 1 << 4; // hda_verbs.h:216
pub const AC_FMT_BITS_20: u32 = 2 << 4; // hda_verbs.h:217
pub const AC_FMT_BITS_24: u32 = 3 << 4; // hda_verbs.h:218
pub const AC_FMT_BITS_32: u32 = 4 << 4; // hda_verbs.h:219
pub const AC_FMT_DIV_SHIFT: u32 = 8; // hda_verbs.h:220
pub const AC_FMT_MULT_SHIFT: u32 = 11; // hda_verbs.h:222
pub const AC_FMT_BASE_48K: u32 = 0 << 14; // hda_verbs.h:225
pub const AC_FMT_BASE_44K: u32 = 1 << 14; // hda_verbs.h:226
pub const AC_FMT_TYPE_PCM: u32 = 0 << 15; // hda_verbs.h:228

/// `AZX_MAX_BDL_ENTRIES` = `BDL_SIZE / 16` (hda_register.h:145): FIXED BY HARDWARE - the list is one 4 KiB page of 16-byte entries.
pub const AZX_MAX_BDL_ENTRIES: u32 = crate::regs::BDL_SIZE / 16; // hda_register.h:145
