// SPDX-License-Identifier: GPL-2.0-only
//! The files the device needs and never embeds: the board record, the base firmware, the library images, the topology and the ACPI NHLT
//! table. Kickstart places them under `CoralOS:System/Firmware/avs/`; the CoralOS-side broker (the other end of [`avs_wire::fw`]) verifies
//! each against the digest the build baked and serves it in slices over the device's firmware endpoints. This module is the device half:
//! it asks, reassembles with every size and range checked, and then PARSES every file again (manifest envelope and version gate, topology
//! blocks, NHLT checksum) before it trusts a byte - the broker's verification says the file is the one the build meant, the parse says it is
//! one this device can use.
//!
//! A missing or refused file is [`FwFail`], and the status says FIRMWARE-UNAVAILABLE with the file's name as the step. The device never
//! starts a DSP boot on a partial set.
use crate::machine::{FwRecv, Machine};
use alloc::vec::Vec;
use avs_fw_core::error::ManifestFamily;
use avs_fw_core::manifest::{skl_min_version, strip_verify, VersionPolicy};
use avs_fw_core::regs::AVS_LIB_NAME_SIZE;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use avs_wire::fw::{check_name, decode_stat_reply, FwRead, FwStat, FW_SLICE};
use avs_wire::msg::split_reply;
use avs_wire::{frame, Header, Opcode, Status, FLAG_REPLY, MAX_FRAME_BYTES};

/// The base firmware (`dsp_basefw.bin`, loader.c `skl_desc` row; Atlas ships 9.21.0.4899).
pub const BASEFW_FILE: &str = "skl/dsp_basefw.bin";
/// The headset's topology: DA7219 on SSP1 (`avs/boards/da7219.c` names `da7219-tplg.bin`).
pub const TOPOLOGY_FILE: &str = "da7219-tplg.bin";
/// The ACPI NHLT table, extracted by the loader from the platform tables.
pub const NHLT_FILE: &str = "nhlt.bin";
/// Library images are `skl/<name>` where `<name>` is the topology's library name (loader.c:509-510 below the avs root).
pub const LIB_DIR: &str = "skl/";

/// A broker exchange is given this long. The broker is a local service reading a file it already has.
pub const EXCHANGE_TIMEOUT_MS: u32 = 2000;

// VALIDATION limits: a file larger than this is not what it says it is. They bound a request's allocation; they are not table sizes.
pub const MAX_BOARD_BYTES: u64 = 4096;
pub const MAX_BASEFW_BYTES: u64 = 4 << 20;
pub const MAX_LIB_BYTES: u64 = 4 << 20;
pub const MAX_TOPOLOGY_BYTES: u64 = 1 << 20;
pub const MAX_NHLT_BYTES: u64 = 256 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwError {
    /// No broker answered the firmware endpoint.
    Unavailable,
    /// The broker did not answer in time.
    Timeout,
    NotFound,
    BadName,
    /// The reply did not match the request (wrong opcode, id, length, or a malformed frame).
    BadReply(&'static str),
    TooLarge { size: u64, max: u64 },
    OutOfMemory,
    /// The broker refused for a reason other than not-found.
    Refused(Status),
}

impl FwError {
    pub const fn name(&self) -> &'static str {
        match self {
            FwError::Unavailable => "fw-broker-unavailable",
            FwError::Timeout => "fw-broker-timeout",
            FwError::NotFound => "fw-not-found",
            FwError::BadName => "fw-bad-name",
            FwError::BadReply(_) => "fw-bad-reply",
            FwError::TooLarge { .. } => "fw-too-large",
            FwError::OutOfMemory => "fw-out-of-memory",
            FwError::Refused(_) => "fw-refused",
        }
    }
}

fn exchange<M: Machine>(m: &mut M, id: u64, opcode: Opcode, payload: &[u8], buf: &mut [u8]) -> Result<usize, FwError> {
    let mut req = [0u8; 256];
    let n = frame(opcode, 0, id, payload, &mut req).map_err(|_| FwError::BadName)?;
    match m.fw_exchange(&req[..n], buf, EXCHANGE_TIMEOUT_MS) {
        FwRecv::Frame(n) => Ok(n),
        FwRecv::Timeout => Err(FwError::Timeout),
        FwRecv::Unavailable => Err(FwError::Unavailable),
    }
}

fn reply(frame: &[u8], id: u64, op: Opcode) -> Result<&[u8], FwError> {
    let (h, payload) = Header::decode(frame).map_err(|_| FwError::BadReply("header"))?;
    if h.opcode != op || h.flags != FLAG_REPLY || h.request_id != id {
        return Err(FwError::BadReply("correlation"));
    }
    let (st, body) = split_reply(payload).map_err(|_| FwError::BadReply("status"))?;
    match st {
        Status::Ok => Ok(body),
        Status::FwNotFound => Err(FwError::NotFound),
        other => Err(FwError::Refused(other)),
    }
}

