// SPDX-License-Identifier: GPL-2.0-only
//! The data types of include/uapi/drm/i915_drm.h this port exchanges on the FLATTENED wire, as plain Rust data with a
//! little-endian encoding. Pointer fields (`buffers_ptr`, `relocs_ptr`, `cliprects_ptr`, `data_ptr`, `next_extension`) do NOT
//! exist here: the closed shim flattens each pointed-to array into the payload (KBL-I915-PORT-PLAN section 2) and this crate
//! decodes it. Field names, widths and order follow the header; each type records its header line and its C `sizeof`.
use crate::codec::{u16_at, u32_at, u64_at, Writer};
use crate::error::UapiError;

/// `enum drm_i915_gem_engine_class` (i915_drm.h:176-222).
pub mod engine_class {
    pub const RENDER: u16 = 0; // i915_drm.h:176
    pub const COPY: u16 = 1; // i915_drm.h:187
    pub const VIDEO: u16 = 2; // i915_drm.h:196
    pub const VIDEO_ENHANCE: u16 = 3; // i915_drm.h:204
    pub const COMPUTE: u16 = 4; // i915_drm.h:213
    /// `I915_ENGINE_CLASS_INVALID = -1` as the u16 the struct carries.
    pub const INVALID: u16 = 0xffff; // i915_drm.h:222
}

/// `enum drm_i915_gem_memory_class` (i915_drm.h:3458-3460).
pub mod memory_class {
    pub const SYSTEM: u16 = 0; // i915_drm.h:3458
    pub const DEVICE: u16 = 1; // i915_drm.h:3460
}

/// `struct drm_i915_gem_exec_object2` with `relocs_ptr` removed (i915_drm.h:1239). C `sizeof` is 56; flattened it is 48.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecObject2 {
    pub handle: u32,
    /// How many [`RelocationEntry`] this object owns in the relocation table (they follow in object order).
    pub relocation_count: u32,
    pub alignment: u64,
    pub offset: u64,
    pub flags: u64,
    /// `rsvd1`, which is `pad_to_size` when `EXEC_OBJECT_PAD_TO_SIZE` is set (i915_drm.h:1313).
    pub pad_to_size: u64,
    pub rsvd2: u64,
}

impl ExecObject2 {
    pub const WIRE_BYTES: usize = 48;
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> ExecObject2 {
        ExecObject2 {
            handle: u32_at(b, 0),
            relocation_count: u32_at(b, 4),
            alignment: u64_at(b, 8),
            offset: u64_at(b, 16),
            flags: u64_at(b, 24),
            pad_to_size: u64_at(b, 32),
            rsvd2: u64_at(b, 40),
        }
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u32(self.handle)?;
        w.u32(self.relocation_count)?;
        w.u64(self.alignment)?;
        w.u64(self.offset)?;
        w.u64(self.flags)?;
        w.u64(self.pad_to_size)?;
        w.u64(self.rsvd2)
    }
}

/// `struct drm_i915_gem_relocation_entry` (i915_drm.h:1118). `sizeof` 32.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RelocationEntry {
    /// An exec-object handle, or an index into the exec list under `I915_EXEC_HANDLE_LUT` (i915_drm.h:1118).
    pub target_handle: u32,
    pub delta: u32,
    pub offset: u64,
    pub presumed_offset: u64,
    pub read_domains: u32,
    pub write_domain: u32,
}

impl RelocationEntry {
    pub const WIRE_BYTES: usize = 32;
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> RelocationEntry {
        RelocationEntry {
            target_handle: u32_at(b, 0),
            delta: u32_at(b, 4),
            offset: u64_at(b, 8),
            presumed_offset: u64_at(b, 16),
            read_domains: u32_at(b, 24),
            write_domain: u32_at(b, 28),
        }
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u32(self.target_handle)?;
        w.u32(self.delta)?;
        w.u64(self.offset)?;
        w.u64(self.presumed_offset)?;
        w.u32(self.read_domains)?;
        w.u32(self.write_domain)
    }
}

/// `struct drm_i915_gem_exec_fence` (i915_drm.h:1327). `sizeof` 8. `handle` is a syncobj handle of the session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecFence {
    pub handle: u32,
    pub flags: u32,
}

impl ExecFence {
    pub const WIRE_BYTES: usize = 8;
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> ExecFence {
        ExecFence { handle: u32_at(b, 0), flags: u32_at(b, 4) }
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u32(self.handle)?;
        w.u32(self.flags)
    }
}

/// `struct i915_user_extension` without `next_extension` (the chain is the order of the records on the wire; i915_drm.h:102).
/// `sizeof` 32 in C; flattened 24.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtensionHeader {
    pub name: u32,
    pub flags: u32,
    pub rsvd: [u32; 4],
}

