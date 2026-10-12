// SPDX-License-Identifier: GPL-2.0-only
//! `DRM_IOCTL_I915_QUERY`: a port of `i915_query_ioctl` and the query functions Gen9 answers (drivers/gpu/drm/i915/i915_query.c,
//! v7.0-rc6): topology info (:34-90 and :92-101), engine info (:130-178), memory regions (:462-532). The remaining table entries answer the
//! error Linux gives a device without that feature (perf config, HWCONFIG blob, geometry subslices, GuC submission version).
//!
//! The two-step protocol is Linux's: `length == 0` returns the size needed; a non-zero `length` below it is `-EINVAL`; otherwise
//! the data is produced and the produced size returned. A per-item failure becomes the item's negative `length`; the ioctl itself
//! succeeds. Wire layout: request = `num_items: u32, flags: u32` + `num_items` x [`QueryItem`] (32 bytes); reply =
//! `status: u32, errno: i32, num_items: u32, pad: u32` + per item `length: i32, data_len: u32, data (zero-padded to 8)`.
use crate::codec::{Reader, Writer};
use crate::consts::misc as m;
use crate::consts::query as q;
use crate::device::{UabiEngine, UapiDevice};
use crate::error::UapiError;
use crate::types::{
    engine_class, memory_class, EngineInfo, MemoryRegionInfo, QueryItem, TopologyInfoHeader, QUERY_LIST_HEADER_BYTES,
};

/// This port's bound on items per call (Linux iterates `num_items` without a bound; there are 7 query ids).
pub const MAX_QUERY_ITEMS: u32 = 16;
pub const QUERY_REQUEST_HEADER_BYTES: usize = 8;
pub const QUERY_REPLY_HEADER_BYTES: usize = 16;
/// `GEN_SSEU_STRIDE(max_entries)` = `DIV_ROUND_UP(max_entries, BITS_PER_BYTE)` (gt/intel_sseu.h:48).
const fn sseu_stride(max_entries: u16) -> u16 {
    max_entries.div_ceil(8)
}

/// The result of one item: what Linux leaves in `item.length`, and how many data bytes were produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemResult {
    pub length: i32,
    pub written: usize,
}

/// `copy_query_item`'s size protocol (i915_query.c:17-32). `Ok(false)` = "size probe, return `total`".
fn proceed(item: &QueryItem, total: u32) -> Result<bool, UapiError> {
    if item.length == 0 {
        return Ok(false); // :21-22
    }
    // `query_item->length < total_length` compares an s32 with a u32 (i915_query.c:24): a negative length converts to a huge
    // unsigned value and passes. Faithful; the data is then bounded by the reply buffer, not by `length`.
    if (item.length as u32) < total {
        return Err(UapiError::QueryLengthTooSmall { query_id: item.query_id, have: item.length, need: total });
    }
    Ok(true)
}

/// Run ONE query item against `data` (the reply space for it). Never partial: on `Err` nothing was written.
pub fn query_item(dev: &UapiDevice, item: &QueryItem, data: &mut [u8]) -> ItemResult {
    match run(dev, item, data) {
        Ok(r) => r,
        Err(e) => ItemResult { length: e.errno().neg(), written: 0 },
    }
}

fn run(dev: &UapiDevice, item: &QueryItem, data: &mut [u8]) -> Result<ItemResult, UapiError> {
    if item.query_id == 0 {
        return Err(UapiError::QueryIdZero); // :618-619
    }
    match item.query_id {
        id if id == u64::from(q::DRM_I915_QUERY_TOPOLOGY_INFO) => query_topology(dev, item, data),
        id if id == u64::from(q::DRM_I915_QUERY_ENGINE_INFO) => query_engine_info(dev, item, data),
        id if id == u64::from(q::DRM_I915_QUERY_PERF_CONFIG) => query_perf_config(item),
        id if id == u64::from(q::DRM_I915_QUERY_MEMORY_REGIONS) => query_memregion_info(dev, item, data),
        id if id == u64::from(q::DRM_I915_QUERY_HWCONFIG_BLOB) => {
            // :540-541 `!hwconfig->size || !hwconfig->ptr` -> -ENODEV: only Xe_HP-class firmware provides a table; Gen9 has none.
            Err(UapiError::QueryNoDevice { query_id: item.query_id, why: "no hwconfig table on Gen9" })
        }
        id if id == u64::from(q::DRM_I915_QUERY_GEOMETRY_SUBSLICES) => {
            // :110-111 `GRAPHICS_VER_FULL < IP_VER(12, 55)` -> -ENODEV
            Err(UapiError::QueryNoDevice { query_id: item.query_id, why: "geometry subslices exist from Xe_HP (12.55)" })
        }
        id if id == u64::from(q::DRM_I915_QUERY_GUC_SUBMISSION_VERSION) => {
            // :567-568 `!intel_uc_uses_guc_submission` -> -ENODEV (execlists only)
            Err(UapiError::QueryNoDevice { query_id: item.query_id, why: "no GuC submission" })
        }
        _ => Err(UapiError::QueryUnknownId { query_id: item.query_id }), // :624-630
    }
}

