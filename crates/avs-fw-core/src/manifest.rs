// SPDX-License-Identifier: GPL-2.0-only
//! The envelope of a firmware or library image, exactly as `avs_fw_manifest_strip_verify` reads it (loader.c:102-136):
//! an optional extended manifest (`$AE1`, 16 bytes of header, `len` bytes in all) is stripped; the first dword of what is
//! left selects the family; the manifest proper sits at that family's fixed offset and carries the library name, the
//! preload page count and the firmware version.
//!
//! Nothing past the manifest is inspected: the sections are firmware, and the ROM parses them.
use crate::error::{FwError, FwVersion, ManifestFamily};
use crate::regs::*;

/// `struct avs_fw_manifest` (loader.c:51-59), all little-endian on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub id: u32,
    pub len: u32,
    pub name: [u8; AVS_LIB_NAME_SIZE],
    pub preload_page_count: u32,
    pub img_flags: u32,
    pub feature_mask: u32,
    pub version: FwVersion,
}

/// `struct avs_fw_ext_manifest` (loader.c:62-68).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtManifest {
    pub id: u32,
    pub len: u32,
    pub version_major: u16,
    pub version_minor: u16,
    pub entries: u32,
}

/// What `avs_fw_manifest_strip_verify` leaves: the stripped image and where its manifest is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stripped<'a> {
    /// The extended manifest, when the image had one (`None` for an image that begins at the family magic).
    pub ext: Option<ExtManifest>,
    /// Bytes removed from the front of the image (`man->len`, or 0). The CLDMA transfers `body`, NOT the whole file.
    pub stripped: usize,
    /// The image after the extended manifest - what `avs_cldma_load_basefw` / `avs_cldma_load_library` DMA.
    pub body: &'a [u8],
    pub family: ManifestFamily,
    /// Offset of the manifest inside `body` (0x284 for Skylake).
    pub manifest_offset: usize,
    pub manifest: Manifest,
    /// `Some` when the version gate failed but the caller chose to ignore it (`ignore_fw_version`, loader.c:44-47, :131-133).
    pub version_ignored: Option<FwVersion>,
}

/// Whether a version mismatch refuses (`debug_ignore_fw_version` false, the default) or is only reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionPolicy {
    Enforce,
    Ignore,
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// Parse `struct avs_fw_ext_manifest` from the front of `image`. `None` when the first dword is not `$AE1`.
pub fn parse_ext_manifest(image: &[u8]) -> Result<Option<ExtManifest>, FwError> {
    if image.len() < AVS_FW_EXT_MANIFEST_SIZE {
        return Err(FwError::ImageTooShort { have: image.len(), need: AVS_FW_EXT_MANIFEST_SIZE }); // loader.c:75-76
    }
    if le32(image, 0) != AVS_EXT_MANIFEST_MAGIC {
        return Ok(None); // loader.c:79
    }
    Ok(Some(ExtManifest {
        id: le32(image, 0),
        len: le32(image, 4),
        version_major: le16(image, 8),
        version_minor: le16(image, 10),
        entries: le32(image, 12),
    }))
}

/// `avs_fw_manifest_offset` (loader.c:87-100): the family and the fixed offset its magic selects.
pub fn manifest_family(body: &[u8]) -> Result<(ManifestFamily, usize), FwError> {
    if body.len() < 4 {
        return Err(FwError::ImageTooShort { have: body.len(), need: 4 });
    }
    match le32(body, 0) {
        SKL_MANIFEST_MAGIC => Ok((ManifestFamily::Skl, SKL_ADSPFW_OFFSET as usize)), // loader.c:93-94
        APL_MANIFEST_MAGIC => Ok((ManifestFamily::Apl, APL_ADSPFW_OFFSET as usize)), // loader.c:95-96
        magic => Err(FwError::UnknownManifestMagic { magic }),                       // loader.c:97-98
    }
}

/// Parse `struct avs_fw_manifest` at `body[offset..]`.
pub fn parse_manifest(body: &[u8], offset: usize) -> Result<Manifest, FwError> {
    let need = offset + AVS_FW_MANIFEST_SIZE;
    if body.len() < need {
        return Err(FwError::ImageTooShort { have: body.len(), need }); // loader.c:116-117
    }
    let m = &body[offset..offset + AVS_FW_MANIFEST_SIZE];
    let mut name = [0u8; AVS_LIB_NAME_SIZE];
    name.copy_from_slice(&m[8..16]);
    Ok(Manifest {
        id: le32(m, 0),
        len: le32(m, 4),
        name,
        preload_page_count: le32(m, 16),
        img_flags: le32(m, 20),
        feature_mask: le32(m, 24),
        version: FwVersion { major: le16(m, 28), minor: le16(m, 30), hotfix: le16(m, 32), build: le16(m, 34) },
    })
}

/// `avs_fw_manifest_strip_verify(adev, fw, min)` for the Skylake family (the only one Kaby Lake loads).
///
/// Refusals beyond Linux's own: an extended manifest longer than the image or shorter than its own header, an image that is
/// too short for the 4-byte magic read, and a manifest family other than the one asked for.
pub fn strip_verify<'a>(
    image: &'a [u8],
    expect: ManifestFamily,
    min: Option<FwVersion>,
    policy: VersionPolicy,
) -> Result<Stripped<'a>, FwError> {
    let ext = parse_ext_manifest(image)?;
    let mut stripped = 0usize;
    if let Some(e) = ext {
        if (e.len as usize) < AVS_FW_EXT_MANIFEST_SIZE {
            return Err(FwError::ExtManifestShorterThanHeader { ext_len: e.len });
        }
        if e.len as usize > image.len() {
            return Err(FwError::ExtManifestPastImage { ext_len: e.len, image_len: image.len() });
        }
        stripped = e.len as usize; // loader.c:79-82
    }
    let body = &image[stripped..];
    let (family, manifest_offset) = manifest_family(body)?;
    if family != expect {
        return Err(FwError::ManifestFamilyMismatch { found: family, expected: expect });
    }
    let manifest = parse_manifest(body, manifest_offset)?;
    let mut version_ignored = None;
    if let Some(min) = min {
        let v = manifest.version;
        // loader.c:122-125: major, minor and hotfix must be EQUAL to the minimum's; only build may be newer.
        if v.major != min.major || v.minor != min.minor || v.hotfix != min.hotfix || v.build < min.build {
            match policy {
                VersionPolicy::Enforce => return Err(FwError::FwVersionMismatch { found: v, min }),
                VersionPolicy::Ignore => version_ignored = Some(v), // loader.c:131-133
            }
        }
    }
    Ok(Stripped { ext, stripped, body, family, manifest_offset, manifest, version_ignored })
}

/// The minimum version of the Skylake/Kaby Lake row (`skl_desc.min_fw_version`, core.c:817) as a [`FwVersion`].
pub const fn skl_min_version() -> FwVersion {
    let (major, minor, hotfix, build) = SKL_MIN_FW_VERSION;
    FwVersion { major, minor, hotfix, build }
}
