// SPDX-License-Identifier: GPL-2.0-only
//! DMC package layout and parsing: CSS header, package header, per-stepping fw_info table, DMC header (v1/v3),
//! MMIO address sanity check. Mirrors parse_dmc_fw()/parse_dmc_fw_css()/parse_dmc_fw_package()/
//! parse_dmc_fw_header() of intel_dmc.c. Total: every malformed input is a NAMED refusal, never a panic.
//!
//! One deliberate tightening: Linux tests `offset > fw->size` (intel_dmc.c:1308) and relies on the later
//! `rem_size` checks; we use checked arithmetic throughout so a 32-bit wrap cannot alias a header.

use alloc::vec::Vec;

/// enum intel_dmc_id — intel_dmc.c:55 (DMC_FW_MAIN .. DMC_FW_PIPED, DMC_FW_MAX).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DmcId {
    Main = 0,
    PipeA = 1,
    PipeB = 2,
    PipeC = 3,
    PipeD = 4,
}
/// `DMC_FW_MAX` — the count of ids.
pub const DMC_FW_MAX: usize = 5;

impl DmcId {
    pub const ALL: [DmcId; DMC_FW_MAX] = [DmcId::Main, DmcId::PipeA, DmcId::PipeB, DmcId::PipeC, DmcId::PipeD];
    /// `is_valid_dmc_id()` — intel_dmc.c:405; None for an id the enum does not have.
    pub const fn from_u8(v: u8) -> Option<DmcId> {
        match v {
            0 => Some(DmcId::Main),
            1 => Some(DmcId::PipeA),
            2 => Some(DmcId::PipeB),
            3 => Some(DmcId::PipeC),
            4 => Some(DmcId::PipeD),
            _ => None,
        }
    }
}

/// `PACKAGE_MAX_FW_INFO_ENTRIES 20` — intel_dmc.c:254.
pub const PACKAGE_MAX_FW_INFO_ENTRIES: u32 = 20;
/// `PACKAGE_V2_MAX_FW_INFO_ENTRIES 32` — intel_dmc.c:255.
pub const PACKAGE_V2_MAX_FW_INFO_ENTRIES: u32 = 32;
/// `DMC_V1_MAX_MMIO_COUNT 8` — intel_dmc.c:256.
pub const DMC_V1_MAX_MMIO_COUNT: u32 = 8;
/// `DMC_V3_MAX_MMIO_COUNT 20` — intel_dmc.c:257.
pub const DMC_V3_MAX_MMIO_COUNT: u32 = 20;
/// `DMC_V1_MMIO_START_RANGE 0x80000` — intel_dmc.c:258 (== intel_dmc_regs.h:508).
pub const DMC_V1_MMIO_START_RANGE: u32 = 0x80000;
/// `DMC_MMIO_START_RANGE 0x80000` — intel_dmc_regs.h:506.
pub const DMC_MMIO_START_RANGE: u32 = 0x80000;
/// `DMC_MMIO_END_RANGE 0x8FFFF` — intel_dmc_regs.h:507.
pub const DMC_MMIO_END_RANGE: u32 = 0x8FFFF;

/// sizeof(struct intel_css_header): 10 u32 + reserved1[12] + version + reserved2[8] + kernel_header_info
/// — intel_dmc.c:262-304 (header_len must be 32 dwords).
pub const CSS_HEADER_SIZE: usize = 128;
/// Offset of `version` in intel_css_header (after 10 + 12 u32) — intel_dmc.c:262-304.
const CSS_VERSION_OFF: usize = 88;
/// Offset of `header_len` in intel_css_header (second u32) — intel_dmc.c:262-304.
const CSS_HEADER_LEN_OFF: usize = 4;
/// sizeof(struct intel_package_header): u8,u8,u8[10],u32 — intel_dmc.c:322-333.
pub const PACKAGE_HEADER_SIZE: usize = 16;
/// sizeof(struct intel_fw_info): u8,u8,char,char,u32,u32 — intel_dmc.c:306-320.
pub const FW_INFO_SIZE: usize = 12;
/// sizeof(struct intel_dmc_header_base): signature,u8,u8,u16,project,fw_size,fw_version — intel_dmc.c:335-356.
pub const DMC_HEADER_BASE_SIZE: usize = 20;
/// sizeof(struct intel_dmc_header_v1): base + mmio_count + 8 addr + 8 data + dfile[32] + reserved1[2]
/// — intel_dmc.c:358-374.
pub const DMC_HEADER_V1_SIZE: usize = DMC_HEADER_BASE_SIZE + 4 + 4 * 8 + 4 * 8 + 32 + 8;
/// sizeof(struct intel_dmc_header_v3): base + start_mmioaddr + reserved[9] + dfile[32] + mmio_count + 20 addr
/// + 20 data — intel_dmc.c:376-396.
pub const DMC_HEADER_V3_SIZE: usize = DMC_HEADER_BASE_SIZE + 4 + 36 + 32 + 4 + 4 * 20 + 4 * 20;