/// Fetch `name` whole, refusing a file larger than `max`.
pub fn fetch<M: Machine>(m: &mut M, name: &str, max: u64) -> Result<Vec<u8>, FwError> {
    check_name(name.as_bytes()).map_err(|_| FwError::BadName)?;
    let mut payload = [0u8; 128];
    let mut buf = alloc::vec![0u8; 0];
    buf.try_reserve_exact(MAX_FRAME_BYTES).map_err(|_| FwError::OutOfMemory)?;
    buf.resize(MAX_FRAME_BYTES, 0);
    let n = FwStat { name: name.as_bytes() }.encode(&mut payload).map_err(|_| FwError::BadName)?;
    let id = 1;
    let got = exchange(m, id, Opcode::FwStat, &payload[..n], &mut buf)?;
    let size = decode_stat_reply(reply(&buf[..got], id, Opcode::FwStat)?).map_err(|_| FwError::BadReply("stat body"))?;
    if size > max {
        return Err(FwError::TooLarge { size, max });
    }
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve_exact(size as usize).map_err(|_| FwError::OutOfMemory)?;
    let mut off = 0u64;
    while off < size {
        let len = (size - off).min(FW_SLICE as u64) as u32;
        let n = FwRead { name: name.as_bytes(), offset: off, len }.encode(&mut payload).map_err(|_| FwError::BadName)?;
        let id = 2 + off / FW_SLICE as u64;
        let got = exchange(m, id, Opcode::FwRead, &payload[..n], &mut buf)?;
        let data = reply(&buf[..got], id, Opcode::FwRead)?;
        if data.len() != len as usize {
            return Err(FwError::BadReply("slice length"));
        }
        out.extend_from_slice(data);
        off += len as u64;
    }
    Ok(out)
}

/// Why the set could not be loaded: the file and what was wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FwFail {
    pub file: &'static str,
    pub why: &'static str,
}

/// The loaded, parsed-and-checked set. The byte vectors are owned here; a boot borrows them.
pub struct Files {
    pub base: Vec<u8>,
    pub base_body_at: usize,
    pub topology: Vec<u8>,
    pub nhlt: Vec<u8>,
    pub libs: Vec<LibFile>,
}

pub struct LibFile {
    pub bytes: Vec<u8>,
    pub body_at: usize,
    pub name: [u8; AVS_LIB_NAME_SIZE],
}

impl Files {
    pub fn base_body(&self) -> &[u8] {
        &self.base[self.base_body_at..]
    }
}

fn fail(file: &'static str, why: &'static str) -> FwFail {
    FwFail { file, why }
}

/// Fetch and check everything a boot needs. `file` names the first thing that is wrong.
pub fn load_all<M: Machine>(m: &mut M) -> Result<Files, FwFail> {
    let base = fetch(m, BASEFW_FILE, MAX_BASEFW_BYTES).map_err(|e| fail("basefw", e.name()))?;
    // the envelope and the version gate (loader.c avs_fw_manifest_strip_verify; skl_desc min version)
    let base_body_at = {
        let s = strip_verify(&base, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Enforce).map_err(|e| fail("basefw", e.name()))?;
        s.stripped
    };
    let topology = fetch(m, TOPOLOGY_FILE, MAX_TOPOLOGY_BYTES).map_err(|e| fail("topology", e.name()))?;
    let topo = Topology::parse(&topology, None).map_err(|_| fail("topology", "topology-parse"))?;
    let nhlt = fetch(m, NHLT_FILE, MAX_NHLT_BYTES).map_err(|e| fail("nhlt", e.name()))?;
    Nhlt::parse(&nhlt, true).map_err(|_| fail("nhlt", "nhlt-parse"))?;
    let mut libs: Vec<LibFile> = Vec::new();
    libs.try_reserve_exact(topo.libs.len()).map_err(|_| fail("libraries", "fw-out-of-memory"))?;
    for lib in topo.libs.iter() {
        let mut name = alloc::string::String::new();
        name.try_reserve(LIB_DIR.len() + AVS_LIB_NAME_SIZE).map_err(|_| fail("libraries", "fw-out-of-memory"))?;
        name.push_str(LIB_DIR);
        let raw = lib.name.as_bytes();
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        name.push_str(core::str::from_utf8(&raw[..end]).map_err(|_| fail("libraries", "fw-bad-name"))?);
        let bytes = fetch(m, &name, MAX_LIB_BYTES).map_err(|e| fail("library", e.name()))?;
        let (body_at, lib_name) = {
            let s = strip_verify(&bytes, ManifestFamily::Skl, None, VersionPolicy::Enforce).map_err(|e| fail("library", e.name()))?;
            (s.stripped, s.manifest.name)
        };
        libs.push(LibFile { bytes, body_at, name: lib_name });
    }
    Ok(Files { base, base_body_at, topology, nhlt, libs })
}