impl ExtensionHeader {
    pub const WIRE_BYTES: usize = 24;
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> ExtensionHeader {
        ExtensionHeader { name: u32_at(b, 0), flags: u32_at(b, 4), rsvd: [u32_at(b, 8), u32_at(b, 12), u32_at(b, 16), u32_at(b, 20)] }
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u32(self.name)?;
        w.u32(self.flags)?;
        for r in self.rsvd {
            w.u32(r)?;
        }
        Ok(())
    }
}

/// `struct drm_i915_query_item` flattened (i915_drm.h:3057): `data_ptr` is replaced by `hdr`, a snapshot of the first 16
/// bytes of the user's data buffer (the `drm_i915_query_engine_info` / `_memory_regions` header userspace must have zeroed),
/// taken BEFORE validation so a concurrent writer in the client cannot change what was checked. C `sizeof` is 24; flattened 32.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryItem {
    pub query_id: u64,
    /// The capacity of the user buffer in bytes (`0` = "tell me how big"), later the produced length or `-errno`.
    pub length: i32,
    pub flags: u32,
    pub hdr: [u32; 4],
}

impl QueryItem {
    pub const WIRE_BYTES: usize = 32;
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> QueryItem {
        QueryItem {
            query_id: u64_at(b, 0),
            length: u32_at(b, 8) as i32,
            flags: u32_at(b, 12),
            hdr: [u32_at(b, 16), u32_at(b, 20), u32_at(b, 24), u32_at(b, 28)],
        }
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u64(self.query_id)?;
        w.u32(self.length as u32)?;
        w.u32(self.flags)?;
        for h in self.hdr {
            w.u32(h)?;
        }
        Ok(())
    }
}

/// `struct drm_i915_query_topology_info` header (i915_drm.h:3167). `sizeof` 16; the `data[]` follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopologyInfoHeader {
    pub flags: u16,
    pub max_slices: u16,
    pub max_subslices: u16,
    pub max_eus_per_subslice: u16,
    pub subslice_offset: u16,
    pub subslice_stride: u16,
    pub eu_offset: u16,
    pub eu_stride: u16,
}

impl TopologyInfoHeader {
    pub const WIRE_BYTES: usize = 16;
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        for v in [
            self.flags,
            self.max_slices,
            self.max_subslices,
            self.max_eus_per_subslice,
            self.subslice_offset,
            self.subslice_stride,
            self.eu_offset,
            self.eu_stride,
        ] {
            w.u16(v)?;
        }
        Ok(())
    }
    pub fn decode(b: &[u8; Self::WIRE_BYTES]) -> TopologyInfoHeader {
        TopologyInfoHeader {
            flags: u16_at(b, 0),
            max_slices: u16_at(b, 2),
            max_subslices: u16_at(b, 4),
            max_eus_per_subslice: u16_at(b, 6),
            subslice_offset: u16_at(b, 8),
            subslice_stride: u16_at(b, 10),
            eu_offset: u16_at(b, 12),
            eu_stride: u16_at(b, 14),
        }
    }
}

/// `struct drm_i915_engine_info` (i915_drm.h:3345). `sizeof` 56.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineInfo {
    pub engine_class: u16,
    pub engine_instance: u16,
    pub rsvd0: u32,
    pub flags: u64,
    pub capabilities: u64,
    pub logical_instance: u16,
}

impl EngineInfo {
    pub const WIRE_BYTES: usize = 56;
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u16(self.engine_class)?;
        w.u16(self.engine_instance)?;
        w.u32(self.rsvd0)?;
        w.u64(self.flags)?;
        w.u64(self.capabilities)?;
        w.u16(self.logical_instance)?;
        w.zeros(6 + 24) // rsvd1[3] (u16) + rsvd2[3] (u64)
    }
}

/// `struct drm_i915_memory_region_info` (i915_drm.h:3482). `sizeof` 88.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryRegionInfo {
    pub memory_class: u16,
    pub memory_instance: u16,
    pub probed_size: u64,
    pub unallocated_size: u64,
    pub probed_cpu_visible_size: u64,
    pub unallocated_cpu_visible_size: u64,
}

impl MemoryRegionInfo {
    pub const WIRE_BYTES: usize = 88;
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), UapiError> {
        w.u16(self.memory_class)?;
        w.u16(self.memory_instance)?;
        w.u32(0)?; // rsvd0
        w.u64(self.probed_size)?;
        w.u64(self.unallocated_size)?;
        w.u64(self.probed_cpu_visible_size)?;
        w.u64(self.unallocated_cpu_visible_size)?;
        w.zeros(6 * 8) // the rest of rsvd1[8]
    }
}

/// The 16-byte header of `drm_i915_query_engine_info` (`num_engines`, `rsvd[3]`; i915_drm.h:3376) and of
/// `drm_i915_query_memory_regions` (`num_regions`, `rsvd[3]`; :3612): same shape.
pub const QUERY_LIST_HEADER_BYTES: usize = 16;