/// Display stepping the package is matched against (`struct stepping_info`, intel_dmc.c:397). Linux seeds it
/// `{'*','*'}` (intel_dmc.c:1278) and overrides from the display step name; the caller supplies the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stepping {
    pub stepping: u8,
    pub substepping: u8,
}
impl Stepping {
    /// `{ '*', '*' }` — intel_dmc.c:1278.
    pub const WILDCARD: Stepping = Stepping { stepping: b'*', substepping: b'*' };
}

/// One validated firmware image for one DMC id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmcImage {
    pub id: DmcId,
    /// DMC header version (1 or 3).
    pub header_ver: u8,
    /// `start_mmioaddr`: where DMC_PROGRAM(start, i) writes dword i (v1: DMC_V1_MMIO_START_RANGE).
    pub start_mmioaddr: u32,
    /// Program memory, `fw_size` dwords, little endian decoded.
    pub payload: Vec<u32>,
    /// Validated (offset, value) register pairs in header order.
    pub mmio: Vec<(u32, u32)>,
    /// `fw_version` from the DMC header (informational).
    pub fw_version: u32,
}

/// A whole validated package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmcPackage {
    /// css_header->version (dmc->version), DMC_VERSION(major, minor).
    pub version: u32,
    pub images: Vec<DmcImage>,
}
impl DmcPackage {
    pub fn image(&self, id: DmcId) -> Option<&DmcImage> {
        self.images.iter().find(|i| i.id == id)
    }
}

/// Named refusals; each says what refused and the offending value/bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmcError {
    /// "Truncated DMC firmware, refusing." (intel_dmc.c:1188, and the other error_truncated paths)
    Truncated { what: &'static str, need: usize, have: usize },
    /// "DMC firmware has wrong CSS header length" (intel_dmc.c:1261).
    WrongCssHeaderLen { bytes: u32 },
    /// "DMC firmware has unknown header version" (intel_dmc.c:1211).
    UnknownPackageVersion { ver: u8 },
    /// "DMC firmware has wrong package header length" (intel_dmc.c:1225).
    WrongPackageHeaderLen { bytes: u32, expected: u32 },
    /// WARN_ON(num_entries > max_entries) (intel_dmc.c:1231): Linux clamps; we refuse and name it.
    TooManyEntries { num: u32, max: u32 },
    /// "Unknown DMC fw header version" (intel_dmc.c:1104).
    UnknownHeaderVersion { ver: u8 },
    /// "DMC firmware has wrong dmc header length" (intel_dmc.c:1110).
    WrongDmcHeaderLen { bytes: u32, expected: u32 },
    /// "DMC firmware has wrong mmio count" (intel_dmc.c:1117).
    WrongMmioCount { count: u32, max: u32 },
    /// "DMC firmware has Wrong MMIO Addresses" (intel_dmc.c:1123); names the first offender.
    MmioAddrOutOfRange { addr: u32, start: u32, end: u32 },
    /// "Unknown mmio range for sanity check" (intel_dmc.c:1040): a pipe-DMC header
    /// version 3 image has no known range for Kaby Lake; refused rather than guessed.
    UnknownMmioRange { id: u8 },
    /// "DMC FW too big" (intel_dmc.c:1173).
    PayloadTooBig { bytes: u32, max: u32 },
    /// An fw_info `offset` that, in bytes, does not fit (Linux: "Reading beyond the fw_size").
    OffsetBeyondFirmware { offset_dwords: u32, size: usize },
    /// "DMC firmware main program not found" (intel_dmc.c:1318).
    MainProgramMissing,
}

