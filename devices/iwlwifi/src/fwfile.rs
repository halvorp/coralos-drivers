// SPDX-License-Identifier: GPL-2.0-only
//! The firmware file, verified and parsed once. The device never embeds firmware (the kernel embeds
//! nothing and neither does a driver): the CoralOS-side wrapper reads `iwlwifi-9260-th-b0-jf-b0-46.ucode`
//! from the data volume (`System/Firmware/intel/`) and hands the bytes in with the SHA-256 the build
//! baked. This module (1) computes the digest itself and refuses a mismatch BY NAME, (2) walks the TLVs
//! with the ported `iwl_parse_tlv_firmware` arms, (3) refuses a file this driver cannot use (no INIT or
//! REGULAR sections, no multi-queue RX capability, no PHY_SKU), and (4) keeps the section tables as
//! offsets into the one blob. Nothing is sent to the card from a file that failed any of it.
use crate::sha256::sha256;
use alloc::vec::Vec;
use iwlwifi_fw_image_core::calib::{default_calib, CalibCtrl, CalibError};
use iwlwifi_fw_image_core::cmdver::{cmd_versions, CmdVerError, CmdVersions};
use iwlwifi_fw_image_core::sections::{sections, ImageError, Section, UcodeType};
use iwlwifi_fw_tlv_core::capa::CAPA_MULTI_QUEUE_RX_SUPPORT;
use iwlwifi_fw_tlv_core::params::{parse_fw_tlvs, FwTlvInfo, ParamsError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FwError {
    /// The file is not the one the build baked (SHA-256).
    DigestMismatch { want: [u8; 32], got: [u8; 32] },
    Params(ParamsError),
    Image(ImageError),
    Calib(CalibError),
    CmdVer(CmdVerError),
    /// An image with no loadable section: the file is a different firmware family.
    NoSections { image: &'static str },
    /// The firmware does not advertise multi-queue RX; the transport programs the multi-queue RFH.
    NoMultiQueueRx,
    /// `IWL_UCODE_TLV_PHY_SKU` is absent: `iwl_mvm_nic_config` has nothing to write.
    NoPhySku,
    OutOfMemory,
}

impl FwError {
    pub fn name(&self) -> &'static str {
        match self {
            FwError::DigestMismatch { .. } => "IWL-FW-DIGEST",
            FwError::Params(_) => "IWL-FW-TLV",
            FwError::Image(_) => "IWL-FW-SECTIONS",
            FwError::Calib(_) => "IWL-FW-CALIB",
            FwError::CmdVer(_) => "IWL-FW-CMDVER",
            FwError::NoSections { .. } => "IWL-FW-NO-SECTIONS",
            FwError::NoMultiQueueRx => "IWL-FW-NO-MQ-RX",
            FwError::NoPhySku => "IWL-FW-NO-PHY-SKU",
            FwError::OutOfMemory => "IWL-FW-NOMEM",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SecRef {
    offset: u32,
    start: usize,
    len: usize,
}

/// A verified, parsed firmware file.
pub struct FwImage {
    blob: Vec<u8>,
    pub info: FwTlvInfo,
    pub digest: [u8; 32],
    init: Vec<SecRef>,
    regular: Vec<SecRef>,
    pub init_calib: CalibCtrl,
    pub regular_calib: CalibCtrl,
}

fn collect(blob: &[u8], ty: UcodeType) -> Result<Vec<SecRef>, FwError> {
    let mut v = Vec::new();
    for s in sections(blob, ty).map_err(FwError::Image)? {
        let s = s.map_err(FwError::Image)?;
        v.try_reserve(1).map_err(|_| FwError::OutOfMemory)?;
        let start = (s.data.as_ptr() as usize).wrapping_sub(blob.as_ptr() as usize);
        v.push(SecRef { offset: s.offset, start, len: s.data.len() });
    }
    Ok(v)
}

impl FwImage {
    /// Verify and parse. `want` is the digest the build baked for this file.
    pub fn load(blob: Vec<u8>, want: &[u8; 32]) -> Result<FwImage, FwError> {
        let got = sha256(&blob);
        if &got != want {
            return Err(FwError::DigestMismatch { want: *want, got });
        }
        let info = parse_fw_tlvs(&blob).map_err(FwError::Params)?;
        if info.phy_config.is_none() {
            return Err(FwError::NoPhySku);
        }
        if !info.has_capa(CAPA_MULTI_QUEUE_RX_SUPPORT) {
            return Err(FwError::NoMultiQueueRx);
        }
        let init = collect(&blob, UcodeType::Init)?;
        if init.is_empty() {
            return Err(FwError::NoSections { image: "INIT" });
        }
        let regular = collect(&blob, UcodeType::Regular)?;
        if regular.is_empty() {
            return Err(FwError::NoSections { image: "REGULAR" });
        }
        let init_calib = default_calib(&blob, UcodeType::Init).map_err(FwError::Calib)?.unwrap_or_default();
        let regular_calib = default_calib(&blob, UcodeType::Regular).map_err(FwError::Calib)?.unwrap_or_default();
        cmd_versions(&blob).map_err(FwError::CmdVer)?;
        Ok(FwImage { blob, info, digest: got, init, regular, init_calib, regular_calib })
    }

    pub fn len(&self) -> usize {
        self.blob.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blob.is_empty()
    }

    /// The sections of one image, in file order, borrowing the blob.
    pub fn sections(&self, ty: UcodeType) -> Result<Vec<Section<'_>>, FwError> {
        let refs = match ty {
            UcodeType::Init => &self.init,
            UcodeType::Regular => &self.regular,
            UcodeType::Wowlan => return Ok(Vec::new()),
        };
        let mut v = Vec::new();
        v.try_reserve(refs.len()).map_err(|_| FwError::OutOfMemory)?;
        for r in refs {
            v.push(Section { offset: r.offset, data: &self.blob[r.start..r.start + r.len] });
        }
        Ok(v)
    }

    /// The command-version table (`iwl_fw_lookup_cmd_ver` and `_notif_ver`).
    pub fn cmd_versions(&self) -> CmdVersions<'_> {
        cmd_versions(&self.blob).unwrap_or_default()
    }

    /// The raw bytes of the file (the sim checks the upload against them).
    pub fn bytes(&self) -> &[u8] {
        &self.blob
    }
}