fn flags_zero(item: &QueryItem) -> Result<(), UapiError> {
    if item.flags != 0 {
        return Err(UapiError::QueryItemFlags { query_id: item.query_id, flags: item.flags });
    }
    Ok(())
}

/// `query_topology_info` + `fill_topology_info` (i915_query.c:34-90, :92-101): the SSEU as the three masks.
fn query_topology(dev: &UapiDevice, item: &QueryItem, data: &mut [u8]) -> Result<ItemResult, UapiError> {
    flags_zero(item)?; // :97-98
    let sseu = &dev.sseu;
    if sseu.max_slices == 0 {
        return Err(UapiError::QueryNoDevice { query_id: item.query_id, why: "no slices" }); // :46-47
    }
    let ss_stride = sseu_stride(u16::from(sseu.max_subslices)); // :40
    let eu_stride = sseu_stride(u16::from(sseu.max_eus_per_subslice)); // :41
    let slice_length: u16 = 1; // sizeof(sseu->slice_mask), :49
    let subslice_length = u16::from(sseu.max_slices) * ss_stride; // :50
    let eu_length = u16::from(sseu.max_slices) * u16::from(sseu.max_subslices) * eu_stride; // :51
    let total = (TopologyInfoHeader::WIRE_BYTES as u32) + u32::from(slice_length) + u32::from(subslice_length) + u32::from(eu_length); // :52-53
    if !proceed(item, total)? {
        return Ok(ItemResult { length: total as i32, written: 0 });
    }
    let hdr = TopologyInfoHeader {
        flags: 0,
        max_slices: u16::from(sseu.max_slices),
        max_subslices: u16::from(sseu.max_subslices),
        max_eus_per_subslice: u16::from(sseu.max_eus_per_subslice),
        subslice_offset: slice_length,
        subslice_stride: ss_stride,
        eu_offset: slice_length + subslice_length,
        eu_stride,
    }; // :60-68
    let mut w = Writer::new(data);
    hdr.encode(&mut w)?;
    w.bytes(&[sseu.slice_mask])?; // :74-76 copy of `sseu->slice_mask`
    // intel_sseu_copy_ssmask_to_user (gt/intel_sseu.c:130-150): bit `s * ss_stride * 8 + ss` per present subslice.
    let mut ss_mask = [0u8; 32];
    for s in 0..usize::from(sseu.max_slices) {
        for ss in 0..usize::from(sseu.max_subslices) {
            if sseu.subslice_mask.get(s).is_some_and(|mk| mk & (1 << ss) != 0) {
                let i = s * usize::from(ss_stride) * 8 + ss;
                ss_mask[i / 8] |= 1 << (i % 8);
            }
        }
    }
    w.bytes(&ss_mask[..usize::from(subslice_length)])?;
    // intel_sseu_copy_eumask_to_user (gt/intel_sseu.c:96-118): `eu_stride` little-endian bytes per (slice, subslice).
    let mut eu_mask = [0u8; 64];
    for s in 0..usize::from(sseu.max_slices) {
        for ss in 0..usize::from(sseu.max_subslices) {
            let off = s * usize::from(sseu.max_subslices) * usize::from(eu_stride) + ss * usize::from(eu_stride);
            let mask = u16::from(sseu.eu_mask.get(s).and_then(|row| row.get(ss)).copied().unwrap_or(0));
            for i in 0..usize::from(eu_stride) {
                eu_mask[off + i] = (mask >> (8 * i)) as u8;
            }
        }
    }
    w.bytes(&eu_mask[..usize::from(eu_length)])?;
    Ok(ItemResult { length: total as i32, written: w.pos() })
}