fn rd32(d: &[u8], off: usize) -> Option<u32> {
    let b = d.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// `fw_info_matches_stepping()` — intel_dmc.c:966-985.
pub fn fw_info_matches_stepping(fw_stepping: u8, fw_sub: u8, si: Stepping) -> bool {
    (fw_sub == b'*' && si.stepping == fw_stepping)
        || (si.stepping == fw_stepping && si.substepping == fw_sub)
        // "If we don't find a more specific one from above two checks, we then check for the generic one"
        || (si.stepping == b'*' && si.substepping == fw_sub)
        || (fw_stepping == b'*' && fw_sub == b'*')
}

/// `dmc_mmio_addr_sanity_check()` — intel_dmc.c:1019-1050, restricted to display version 9: header v1 uses
/// DMC_MMIO_START/END_RANGE; the TGL/ADL-P ranges (intel_dmc_regs.h:509-522) belong to display ver >= 12 and so are
/// refused with `UnknownMmioRange` (Linux's own `drm_warn("Unknown mmio range")` + false for a ver < 12 part).
fn mmio_sanity(addrs: &[u32], header_ver: u8, id: DmcId) -> Result<(), DmcError> {
    let (start, end) = if header_ver == 1 {
        (DMC_MMIO_START_RANGE, DMC_MMIO_END_RANGE)
    } else {
        return Err(DmcError::UnknownMmioRange { id: id as u8 });
    };
    for &a in addrs {
        if a < start || a > end {
            return Err(DmcError::MmioAddrOutOfRange { addr: a, start, end });
        }
    }
    Ok(())
}

/// `parse_dmc_fw_header()` — intel_dmc.c:1052-1190. `data` starts at the DMC header, `rem` is its extent.
fn parse_header(
    data: &[u8],
    id: DmcId,
    max_fw_size: u32,
) -> Result<DmcImage, DmcError> {
    let trunc = |what, need, have| DmcError::Truncated { what, need, have };
    if data.len() < DMC_HEADER_BASE_SIZE {
        return Err(trunc("dmc header base", DMC_HEADER_BASE_SIZE, data.len()));
    }
    // base: signature u32 @0, header_len u8 @4, header_ver u8 @5, dmcc_ver u16 @6, project @8, fw_size @12, fw_version @16
    let header_len = data[4];
    let header_ver = data[5];
    let fw_size = rd32(data, 12).unwrap_or(0);
    let fw_version = rd32(data, 16).unwrap_or(0);

    let (count_off, addr_off, data_off, count_max, header_len_bytes, start, hdr_size): (usize, usize, usize, u32, u32, u32, usize);
    if header_ver == 3 {
        if data.len() < DMC_HEADER_V3_SIZE {
            return Err(trunc("dmc header v3", DMC_HEADER_V3_SIZE, data.len()));
        }
        // v3: start_mmioaddr @20, reserved[9] @24, dfile[32] @60, mmio_count @92, addr[20] @96, data[20] @176
        count_off = 92;
        addr_off = 96;
        data_off = 176;
        count_max = DMC_V3_MAX_MMIO_COUNT;
        header_len_bytes = u32::from(header_len) * 4; // "header_len is in dwords" (intel_dmc.c:1085)
        start = rd32(data, 20).unwrap_or(0);
        hdr_size = DMC_HEADER_V3_SIZE;
    } else if header_ver == 1 {
        if data.len() < DMC_HEADER_V1_SIZE {
            return Err(trunc("dmc header v1", DMC_HEADER_V1_SIZE, data.len()));
        }
        // v1: mmio_count @20, addr[8] @24, data[8] @56, dfile[32] @88, reserved1[2] @120
        count_off = 20;
        addr_off = 24;
        data_off = 56;
        count_max = DMC_V1_MAX_MMIO_COUNT;
        header_len_bytes = u32::from(header_len); // v1 header_len is in BYTES (intel_dmc.c:1100)
        start = DMC_V1_MMIO_START_RANGE;
        hdr_size = DMC_HEADER_V1_SIZE;
    } else {
        return Err(DmcError::UnknownHeaderVersion { ver: header_ver });
    }
    if header_len_bytes as usize != hdr_size {
        return Err(DmcError::WrongDmcHeaderLen { bytes: header_len_bytes, expected: hdr_size as u32 });
    }
    let count = rd32(data, count_off).unwrap_or(0);
    if count > count_max {
        return Err(DmcError::WrongMmioCount { count, max: count_max });
    }
    let mut addrs = Vec::new();
    let mut mmio = Vec::new();
    for i in 0..count as usize {
        let a = rd32(data, addr_off + 4 * i).unwrap_or(0);
        let v = rd32(data, data_off + 4 * i).unwrap_or(0);
        addrs.push(a);
        mmio.push((a, v));
    }
    mmio_sanity(&addrs, header_ver, id)?;

    let rem = data.len() - header_len_bytes as usize;
    let payload_bytes = (fw_size as u64) * 4; // "fw_size is in dwords" (intel_dmc.c:1167)
    if (rem as u64) < payload_bytes {
        return Err(trunc("dmc payload", payload_bytes as usize, rem));
    }
    if payload_bytes > u64::from(max_fw_size) {
        return Err(DmcError::PayloadTooBig { bytes: payload_bytes as u32, max: max_fw_size });
    }
    let p = &data[header_len_bytes as usize..header_len_bytes as usize + payload_bytes as usize];
    let payload = p.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    Ok(DmcImage { id, header_ver, start_mmioaddr: start, payload, mmio, fw_version })
}

/// `parse_dmc_fw()` — intel_dmc.c:1272-1323: CSS header, package header + fw_info table (stepping match,
/// first match per id wins — dmc_set_fw_offset, intel_dmc.c:987-1017), then each present image's DMC header.
/// Like Linux, an individual pipe image that fails to parse is dropped (Linux ignores the 0 return and leaves
/// `payload` NULL); the MAIN image is mandatory (`MainProgramMissing`) — and its own error is surfaced instead
/// of the bare ENOENT so the refusal names the real reason.
pub fn parse_package(fw: &[u8], si: Stepping, max_fw_size: u32) -> Result<DmcPackage, DmcError> {
    // --- parse_dmc_fw_css (intel_dmc.c:1248-1270)
    if fw.len() < CSS_HEADER_SIZE {
        return Err(DmcError::Truncated { what: "css header", need: CSS_HEADER_SIZE, have: fw.len() });
    }
    let css_len = rd32(fw, CSS_HEADER_LEN_OFF).unwrap_or(0).wrapping_mul(4);
    if css_len as usize != CSS_HEADER_SIZE {
        return Err(DmcError::WrongCssHeaderLen { bytes: css_len });
    }
    let version = rd32(fw, CSS_VERSION_OFF).unwrap_or(0);
    let mut read = CSS_HEADER_SIZE;

    // --- parse_dmc_fw_package (intel_dmc.c:1193-1246)
    let pk = &fw[read..];
    if pk.len() < PACKAGE_HEADER_SIZE {
        return Err(DmcError::Truncated { what: "package header", need: PACKAGE_HEADER_SIZE, have: pk.len() });
    }
    let (header_len, header_ver) = (pk[0], pk[1]);
    let num_entries = rd32(pk, 12).unwrap_or(0);
    let max_entries = match header_ver {
        1 => PACKAGE_MAX_FW_INFO_ENTRIES,
        2 => PACKAGE_V2_MAX_FW_INFO_ENTRIES,
        v => return Err(DmcError::UnknownPackageVersion { ver: v }),
    };
    let package_size = PACKAGE_HEADER_SIZE + max_entries as usize * FW_INFO_SIZE;
    if pk.len() < package_size {
        return Err(DmcError::Truncated { what: "package fw_info table", need: package_size, have: pk.len() });
    }
    if u32::from(header_len) * 4 != package_size as u32 {
        return Err(DmcError::WrongPackageHeaderLen { bytes: u32::from(header_len) * 4, expected: package_size as u32 });
    }
    if num_entries > max_entries {
        return Err(DmcError::TooManyEntries { num: num_entries, max: max_entries });
    }
    // dmc_set_fw_offset
    let mut offsets: [Option<u32>; DMC_FW_MAX] = [None; DMC_FW_MAX];
    for i in 0..num_entries as usize {
        let e = &pk[PACKAGE_HEADER_SIZE + i * FW_INFO_SIZE..][..FW_INFO_SIZE];
        // intel_fw_info: reserved1 @0, dmc_id @1, stepping @2, substepping @3, offset @4, reserved2 @8
        let raw_id = if header_ver <= 1 { 0 } else { e[1] };
        let Some(id) = DmcId::from_u8(raw_id) else { continue }; // "Unsupported firmware id" -> continue
        if offsets[id as usize].is_some() {
            continue; // "More specific versions come first" (intel_dmc.c:1005-1009)
        }
        if fw_info_matches_stepping(e[2], e[3], si) {
            offsets[id as usize] = Some(u32::from_le_bytes([e[4], e[5], e[6], e[7]]));
        }
    }
    read += package_size;

    // --- per-id headers (intel_dmc.c:1299-1312)
    let mut images = Vec::new();
    let mut main_err = None;
    for id in DmcId::ALL {
        let Some(off_dw) = offsets[id as usize] else { continue };
        let off = (off_dw as usize).checked_mul(4).and_then(|o| o.checked_add(read));
        let off = match off {
            Some(o) if o <= fw.len() => o,
            _ => {
                if id == DmcId::Main {
                    main_err = Some(DmcError::OffsetBeyondFirmware { offset_dwords: off_dw, size: fw.len() });
                }
                continue;
            }
        };
        match parse_header(&fw[off..], id, max_fw_size) {
            Ok(img) => images.push(img),
            Err(e) => {
                if id == DmcId::Main {
                    main_err = Some(e);
                }
            }
        }
    }
    if !images.iter().any(|i| i.id == DmcId::Main) {
        return Err(main_err.unwrap_or(DmcError::MainProgramMissing));
    }
    Ok(DmcPackage { version, images })
}
