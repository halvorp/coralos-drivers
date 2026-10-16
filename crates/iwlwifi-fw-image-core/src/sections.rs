// SPDX-License-Identifier: GPL-2.0-only
//! `iwl_store_ucode_sec` and the SEC_* TLV arms of `iwl_parse_tlv_firmware`: the ordered section list
//! of the INIT and the REGULAR image. Ported from Linux `iwl-drv.c:493-520, :999-1008, :1031-1042`,
//! `fw/file.h:68-75, :580-581`, `fw/img.h:25-31`.

use iwlwifi_fw_tlv_core::container::{tlvs, ContainerError, Tlv, Tlvs};

pub const IWL_UCODE_TLV_SEC_RT: u32 = 19; // fw/file.h:68
pub const IWL_UCODE_TLV_SEC_INIT: u32 = 20; // fw/file.h:69
pub const IWL_UCODE_TLV_SEC_WOWLAN: u32 = 21; // fw/file.h:70
pub const IWL_UCODE_TLV_SECURE_SEC_RT: u32 = 24; // fw/file.h:73
pub const IWL_UCODE_TLV_SECURE_SEC_INIT: u32 = 25; // fw/file.h:74
pub const IWL_UCODE_TLV_SECURE_SEC_WOWLAN: u32 = 26; // fw/file.h:75

/// Between CPU1's sections and CPU2's: its "offset" word is this, its data is not loaded.
pub const CPU1_CPU2_SEPARATOR_SECTION: u32 = 0xFFFF_CCCC; // fw/file.h:580
/// Between CPU2's non-paged sections and its paging sections.
pub const PAGING_SEPARATOR_SECTION: u32 = 0xAAAA_BBBB; // fw/file.h:581

/// `enum iwl_ucode_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UcodeType {
    Regular = 0, // fw/img.h:26
    Init = 1,    // fw/img.h:27
    Wowlan = 2,  // fw/img.h:28
}

impl UcodeType {
    /// The SEC_* and SECURE_SEC_* TLV types that feed this image. // iwl-drv.c:999-1013, :1031-1048
    pub fn carries(self, tlv_type: u32) -> bool {
        match self {
            UcodeType::Regular => matches!(tlv_type, IWL_UCODE_TLV_SEC_RT | IWL_UCODE_TLV_SECURE_SEC_RT),
            UcodeType::Init => matches!(tlv_type, IWL_UCODE_TLV_SEC_INIT | IWL_UCODE_TLV_SECURE_SEC_INIT),
            UcodeType::Wowlan => matches!(tlv_type, IWL_UCODE_TLV_SEC_WOWLAN | IWL_UCODE_TLV_SECURE_SEC_WOWLAN),
        }
    }
}

/// `struct fw_sec`: the device address the bytes load to, and the bytes. // iwl-drv.c:511-515
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub offset: u32,
    pub data: &'a [u8],
}

impl Section<'_> {
    /// A separator carries no loadable bytes. // pcie/gen1_2/trans.c:741-743 (:808-810 for the non-8000 walk)
    pub fn is_separator(&self) -> bool {
        self.offset == CPU1_CPU2_SEPARATOR_SECTION || self.offset == PAGING_SEPARATOR_SECTION
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    Container(ContainerError),
    /// A section TLV shorter than its 4-byte offset word. Linux computes `size - 4` on a size_t.
    /// // iwl-drv.c:515
    SectionTooShort { tlv_len: usize },
}

/// The sections of one image, in file order. // iwl-drv.c:493-520: appended in the order the TLVs appear
pub struct Sections<'a> {
    tlvs: Tlvs<'a>,
    ty: UcodeType,
    failed: bool,
}

pub fn sections(blob: &[u8], ty: UcodeType) -> Result<Sections<'_>, ImageError> {
    Ok(Sections { tlvs: tlvs(blob).map_err(ImageError::Container)?, ty, failed: false })
}

impl<'a> Iterator for Sections<'a> {
    type Item = Result<Section<'a>, ImageError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        loop {
            let t: Tlv<'a> = match self.tlvs.next()? {
                Ok(t) => t,
                Err(e) => {
                    self.failed = true;
                    return Some(Err(ImageError::Container(e)));
                }
            };
            if !self.ty.carries(t.ty) {
                continue;
            }
            if t.data.len() < 4 {
                self.failed = true;
                return Some(Err(ImageError::SectionTooShort { tlv_len: t.data.len() }));
            }
            let offset = u32::from_le_bytes([t.data[0], t.data[1], t.data[2], t.data[3]]); // iwl-drv.c:513
            return Some(Ok(Section { offset, data: &t.data[4..] })); // iwl-drv.c:514-515
        }
    }
}

/// `iwl_pcie_get_num_sections`: sections from `start` up to the next separator.
/// // pcie/gen1_2/internal.h:852-865
pub fn num_sections(secs: &[Section<'_>], start: usize) -> usize {
    secs.get(start..).unwrap_or(&[]).iter().take_while(|s| !s.is_separator()).count()
}