/// `__setup_engine_capabilities` (gt/intel_engine_cs.c:620-650) for the generations carried (Gen9): HEVC and SFC on VCS0 only;
/// the VECS SFC bit needs `gt->info.sfc_mask`, which stays zero below Gen11 (intel_engine_cs.c:760-761).
fn engine_capabilities(gen: u8, e: &UabiEngine) -> u64 {
    let mut caps = 0u64;
    if e.class == engine_class::VIDEO {
        if gen >= 11 || (gen >= 9 && e.instance == 0) {
            caps |= u64::from(m::I915_VIDEO_CLASS_CAPABILITY_HEVC); // :629-632
        }
        if gen >= 9 && e.instance == 0 {
            caps |= u64::from(m::I915_VIDEO_AND_ENHANCE_CLASS_CAPABILITY_SFC); // :638-643
        }
    }
    caps
}

/// `query_engine_info` (i915_query.c:130-178).
fn query_engine_info(dev: &UapiDevice, item: &QueryItem, data: &mut [u8]) -> Result<ItemResult, UapiError> {
    flags_zero(item)?; // :142-143
    let engines = dev.engines();
    let total = (QUERY_LIST_HEADER_BYTES + engines.len() * EngineInfo::WIRE_BYTES) as u32; // :148 struct_size()
    if !proceed(item, total)? {
        return Ok(ItemResult { length: total as i32, written: 0 });
    }
    // :154-156 userspace's header must be zero (num_engines and rsvd[3]).
    if item.hdr != [0; 4] {
        return Err(UapiError::QueryHeaderNotZero { query_id: item.query_id });
    }
    let mut w = Writer::new(data);
    w.u32(engines.len() as u32)?; // query.num_engines, written last in Linux (:174-175); same bytes
    w.zeros(12)?; // rsvd[3]
    for e in engines.as_slice() {
        EngineInfo {
            engine_class: e.class,
            engine_instance: e.instance,
            rsvd0: 0,
            flags: u64::from(m::I915_ENGINE_INFO_HAS_LOGICAL_INSTANCE), // :163
            capabilities: engine_capabilities(dev.gen(), e),             // :164
            logical_instance: e.instance, // :165 ilog2(engine->logical_mask), mask = BIT(instance) (intel_engine_cs.c:497)
        }
        .encode(&mut w)?;
    }
    Ok(ItemResult { length: total as i32, written: w.pos() })
}

/// `query_perf_config` (i915_query.c:447-460): the sub-queries answer `-ENODEV` when `perf->i915` is NULL (:244-245, :385-386), which is
/// the state of a device whose perf/OA is not ported; any other `flags` is `-EINVAL` (:458).
fn query_perf_config(item: &QueryItem) -> Result<ItemResult, UapiError> {
    match item.flags {
        q::DRM_I915_QUERY_PERF_CONFIG_LIST | q::DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_UUID | q::DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_ID => {
            Err(UapiError::QueryNoDevice { query_id: item.query_id, why: "i915 perf/OA is not ported" })
        }
        flags => Err(UapiError::QueryItemFlags { query_id: item.query_id, flags }),
    }
}

/// `query_memregion_info` (i915_query.c:462-532). Gen9 has exactly one PUBLIC region, SYSTEM (class 0, instance 0); the stolen
/// region is `private` and skipped (gem/i915_gem_stolen.c:998,1019).
fn query_memregion_info(dev: &UapiDevice, item: &QueryItem, data: &mut [u8]) -> Result<ItemResult, UapiError> {
    flags_zero(item)?; // :475-476
    let total = (QUERY_LIST_HEADER_BYTES + MemoryRegionInfo::WIRE_BYTES) as u32; // :479-484
    if !proceed(item, total)? {
        return Ok(ItemResult { length: total as i32, written: 0 });
    }
    if item.hdr != [0; 4] {
        return Err(UapiError::QueryHeaderNotZero { query_id: item.query_id }); // :490-495
    }
    let probed = dev.system_memory_bytes;
    // :511-519 perfmon_capable() sees live availability; everyone else sees probed == unallocated.
    let unallocated = if dev.perfmon_capable { dev.system_available_bytes } else { probed };
    let mut w = Writer::new(data);
    w.u32(1)?; // query.num_regions
    w.zeros(12)?;
    MemoryRegionInfo {
        memory_class: memory_class::SYSTEM, // :502 mr->type
        memory_instance: 0,                 // :503 mr->instance
        probed_size: probed,                // :504
        unallocated_size: unallocated,
        probed_cpu_visible_size: probed, // :506-509 non-LOCAL regions: probed_cpu_visible_size = mr->total
        unallocated_cpu_visible_size: unallocated,
    }
    .encode(&mut w)?;
    Ok(ItemResult { length: total as i32, written: w.pos() })
}

/// Decode a QUERY request payload, run every item, encode the reply. Transactional: any whole-call refusal (`flags != 0`, a zero
/// `query_id`, a malformed payload) returns an error and produces NO per-item results (Linux would already have written back the
/// lengths of earlier items; here the reply either carries every item or only the errno).
pub fn handle_query(dev: &UapiDevice, payload: &[u8], out: &mut [u8]) -> Result<usize, UapiError> {
    let mut r = Reader::new(payload);
    let num_items = r.u32("query.num_items")?;
    let flags = r.u32("query.flags")?;
    if num_items > MAX_QUERY_ITEMS {
        return Err(UapiError::LimitExceeded { what: "query.num_items", got: u64::from(num_items), max: u64::from(MAX_QUERY_ITEMS) });
    }
    let mut items = [QueryItem::default(); MAX_QUERY_ITEMS as usize];
    for slot in items.iter_mut().take(num_items as usize) {
        let b = r.take("query.item", QueryItem::WIRE_BYTES)?;
        let mut a = [0u8; QueryItem::WIRE_BYTES];
        a.copy_from_slice(b);
        *slot = QueryItem::decode(&a);
    }
    if r.remaining() != 0 {
        return Err(UapiError::TrailingBytes { extra: r.remaining() });
    }
    let items = &items[..num_items as usize];

    let mut w = Writer::new(out);
    w.u32(i915_wire::Status::Ok as u32)?;
    // ioctl-level refusals: -EINVAL for flags (:607-608) and for a zero query_id (:618-619).
    let whole = if flags != 0 {
        Some(UapiError::QueryFlagsNonZero { flags })
    } else if items.iter().any(|i| i.query_id == 0) {
        Some(UapiError::QueryIdZero)
    } else {
        None
    };
    if let Some(e) = whole {
        w.u32(e.errno().neg() as u32)?;
        w.u32(0)?;
        w.u32(0)?;
        return Ok(w.pos());
    }
    w.u32(0)?; // errno
    w.u32(num_items)?;
    w.u32(0)?;
    let mut pos = w.pos();
    for item in items {
        // the item's data goes right after its 8-byte result header
        let data_start = pos + 8;
        if data_start > out.len() {
            return Err(UapiError::QueryReplyTooSmall { need: data_start, have: out.len() });
        }
        // A reply buffer too small for what a query produced is the device's sizing bug, not a per-item answer: refuse the call.
        let res = match run(dev, item, &mut out[data_start..]) {
            Ok(r) => r,
            Err(e @ UapiError::QueryReplyTooSmall { .. }) => return Err(e),
            Err(e) => ItemResult { length: e.errno().neg(), written: 0 },
        };
        let padded = res.written.div_ceil(8) * 8;
        if data_start + padded > out.len() {
            return Err(UapiError::QueryReplyTooSmall { need: data_start + padded, have: out.len() });
        }
        out[data_start + res.written..data_start + padded].fill(0);
        let mut h = Writer::new(&mut out[pos..data_start]);
        h.u32(res.length as u32)?;
        h.u32(res.written as u32)?;
        pos = data_start + padded;
    }
    Ok(pos)
}
